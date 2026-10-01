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
