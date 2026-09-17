//! Shared implementation for every provider that exposes an
//! OpenAI-compatible `/chat/completions` endpoint (OpenAI itself, DeepSeek,
//! Qwen/DashScope). Only the base URL differs between them - the request
//! and response JSON shapes, and the `Authorization: Bearer` auth scheme,
//! are identical.

use super::{ChatMessage, ProviderError, Role};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct RequestMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<RequestMessage<'a>>,
    temperature: f32,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: String,
}

fn role_str(role: Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

pub async fn send(
    client: &reqwest::Client,
    base_url: &str,
    model: &str,
    system_prompt: Option<&str>,
    messages: &[ChatMessage],
    api_key: &str,
    temperature: f32,
) -> Result<String, ProviderError> {
    // System prompt becomes a regular "system"-role message up front, since
    // that's how the OpenAI-compatible APIs expect it (unlike Anthropic,
    // which wants it in a separate top-level field).
    let mut request_messages = Vec::with_capacity(messages.len() + 1);
    if let Some(system) = system_prompt {
        request_messages.push(RequestMessage {
            role: "system",
            content: system,
        });
    }
    for m in messages {
        request_messages.push(RequestMessage {
            role: role_str(m.role),
            content: &m.content,
        });
    }

    let body = ChatRequest {
        model,
        messages: request_messages,
        temperature,
    };

    let response = client
        .post(base_url)
        .bearer_auth(api_key)
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
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .ok_or_else(|| ProviderError::Parse("response had no choices".to_string()))
}
