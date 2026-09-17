//! Anthropic-specific implementation. Unlike the OpenAI-compatible
//! providers, Anthropic uses a different endpoint shape (`/v1/messages`),
//! a different auth header (`x-api-key` + `anthropic-version`, not
//! `Authorization: Bearer`), a top-level `system` field instead of a
//! "system"-role message, a required `max_tokens`, and a response body
//! whose text lives in `content[0].text` rather than
//! `choices[0].message.content`.

use super::{ChatMessage, ProviderError, Role};
use serde::{Deserialize, Serialize};

const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
// Anthropic requires `max_tokens`; the caller doesn't have a way to
// override this yet (not part of `send_chat_message`'s signature), so a
// reasonable fixed default is used for now.
const DEFAULT_MAX_TOKENS: u32 = 4096;

#[derive(Serialize)]
struct RequestMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: Vec<RequestMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<&'a str>,
}

#[derive(Deserialize)]
struct ChatResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: String,
}

fn role_str(role: Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

pub async fn send(
    client: &reqwest::Client,
    model: &str,
    system_prompt: Option<&str>,
    messages: &[ChatMessage],
    api_key: &str,
    // Newer Claude models have deprecated `temperature` outright (a 400
    // "`temperature` is deprecated for this model" error even to just try
    // sending it), while older ones still accept it. To work uniformly
    // across all Claude models without per-model version-sniffing, we just
    // never send it and let Anthropic use its own default. Kept in the
    // signature so it matches the shared `send_chat_message` contract.
    _temperature: f32,
) -> Result<String, ProviderError> {
    let request_messages: Vec<RequestMessage> = messages
        .iter()
        .map(|m| RequestMessage {
            role: role_str(m.role),
            content: &m.content,
        })
        .collect();

    let body = ChatRequest {
        model,
        max_tokens: DEFAULT_MAX_TOKENS,
        messages: request_messages,
        system: system_prompt,
    };

    let response = client
        .post(ANTHROPIC_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&body)
        .send()
        .await?;

    let status = response.status();
    let raw_body = response.text().await?;

    if !status.is_success() {
        return Err(ProviderError::Http {
            status: status.as_u16(),
            body: raw_body,
        });
    }

    let parsed: ChatResponse =
        serde_json::from_str(&raw_body).map_err(|e| ProviderError::Parse(e.to_string()))?;

    parsed
        .content
        .into_iter()
        .next()
        .map(|block| block.text)
        .ok_or_else(|| ProviderError::Parse("response had no content blocks".to_string()))
}
