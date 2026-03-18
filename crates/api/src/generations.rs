use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// An in-progress LLM generation tied to a conversation.
pub struct ActiveGeneration {
    /// Multi-subscriber broadcast for SSE chunks.
    pub tx: broadcast::Sender<String>,
    /// Accumulated chunks so reconnecting clients can catch up.
    pub buffer: Arc<Mutex<Vec<String>>>,
    /// Token to cancel the generation server-side.
    pub cancel: CancellationToken,
    /// When this generation started (for stale cleanup).
    pub started_at: Instant,
}

/// Registry of in-progress generations, keyed by conversation ID.
#[derive(Clone)]
pub struct ActiveGenerations {
    inner: Arc<Mutex<HashMap<Uuid, ActiveGeneration>>>,
}

impl ActiveGenerations {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a new generation for `conv_id`.
    /// If one already exists, cancels it first.
    /// Returns (broadcast::Sender, buffer, CancellationToken) for the spawned task.
    pub fn start(
        &self,
        conv_id: Uuid,
    ) -> (broadcast::Sender<String>, Arc<Mutex<Vec<String>>>, CancellationToken) {
        let mut map = self.inner.lock().unwrap();

        // Cancel any existing generation for this conversation
        if let Some(old) = map.remove(&conv_id) {
            old.cancel.cancel();
        }

        let (tx, _rx) = broadcast::channel::<String>(256);
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let cancel = CancellationToken::new();

        let gen = ActiveGeneration {
            tx: tx.clone(),
            buffer: buffer.clone(),
            cancel: cancel.clone(),
            started_at: Instant::now(),
        };

        map.insert(conv_id, gen);

        (tx, buffer, cancel)
    }

    /// Subscribe to an active generation.
    /// Returns (broadcast::Receiver, catchup buffer snapshot, CancellationToken),
    /// or None if no active generation exists.
    pub fn subscribe(
        &self,
        conv_id: Uuid,
    ) -> Option<(broadcast::Receiver<String>, Vec<String>, CancellationToken)> {
        let map = self.inner.lock().unwrap();
        let gen = map.get(&conv_id)?;
        let rx = gen.tx.subscribe();
        let catchup = gen.buffer.lock().unwrap().clone();
        let cancel = gen.cancel.clone();
        Some((rx, catchup, cancel))
    }

    /// Remove a finished generation from the registry.
    pub fn finish(&self, conv_id: Uuid) {
        let mut map = self.inner.lock().unwrap();
        map.remove(&conv_id);
    }

    /// Cancel an active generation. Returns true if one existed.
    pub fn stop(&self, conv_id: Uuid) -> bool {
        let map = self.inner.lock().unwrap();
        if let Some(gen) = map.get(&conv_id) {
            gen.cancel.cancel();
            true
        } else {
            false
        }
    }

    /// Remove generations older than `max_age` (safety net for leaked entries).
    pub fn cleanup_stale(&self, max_age: std::time::Duration) {
        let mut map = self.inner.lock().unwrap();
        let before = map.len();
        map.retain(|_id, gen| gen.started_at.elapsed() < max_age);
        let removed = before - map.len();
        if removed > 0 {
            tracing::info!(removed, "cleaned up stale generations");
        }
    }
}
