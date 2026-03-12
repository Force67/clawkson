//! Debounced conversation memory embedder.
//!
//! Buffers chat turns per conversation and embeds them into the user's "Memory"
//! knowledge base after a quiet period (no new messages for `DEBOUNCE_SECS`).
//! This avoids hammering the embedding API during rapid back-and-forth exchanges.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::task::AbortHandle;
use uuid::Uuid;

const DEBOUNCE_SECS: u64 = 30;

/// A single buffered chat turn.
struct BufferedTurn {
    user_content: String,
    assistant_content: String,
}

/// Per-conversation buffer state.
struct ConversationBuffer {
    user_id: Uuid,
    conversation_title: String,
    turns: Vec<BufferedTurn>,
    /// Abort handle for the pending flush timer.
    timer_handle: Option<AbortHandle>,
}

/// Debounced memory embedder — add to `AppState` and call `push_turn` after each chat.
#[derive(Clone)]
pub struct MemoryEmbedder {
    buffers: Arc<Mutex<HashMap<Uuid, ConversationBuffer>>>,
    db: clawkson_db::Db,
}

impl MemoryEmbedder {
    pub fn new(db: clawkson_db::Db) -> Self {
        Self {
            buffers: Arc::new(Mutex::new(HashMap::new())),
            db,
        }
    }

    /// Buffer a chat turn for later embedding. Resets the debounce timer.
    pub async fn push_turn(
        &self,
        conversation_id: Uuid,
        user_id: Uuid,
        conversation_title: String,
        user_content: String,
        assistant_content: String,
    ) {
        let mut buffers = self.buffers.lock().await;

        let buf = buffers.entry(conversation_id).or_insert_with(|| ConversationBuffer {
            user_id,
            conversation_title: conversation_title.clone(),
            turns: Vec::new(),
            timer_handle: None,
        });

        // Update title in case it changed
        buf.conversation_title = conversation_title;

        // Cancel the previous timer
        if let Some(handle) = buf.timer_handle.take() {
            handle.abort();
        }

        buf.turns.push(BufferedTurn {
            user_content,
            assistant_content,
        });

        // Start a new debounce timer
        let embedder = self.clone();
        let conv_id = conversation_id;
        let task = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(DEBOUNCE_SECS)).await;
            embedder.flush(conv_id).await;
        });
        buf.timer_handle = Some(task.abort_handle());
    }

    /// Flush buffered turns for a conversation — combine and embed them.
    async fn flush(&self, conversation_id: Uuid) {
        let entry = {
            let mut buffers = self.buffers.lock().await;
            buffers.remove(&conversation_id)
        };

        let Some(buf) = entry else { return };
        if buf.turns.is_empty() {
            return;
        }

        let user_id = buf.user_id;
        let title = buf.conversation_title;

        // Combine all buffered turns into a single passage
        let mut combined = String::new();
        for (i, turn) in buf.turns.iter().enumerate() {
            if i > 0 {
                combined.push_str("\n\n---\n\n");
            }
            combined.push_str(&format!("User: {}\n\nAssistant: {}", turn.user_content, turn.assistant_content));
        }

        // Truncate to avoid embedding API limits
        if combined.len() > 8000 {
            combined.truncate(8000);
            combined.push_str("...");
        }

        tracing::info!(
            conversation_id = %conversation_id,
            user_id = %user_id,
            turns = buf.turns.len(),
            chars = combined.len(),
            "memory embed: flushing buffered turns"
        );

        // Load embedding config
        let settings = match clawkson_db::settings::get(&self.db).await {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!("memory embed: no settings, skipping: {e}");
                return;
            }
        };

        let embed_config = crate::embeddings::EmbeddingConfig {
            base_url: settings.embedding_api_base_url,
            api_key: settings.embedding_api_key.clone(),
            model: settings.embedding_model.clone(),
        };

        // Get or create the user's memory KB
        let memory_kb = match clawkson_db::knowledge_base::get_or_create_memory_kb(
            self.db.pool(),
            user_id,
            &settings.embedding_model,
        )
        .await
        {
            Ok(kb) => kb,
            Err(e) => {
                tracing::warn!("memory embed: failed to get/create memory KB: {e}");
                return;
            }
        };

        // Create a knowledge entry
        let entry = match clawkson_db::knowledge_entry::create(
            self.db.pool(),
            memory_kb.id,
            &title,
            &combined,
            None,
        )
        .await
        {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("memory embed: failed to create entry: {e}");
                return;
            }
        };

        // Generate and store embedding
        match crate::embeddings::generate_one(&embed_config, &memory_kb.embedding_model, &combined).await {
            Ok(embedding) => {
                if let Err(e) = clawkson_db::knowledge_entry::set_embedding(
                    self.db.pool(),
                    entry.id,
                    &embedding,
                    None,
                )
                .await
                {
                    tracing::warn!("memory embed: failed to store embedding: {e}");
                } else {
                    tracing::info!(
                        user_id = %user_id,
                        turns = buf.turns.len(),
                        "memory embed: successfully embedded buffered turns"
                    );
                }
            }
            Err(e) => {
                tracing::warn!("memory embed: failed to generate embedding: {e}");
            }
        }
    }
}
