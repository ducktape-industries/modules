//! A notice as a module emits it: finished, for named accounts, in its own
//! words. The module decides what becomes a notice, who receives it, its
//! title and body, the tag it folds under, where it links and when it is
//! retracted; this crate fixes only the shape and its bytes. A program
//! emits [`encode`] with `ctx.event(..)`; the inbox reads each event of a
//! counted receipt with [`decode`], which tells a notice from any other
//! event by [`PREFIX`]. The source program is the receipt's, which no
//! program can forge.
use std::io::{Error, ErrorKind, Read};

use borsh::{BorshDeserialize, BorshSerialize};

/// What every notice event starts with.
pub const PREFIX: &[u8] = b"ducktape:notice:";
/// The most bytes a title or a body carries: [`cut`] fits one.
pub const MAX_TEXT_BYTES: usize = 512;
/// The most accounts one notice names.
pub const MAX_RECIPIENTS: usize = 64;

/// One event. The enum only grows at its end (`event_variants_only_append`):
/// the inbox decodes old events with the current code, and stops at a tag it
/// does not know ([`DecodeError::Unknown`]).
#[derive(BorshSerialize, Debug, Clone, PartialEq, Eq)]
pub enum NoticeEvent {
    Post(Notice),
    /// Every notice of this program under `key` is withdrawn.
    Retract {
        key: String,
    },
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// Stable per program (chat's `<channel>/<seq>`): a `Retract` names it.
    pub key: String,
    /// The accounts it is for, at most [`MAX_RECIPIENTS`]; never `from`.
    pub to: Vec<u64>,
    /// The account that caused it.
    pub from: Option<u64>,
    pub kind: Kind,
    /// At most [`MAX_TEXT_BYTES`].
    pub title: String,
    /// At most [`MAX_TEXT_BYTES`].
    pub body: String,
    /// What the app folds rows by and a view's seen watermark names.
    pub tag: String,
    pub link: Link,
}

/// Why it is for them. The enum only grows at its end
/// (`kind_variants_only_append`).
#[derive(BorshSerialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Mention,
    Direct,
    Reply,
}

/// Where opening it goes, without the chain: the app mints
/// `duck://<chain>/<program>/<segments..>` (`ducklink::mint`), as for any
/// other link; a program does not know the chain id.
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub program: String,
    pub segments: Vec<String>,
}

/// The event's bytes: [`PREFIX`] then its borsh.
pub fn encode(event: &NoticeEvent) -> Vec<u8> {
    let body = borsh::to_vec(event).expect("borsh of an in-memory value cannot fail");
    [PREFIX, &body].concat()
}

/// An event's bytes read back: `None` for an event that is not a notice.
pub fn decode(bytes: &[u8]) -> Option<Result<NoticeEvent, DecodeError>> {
    let body = bytes.strip_prefix(PREFIX)?;
    Some(borsh::from_slice(body).map_err(|error| {
        match error.get_ref().and_then(|inner| inner.downcast_ref()) {
            Some(Unknown(tag)) => DecodeError::Unknown(*tag),
            None => DecodeError::Malformed(error.to_string()),
        }
    }))
}

/// Why a notice's bytes did not decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// A variant tag this code does not know: a program newer than its reader.
    Unknown(u8),
    /// Bytes that do not decode under the tags they name.
    Malformed(String),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Unknown(tag) => {
                write!(f, "a notice variant this reader does not know: {tag}")
            }
            DecodeError::Malformed(why) => write!(f, "a malformed notice: {why}"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// `text` cut to at most [`MAX_TEXT_BYTES`], on a char boundary.
pub fn cut(text: &str) -> String {
    text[..text.floor_char_boundary(MAX_TEXT_BYTES)].to_owned()
}

/// A variant tag past the known ones, carried inside borsh's error so
/// [`decode`] tells it from malformed bytes.
#[derive(Debug)]
struct Unknown(u8);

impl std::fmt::Display for Unknown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown variant tag {}", self.0)
    }
}

impl std::error::Error for Unknown {}

/// The next variant tag, refused as [`Unknown`] at `count` or past it. The
/// two enums decode by hand for this alone; their tags are their
/// declaration order, as borsh's derive writes them.
fn tag<R: Read>(reader: &mut R, count: u8) -> std::io::Result<u8> {
    let tag = u8::deserialize_reader(reader)?;
    if tag >= count {
        return Err(Error::new(ErrorKind::InvalidData, Unknown(tag)));
    }
    Ok(tag)
}

impl BorshDeserialize for NoticeEvent {
    fn deserialize_reader<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        Ok(match tag(reader, 2)? {
            0 => NoticeEvent::Post(Notice::deserialize_reader(reader)?),
            _ => NoticeEvent::Retract {
                key: String::deserialize_reader(reader)?,
            },
        })
    }
}

impl BorshDeserialize for Kind {
    fn deserialize_reader<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        const KINDS: [Kind; 3] = [Kind::Mention, Kind::Direct, Kind::Reply];
        Ok(KINDS[usize::from(tag(reader, KINDS.len() as u8)?)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(kind: Kind) -> Notice {
        Notice {
            key: "general/7".into(),
            to: vec![2, 3],
            from: Some(1),
            kind,
            title: "Ada mentioned you in #general".into(),
            body: "hi".into(),
            tag: "general".into(),
            link: Link {
                program: "chat".into(),
                segments: vec!["general".into(), "7".into()],
            },
        }
    }

    /// Old events decode with the current code: append a variant, never
    /// reorder.
    #[test]
    fn event_variants_only_append() {
        assert_eq!(describe::variants::<NoticeEvent>(), ["Post", "Retract"]);
    }

    #[test]
    fn kind_variants_only_append() {
        assert_eq!(describe::variants::<Kind>(), ["Mention", "Direct", "Reply"]);
    }

    /// The bytes on chain, written out by hand in field order: a field
    /// added to or moved inside `Notice` or `Link` breaks every reader of
    /// the old ones, so it fails here first.
    #[test]
    fn an_event_is_its_committed_bytes() {
        let text = |text: &str| [&(text.len() as u32).to_le_bytes()[..], text.as_bytes()].concat();
        let post = [
            PREFIX,
            &[0],               // Post
            &text("general/7"), // key
            &[2, 0, 0, 0],      // to: 2 accounts
            &2u64.to_le_bytes(),
            &3u64.to_le_bytes(),
            &[1], // from: Some
            &1u64.to_le_bytes(),
            &[0],                                   // kind: Mention
            &text("Ada mentioned you in #general"), // title
            &text("hi"),                            // body
            &text("general"),                       // tag
            &text("chat"),                          // link.program
            &[2, 0, 0, 0],                          // link.segments
            &text("general"),
            &text("7"),
        ]
        .concat();
        assert_eq!(encode(&NoticeEvent::Post(notice(Kind::Mention))), post);
        let retract = [PREFIX, &[1], &text("general/7")].concat();
        let key = "general/7".into();
        assert_eq!(encode(&NoticeEvent::Retract { key }), retract);
    }

    #[test]
    fn an_event_round_trips_behind_its_prefix() {
        for event in [
            NoticeEvent::Post(notice(Kind::Mention)),
            NoticeEvent::Post(notice(Kind::Direct)),
            NoticeEvent::Post(notice(Kind::Reply)),
            NoticeEvent::Retract {
                key: "general/7".into(),
            },
        ] {
            let bytes = encode(&event);
            assert!(bytes.starts_with(PREFIX));
            assert_eq!(decode(&bytes), Some(Ok(event)));
        }
    }

    /// Another event is not a notice; an unknown tag, at either enum, is
    /// told from bytes that fail under a known one.
    #[test]
    fn decode_tells_others_unknown_and_malformed_apart() {
        assert_eq!(decode(b"chat:something else"), None);
        assert_eq!(
            decode(&[PREFIX, &[2]].concat()),
            Some(Err(DecodeError::Unknown(2)))
        );
        let mut post = encode(&NoticeEvent::Post(notice(Kind::Reply)));
        // the kind's tag follows key, to and from
        let at = PREFIX.len() + 1 + (4 + 9) + (4 + 16) + 9;
        assert_eq!(post[at], 2);
        post[at] = 3;
        assert_eq!(decode(&post), Some(Err(DecodeError::Unknown(3))));
        let mut short = encode(&NoticeEvent::Post(notice(Kind::Reply)));
        short.pop();
        assert!(matches!(
            decode(&short),
            Some(Err(DecodeError::Malformed(_)))
        ));
        let mut long = encode(&NoticeEvent::Retract { key: "k".into() });
        long.push(0);
        assert!(matches!(
            decode(&long),
            Some(Err(DecodeError::Malformed(_)))
        ));
    }

    #[test]
    fn cut_keeps_whole_chars_within_the_cap() {
        assert_eq!(cut("short"), "short");
        let wide = "가".repeat(200); // 3 bytes each
        let fitted = cut(&wide);
        assert_eq!(fitted.len(), 510);
        assert!(wide.starts_with(&fitted));
        assert_eq!(cut(&"a".repeat(600)).len(), MAX_TEXT_BYTES);
    }
}
