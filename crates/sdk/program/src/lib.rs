//! What a program is to everything that addresses it: its name on the node
//! and the three borsh types it speaks. A program crate implements
//! [`Program`] once, on the same type that implements `guest::Module`; a view
//! names the program by that type (`Query<chat::Chat>`, `Submit<forge::Forge>`,
//! `Changes<identity::Identity>`), and the method encodes `NAME` as the
//! target. The crate is below both SDKs so neither side links the other.
use std::fmt::Debug;

use borsh::{BorshDeserialize, BorshSerialize};

/// A program: its name on the node and the types it speaks. A read-only
/// program names `()` as its `Op`. The bounds are borsh on both sides: a
/// program's own request rides a method with no second encoding around it.
pub trait Program {
    /// The id the program runs under: the target a view's method names.
    const NAME: &'static str;
    type Op: BorshSerialize + BorshDeserialize + Debug;
    type Query: BorshSerialize + BorshDeserialize + Debug;
    type Reply: BorshSerialize + BorshDeserialize;
}

/// One question to a program, as its own type: the program it asks, the
/// reply that answers it, and how it rides the wire (`into` the program's
/// `Query`, so its bytes are the enum's). A caller asks it and gets that
/// reply, typed, with no arm for the others.
///
/// `#[derive(Ask)]` on a program's `Query` writes them: `#[ask(Program)]`
/// on the enum, and on each variant a caller asks alone the reply that
/// answers it, `#[ask(Reply::List(PageResponse<Row>))]` (or a reply's
/// named fields, `#[ask(Reply::Thread { root: Option<Row>, replies:
/// PageResponse<Row> })]`, answered as their tuple). Each such variant
/// becomes a type of the same name and fields in a `pub mod ask` beside
/// the enum (`identity::ask::List { page }`). A field type that shares a
/// variant's name is written by its full path (`crate::Profile`).
pub trait Ask: Into<<Self::Program as Program>::Query> {
    type Program: Program;
    type Reply;
    /// The answer, out of the program's reply; `None` when it answers
    /// another question.
    fn answer(reply: <Self::Program as Program>::Reply) -> Option<Self::Reply>;
}

/// Which tables a query reads, so a follower re-reads it only when a block
/// wrote to one of them: a view asks a block's change whether it touches
/// the query (`change.touches::<P, _>(&query)`, `P` the program whose
/// block it is) and never maps keys to queries itself.
///
/// `#[derive(Ask)]` writes it from `#[reads(TABLE, ..)]` on each variant:
/// the `Map`/`Set`/`Item` consts the program answers that question from
/// (`#[reads(MESSAGES, REACTIONS)]`), each asked `owns(&key)`. A question
/// answered partly from another program's tables names them in a second
/// attribute, that program first (`#[reads(chat::Chat:
/// chat::tables::ANSWERED, chat::tables::MESSAGES)]`), so a follower of
/// that program's blocks re-reads the question too. The program states it,
/// since only the program knows what it reads. A variant with no `#[reads]`
/// is touched by every block of every program a follower asks about, which
/// is what a follower did before, so a program adopts this one query at a
/// time. A variant with ask types ([`Ask`]) gives them the same answer.
pub trait Reads {
    /// Whether a block of `program` (its [`Program::NAME`]) that wrote
    /// `keys` can have changed this query's answer.
    fn touched_by(&self, program: &str, keys: &[Vec<u8>]) -> bool;
}

pub use program_derive::Ask;

/// Whether `name` is a program id as the network spells one: `1..=64`
/// bytes of lowercase ASCII letters, digits, `-` and `_`. One spelling per
/// id, so no id reads as another (no capital, no look-alike letter, no
/// bidi control). The one rule for a manifest's `targets`, a host's
/// `Call.target` and an id the registry lands.
pub const fn is_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    let mut i = 0;
    while i < bytes.len() {
        if !matches!(bytes[i], b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_') {
            return false;
        }
        i += 1;
    }
    true
}

/// A role as a view follows it, without linking the program that fills it.
/// A view's targets are fixed in its manifest, so it names the program
/// networks bind to the role; a module asks the binding (`Env.roles`)
/// instead. The one place a role's program id is spelled: the program crate
/// takes its `MODULE` from here.
pub mod role {
    use super::Program;

    /// The identity role (`abi::role::identity`), as the `identity`
    /// program fills it. A view sends it nothing.
    pub struct Identity;
    impl Program for Identity {
        const NAME: &'static str = "identity";
        type Op = ();
        type Query = abi::role::identity::Query;
        type Reply = abi::role::identity::Reply;
    }
}

#[test]
fn a_program_id_has_one_spelling() {
    assert!(is_name("module-registry") && is_name("a-b_c9"));
    for name in ["", "a b", "System", "\u{0441}hat", "chat\u{202e}"] {
        assert!(!is_name(name), "{name:?}");
    }
    assert!(is_name(&"x".repeat(64)) && !is_name(&"x".repeat(65)));
}
