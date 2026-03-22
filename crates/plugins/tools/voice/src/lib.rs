/// Voice / Speech plugin for Clawkson.
/// Provides TTS (ElevenLabs, OpenAI) and STT (Whisper) capabilities,
/// plus HTTP routes for audio upload/download.
use std::collections::HashSet;
use std::sync::Arc;

use axum::{
    extract::Multipart,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use clawkson_plugin::{
    ClawksonPlugin, PluginCapability, PluginContext, PluginManifest,
    RouteProvider, ToolContext, ToolProvider,
};
use denkwerk::DynKernelFunction;
use serde::{Deserialize, Serialize};

/// Supported TTS providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TtsProvider {
    ElevenLabs,
    OpenAi,
}

/// Supported STT providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SttProvider {
    Whisper,
}

/// Configuration for the voice plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceConfig {
    /// TTS provider to use.
    pub tts_provider: TtsProvider,
    /// STT provider to use.
    pub stt_provider: SttProvider,
    /// API key for the TTS provider.
    pub api_key: String,
    /// Voice ID or name for TTS (provider-specific).
    #[serde(default = "default_voice")]
    pub voice: String,
}

fn default_voice() -> String {
    "alloy".to_string()
}

#[derive(Debug)]
pub struct VoicePlugin {
    manifest: PluginManifest,
    client: reqwest::Client,
}

impl VoicePlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::Tools);
        caps.insert(PluginCapability::Routes);

        Self {
            manifest: PluginManifest {
                name: "voice".to_string(),
                display_name: "Voice / Speech".to_string(),
                description: "Text-to-speech (ElevenLabs, OpenAI) and speech-to-text (Whisper) tools with audio routes.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![],
                capabilities: caps,
                frontend: None,
            },
            client: reqwest::Client::new(),
        }
    }

    /// Synthesize text to speech (stub).
    async fn text_to_speech(
        &self,
        _text: &str,
        _voice: &str,
    ) -> anyhow::Result<Vec<u8>> {
        tracing::info!("TTS requested (stub)");
        anyhow::bail!("Text-to-speech not yet implemented")
    }

    /// Transcribe audio to text (stub).
    async fn speech_to_text(
        &self,
        _audio_data: &[u8],
    ) -> anyhow::Result<String> {
        tracing::info!("STT requested (stub)");
        anyhow::bail!("Speech-to-text not yet implemented")
    }
}

impl Default for VoicePlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for VoicePlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("Voice plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        tracing::info!("Voice plugin shut down");
        Ok(())
    }
}

#[async_trait::async_trait]
impl ToolProvider for VoicePlugin {
    async fn tools(&self, _ctx: &ToolContext) -> Vec<DynKernelFunction> {
        tracing::info!("Voice: listing tools (stub)");

        // In a full implementation, this would return two DynKernelFunctions:
        //
        // 1. text_to_speech(text: String, voice?: String) -> String
        //    Converts text to audio and returns a URL to download the audio file.
        //
        // 2. speech_to_text(audio_url: String) -> String
        //    Transcribes an audio file and returns the text.
        Vec::new()
    }
}

impl RouteProvider for VoicePlugin {
    fn prefix(&self) -> &str {
        "/api/plugins/voice"
    }

    fn routes(&self) -> Router {
        Router::new()
            .route("/tts", post(handle_tts))
            .route("/stt", post(handle_stt))
            .route("/health", get(handle_health))
    }
}

/// POST /api/plugins/voice/tts — text-to-speech endpoint (stub).
async fn handle_tts(
    Json(payload): Json<TtsRequest>,
) -> impl IntoResponse {
    tracing::info!(text_len = payload.text.len(), "Voice TTS route called (stub)");
    Json(serde_json::json!({
        "error": "TTS not yet implemented",
        "text_length": payload.text.len()
    }))
}

/// POST /api/plugins/voice/stt — speech-to-text endpoint (stub).
async fn handle_stt(
    mut multipart: Multipart,
) -> impl IntoResponse {
    let mut audio_size = 0usize;
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("audio") {
            if let Ok(data) = field.bytes().await {
                audio_size = data.len();
            }
        }
    }
    tracing::info!(audio_size, "Voice STT route called (stub)");
    Json(serde_json::json!({
        "error": "STT not yet implemented",
        "audio_size": audio_size
    }))
}

/// GET /api/plugins/voice/health — health check.
async fn handle_health() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "plugin": "voice"
    }))
}

#[derive(Debug, Deserialize)]
struct TtsRequest {
    text: String,
    #[serde(default = "default_voice")]
    #[allow(dead_code)]
    voice: String,
}
