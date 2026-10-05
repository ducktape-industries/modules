//! Every kind a view may ask its host for, with the request and reply each
//! carries. This is the ONE list: a view names a method by its type, the host
//! answers by the same type, and a kind that is not here is a compile error
//! on one side and [`refusal::UNKNOWN_REQUEST`] on the other. [`ALL`] is
//! what a host test checks its handlers against, and [`refusal`] lists every
//! code a host refuses one with.
//!
//! THE CODEC RULE. Two layers cross the guest boundary and they want
//! opposite things. The tree a view draws (`Frame`, [`WidgetCommand`]) must
//! tolerate a hundred optional fields and is decoded under a budget — that
//! is MessagePack, in `codec`. Everything a method carries is DATA: the
//! bytes a program signs, stores or answers with, where the same value must
//! be the same bytes and an unknown field is a fault. That is borsh, the
//! codec the program abi is written in, so a program's own request rides a
//! method with no second encoding around it. The rule is held by types, not
//! by review: [`Method`] is sealed, so a view cannot declare a kind or pick a
//! codec, and [`Program`]'s bounds are borsh, so a program that speaks
//! anything else does not have a method.
//!
//! ABSENT is `None`, never a refusal: a method whose thing may not exist replies `Option`, and a refusal means the ask itself failed.
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::WidgetCommand;
pub use describe::{Description, Field, Value};
pub use program::Program;

mod sealed {
    pub trait Sealed {}
}

/// One kind a view may ask for. Sealed: the methods are the ones in this
/// module.
pub trait Method: sealed::Sealed {
    const KIND: &'static str;
    /// The program a node method is addressed to, so a test host can key on
    /// it; `None` for the host's own methods.
    const TARGET: Option<&'static str> = None;
    type Request: std::fmt::Debug;
    type Reply;
    fn encode_request(request: &Self::Request) -> Vec<u8>;
    fn decode_request(bytes: &[u8]) -> Result<Self::Request, String>;
    fn encode_reply(reply: &Self::Reply) -> Vec<u8>;
    fn decode_reply(bytes: &[u8]) -> Result<Self::Reply, String>;
}

pub fn encode<T: BorshSerialize>(value: &T) -> Vec<u8> {
    borsh::to_vec(value).expect("borsh encodes an in-memory value")
}

pub fn decode<T: BorshDeserialize>(bytes: &[u8]) -> Result<T, String> {
    borsh::from_slice(bytes).map_err(|error| error.to_string())
}

macro_rules! method {
    ($(#[$doc:meta])* $name:ident, $kind:literal, $request:ty, $reply:ty) => {
        $(#[$doc])*
        pub struct $name;
        impl sealed::Sealed for $name {}
        impl Method for $name {
            const KIND: &'static str = $kind;
            type Request = $request;
            type Reply = $reply;
            fn encode_request(request: &$request) -> Vec<u8> {
                encode(request)
            }
            fn decode_request(bytes: &[u8]) -> Result<$request, String> {
                decode(bytes)
            }
            fn encode_reply(reply: &$reply) -> Vec<u8> {
                encode(reply)
            }
            fn decode_reply(bytes: &[u8]) -> Result<$reply, String> {
                decode(bytes)
            }
        }
    };
}

/// Every [`method!`] below, and [`ALL`] from the same list, so a method is
/// never declared without being listed. `also` names the kinds written by
/// hand: the three node methods generic over a [`Program`], and [`HostWidget`].
macro_rules! methods {
    (
        also: [$($also:expr),* $(,)?];
        $($(#[$doc:meta])* $name:ident, $kind:literal, $request:ty, $reply:ty;)*
    ) => {
        $(method!($(#[$doc])* $name, $kind, $request, $reply);)*

        /// Every kind, so a host can assert it answers each one.
        pub const ALL: &[&str] = &[$($also,)* $($kind),*];
    };
}

// ---------- the node ----------

// A node method is addressed to a [`Program`]: the program crate's own
// impl, on the type that is its `guest::Module` (`Query<chat::Chat>`), or a
// role's (`program::role::Identity`). `program` is below both SDKs, so a
// program's wasm links no view runtime to be named by a view.

/// The envelope of a node method: the program addressed and the bytes it
/// gets, which the host signs into a frame without reading.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[cfg_attr(feature = "schema", derive(borsh::BorshSchema))]
pub struct Call {
    pub target: String,
    #[serde(with = "crate::codec::bin")]
    pub body: Vec<u8>,
}

fn decode_call<T: BorshDeserialize>(bytes: &[u8], target: &str) -> Result<T, String> {
    let call: Call = decode(bytes)?;
    if call.target != target {
        return Err(format!("expected target {target}, got {}", call.target));
    }
    decode(&call.body)
}

/// `module.query`: one query to `P`, answered with the bytes it `Respond`ed.
/// The node answers from its preconfirmed state: the last block and the ops
/// it has accepted since, ahead of the block that will carry them.
pub struct Query<P>(std::marker::PhantomData<P>);
impl<P: Program> sealed::Sealed for Query<P> {}
impl<P: Program> Method for Query<P> {
    const KIND: &'static str = "module.query";
    const TARGET: Option<&'static str> = Some(P::NAME);
    type Request = P::Query;
    type Reply = P::Reply;
    fn encode_request(request: &P::Query) -> Vec<u8> {
        encode(&Call {
            target: P::NAME.into(),
            body: encode(request),
        })
    }
    fn decode_request(bytes: &[u8]) -> Result<P::Query, String> {
        decode_call(bytes, P::NAME)
    }
    fn encode_reply(reply: &P::Reply) -> Vec<u8> {
        encode(reply)
    }
    fn decode_reply(bytes: &[u8]) -> Result<P::Reply, String> {
        decode(bytes)
    }
}

/// `op.submit`: one operation to `P`, signed with the seated key; the
/// reply is the receipt's output, the program's own bytes. The receipt is
/// the node's preconfirmation, not finality: the op ran over the
/// preconfirmed state and waits for a block. Until a block carries it the
/// node can still drop it (a block that lands first and makes it fail, a
/// node restart), and no view is told; a `Query` reads it as landed
/// meanwhile.
pub struct Submit<P>(std::marker::PhantomData<P>);
impl<P: Program> sealed::Sealed for Submit<P> {}
impl<P: Program> Method for Submit<P> {
    const KIND: &'static str = "op.submit";
    const TARGET: Option<&'static str> = Some(P::NAME);
    type Request = P::Op;
    type Reply = Vec<u8>;
    fn encode_request(request: &P::Op) -> Vec<u8> {
        encode(&Call {
            target: P::NAME.into(),
            body: encode(request),
        })
    }
    fn decode_request(bytes: &[u8]) -> Result<P::Op, String> {
        decode_call(bytes, P::NAME)
    }
    fn encode_reply(reply: &Vec<u8>) -> Vec<u8> {
        reply.clone()
    }
    fn decode_reply(bytes: &[u8]) -> Result<Vec<u8>, String> {
        Ok(bytes.to_vec())
    }
}

/// One block's writes to a program, as `module.changes` carries them: the
/// block's height and every key it wrote under the program, as the node
/// publishes them. A view asks it which of its reads moved
/// ([`Change::touches`]) and re-reads those.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[cfg_attr(feature = "schema", derive(borsh::BorshSchema))]
pub struct Change {
    pub height: u64,
    pub keys: Vec<Vec<u8>>,
}

impl Change {
    /// Whether this block can have changed the answer to `query`: it wrote
    /// a key of a table the query reads, as the program declares them
    /// ([`program::Reads`]). `P` is the program whose block this is, the
    /// one the `Changes<P>` subscription that carried it names: a query
    /// answered partly from another program's tables is touched by that
    /// program's blocks too. A query whose program declares nothing for it
    /// is touched by every block.
    pub fn touches<P: Program, Q: program::Reads + ?Sized>(&self, query: &Q) -> bool {
        query.touched_by(P::NAME, &self.keys)
    }
}

/// `module.changes`: a subscription to `P`, one item per block that wrote to
/// it, carrying the block's height and the keys it wrote ([`Change`]);
/// `None` when the node link was reopened and the view should re-read
/// everything. The request is `P`'s name, as the host reads it.
pub struct Changes<P>(std::marker::PhantomData<P>);
impl<P: Program> sealed::Sealed for Changes<P> {}
impl<P: Program> Method for Changes<P> {
    const KIND: &'static str = "module.changes";
    const TARGET: Option<&'static str> = Some(P::NAME);
    type Request = ();
    type Reply = Option<Change>;
    fn encode_request(_: &()) -> Vec<u8> {
        encode(&P::NAME.to_owned())
    }
    fn decode_request(bytes: &[u8]) -> Result<(), String> {
        let name: String = decode(bytes)?;
        match name == P::NAME {
            true => Ok(()),
            false => Err(format!("expected program {}, got {name}", P::NAME)),
        }
    }
    fn encode_reply(reply: &Option<Change>) -> Vec<u8> {
        encode(reply)
    }
    fn decode_reply(bytes: &[u8]) -> Result<Option<Change>, String> {
        decode(bytes)
    }
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[cfg_attr(feature = "schema", derive(borsh::BorshSchema))]
pub struct NodeStatus {
    pub chain_id: String,
    pub time: u64,
    pub block_time_ms: u64,
    pub epoch_length: u64,
    pub height: u64,
    pub tip: [u8; 32],
    pub root: [u8; 32],
    pub epoch: u64,
    pub identity: Vec<u8>,
    pub contract: u32,
}
/// Every member of the epoch as the connected node sees it: its applied
/// tip, and the members in key order. No member's `signed` exceeds `height`.
#[derive(
    Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[cfg_attr(feature = "schema", derive(borsh::BorshSchema))]
pub struct NetworkStatus {
    pub height: u64,
    pub members: Vec<Peer>,
}
/// One member: the newest block the connected node applied that this key
/// sent a finalize vote for, as the node's consensus engine heard it. A
/// vote for a block not applied yet counts once it is. `None` for a
/// resident, for a validator not heard since the node started or began
/// validating, and for every member while the node is not itself seated as
/// a validator: it runs no engine, so it hears no votes.
#[derive(
    Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[cfg_attr(feature = "schema", derive(borsh::BorshSchema))]
pub struct Peer {
    pub key: Vec<u8>,
    pub signed: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[cfg_attr(feature = "schema", derive(borsh::BorshSchema))]
pub struct CreateInvite {
    pub ttl_days: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[cfg_attr(feature = "schema", derive(borsh::BorshSchema))]
pub struct Invite {
    pub invite: String,
    pub notes: Vec<crate::Error>,
}
/// A page of finalized blocks, newest first: those below `before` (from the
/// tip when `None`), at most `limit` (the node caps a page at 100).
#[derive(
    Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[cfg_attr(feature = "schema", derive(borsh::BorshSchema))]
pub struct BlockPage {
    pub before: Option<u64>,
    pub limit: u32,
}
/// One finalized block, by height or by its id (the block digest).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[cfg_attr(feature = "schema", derive(borsh::BorshSchema))]
pub enum BlockRef {
    Height(u64),
    Id([u8; 32]),
}
/// One applied frame of a block. `hash` is sha256 over the frame's exact
/// bytes; `payload` is the op the target program was handed. `receipt` is
/// its run as the node kept it when it applied the block; `None` where the
/// node keeps none (it never ran the block: one below its state-sync anchor).
#[derive(
    Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[cfg_attr(feature = "schema", derive(borsh::BorshSchema))]
pub struct Tx {
    pub hash: [u8; 32],
    pub signer: Vec<u8>,
    pub seq: u64,
    pub target: String,
    pub payload: Vec<u8>,
    pub receipt: Option<Receipt>,
}
/// One run: the program's, how it ended, what it announced, and the runs
/// its messages caused, in order. A nested run's `Applied` stands only
/// where every run above it applied too: an ancestor's rejection undid it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[cfg_attr(feature = "schema", derive(borsh::BorshSchema))]
pub struct Receipt {
    pub program: String,
    pub outcome: Outcome,
    pub events: Vec<Vec<u8>>,
    pub nested: Vec<Receipt>,
}
/// How a run ended: applied with the program's output, or rejected with
/// its refusal (`code` the refusal's reason token, `message` its sentence).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[cfg_attr(feature = "schema", derive(borsh::BorshSchema))]
pub enum Outcome {
    Applied { output: Vec<u8> },
    Rejected(crate::Error),
}
/// A finalized block as the node's archive keeps it. `proposer` is the
/// validator key that led its round, where the node holds its certificate.
/// No state root or writes: the node keeps neither per height.
#[derive(
    Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[cfg_attr(feature = "schema", derive(borsh::BorshSchema))]
pub struct Block {
    pub height: u64,
    pub id: [u8; 32],
    pub parent: [u8; 32],
    pub time: u64,
    pub epoch: u64,
    pub proposer: Option<Vec<u8>>,
    pub txs: Vec<Tx>,
}
/// One finalized head, as `chain.heads` pushes it.
#[derive(
    Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[cfg_attr(feature = "schema", derive(borsh::BorshSchema))]
pub struct Head {
    pub height: u64,
    pub time: u64,
    pub id: [u8; 32],
}
// ---------- the host ----------

/// The session facts every view is handed: the connection, the
/// network (`<label>#<salt>`), the seated key (hex), the account it belongs
/// to (`None` while the host has not resolved one, or the key has none yet;
/// an item follows when that changes) and the read-only endpoint.
#[derive(
    Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[cfg_attr(feature = "schema", derive(borsh::BorshSchema))]
pub struct Session {
    pub connected: bool,
    pub chain_id: String,
    pub signer: String,
    pub account: Option<u64>,
    pub endpoint: String,
}
/// `host.widget`: a command on the mounted tree. The one method on the TREE
/// side of the codec rule — a [`WidgetCommand`] names typed element ids the
/// tree is drawn with — so it is the one method in the tree's MessagePack.
pub struct HostWidget;
impl sealed::Sealed for HostWidget {}
impl Method for HostWidget {
    const KIND: &'static str = "host.widget";
    type Request = WidgetCommand;
    type Reply = ();
    fn encode_request(request: &WidgetCommand) -> Vec<u8> {
        crate::encode(request)
    }
    fn decode_request(bytes: &[u8]) -> Result<WidgetCommand, String> {
        crate::decode(bytes)
    }
    fn encode_reply(_: &()) -> Vec<u8> {
        Vec::new()
    }
    fn decode_reply(_: &[u8]) -> Result<(), String> {
        Ok(())
    }
}

// ---------- the device ----------

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[cfg_attr(feature = "schema", derive(borsh::BorshSchema))]
pub struct Clipboard {
    pub text: String,
}
/// One notice for the host to decide on: `notify.post`. The view asks; the
/// host logs it in its notification centre and decides whether a banner
/// reaches the screen (the person's per-view choice, focus, a burst limit).
/// `link` is a `duck://` link the centre opens when the notice is picked,
/// or empty.
#[derive(
    Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[cfg_attr(feature = "schema", derive(borsh::BorshSchema))]
pub struct Notification {
    pub title: String,
    pub body: String,
    pub tag: String,
    pub link: String,
}
/// What the host did with a [`Notification`].
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[cfg_attr(feature = "schema", derive(borsh::BorshSchema))]
pub enum Delivery {
    /// Logged, and a banner was raised.
    Banner,
    /// Logged in the centre only: no banner (not yet allowed, silenced,
    /// in front, over the burst limit, or banners are off).
    Logged,
    /// The person blocked this view's notices: dropped, not logged.
    Blocked,
}
methods! {
    also: ["module.query", "op.submit", "module.changes", HostWidget::KIND];
    /// `chain.status`: the connected node's status.
    ChainStatus, "chain.status", (), NodeStatus;
    /// `chain.network`: every member as the connected node sees it.
    ChainNetwork, "chain.network", (), NetworkStatus;
    /// `invite.create`: mint one invite, once (never retried).
    InviteCreate, "invite.create", CreateInvite, Invite;
    /// `chain.blocks`: a page of finalized blocks, newest first.
    ChainBlocks, "chain.blocks", BlockPage, Vec<Block>;
    /// `chain.block`: one finalized block; `None` where the node has none by
    /// that name.
    ChainBlock, "chain.block", BlockRef, Option<Block>;
    /// `blob.get`: a blob by `sha256:<hex>` or `sha1:<hex>` id, unframed;
    /// `None` where the node holds no blob by that id.
    BlobGet, "blob.get", String, Option<Vec<u8>>;
    /// `host.session`: a subscription to [`Session`], an item per change.
    HostSession, "host.session", (), Session;
    /// `host.visible`: whether the view is on screen, an item per change.
    HostVisible, "host.visible", (), bool;
    /// `host.badge`: the count of things unread in the view, shown on its
    /// tab (the app words it "N unread").
    HostBadge, "host.badge", i64, ();
    /// `link.open`: the one way out: a `duck://` link, opened in the
    /// app, or an `https://` one, handed to the system browser. Any other
    /// scheme is refused (`malformed_request`).
    LinkOpen, "link.open", String, ();
    /// `host.route`: a subscription, one item per `duck://` link opened into
    /// this view: the path after the view's own segment (`tx/<hash>` of
    /// `duck://<chain>/explorer/tx/<hash>`). The route is delivered DECODED:
    /// the link spells each segment percent-encoded (`ducklink`), the host
    /// decodes it and joins the segments with `/`, so `forge%3Aweb%3A3/42`
    /// arrives as `forge:web:3/42`. A segment is nonempty, not `.` or `..`,
    /// and holds no `/` and no control character; the whole route is at most
    /// 256 bytes. A link that mounted the view is its first item.
    HostRoute, "host.route", (), String;
    /// `host.id`: a fresh id under the named prefix.
    HostId, "host.id", String, String;
    /// `clock.ticks`: an item per period, in milliseconds.
    ClockTicks, "clock.ticks", i64, ();
    /// `host.log`: one line to the host's log.
    HostLog, "host.log", String, ();
    /// `clipboard.read`: the clipboard's text.
    ClipboardRead, "clipboard.read", (), Clipboard;
    /// `clipboard.write`: text onto the clipboard.
    ClipboardWrite, "clipboard.write", String, ();
    /// `notify.post`: hand the host a notice; it says what it did.
    NotifyPost, "notify.post", Notification, Delivery;
    /// `notify.seen`: the reader has seen what this view posted under a
    /// tag; the host marks this view's rows under it read and takes down
    /// its standing banner. Another view's rows are never touched.
    NotifySeen, "notify.seen", String, ();
    /// `store.get`: the value this view keeps under a key on this device,
    /// for the network in hand; `None` where it keeps none. A view sees
    /// only its own keys, and only on the network it runs on.
    StoreGet, "store.get", String, Option<Vec<u8>>;
    /// `store.set`: keep a value under a key, or drop it with `None`.
    StoreSet, "store.set", (String, Option<Vec<u8>>), ();
    /// `chain.heads`: a subscription, one item per finalized block, oldest
    /// first, from the tip at subscribe on. Heights may skip where the host
    /// could not fill a gap (a reconnect, a node with no archive); a view
    /// that must see every block reads the gap with `chain.blocks`.
    ChainHeads, "chain.heads", (), Head;
    /// `module.describe`: an op as its program says a person reads it,
    /// from the `ducktape.describe` module in the program's current code;
    /// `None` where the code carries none or it cannot read these bytes.
    ModuleDescribe, "module.describe", (String, Vec<u8>), Option<Description>;
}

/// The `<capability>` half of every kind in [`ALL`]: what a view's manifest
/// declares, and the only names it may. A method is reached only through the
/// capability its kind starts with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Capability {
    Chain,
    Module,
    Op,
    Invite,
    Link,
    Blob,
    Host,
    Clock,
    Clipboard,
    Notify,
    Store,
}

impl Capability {
    pub const ALL: [Self; 11] = [
        Self::Chain,
        Self::Module,
        Self::Op,
        Self::Invite,
        Self::Link,
        Self::Blob,
        Self::Host,
        Self::Clock,
        Self::Clipboard,
        Self::Notify,
        Self::Store,
    ];

    /// The name a manifest and a kind spell it with.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chain => "chain",
            Self::Module => "module",
            Self::Op => "op",
            Self::Invite => "invite",
            Self::Link => "link",
            Self::Blob => "blob",
            Self::Host => "host",
            Self::Clock => "clock",
            Self::Clipboard => "clipboard",
            Self::Notify => "notify",
            Self::Store => "store",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|known| known.as_str() == name)
    }

    /// A kind split into its capability and its verb: `chain.status` is
    /// `(Chain, "status")`. `None` for a kind no capability starts.
    pub fn of_kind(kind: &str) -> Option<(Self, &str)> {
        let (capability, verb) = kind.split_once('.')?;
        Some((Self::parse(capability)?, verb))
    }
}

/// Every code a host refuses a method with, beside the program codes in
/// [`crate::code`] that ride through as the node or the program wrote them.
/// A view branches on these; the strings are wire, so one never changes.
pub mod refusal {
    /// The kind is not in [`super::ALL`], or this host does not answer it.
    pub const UNKNOWN_REQUEST: &str = "unknown_request";
    /// The kind's capability is not in the view's manifest.
    pub const UNDECLARED_CAPABILITY: &str = "undeclared_capability";
    /// The node method names a program the view's manifest does not list
    /// among its targets.
    pub const UNDECLARED_TARGET: &str = "undeclared_target";
    /// The op needed the person's confirmation on the host and did not get
    /// it: cancelled, or another confirmation for this view still waits.
    pub const CONSENT_REFUSED: &str = "consent_refused";
    /// The payload does not decode as the method's request, or says nothing
    /// the method can act on.
    pub const MALFORMED_REQUEST: &str = "malformed_request";
    /// The payload, or what it would pull into the view, is over the host's
    /// limit.
    pub const TOO_LARGE: &str = "too_large";
    /// More requests waiting on the host at once than it takes.
    pub const IN_FLIGHT_LIMIT: &str = "in_flight_limit";
    /// More subscriptions of one kind than the host takes.
    pub const SUBSCRIPTION_LIMIT: &str = "subscription_limit";
    /// No node is connected.
    pub const NOT_CONNECTED: &str = "not_connected";
    /// The node connection changed while the request waited.
    pub const STALE_CONNECTION: &str = "stale_connection";
    /// The method signs, and no key is unlocked in this session.
    pub const SESSION_LOCKED: &str = "session_locked";
    /// Nothing reached the node: safe to send again.
    pub const RPC_CLIENT: &str = "rpc_client";
    /// The node failed the request, or its answer went missing: it may have
    /// run.
    pub const NODE_FAILED: &str = "node_failed";
    /// The host itself failed (its disk, its config), not the request.
    pub const HOST_FAULT: &str = "host_fault";
    /// The widget command does not apply to the tree the view shows now.
    pub const INVALID_WIDGET_COMMAND: &str = "invalid_widget_command";
    /// The widget refused the command.
    pub const WIDGET_COMMAND_FAILED: &str = "widget_command_failed";
    /// The command's target left the tree before it ran.
    pub const WIDGET_UNMOUNTED: &str = "widget_unmounted";
    /// The method needs a person's input (a press or a key) in the view, and
    /// there was none, or it was spent on an earlier request.
    pub const NEEDS_GESTURE: &str = "needs_gesture";
    /// More links opened by the view in the last minute than the host takes.
    pub const LINK_LIMIT: &str = "link_limit";
    /// The node does not mint invites.
    pub const INVITE_UNSUPPORTED: &str = "invite_unsupported";
    /// The host closed the request with no answer; written on the view's
    /// side, by the SDK.
    pub const REQUEST_CLOSED: &str = "request_closed";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_are_exactly_the_prefixes_of_every_kind() {
        let mut prefixes: Vec<&str> = ALL
            .iter()
            .map(|kind| Capability::of_kind(kind).unwrap().0.as_str())
            .collect();
        prefixes.sort_unstable();
        prefixes.dedup();
        let mut known = Capability::ALL.map(Capability::as_str).to_vec();
        known.sort_unstable();
        assert_eq!(prefixes, known);
        assert_eq!(Capability::parse("link"), Some(Capability::Link));
        for name in ["rpc", "chat", "module.query", "", "Chain"] {
            assert_eq!(Capability::parse(name), None, "{name:?}");
        }
        assert_eq!(
            Capability::of_kind("chain.status"),
            Some((Capability::Chain, "status"))
        );
        assert_eq!(Capability::of_kind("chain"), None);
    }

    struct Binary;
    impl Program for Binary {
        const NAME: &'static str = "binary";
        type Op = ();
        type Query = (u64, String);
        type Reply = Vec<u32>;
    }

    #[test]
    fn program_methods_address_their_program_and_carry_its_bytes() {
        let request = (42, "query".to_owned());
        let bytes = Query::<Binary>::encode_request(&request);
        let call: Call = decode(&bytes).unwrap();
        assert_eq!(call.target, "binary");
        assert_eq!(call.body, encode(&request));
        assert_eq!(Query::<Binary>::decode_request(&bytes).unwrap(), request);
        let other = encode(&Call {
            target: "other".into(),
            body: encode(&request),
        });
        assert!(Query::<Binary>::decode_request(&other).is_err());
        let reply = vec![1, 2, 3];
        assert_eq!(
            Query::<Binary>::decode_reply(&Query::<Binary>::encode_reply(&reply)).unwrap(),
            reply
        );
    }

    #[test]
    fn a_new_method_goes_last() {
        assert_eq!(ALL.last(), Some(&"module.describe"));
    }

    #[test]
    fn every_kind_is_listed_once() {
        let mut kinds = ALL.to_vec();
        kinds.sort_unstable();
        kinds.dedup();
        assert_eq!(kinds.len(), ALL.len());
        assert!(ALL.iter().all(|kind| kind.split_once('.').is_some()));
    }

    /// The one place the refusal strings are spelled out: the host and the
    /// views name the consts, so a changed string would pass every other
    /// test and break each view built before it.
    #[test]
    fn the_refusal_codes_are_the_wire_strings() {
        use refusal::*;
        let codes = [
            (UNKNOWN_REQUEST, "unknown_request"),
            (UNDECLARED_CAPABILITY, "undeclared_capability"),
            (UNDECLARED_TARGET, "undeclared_target"),
            (CONSENT_REFUSED, "consent_refused"),
            (MALFORMED_REQUEST, "malformed_request"),
            (TOO_LARGE, "too_large"),
            (IN_FLIGHT_LIMIT, "in_flight_limit"),
            (SUBSCRIPTION_LIMIT, "subscription_limit"),
            (NOT_CONNECTED, "not_connected"),
            (STALE_CONNECTION, "stale_connection"),
            (SESSION_LOCKED, "session_locked"),
            (RPC_CLIENT, "rpc_client"),
            (NODE_FAILED, "node_failed"),
            (HOST_FAULT, "host_fault"),
            (INVALID_WIDGET_COMMAND, "invalid_widget_command"),
            (WIDGET_COMMAND_FAILED, "widget_command_failed"),
            (WIDGET_UNMOUNTED, "widget_unmounted"),
            (NEEDS_GESTURE, "needs_gesture"),
            (LINK_LIMIT, "link_limit"),
            (INVITE_UNSUPPORTED, "invite_unsupported"),
            (REQUEST_CLOSED, "request_closed"),
        ];
        for (code, wire) in codes {
            assert_eq!(code, wire);
        }
    }
}
