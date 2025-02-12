use crate::protocol::{
    BroadcastMessage, HeartbeatMessage, PhoenixMessage, PostgresChangesMessage,
    PresenceDiffMessage, PresenceStateMessage, Topic,
};
use std::{collections::HashMap, marker::PhantomData, task::Poll, time::Duration};
use tokio::{
    sync::mpsc::{Receiver, Sender},
    task::{self, AbortHandle},
};

#[derive(Debug)]
pub struct Broadcast;

#[derive(Debug)]
pub struct Presence;

#[derive(Debug)]
pub struct Postgres;

/// Subscription instance.
pub struct Subscription<T> {
    pub(crate) _t: PhantomData<T>,
    pub(crate) topic: Topic,
    pub(crate) receiver: Receiver<PhoenixMessage>,
    pub(crate) sender: Sender<PhoenixMessage>,
    pub(crate) heartbeat: AbortHandle,
}

impl<T> Subscription<T> {
    pub fn new(
        topic: Topic,
        receiver: Receiver<PhoenixMessage>,
        sender: Sender<PhoenixMessage>,
    ) -> Self {
        // spawn heartbeat task. cleaned up on drop.
        let heartbeat_sender = sender.clone();
        let heartbeat_topic = topic.clone();
        let heartbeat = task::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(25)).await;
                if let Err(e) = heartbeat_sender
                    .send(PhoenixMessage::Heartbeat(HeartbeatMessage {
                        topic: heartbeat_topic.clone(),
                        payload: HashMap::new(),
                        reference: "".to_owned(), // todo: use an actual reference
                    }))
                    .await
                {
                    tracing::error!("Send heartbeat failed: {}", e);
                }
            }
        })
        .abort_handle();

        Self {
            _t: PhantomData::default(),
            topic,
            sender,
            receiver,
            heartbeat,
        }
    }
}

impl<T> Drop for Subscription<T> {
    fn drop(&mut self) {
        self.heartbeat.abort();
    }
}

impl futures::Stream for Subscription<Broadcast> {
    type Item = BroadcastMessage;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match self.receiver.poll_recv(cx) {
            Poll::Ready(msg) => match msg {
                Some(PhoenixMessage::Broadcast(bcast)) => Poll::Ready(Some(bcast)),
                Some(_) => {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                None => Poll::Ready(None),
            },
            Poll::Pending => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
}

#[derive(Debug)]
pub enum PresenceMessage {
    State(PresenceStateMessage),
    Diff(PresenceDiffMessage),
}

impl futures::Stream for Subscription<Presence> {
    type Item = PresenceMessage;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        match self.receiver.poll_recv(cx) {
            Poll::Ready(msg) => match msg {
                Some(PhoenixMessage::PresenceState(state)) => {
                    Poll::Ready(Some(PresenceMessage::State(state)))
                }
                Some(PhoenixMessage::PresenceDiff(diff)) => {
                    Poll::Ready(Some(PresenceMessage::Diff(diff)))
                }
                _ => Poll::Pending,
            },
            Poll::Pending => Poll::Pending,
        }
    }
}

impl futures::Stream for Subscription<Postgres> {
    type Item = PostgresChangesMessage;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match self.receiver.poll_recv(cx) {
            Poll::Ready(msg) => match msg {
                Some(PhoenixMessage::PostgresChanges(changes)) => Poll::Ready(Some(changes)),
                _ => Poll::Pending,
            },
            Poll::Pending => Poll::Pending,
        }
    }
}
