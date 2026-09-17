mod anthropic;
mod openai_compatible;

use serde::{Deserialize, Serialize};

/// A role a message can be authored under. Deliberately excludes "system" -
/// the system prompt is passed to [`send_chat_message`] separately, and each
/// provider implementation maps it into whatever shape that provider wants
/// (a "system"-role message for the OpenAI-compatible APIs, a top-level
/// `system` field for Anthropic).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

/// Everything that can go wrong while talking to a provider's HTTP API.
#[derive(Debug)]
pub enum ProviderError {
    /// The request itself failed (DNS, TCP, TLS, timeout, ...).
    Network(reqwest::Error),
    /// The provider responded with a non-2xx status. `body` is the raw
    /// response text, so the caller can see exactly what the API said.
    Http { status: u16, body: String },
    /// The response body wasn't shaped the way we expected.
    Parse(String),
    /// `provider` didn't match any of the known provider names.
    UnknownProvider(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::Network(e) => write!(f, "network error: {e}"),
            ProviderError::Http { status, body } => {
                write!(f, "provider returned HTTP {status}: {body}")
            }
            ProviderError::Parse(msg) => write!(f, "failed to parse provider response: {msg}"),
            ProviderError::UnknownProvider(p) => write!(f, "unknown provider: \"{p}\""),
        }
    }
}

impl std::error::Error for ProviderError {}

impl From<reqwest::Error> for ProviderError {
    fn from(e: reqwest::Error) -> Self {
        ProviderError::Network(e)
    }
}

const OPENAI_BASE_URL: &str = "https://api.openai.com/v1/chat/completions";
const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com/chat/completions";
// International DashScope endpoint - the China-region one is a separate
// concern for later, not handled here.
// Note: this constant (like OPENAI_BASE_URL/DEEPSEEK_BASE_URL above) is the
// full request URL, not just the "base_url" Alibaba's docs refer to - it
// must end in `/chat/completions`, not just `/compatible-mode/v1`.
const QWEN_BASE_URL: &str =
    "https://ws-dxasge76aqlq8gfb.us-east-1.maas.aliyuncs.com/compatible-mode/v1/chat/completions";

/// Send a single chat request to whichever provider `provider` names and
/// return the assistant's reply text.
///
/// `client` is expected to be a shared, reused `reqwest::Client` (e.g. one
/// held as Tauri managed state) rather than a fresh one per call.
pub async fn send_chat_message(
    client: &reqwest::Client,
    provider: &str,
    model: &str,
    system_prompt: Option<&str>,
    messages: &[ChatMessage],
    api_key: &str,
    temperature: f32,
) -> Result<String, ProviderError> {
    match provider {
        "openai" => {
            openai_compatible::send(
                client,
                OPENAI_BASE_URL,
                model,
                system_prompt,
                messages,
                api_key,
                temperature,
            )
            .await
        }
        "deepseek" => {
            openai_compatible::send(
                client,
                DEEPSEEK_BASE_URL,
                model,
                system_prompt,
                messages,
                api_key,
                temperature,
            )
            .await
        }
        "qwen" => {
            openai_compatible::send(
                client,
                QWEN_BASE_URL,
                model,
                system_prompt,
                messages,
                api_key,
                temperature,
            )
            .await
        }
        "anthropic" => {
            anthropic::send(client, model, system_prompt, messages, api_key, temperature).await
        }
        other => Err(ProviderError::UnknownProvider(other.to_string())),
    }
}
