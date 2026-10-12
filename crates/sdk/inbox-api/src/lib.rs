//! What the app and a notification inbox say to each other at
//! `<rpc>`[`PATH`]: the [`Login`] message a device key signs, and every JSON
//! request, reply and `live` push. The inbox is a separate service in front
//! of a node; this crate is the one copy of its interface, so the app and
//! the inbox break together.
//!
//! **Notification logic belongs to each module.** What becomes a notice,
//! whom it is for, its words, its tag, its link and when it is retracted
//! are decided in the module's own program, which emits it finished as a
//! `notices` event. The inbox and the app are generic carriers: nothing
//! here, in the inbox or in the app names a module or branches on one.
//!
//! | call | body | answer |
//! |---|---|---|
//! | `GET status` | | [`Status`] |
//! | `POST challenge` | | [`Challenge`] |
//! | `POST login` | [`LoginRequest`] | [`LoginReply`] |
//! | `POST logout` | | 204 |
//! | `GET entries?before=<id>&limit=<n>` | | [`Entries`] |
//! | `POST read` | [`Read`] | [`Changed`] |
//! | `POST hide` | [`Hide`] | [`Changed`] |
//! | `GET`/`PUT prefs` | [`Mutes`] | [`Mutes`] |
//! | `WS live` | | [`Push`], one JSON text message each |
//!
//! Every call but `status`, `challenge` and `login` carries
//! `Authorization: Bearer <token>`, the `live` socket too. Bytes travel as
//! lowercase hex.
use borsh::BorshSerialize;
use notices::{Kind, Link};
use serde::{Deserialize, Serialize};

/// Where an inbox answers, under the RPC address the app connected to. A
/// 404 at `<rpc>/inbox/v1/status` means that address keeps no inbox.
pub const PATH: &str = "/inbox/v1";

/// What a device key signs a [`Login`] under. Collides with none in use
/// (`ducktape:frame`, `ducktape:identity:consent`, `ducktape:invite`).
pub const LOGIN_NAMESPACE: &[u8] = b"ducktape:inbox-login";

/// The login message, signed by the device key (Ed25519) under
/// [`LOGIN_NAMESPACE`]. The inbox rebuilds it from its own origin and chain,
/// so a signature made for another address or chain does not verify.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct Login {
    /// The RPC address the app connected to, without a trailing `/`.
    pub origin: String,
    /// The chain id `<network>#<salt>` (`ducklink::ChainId::of`).
    pub chain: String,
    /// The [`Challenge`]'s.
    pub nonce: [u8; 32],
    /// The [`Challenge`]'s, unix ms.
    pub expires: u64,
}

impl Login {
    /// The bytes the device key signs: the borsh of the four fields, in order.
    pub fn preimage(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("borsh of an in-memory value cannot fail")
    }
}

/// `GET status`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    /// The chain id `<network>#<salt>`.
    pub chain: String,
    /// Every block through it is indexed.
    pub indexed_height: u64,
    /// The node's applied tip as the inbox last read it.
    pub tip: u64,
    /// Set when the inbox stopped following; nothing at or above its
    /// height is indexed until the operator acts.
    pub stopped: Option<Stopped>,
    /// Set while the inbox's own store fails to keep a block (a full disk,
    /// say); it tries again on every tick and clears this once it does.
    pub stalled: Option<Stopped>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stopped {
    pub height: u64,
    pub reason: String,
}

/// `POST challenge`: single use. The inbox keeps nothing when it issues
/// one: the nonce carries its own proof, so the login hands both back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Challenge {
    /// 32 bytes, hex.
    pub nonce: String,
    /// Unix ms; a login after it is refused.
    pub expires: u64,
}

/// `POST login`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginRequest {
    /// The device key's Ed25519 public key, hex.
    pub pubkey: String,
    /// Over [`Login::preimage`] under [`LOGIN_NAMESPACE`], hex.
    pub signature: String,
    /// The [`Challenge`]'s.
    pub nonce: String,
    /// The [`Challenge`]'s.
    pub expires: u64,
    /// The device, as a session list would name it.
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginReply {
    /// 32 bytes, hex: `Authorization: Bearer <token>`. Kept in memory.
    pub token: String,
    /// The account the key acts as.
    pub account: u64,
}

/// One notice for one account.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The inbox's id: newer is larger; `before` pages by it.
    pub id: i64,
    /// The program that emitted it (its receipt's, which no program forges).
    pub program: String,
    /// The program's key for it; a retract names it.
    pub key: String,
    pub kind: Kind,
    /// The account that caused it.
    pub from: Option<u64>,
    pub title: String,
    pub body: String,
    /// What the app folds rows by; `read` by tag marks through a height.
    pub tag: String,
    /// Where opening it goes; the app mints `duck://<chain>/<program>/..`.
    pub link: Link,
    /// The block it landed in.
    pub height: u64,
    /// The tx's hash, hex.
    pub tx: String,
    /// Where in the tx's receipt tree: nested indices, then `#event`.
    pub path: String,
    /// The block's time, unix ms.
    pub at_ms: u64,
    pub read: bool,
}

/// `GET entries`: newest first, muted and hidden entries left out.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entries {
    pub entries: Vec<Entry>,
    /// The account's unread entries, over the same rows.
    pub unread: u64,
}

/// `POST read`: which entries become read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Read {
    Ids {
        ids: Vec<i64>,
    },
    /// A view's seen watermark: the tag's entries through `through_height`.
    Tag {
        program: String,
        tag: String,
        through_height: u64,
    },
    /// `{"all": true}`; `false` is refused.
    All {
        all: bool,
    },
}

/// `POST hide`: `{"read": true}` hides every read entry, on every device
/// (the centre's "Clear read"); `false` is refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hide {
    pub read: bool,
}

/// What a `read` or a `hide` changed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Changed {
    pub ids: Vec<i64>,
}

/// `GET`/`PUT prefs`: what an account keeps out. A muted entry is stored but
/// never pushed, listed or counted. A `PUT` replaces the whole set and
/// applies to every stored entry: muting hides, unmuting shows again.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Mutes {
    pub programs: Vec<String>,
    pub kinds: Vec<Kind>,
    pub tags: Vec<TagMute>,
    /// By `from`.
    pub senders: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TagMute {
    pub program: String,
    pub tag: String,
}

/// One `live` message, to every socket of the account.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Push {
    /// A new entry (never a muted one).
    Entry { entry: Box<Entry> },
    /// Marked read, on any device.
    Read { ids: Vec<i64> },
    /// Retracted by its program, or evicted by the inbox's caps.
    Gone { ids: Vec<i64> },
    /// Hidden, on any device.
    Hidden { ids: Vec<i64> },
}

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;
    use serde_json::{Value, json};

    use super::*;

    /// `value` writes `json` and reads back from it.
    fn golden<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(
        value: T,
        json: Value,
    ) {
        assert_eq!(serde_json::to_value(&value).unwrap(), json);
        assert_eq!(serde_json::from_value::<T>(json).unwrap(), value);
    }

    fn entry() -> Entry {
        Entry {
            id: 12,
            program: "chat".into(),
            key: "general/7".into(),
            kind: Kind::Mention,
            from: Some(1),
            title: "Ada mentioned you in #general".into(),
            body: "hi".into(),
            tag: "general".into(),
            link: Link {
                program: "chat".into(),
                segments: vec!["general".into(), "7".into()],
            },
            height: 40,
            tx: "ab".into(),
            path: "/0#1".into(),
            at_ms: 5,
            read: false,
        }
    }

    fn entry_json() -> Value {
        json!({
            "id": 12,
            "program": "chat",
            "key": "general/7",
            "kind": "Mention",
            "from": 1,
            "title": "Ada mentioned you in #general",
            "body": "hi",
            "tag": "general",
            "link": {"program": "chat", "segments": ["general", "7"]},
            "height": 40,
            "tx": "ab",
            "path": "/0#1",
            "at_ms": 5,
            "read": false,
        })
    }

    /// The bytes a device key signs, written out by hand: each string a
    /// u32 length then its bytes, the nonce raw, `expires` u64 LE.
    #[test]
    fn a_login_signs_its_fields_in_order() {
        let login = Login {
            origin: "o".into(),
            chain: "c#1".into(),
            nonce: [7; 32],
            expires: 5,
        };
        let mut bytes = vec![1, 0, 0, 0, b'o', 3, 0, 0, 0, b'c', b'#', b'1'];
        bytes.extend([7; 32]);
        bytes.extend([5, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(login.preimage(), bytes);
        assert_eq!(PATH, "/inbox/v1");
        assert_eq!(LOGIN_NAMESPACE, b"ducktape:inbox-login");
    }

    #[test]
    fn status_challenge_and_login() {
        golden(
            Status {
                chain: "net#00ff00ff".into(),
                indexed_height: 9,
                tip: 10,
                stopped: None,
                stalled: None,
            },
            json!({
                "chain": "net#00ff00ff",
                "indexed_height": 9,
                "tip": 10,
                "stopped": null,
                "stalled": null,
            }),
        );
        golden(
            Status {
                chain: "net#00ff00ff".into(),
                indexed_height: 9,
                tip: 10,
                stopped: Some(Stopped {
                    height: 10,
                    reason: "a hole".into(),
                }),
                stalled: Some(Stopped {
                    height: 10,
                    reason: "disk full".into(),
                }),
            },
            json!({
                "chain": "net#00ff00ff",
                "indexed_height": 9,
                "tip": 10,
                "stopped": {"height": 10, "reason": "a hole"},
                "stalled": {"height": 10, "reason": "disk full"},
            }),
        );
        golden(
            Challenge {
                nonce: "07".into(),
                expires: 3,
            },
            json!({"nonce": "07", "expires": 3}),
        );
        golden(
            LoginRequest {
                pubkey: "aa".into(),
                signature: "bb".into(),
                nonce: "07".into(),
                expires: 3,
                label: "Desktop".into(),
            },
            json!({
                "pubkey": "aa",
                "signature": "bb",
                "nonce": "07",
                "expires": 3,
                "label": "Desktop",
            }),
        );
        golden(
            LoginReply {
                token: "cc".into(),
                account: 7,
            },
            json!({"token": "cc", "account": 7}),
        );
    }

    #[test]
    fn entries_carry_kind_and_link_as_notices_spell_them() {
        golden(
            Entries {
                entries: vec![entry()],
                unread: 1,
            },
            json!({"entries": [entry_json()], "unread": 1}),
        );
        for (kind, name) in [
            (Kind::Mention, "Mention"),
            (Kind::Direct, "Direct"),
            (Kind::Reply, "Reply"),
        ] {
            golden(kind, json!(name));
        }
        golden(
            Entry {
                from: None,
                ..entry()
            },
            {
                let mut json = entry_json();
                json["from"] = Value::Null;
                json
            },
        );
    }

    #[test]
    fn read_hide_and_changed() {
        golden(Read::Ids { ids: vec![1, 2] }, json!({"ids": [1, 2]}));
        golden(
            Read::Tag {
                program: "chat".into(),
                tag: "general/3".into(),
                through_height: 40,
            },
            json!({"program": "chat", "tag": "general/3", "through_height": 40}),
        );
        golden(Read::All { all: true }, json!({"all": true}));
        golden(Hide { read: true }, json!({"read": true}));
        golden(Changed { ids: vec![4] }, json!({"ids": [4]}));
    }

    #[test]
    fn mutes_name_programs_kinds_tags_and_senders() {
        golden(
            Mutes {
                programs: vec!["forge".into()],
                kinds: vec![Kind::Direct],
                tags: vec![TagMute {
                    program: "chat".into(),
                    tag: "random".into(),
                }],
                senders: vec![9],
            },
            json!({
                "programs": ["forge"],
                "kinds": ["Direct"],
                "tags": [{"program": "chat", "tag": "random"}],
                "senders": [9],
            }),
        );
        // a field left out is empty; one this shape does not know is refused
        assert_eq!(
            serde_json::from_value::<Mutes>(json!({})).unwrap(),
            Mutes::default()
        );
        assert!(serde_json::from_value::<Mutes>(json!({"bogus": 1})).is_err());
        assert!(serde_json::from_value::<Mutes>(json!({"kinds": ["Shout"]})).is_err());
    }

    #[test]
    fn live_pushes_are_tagged_by_type() {
        golden(
            Push::Entry {
                entry: Box::new(entry()),
            },
            json!({"type": "entry", "entry": entry_json()}),
        );
        golden(
            Push::Read { ids: vec![1] },
            json!({"type": "read", "ids": [1]}),
        );
        golden(
            Push::Gone { ids: vec![2] },
            json!({"type": "gone", "ids": [2]}),
        );
        golden(
            Push::Hidden { ids: vec![3] },
            json!({"type": "hidden", "ids": [3]}),
        );
    }
}
