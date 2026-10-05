//! The wire, committed: its bytes and its shape. `frame.bin` holds a
//! `Frame` with every `Node` variant in its tree and every style field in
//! its table, then every `Event`, every
//! `WidgetCommand`, and every unit variant those leave out
//! (`golden/units.rs`); `methods.bin` holds at least one request and reply
//! through every method in `methods::ALL`; each has a JSON twin beside it
//! for readable diffs. `tests/golden/schema.txt` (`golden/schema.rs`) holds
//! the shape of every type that crosses. The bytes fail on any sampled byte
//! that moves. The shape holds every field and variant, sampled or not, and
//! `golden/coverage.rs` fails when a sample leaves one out: on the tree side
//! every variant, and every struct field seen present, that `Frame`,
//! `Event` or `WidgetCommand` reaches; on the borsh side every variant,
//! `Option`'s `None` and `Some` included, that a method with no target
//! reaches. A failure means the wire changed; if that was intended,
//! regenerate with `WIRE_GOLDEN_WRITE=1`, which moves `WIRE_ID` (`build.rs`
//! hashes all three files).
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::PathBuf;

use gpui::{Bounds, Pixels, StyleRefinement, point, px, size};
use view_wire::list::{
    ListCommand, UniformListHorizontalSizing, UniformListScrollRequest, UniformListScrollStrategy,
    UniformListSizing,
};
use view_wire::methods::{self, Method, Program};
use view_wire::{
    Action, ActionData, Anchor, AnchoredFitMode, AnchoredPositionMode, Aria, AriaCurrent,
    CanvasCommand, CanvasLineCap, CanvasLineJoin, CanvasSegment, CanvasShape, CanvasStroke,
    ContainerNode, DispatchPhase, Edit, ElementIdAtom, ElementIdWire, Error, Event, Frame,
    GroupRefinement, HasPopup, ImageData, ImageObjectFit, ImageStyle, Interactivity, Invalid,
    KeyClaim, ListAlignment, ListOffset, ListRequest, ListScroll, ListSizingBehavior, Live, Node,
    Patch, Request, RichTextHighlightStyle, RichTextHover, RichTextRuns, Style, StyleId, SvgSource,
    SvgTransformation, TextChange, TextNode, TextRange, TextToken, Tooltip, TooltipResponse,
    WidgetCommand, click, interactivity, keyboard, mouse,
};

const MESSAGE: &str =
    "the wire changed: if intended, regenerate with WIRE_GOLDEN_WRITE=1 (this changes WIRE_ID)";

/// Bumped by hand with the enum: `node_variant` and `event_variant` fail to compile until
/// the fixture names the new one, and this count keeps the fixture honest.
const NODE_VARIANTS: usize = 16;
const EVENT_VARIANTS: usize = 34;

fn golden(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

/// Compares `actual` to the committed fixture, or rewrites it under
/// `WIRE_GOLDEN_WRITE=1`. `json` is the readable twin, never compared.
fn check(name: &str, actual: &[u8], json: &str) {
    let bin = golden(&format!("{name}.bin"));
    if std::env::var_os("WIRE_GOLDEN_WRITE").is_some() {
        std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
        std::fs::write(&bin, actual).unwrap();
        std::fs::write(golden(&format!("{name}.json")), json).unwrap();
        return;
    }
    let expected =
        std::fs::read(&bin).unwrap_or_else(|error| panic!("{}: {error}; {MESSAGE}", bin.display()));
    if expected != actual {
        let first = expected
            .iter()
            .zip(actual)
            .position(|(a, b)| a != b)
            .unwrap_or(expected.len().min(actual.len()));
        panic!(
            "{MESSAGE} ({name}.bin: {} bytes expected, {} built, first difference at byte {first})",
            expected.len(),
            actual.len()
        );
    }
}

fn id(name: &str) -> ElementIdWire {
    ElementIdWire::Name(name.into())
}

/// The style of a sample node whose style is not what it samples: the
/// empty one, the first entry of the frame's table (`nodes::every_style`).
fn style() -> StyleId {
    StyleId(0)
}

fn text(content: &str) -> Node {
    Node::Text(TextNode {
        id: Some(id(content)),
        style: style(),
        content: content.into(),
    })
}

fn boxed(content: &str) -> Box<Node> {
    Box::new(text(content))
}

fn key_state() -> keyboard::KeyState {
    keyboard::KeyState {
        key: keyboard::Key::Named(keyboard::Named::Enter),
        modified_key: keyboard::Key::Character("\n".into()),
        physical_key: keyboard::Physical::Unidentified(keyboard::NativeCode::MacOS(36)),
        location: keyboard::Location::Standard,
        modifiers: gpui::Modifiers {
            shift: true,
            ..Default::default()
        },
    }
}

/// [`key_state`] on a different native platform: every [`keyboard::NativeCode`]
/// variant besides `MacOS` (already [`key_state`]'s) and `Unidentified`
/// (a unit variant, sampled in `golden/units.rs`).
fn key_state_on(physical: keyboard::NativeCode) -> keyboard::KeyState {
    keyboard::KeyState {
        physical_key: keyboard::Physical::Unidentified(physical),
        ..key_state()
    }
}

fn at(x: f32, y: f32) -> gpui::Point<Pixels> {
    point(px(x), px(y))
}

/// The mechanical completeness gate: every field and variant `schema.rs`
/// says the wire reaches, checked against what the samples show.
#[path = "golden/coverage.rs"]
mod coverage;
#[path = "golden/events.rs"]
mod events_fixture;
/// Every `Node` variant at least once, in one tree. Exhaustive by
/// construction: a variant added to `Node` must be added here, or
/// `node_variant` will not build.
#[path = "golden/nodes.rs"]
mod nodes;
/// The wire's shape, hashed into `WIRE_ID` beside the bytes.
#[path = "golden/schema.rs"]
mod schema;
/// Every unit variant the samples in place leave out: the fourth value
/// `frame.bin` pins.
#[path = "golden/units.rs"]
mod units_fixture;
/// Every `WidgetCommand` variant once: the third value `frame.bin` pins,
/// tree-side like `Frame` and `Event` but never nested under either.
#[path = "golden/widget.rs"]
mod widget_fixture;
use events_fixture::{event_variant, every_event, every_frame};
use nodes::{every_node, every_style, node_variant};
use units_fixture::{Units, every_unit};
use widget_fixture::every_widget_command;

#[test]
fn frame_and_events_are_the_committed_bytes() {
    let frame = every_frame();
    let events = every_event();
    let widget_commands = every_widget_command();
    let units = every_unit(&schema::wire());
    let root = frame.root.as_ref().unwrap();
    let Node::Container(ContainerNode { children, .. }) = root else {
        unreachable!()
    };
    let nodes: BTreeSet<_> = children.iter().chain([root]).map(node_variant).collect();
    assert_eq!(nodes.len(), NODE_VARIANTS, "every Node variant: {nodes:?}");
    let kinds: BTreeSet<_> = events.iter().map(event_variant).collect();
    assert_eq!(
        kinds.len(),
        EVENT_VARIANTS,
        "every Event variant: {kinds:?}"
    );

    // every style field survives the table: an entry reads back as the
    // style it was written from, and `coverage.rs` holds the samples to
    // setting every field
    for (entry, style) in frame.styles.iter().zip(every_style()) {
        assert_eq!(entry.read(), Ok(style));
    }

    let value = (frame, events, widget_commands, units);
    let bytes = view_wire::encode(&value);
    let json = serde_json::to_string_pretty(&value).unwrap();
    check("frame", &bytes, &json);
    assert_eq!(
        view_wire::decode::<(Frame, Vec<Event>, Vec<WidgetCommand>, Units)>(&bytes).unwrap(),
        value
    );
}

/// The program a golden `module.query`/`op.submit`/`module.changes` addresses.
struct Golden;
impl Program for Golden {
    const NAME: &'static str = "golden";
    type Op = String;
    type Query = (u64, String);
    type Reply = Vec<u32>;
}

/// The kind, the request bytes and the reply bytes of one exchange.
type Exchange = (String, Vec<u8>, Vec<u8>);

/// One exchange, plus the values as JSON for the readable twin.
fn exchange<D: Method>(request: D::Request, reply: D::Reply) -> (Exchange, serde_json::Value)
where
    D::Request: serde::Serialize,
    D::Reply: serde::Serialize,
{
    let json = serde_json::json!({ "kind": D::KIND, "request": request, "reply": reply });
    (
        (
            D::KIND.into(),
            D::encode_request(&request),
            D::encode_reply(&reply),
        ),
        json,
    )
}

fn every_method() -> Vec<(Exchange, serde_json::Value)> {
    use methods::*;
    vec![
        exchange::<Query<Golden>>((7, "q".into()), vec![1, 2, 3]),
        exchange::<Submit<Golden>>("op".into(), b"receipt".to_vec()),
        exchange::<ChainStatus>(
            (),
            NodeStatus {
                chain_id: "local#1".into(),
                time: 1,
                block_time_ms: 500,
                epoch_length: 100,
                height: 9,
                tip: [1; 32],
                root: [2; 32],
                epoch: 0,
                identity: vec![3; 32],
                contract: 1,
            },
        ),
        exchange::<ChainNetwork>(
            (),
            NetworkStatus {
                height: 9,
                members: vec![
                    Peer {
                        key: vec![3; 32],
                        signed: Some(9),
                    },
                    Peer {
                        key: vec![4; 32],
                        signed: None,
                    },
                ],
            },
        ),
        exchange::<InviteCreate>(
            CreateInvite { ttl_days: 7 },
            Invite {
                invite: "duck://invite/x".into(),
                notes: vec![Error {
                    code: "not_yet".into(),
                    message: "the invite is not indexed yet".into(),
                }],
            },
        ),
        exchange::<methods::Changes<Golden>>(
            (),
            Some(methods::Change {
                height: 9,
                keys: vec![b"a/1".to_vec(), b"b".to_vec()],
            }),
        ),
        exchange::<methods::Changes<Golden>>((), None),
        exchange::<ChainBlocks>(
            BlockPage {
                before: Some(10),
                limit: 2,
            },
            vec![Block {
                height: 9,
                id: [4; 32],
                parent: [5; 32],
                time: 1,
                epoch: 0,
                proposer: Some(vec![6; 32]),
                txs: vec![Tx {
                    hash: [7; 32],
                    signer: vec![8; 32],
                    seq: 1,
                    target: "chat".into(),
                    payload: vec![9],
                    receipt: Some(Receipt {
                        program: "chat".into(),
                        outcome: Outcome::Rejected(Error {
                            code: "unauthorized".into(),
                            message: "not a member".into(),
                        }),
                        events: vec![vec![1]],
                        nested: vec![Receipt {
                            program: "identity".into(),
                            outcome: Outcome::Applied { output: vec![2] },
                            events: Vec::new(),
                            nested: Vec::new(),
                        }],
                    }),
                }],
            }],
        ),
        exchange::<ChainBlock>(BlockRef::Id([4; 32]), None),
        exchange::<ChainBlock>(
            BlockRef::Height(9),
            Some(Block {
                height: 9,
                id: [4; 32],
                parent: [5; 32],
                time: 1,
                epoch: 0,
                proposer: None,
                txs: vec![Tx {
                    hash: [7; 32],
                    signer: vec![8; 32],
                    seq: 2,
                    target: "chat".into(),
                    payload: vec![9],
                    receipt: None,
                }],
            }),
        ),
        exchange::<BlobGet>("sha256:00".into(), Some(b"blob".to_vec())),
        exchange::<HostSession>(
            (),
            Session {
                connected: true,
                chain_id: "local#1".into(),
                signer: "ab01".into(),
                account: Some(3),
                endpoint: "http://127.0.0.1:1".into(),
            },
        ),
        exchange::<HostVisible>((), true),
        exchange::<HostBadge>(3, ()),
        exchange::<LinkOpen>("duck://chat/room".into(), ()),
        exchange::<HostRoute>((), "tx/00ff".into()),
        exchange::<HostId>("msg".into(), "msg-1".into()),
        exchange::<ClockTicks>(1000, ()),
        exchange::<HostLog>("hello".into(), ()),
        exchange::<HostWidget>(
            WidgetCommand::Focus {
                target: vec![id("input")],
            },
            (),
        ),
        exchange::<ClipboardRead>(
            (),
            Clipboard {
                text: "copied".into(),
            },
        ),
        exchange::<ClipboardWrite>("copied".into(), ()),
        exchange::<NotifyPost>(notification(), Delivery::Banner),
        exchange::<NotifyPost>(notification(), Delivery::Logged),
        exchange::<NotifyPost>(notification(), Delivery::Blocked),
        exchange::<StoreGet>("reads/alice".into(), Some(vec![1, 2])),
        exchange::<StoreSet>(("reads/alice".into(), None), ()),
        exchange::<StoreSet>(("reads/alice".into(), Some(vec![1, 2])), ()),
        exchange::<NotifySeen>("#design".into(), ()),
        exchange::<ChainHeads>(
            (),
            Head {
                height: 9,
                time: 1,
                id: [4; 32],
            },
        ),
        exchange::<ModuleDescribe>(
            ("chat".into(), vec![1, 2]),
            Some(Description {
                title: "Post in #design".into(),
                fields: vec![Field {
                    label: "from".into(),
                    value: Value::List(vec![Value::Account(3), Value::bytes(&[7; 40])]),
                }],
            }),
        ),
        exchange::<ModuleDescribe>(
            ("chat".into(), vec![1, 2]),
            Some(Description {
                title: "Post in #design".into(),
                fields: vec![
                    Field {
                        label: "from".into(),
                        value: Value::List(vec![Value::Account(3), Value::bytes(&[7; 40])]),
                    },
                    Field {
                        label: "text".into(),
                        value: Value::text("hi"),
                    },
                    Field {
                        label: "signer".into(),
                        value: Value::Key(vec![1; 32]),
                    },
                    Field {
                        label: "program".into(),
                        value: Value::Module("chat".into()),
                    },
                    Field {
                        label: "commit".into(),
                        value: Value::Hash(vec![2; 32]),
                    },
                    Field {
                        label: "sent".into(),
                        value: Value::Time(1_700_000_000_000),
                    },
                    Field {
                        label: "quantity".into(),
                        value: Value::Amount {
                            value: 1_000,
                            decimals: 2,
                        },
                    },
                ],
            }),
        ),
        exchange::<ModuleDescribe>(("chat".into(), vec![3]), None),
    ]
}

fn notification() -> methods::Notification {
    methods::Notification {
        title: "alice mentioned you".into(),
        body: "@bob hi".into(),
        tag: "room".into(),
        link: "duck://chat/room".into(),
    }
}

/// The request and reply bytes of each exchange, by kind.
type Methods = BTreeMap<String, Vec<(Vec<u8>, Vec<u8>)>>;

#[test]
fn every_method_carries_the_committed_bytes() {
    let (exchanges, json): (Vec<_>, Vec<_>) = every_method().into_iter().unzip();
    let mut built = Methods::new();
    for (kind, request, reply) in exchanges {
        built.entry(kind).or_default().push((request, reply));
    }
    let kinds: BTreeSet<&str> = built.keys().map(String::as_str).collect();
    let all: BTreeSet<&str> = methods::ALL.iter().copied().collect();
    assert_eq!(kinds, all, "an exchange per method in ALL");
    let bin = golden("methods.bin");
    if std::env::var_os("WIRE_GOLDEN_WRITE").is_some() {
        std::fs::write(&bin, methods::encode(&built)).unwrap();
        std::fs::write(
            golden("methods.json"),
            serde_json::to_string_pretty(&json).unwrap(),
        )
        .unwrap();
        return;
    }
    let committed: Methods = methods::decode(
        &std::fs::read(&bin)
            .unwrap_or_else(|error| panic!("{}: {error}; {MESSAGE}", bin.display())),
    )
    .unwrap();
    assert_eq!(moved(&committed, &built), Vec::<String>::new(), "{MESSAGE}");
}

/// The committed kinds whose bytes `built` changed or dropped. A kind only
/// `built` has is new, and passes here: the shape names every kind, so
/// `schema.txt` fails for it instead.
fn moved(committed: &Methods, built: &Methods) -> Vec<String> {
    committed
        .iter()
        .filter(|(kind, bytes)| built.get(*kind) != Some(bytes))
        .map(|(kind, _)| kind.clone())
        .collect()
}

#[test]
fn a_new_method_passes_and_a_moved_or_dropped_one_fails() {
    let method = |kind: &str, byte: u8| (kind.to_string(), vec![(vec![byte], vec![])]);
    let committed: Methods = [method("a.one", 1), method("a.two", 2)].into();
    let grown: Methods = [method("a.one", 1), method("a.two", 2), method("a.new", 3)].into();
    assert!(moved(&committed, &grown).is_empty());
    let changed: Methods = [method("a.one", 9), method("a.two", 2)].into();
    assert_eq!(moved(&committed, &changed), ["a.one"]);
    let dropped: Methods = [method("a.one", 1)].into();
    assert_eq!(moved(&committed, &dropped), ["a.two"]);
}
