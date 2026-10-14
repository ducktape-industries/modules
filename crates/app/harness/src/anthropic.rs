//! The Anthropic Messages API as JSON: a thread's request in parts (a header, one part per
//! message, a close) so earlier messages are reused blobs, and a reply read back as a turn.
use provider::http::{Head, Method};
use serde_json::{Map, Value, json};

use crate::{Body, Frame, Role, Said, Stop, ThreadRow, Usage};

pub(crate) const VERSION: &str = "2023-06-01";
pub(crate) const CLOSE: &str = "]}";

pub(crate) fn head() -> Head {
    Head {
        method: Method::Post,
        path: "/v1/messages".into(),
        headers: vec![
            ("anthropic-version".into(), VERSION.into()),
            ("content-type".into(), "application/json".into()),
        ],
    }
}

fn cached() -> Value {
    json!({ "type": "ephemeral" })
}

pub(crate) fn header(thread: &ThreadRow, mut tools: Vec<Value>) -> String {
    if let Some(Value::Object(last)) = tools.last_mut() {
        last.insert("cache_control".into(), cached());
    }
    let mut request = Map::new();
    request.insert("model".into(), json!(thread.model));
    request.insert("max_tokens".into(), json!(thread.max_tokens));
    request.insert("tools".into(), Value::Array(tools));
    if !thread.system.is_empty() {
        request.insert(
            "system".into(),
            json!([{ "type": "text", "text": thread.system, "cache_control": cached() }]),
        );
    }
    let mut text = Value::Object(request).to_string();
    text.pop();
    text.push_str(",\"messages\":[");
    text
}

fn blocks(body: &Body) -> Vec<Value> {
    match body {
        Body::Post { text, .. } => vec![json!({ "type": "text", "text": text })],
        Body::Turn { said, .. } => said.iter().filter_map(said_block).collect(),
        Body::Refused { call, problem, .. } => vec![json!({
            "type": "tool_result",
            "tool_use_id": call,
            "is_error": true,
            "content": problem,
        })],
        Body::Result {
            call, text, failed, ..
        } => vec![json!({
            "type": "tool_result",
            "tool_use_id": call,
            "is_error": failed,
            "content": text,
        })],
        Body::Notice { .. } => Vec::new(),
    }
}

fn said_block(said: &Said) -> Option<Value> {
    match said {
        Said::Text(text) if text.is_empty() => None,
        Said::Text(text) => Some(json!({ "type": "text", "text": text })),
        Said::Thinking { text, signature } => Some(json!({
            "type": "thinking",
            "thinking": text,
            "signature": signature,
        })),
        Said::RedactedThinking(data) => Some(json!({ "type": "redacted_thinking", "data": data })),
        Said::ToolUse { id, name, input } => {
            let input: Value = serde_json::from_str(input).unwrap_or_else(|_| json!({}));
            Some(json!({ "type": "tool_use", "id": id, "name": name, "input": input }))
        }
    }
}

pub(crate) fn message(role: Role, frames: &[Frame], first: bool, last: bool) -> String {
    let mut content: Vec<Value> = frames
        .iter()
        .flat_map(|frame| blocks(&frame.body))
        .collect();
    if content.is_empty() {
        content.push(json!({ "type": "text", "text": "(nothing)" }));
    }
    if let (true, Some(Value::Object(block))) = (last, content.last_mut()) {
        block.insert("cache_control".into(), cached());
    }
    let role = match role {
        Role::User => "user",
        Role::Assistant => "assistant",
    };
    let text = json!({ "role": role, "content": content }).to_string();
    match first {
        true => text,
        false => format!(",{text}"),
    }
}

pub(crate) struct Reply {
    pub model: String,
    pub said: Vec<Said>,
    pub stop: Stop,
    pub usage: Usage,
}

fn stop(reason: Option<&str>) -> Stop {
    match reason {
        Some("tool_use") => Stop::ToolUse,
        Some("max_tokens") => Stop::MaxTokens,
        Some("stop_sequence") => Stop::StopSequence,
        Some("pause_turn") => Stop::Pause,
        Some("refusal") => Stop::Refusal,
        _ => Stop::EndTurn,
    }
}

fn text_of(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_owned()
}

fn read_said(block: &Value) -> Option<Said> {
    match block["type"].as_str()? {
        "text" => Some(Said::Text(text_of(block, "text"))),
        "thinking" => Some(Said::Thinking {
            text: text_of(block, "thinking"),
            signature: text_of(block, "signature"),
        }),
        "redacted_thinking" => Some(Said::RedactedThinking(text_of(block, "data"))),
        "tool_use" => Some(Said::ToolUse {
            id: text_of(block, "id"),
            name: text_of(block, "name"),
            input: block["input"].to_string(),
        }),
        _ => None,
    }
}

pub(crate) fn reply(body: &[u8]) -> Result<Reply, String> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| format!("the model's reply is not JSON: {error}"))?;
    let content = value["content"]
        .as_array()
        .ok_or("the model's reply has no content")?;
    let count = |key: &str| value["usage"][key].as_u64().unwrap_or(0);
    Ok(Reply {
        model: text_of(&value, "model"),
        said: content.iter().filter_map(read_said).collect(),
        stop: stop(value["stop_reason"].as_str()),
        usage: Usage {
            input: count("input_tokens"),
            output: count("output_tokens"),
            cache_read: count("cache_read_input_tokens"),
            cache_write: count("cache_creation_input_tokens"),
        },
    })
}

pub(crate) fn failure(status: u16, body: &[u8]) -> String {
    let value: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let kind = value["error"]["type"].as_str();
    let message = value["error"]["message"].as_str();
    match (kind, message) {
        (Some(kind), Some(message)) => format!("the model answered {status}, {kind}: {message}"),
        _ => format!("the model answered {status}"),
    }
}
