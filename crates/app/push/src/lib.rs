//! The `push` program: carries the notices a program decides to the push service, through the
//! member node the chain elects.
//!
//! What becomes a notice, for whom, its words, its tag, its link and when it is retracted are
//! decided by EACH PROGRAM in its own code. This crate fixes the shape ([`Notice`]); the `push`
//! program, the node, the push server and the apps are generic carriers: no program names, no
//! program semantics, no per-program branches in them.
//!
//! A program sends with `ctx.emit(push::PROGRAM, &push::Op::Send { notices }, Reply::Wanted)`
//! and absorbs the reply (its `Program::reply` answers Ok), so a refusal here, or a network
//! founded without `push`, never fails the sender's op. `push` takes the sender's id from the
//! frame's origin (only a program sends), resolves each recipient account to its Ed25519 keys
//! through the identity role (never the account the frame acts as; an account with no key, and a
//! notice left with none, drop out), puts one JSON body as a blob and awaits the program bound to
//! the egress role: `POST /v1/send` to the registered service [`SERVICE`]. The body:
//!
//! ```json
//! {"program": "<sender>", "notices": [{"keys": ["<hex>"], "title": "", "body": "",
//!   "tag": "", "link": ["<segment>"]}]}
//! ```
//!
//! A `link` is path segments under the sending program: the receiver mints
//! `duck://<chain>/<program>/<segments...>`, since a program does not know its chain's id. `push`
//! keeps no state: an error answer (the request expired, no node serves push, the service failed
//! or answered other than 2xx) ends its frame with that error, once, with no retry.
//!
//! The caps are refused here, at the boundary; [`Notice::new`] is the sender's helper that clips
//! a title and a body to [`MAX_TEXT`] on a char boundary, so a sender built on it is never
//! refused for length.
mod program;
mod rules;
#[cfg(test)]
mod tests;

pub use abi::role::identity::AccountNumber;
pub use program::Push;

use borsh::{BorshDeserialize, BorshSerialize};

pub const PROGRAM: &str = "push";
/// The egress service a node posts notices to (`egress.toml` `[services.push]`).
pub const SERVICE: &str = "push";
/// Where on the service: the path `push` posts the body to.
pub const TARGET: &str = "/v1/send";
/// The kind of the blob the body is put as.
pub const KIND: &str = "push";
/// The most bytes a notice's title, and its body, may hold.
pub const MAX_TEXT: usize = 512;
/// The most notices one `Send` carries.
pub const MAX_NOTICES: usize = 64;
/// The most accounts one notice is for.
pub const MAX_RECIPIENTS: usize = 64;

/// One notice, as the sending program words it: who it is for, its title and body, the tag it
/// folds under on a device, and where it opens (path segments under the sending program).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Notice {
    pub to: Vec<AccountNumber>,
    pub title: String,
    pub body: String,
    pub tag: String,
    pub link: Vec<String>,
}

impl Notice {
    /// A notice whose title and body are [`clip`]ped to [`MAX_TEXT`].
    pub fn new(
        to: Vec<AccountNumber>,
        title: &str,
        body: &str,
        tag: String,
        link: Vec<String>,
    ) -> Notice {
        Notice {
            to,
            title: clip(title).to_owned(),
            body: clip(body).to_owned(),
            tag,
            link,
        }
    }
}

/// `text` cut to at most [`MAX_TEXT`] bytes, on a char boundary.
pub fn clip(text: &str) -> &str {
    &text[..text.floor_char_boundary(MAX_TEXT)]
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Op {
    Send { notices: Vec<Notice> },
}

pub fn describe(op: &Op) -> describe::Description {
    use describe::{Value, field};
    let Op::Send { notices } = op;
    let accounts: usize = notices.iter().map(|notice| notice.to.len()).sum();
    describe::Description {
        title: "Send notices".to_owned(),
        fields: vec![
            field("notices", Value::count(notices.len() as u64)),
            field("accounts", Value::count(accounts as u64)),
        ],
    }
}

describe::export!(Op, describe);

/// Old op bytes are described with the current code (`describe`): the op
/// enum only grows at its end. Append a new variant here; never reorder.
#[test]
fn op_variants_only_append() {
    assert_eq!(describe::variants::<Op>(), ["Send"]);
}
