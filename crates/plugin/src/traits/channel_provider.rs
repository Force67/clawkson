use serde_json::Value;

/// A message received from a channel.
#[derive(Debug, Clone)]
pub struct InboundMessage {
    /// Channel-specific sender identifier.
    pub sender_id: String,
    /// Display name of the sender (if available).
    pub sender_name: Option<String>,
    /// The message text.
    pub text: String,
    /// Channel-specific conversation/chat identifier.
    pub channel_chat_id: String,
    /// Optional attachments as JSON metadata.
    pub attachments: Vec<Value>,
}

/// A message to send through a channel.
#[derive(Debug, Clone)]
pub struct OutboundMessage {
    /// Channel-specific conversation/chat identifier.
    pub channel_chat_id: String,
    /// The message text to send.
    pub text: String,
    /// Optional reply-to message ID.
    pub reply_to: Option<String>,
}

/// Extension trait for plugins that provide messaging channels.
#[async_trait::async_trait]
pub trait ChannelProvider: Send + Sync {
    /// Return the channel type name (e.g. "discord", "telegram").
    fn channel_type(&self) -> &str;

    /// JSON schema for the connector config.
    fn config_schema(&self) -> Value;

    /// Start listening for inbound messages. The callback is invoked for each.
    async fn start(
        &self,
        config: Value,
        on_message: Box<dyn Fn(InboundMessage) + Send + Sync>,
    ) -> anyhow::Result<()>;

    /// Stop listening for inbound messages.
    async fn stop(&self) -> anyhow::Result<()>;

    /// Send an outbound message.
    async fn send_message(&self, msg: OutboundMessage) -> anyhow::Result<()>;
}
