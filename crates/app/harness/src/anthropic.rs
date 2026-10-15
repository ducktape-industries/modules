//! The Anthropic API as JSON: a thread's request in parts (a header, one part per message,
//! separators, a close) so earlier messages are reused blobs, a reply read back as a turn, an
//! on-demand compaction's summary, and a model's context window from the models endpoint.
use abi::role::egress::http::{Head, Method, Response};
use serde_json::{Map, Value, json};

use crate::{Body, Frame, Role, Said, Stop, ThreadRow, Usage};

pub(crate) const VERSION: &str = "2023-06-01";
pub(crate) const COMPACTION_BETA: &str = "compact-2026-09-04";
pub(crate) const CLOSE: &str = "]}";
pub(crate) const SUMMARIZE: &str = r#"],"compaction":{"type":"summarize"}}"#;

pub(crate) fn head(summarized: bool) -> Head {
    let mut headers = vec![
        ("anthropic-version".into(), VERSION.into()),
        ("content-type".into(), "application/json".into()),
    ];
    if summarized {
        headers.push(("anthropic-beta".into(), COMPACTION_BETA.into()));
    }
    Head {
        method: Method::Post,
        target: "/v1/messages".into(),
        headers,
    }
}

pub(crate) fn model(model: &str) -> Head {
    Head {
        method: Method::Get,
        target: format!("/v1/models/{model}"),
        headers: vec![("anthropic-version".into(), VERSION.into())],
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
        Body::Notice { .. } | Body::Compaction { .. } => Vec::new(),
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

pub(crate) fn summary_message(block: &str) -> String {
    format!(r#"{{"role":"assistant","content":[{block}]}}"#)
}

pub(crate) struct Reply {
    pub model: String,
    pub said: Vec<Said>,
    pub stop: Stop,
    pub usage: Usage,
}

pub(crate) enum Answered {
    Reply(Reply),
    Overflow,
    Failed(String),
}

pub(crate) struct Summary {
    pub block: String,
    pub tokens: u64,
}

pub(crate) enum Summarized {
    Summary(Summary),
    Overflow,
    Failed(String),
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

fn json(body: &[u8]) -> Result<Value, String> {
    serde_json::from_slice(body).map_err(|error| format!("the model's answer is not JSON: {error}"))
}

fn reply(value: &Value) -> Result<Reply, String> {
    let content = value["content"]
        .as_array()
        .ok_or("the model's reply has no content")?;
    let count = |key: &str| value["usage"][key].as_u64().unwrap_or(0);
    Ok(Reply {
        model: text_of(value, "model"),
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

fn overflowed(response: &Response) -> bool {
    let value: Value = serde_json::from_slice(&response.body).unwrap_or(Value::Null);
    let invalid = value["error"]["type"] == "invalid_request_error";
    let too_long = value["error"]["message"]
        .as_str()
        .is_some_and(|message| message.starts_with("prompt is too long"));
    response.status == 400 && invalid && too_long
}

pub(crate) fn answered(response: &Response) -> Answered {
    if response.status != 200 {
        return match overflowed(response) {
            true => Answered::Overflow,
            false => Answered::Failed(failure(response.status, &response.body)),
        };
    }
    let value = match json(&response.body) {
        Ok(value) => value,
        Err(text) => return Answered::Failed(text),
    };
    if value["stop_reason"] == "model_context_window_exceeded" {
        return Answered::Overflow;
    }
    match reply(&value) {
        Ok(reply) => Answered::Reply(reply),
        Err(text) => Answered::Failed(text),
    }
}

pub(crate) fn summarized(response: &Response) -> Summarized {
    if overflowed(response) {
        return Summarized::Overflow;
    }
    match summary(response) {
        Ok(summary) => Summarized::Summary(summary),
        Err(text) => Summarized::Failed(text),
    }
}

fn summary(response: &Response) -> Result<Summary, String> {
    if response.status != 200 {
        return Err(failure(response.status, &response.body));
    }
    let value = json(&response.body)?;
    let stopped = value["stop_reason"].as_str().unwrap_or_default();
    if stopped != "compaction" {
        return Err(format!(
            "the model did not summarize the thread: it stopped with {stopped}"
        ));
    }
    let block = &value["content"][0];
    let summarized = block["type"] == "compaction" && block["content"].is_string();
    let Some(fields) = block.as_object().filter(|_| summarized) else {
        return Err("the model answered no summary".to_owned());
    };
    let present: Map<String, Value> = fields
        .iter()
        .filter(|(_, value)| !value.is_null())
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let tokens = value["usage"]["iterations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|iteration| iteration["type"] == "compaction")
        .filter_map(|iteration| iteration["output_tokens"].as_u64())
        .sum();
    Ok(Summary {
        block: Value::Object(present).to_string(),
        tokens,
    })
}

pub(crate) fn window(response: &Response) -> Result<Option<u64>, String> {
    if response.status != 200 {
        return Err(failure(response.status, &response.body));
    }
    let value = json(&response.body)?;
    let tokens = value["max_input_tokens"].as_u64();
    Ok(tokens.filter(|tokens| *tokens > 0))
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
