//! Shared implementation for every provider that exposes an
//! OpenAI-compatible `/chat/completions` endpoint (OpenAI itself, DeepSeek,
//! Qwen/DashScope). Only the base URL differs between them - the request
//! and response JSON shapes, and the `Authorization: Bearer` auth scheme,
//! are identical.

use super::{ChatMessage, ContentPart, ProviderError, Role};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct RequestMessage<'a> {
    role: &'a str,
    content: RequestContent<'a>,
}

/// A plain string for text-only messages (exactly what was always sent, and
/// the only shape non-vision models are guaranteed to accept); an array of
/// content blocks only when the message carries an image.
#[derive(Serialize)]
#[serde(untagged)]
enum RequestContent<'a> {
    Text(String),
    Parts(Vec<RequestPart<'a>>),
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum RequestPart<'a> {
    Text { text: &'a str },
    ImageUrl { image_url: ImageUrl },
}

#[derive(Serialize)]
struct ImageUrl {
    /// `data:<mime>;base64,<data>`
    url: String,
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

fn request_content(message: &ChatMessage) -> RequestContent<'_> {
    if !message.has_image() {
        return RequestContent::Text(message.joined_text());
    }
    RequestContent::Parts(
        message
            .content
            .iter()
            .map(|part| match part {
                ContentPart::Text(text) => RequestPart::Text { text },
                ContentPart::Image {
                    mime_type,
                    base64_data,
                } => RequestPart::ImageUrl {
                    image_url: ImageUrl {
                        url: format!("data:{mime_type};base64,{base64_data}"),
                    },
                },
            })
            .collect(),
    )
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
            content: RequestContent::Text(system.to_string()),
        });
    }
    for m in messages {
        request_messages.push(RequestMessage {
            role: role_str(m.role),
            content: request_content(m),
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn text_only_message_stays_a_plain_string() {
        let m = ChatMessage::text(Role::User, "hi");
        assert_eq!(serde_json::to_value(request_content(&m)).unwrap(), json!("hi"));
    }

    #[test]
    fn message_with_image_becomes_content_blocks() {
        let m = ChatMessage {
            role: Role::User,
            content: vec![
                ContentPart::Text("look".into()),
                ContentPart::Image {
                    mime_type: "image/png".into(),
                    base64_data: "AAAA".into(),
                },
            ],
        };
        assert_eq!(
            serde_json::to_value(request_content(&m)).unwrap(),
            json!([
                {"type": "text", "text": "look"},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}}
            ])
        );
    }
}
