/// Chat command parser: intercept `/` commands before LLM call.
///
/// Supported commands:
/// - `/compact` — Force compaction of conversation history
/// - `/status` — Show agent and conversation status
/// - `/new` — Start a new conversation (frontend handles this, but we acknowledge)
/// - `/reset` — Clear conversation history
/// - `/think` — Enable extended reasoning for next message
/// - `/verbose` — Toggle verbose tool output
/// - `/usage` — Show token usage for this conversation
use serde::{Deserialize, Serialize};

/// A parsed chat command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum ChatCommand {
    Compact,
    Status,
    New,
    Reset,
    Think,
    Verbose,
    Usage,
    Unknown { name: String },
}

/// Result of attempting to parse a user message as a command.
pub enum CommandParseResult {
    /// The message is a command — return a synthetic response instead of calling LLM.
    Command(ChatCommand),
    /// The message is not a command — proceed with normal LLM flow.
    NotCommand,
}

/// Try to parse a user message as a chat command.
pub fn parse_command(message: &str) -> CommandParseResult {
    let trimmed = message.trim();
    if !trimmed.starts_with('/') {
        return CommandParseResult::NotCommand;
    }

    // Extract the command name (first word after /)
    let cmd_str = trimmed[1..].split_whitespace().next().unwrap_or("");

    let command = match cmd_str.to_lowercase().as_str() {
        "compact" => ChatCommand::Compact,
        "status" => ChatCommand::Status,
        "new" => ChatCommand::New,
        "reset" => ChatCommand::Reset,
        "think" => ChatCommand::Think,
        "verbose" => ChatCommand::Verbose,
        "usage" => ChatCommand::Usage,
        _ => {
            // Don't treat skill invocations as unknown commands
            // Skills use /skill-name syntax too, so only catch truly unknown ones
            return CommandParseResult::NotCommand;
        }
    };

    CommandParseResult::Command(command)
}

/// Generate a synthetic assistant response for a command.
pub fn command_response(cmd: &ChatCommand) -> String {
    match cmd {
        ChatCommand::Compact => "Compacting conversation history...".to_string(),
        ChatCommand::Status => "Checking status...".to_string(),
        ChatCommand::New => "Starting a new conversation. Use the sidebar to switch back to this one.".to_string(),
        ChatCommand::Reset => "Conversation history has been cleared.".to_string(),
        ChatCommand::Think => "Extended reasoning enabled for the next message. Send your question and I'll think through it more carefully.".to_string(),
        ChatCommand::Verbose => "Verbose mode toggled. Tool outputs will now show more detail.".to_string(),
        ChatCommand::Usage => "Fetching usage statistics...".to_string(),
        ChatCommand::Unknown { name } => format!("Unknown command: /{}", name),
    }
}
