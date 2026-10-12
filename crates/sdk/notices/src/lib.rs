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
/// The most bytes a key carries (chat's longest is 85: a 64-byte channel,
/// `/`, a u64).
pub const MAX_KEY_BYTES: usize = 256;
/// The most bytes a tag carries (chat's longest is a key's).
pub const MAX_TAG_BYTES: usize = 256;
/// The most bytes a link's program carries (a program id is 1..=64).
pub const MAX_LINK_PROGRAM_BYTES: usize = 128;
/// The most segments a link carries (chat's most is 63: a program's room
/// id of 64 bytes, every other byte a `:`).
pub const MAX_LINK_SEGMENTS: usize = 128;
/// The most bytes a link's segments carry in all (chat's most is 84).
pub const MAX_LINK_BYTES: usize = 512;

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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Kind {
    Mention,
    Direct,
    Reply,
}

/// Where opening it goes, without the chain: the app mints
/// `duck://<chain>/<program>/<segments..>` (`ducklink::mint`), as for any
/// other link; a program does not know the chain id.
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
/// A notice over any cap is [`DecodeError::Malformed`], never cut, so every
/// reader drops the same ones (a cut key or tag would miss its retract or
/// its fold).
pub fn decode(bytes: &[u8]) -> Option<Result<NoticeEvent, DecodeError>> {
    let body = bytes.strip_prefix(PREFIX)?;
    let event = match borsh::from_slice(body) {
        Ok(event) => event,
        Err(error) => {
            return Some(Err(
                match error.get_ref().and_then(|inner| inner.downcast_ref()) {
                    Some(Unknown(tag)) => DecodeError::Unknown(*tag),
                    None => DecodeError::Malformed(error.to_string()),
                },
            ));
        }
    };
    Some(match over_cap(&event) {
        Some(field) => Err(DecodeError::Malformed(format!(
            "its {field} is over the cap"
        ))),
        None => Ok(event),
    })
}

/// The first field of `event` over its cap.
fn over_cap(event: &NoticeEvent) -> Option<&'static str> {
    let notice = match event {
        NoticeEvent::Post(notice) => notice,
        NoticeEvent::Retract { key } => return (key.len() > MAX_KEY_BYTES).then_some("key"),
    };
    let link_bytes: usize = notice.link.segments.iter().map(String::len).sum();
    [
        (notice.key.len() > MAX_KEY_BYTES, "key"),
        (notice.to.len() > MAX_RECIPIENTS, "recipient list"),
        (notice.title.len() > MAX_TEXT_BYTES, "title"),
        (notice.body.len() > MAX_TEXT_BYTES, "body"),
        (notice.tag.len() > MAX_TAG_BYTES, "tag"),
        (
            notice.link.program.len() > MAX_LINK_PROGRAM_BYTES,
            "link's program",
        ),
        (
            notice.link.segments.len() > MAX_LINK_SEGMENTS,
            "link's segment count",
        ),
        (link_bytes > MAX_LINK_BYTES, "link's segments"),
    ]
    .into_iter()
    .find_map(|(over, field)| over.then_some(field))
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

    /// Every field at its cap decodes; one byte, segment or recipient past
    /// it is malformed, whichever field it is.
    #[test]
    fn a_notice_over_any_cap_is_malformed() {
        let at_caps = Notice {
            key: "k".repeat(MAX_KEY_BYTES),
            to: vec![2; MAX_RECIPIENTS],
            title: "t".repeat(MAX_TEXT_BYTES),
            body: "b".repeat(MAX_TEXT_BYTES),
            tag: "g".repeat(MAX_TAG_BYTES),
            link: Link {
                program: "p".repeat(MAX_LINK_PROGRAM_BYTES),
                segments: [
                    vec!["s".repeat(MAX_LINK_BYTES)],
                    vec![String::new(); MAX_LINK_SEGMENTS - 1],
                ]
                .concat(),
            },
            ..notice(Kind::Mention)
        };
        let fits = NoticeEvent::Post(at_caps.clone());
        assert_eq!(decode(&encode(&fits)), Some(Ok(fits)));
        let over: [fn(&mut Notice); 8] = [
            |n| n.key.push('k'),
            |n| n.to.push(2),
            |n| n.title.push('t'),
            |n| n.body.push('b'),
            |n| n.tag.push('g'),
            |n| n.link.program.push('p'),
            |n| n.link.segments.push(String::new()),
            |n| n.link.segments[1].push('s'),
        ];
        for grow in over {
            let mut notice = at_caps.clone();
            grow(&mut notice);
            let decoded = decode(&encode(&NoticeEvent::Post(notice)));
            assert!(
                matches!(decoded, Some(Err(DecodeError::Malformed(_)))),
                "{decoded:?}"
            );
        }
        let key = "k".repeat(MAX_KEY_BYTES + 1);
        assert!(matches!(
            decode(&encode(&NoticeEvent::Retract { key })),
            Some(Err(DecodeError::Malformed(_)))
        ));
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
