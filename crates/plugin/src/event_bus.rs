use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{broadcast, RwLock};

/// An event on the bus.
#[derive(Debug, Clone)]
pub struct Event {
    /// Event topic (e.g. "message.received", "agent.status_changed").
    pub topic: String,
    /// Event payload as JSON.
    pub payload: Value,
}

type Subscriber = broadcast::Sender<Event>;

/// Simple pub/sub event bus for inter-plugin communication.
#[derive(Debug, Clone)]
pub struct EventBus {
    topics: Arc<RwLock<HashMap<String, Subscriber>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            topics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Publish an event to a topic.
    pub async fn publish(&self, event: Event) {
        let topics = self.topics.read().await;
        if let Some(tx) = topics.get(&event.topic) {
            // Ignore send errors (no subscribers).
            let _ = tx.send(event);
        }
    }

    /// Subscribe to a topic. Returns a receiver that will get all future events on that topic.
    pub async fn subscribe(&self, topic: &str) -> broadcast::Receiver<Event> {
        let mut topics = self.topics.write().await;
        let tx = topics
            .entry(topic.to_string())
            .or_insert_with(|| broadcast::channel(256).0);
        tx.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
