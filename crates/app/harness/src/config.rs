//! An agent's configuration file: Markdown whose TOML front matter, between `+++` lines, names
//! the model id, `max_tokens` and the blocks egress waits (`wait`), and whose body is the
//! system prompt. Harness parses it at each model call; harness-view writes it.
use std::fmt;

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::Deserialize;

pub const FENCE: &str = "+++";

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct Config {
    pub model: String,
    pub max_tokens: u32,
    pub wait: Option<u64>,
    pub system: String,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum Malformed {
    NotText,
    NoOpeningFence,
    NoClosingFence,
    FrontMatter(String),
    NoTokens,
    NoWait,
}

impl fmt::Display for Malformed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Malformed::NotText => f.write_str("it is not UTF-8 text"),
            Malformed::NoOpeningFence => {
                write!(
                    f,
                    "it does not open with a {FENCE} line before its front matter"
                )
            }
            Malformed::NoClosingFence => {
                write!(f, "its front matter has no closing {FENCE} line")
            }
            Malformed::FrontMatter(reason) => write!(f, "its front matter is not valid: {reason}"),
            Malformed::NoTokens => f.write_str("max_tokens is at least 1"),
            Malformed::NoWait => f.write_str("wait, when given, is at least 1 block"),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FrontMatter {
    model: String,
    max_tokens: u32,
    wait: Option<u64>,
}

fn fence(line: &str) -> bool {
    line.trim_end() == FENCE
}

impl Config {
    pub fn parse(bytes: &[u8]) -> Result<Config, Malformed> {
        let text = std::str::from_utf8(bytes).map_err(|_| Malformed::NotText)?;
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        let opens = lines.first().is_some_and(|line| fence(line));
        if !opens {
            return Err(Malformed::NoOpeningFence);
        }
        let after = &lines[1..];
        let Some(close) = after.iter().position(|line| fence(line)) else {
            return Err(Malformed::NoClosingFence);
        };
        let front = after[..close].concat();
        let matter: FrontMatter = toml::from_str(&front)
            .map_err(|error| Malformed::FrontMatter(error.message().to_owned()))?;
        if matter.max_tokens == 0 {
            return Err(Malformed::NoTokens);
        }
        if matter.wait == Some(0) {
            return Err(Malformed::NoWait);
        }
        Ok(Config {
            model: matter.model,
            max_tokens: matter.max_tokens,
            wait: matter.wait,
            system: after[close + 1..].concat().trim().to_owned(),
        })
    }

    pub fn paragraphs(&self) -> Vec<String> {
        let mut paragraphs = Vec::new();
        let mut lines: Vec<&str> = Vec::new();
        for line in self.system.lines().chain([""]) {
            let blank = line.trim().is_empty();
            match blank {
                false => lines.push(line),
                true if lines.is_empty() => {}
                true => paragraphs.push(std::mem::take(&mut lines).join("\n")),
            }
        }
        paragraphs
    }

    pub fn write(&self) -> String {
        let model = toml::Value::String(self.model.clone());
        let mut front = format!("model = {model}\nmax_tokens = {}\n", self.max_tokens);
        if let Some(wait) = self.wait {
            front.push_str(&format!("wait = {wait}\n"));
        }
        format!("{FENCE}\n{front}{FENCE}\n{}\n", self.system)
    }
}

#[cfg(test)]
mod tests;
