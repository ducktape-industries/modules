//! The wire's shape, committed as `tests/golden/schema.txt` beside the bytes
//! and hashed into `WIRE_ID` with them. The bytes hold what the fixtures
//! sample; the shape holds what the types declare, sampled or not: every
//! field, one `skip_serializing_if` leaves out of the bytes too, every
//! variant of every enum the wire reaches, and every method kind with its
//! target, request and reply. A shape change fails here until regenerated,
//! and regenerating moves `WIRE_ID`.
//!
//! The tree half is serde's, traced from the `Deserialize` side as the host
//! reads it: `Frame`, `Event`, `WidgetCommand` and the `Call` envelope, and
//! every container they reach. The method half is borsh's own schema of
//! each method's request and reply. What neither half holds: the encode
//! functions (`Call` wrapping a node method's body, `op.submit`'s raw reply,
//! `module.changes` sending the program's name) are `methods.bin`'s alone,
//! and a type written as a string (a gpui `Length`, a colour) is `STR`
//! whatever its grammar.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::ops::Range;
use std::sync::Arc;

use borsh::schema::{Declaration, Definition};
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use gpui::{FontFeatures, StyleRefinement};
use serde::de::DeserializeOwned;
use serde_reflection::{
    ContainerFormat, Format, FormatHolder, Named, Registry, Samples, Tracer, TracerConfig,
    VariantFormat,
};
use view_wire::methods::{self, Method, Module};
use view_wire::{Event, Frame, Node, Patch, WidgetCommand};

use super::golden;

/// A tracer whose stand-in string is a colour: gpui writes `Hsla` as hex,
/// and a colour is the one checked string it decodes outside an `Option`,
/// where a refusal would not be cut on the next pass (see [`trace`]).
fn tracer() -> Tracer {
    Tracer::new(
        TracerConfig::default()
            .default_string_value("#000".into())
            .default_borrowed_str_value("#000"),
    )
}

/// Traces `T` from the `Deserialize` side until every format it reaches is
/// known, and answers its own. A decoder that checks what it decodes
/// refuses the tracer's stand-in (an empty name, a zero-length chunk), but
/// what the pass reached before the refusal is kept, and a sequence or
/// option already known is empty on the next pass, so each pass gets
/// further than the last.
fn trace<T: DeserializeOwned>(tracer: &mut Tracer) -> Format {
    let samples = Samples::new();
    let mut refusal = None;
    for _ in 0..1000 {
        match tracer.trace_type::<T>(&samples) {
            Ok((format, _)) => return format,
            Err(error) => refusal = Some(error),
        }
    }
    panic!("{} does not trace: {refusal:?}", std::any::type_name::<T>())
}

type Trace = fn(&mut Tracer) -> Format;

/// Every container [`GPUI`] and `traces` reach, by name, each one whole:
/// every format known, and every enum at its declared variants, which a
/// trace reaches only for an enum traced on its own (a pass revisits a
/// nested enum at its first variant). One partial fails here by name.
fn registry(traces: &[Trace]) -> Registry {
    let mut tracer = tracer();
    for trace in GPUI.iter().chain(traces) {
        trace(&mut tracer);
    }
    let mut registry = tracer.registry_unchecked();
    font_features(&mut registry);
    action_data(&mut registry);
    let partial: Vec<&String> = registry
        .iter_mut()
        .filter_map(|(name, container)| {
            let whole = container.normalize().is_ok()
                && match container {
                    ContainerFormat::Enum(variants) => {
                        variants.keys().copied().eq(0..variants.len() as u32)
                    }
                    _ => true,
                };
            (!whole).then_some(name)
        })
        .collect();
    assert!(
        partial.is_empty(),
        "the wire does not trace whole: trace each of {partial:?} on its own"
    );
    registry
}

/// gpui's `FontFeatures` reads each tag's value through an untagged enum,
/// which no trace can enter (`deserialize_any`), so the trace leaves it
/// unknown: it is what `FontFeatures` writes instead, traced from the
/// `Serialize` side.
fn font_features(registry: &mut Registry) {
    let features = FontFeatures(Arc::new(vec![("calt".into(), 1)]));
    let (written, _) = tracer()
        .trace_value(&mut Samples::new(), &features)
        .unwrap();
    let optional = Format::Option(Box::new(written.clone()));
    for (container, format) in [("TextRun", written), ("TextStyleRefinement", optional)] {
        if let Some(ContainerFormat::Struct(fields)) = registry.get_mut(container) {
            let field = fields
                .iter_mut()
                .find(|field| field.name == "font_features")
                .expect("font_features");
            // Only the map's values are past the trace: a field retyped to
            // anything but a map is a shape of its own, not this one.
            let traced = format!("{:?}", field.value);
            assert!(
                traced.contains("Map {"),
                "{container}.font_features is {traced}"
            );
            field.value = format;
        }
    }
}

/// accesskit's `ActionData` (an `A11yAction`'s data), whole. Its `Point` is
/// accesskit's f64 one, which a trace beside gpui's `Point<Pixels>` would
/// refuse by name: so it is traced alone, and each container it reaches
/// that the tree already names otherwise enters as `accesskit::<name>`.
fn action_data(registry: &mut Registry) {
    if !registry.contains_key("ActionData") {
        return;
    }
    let mut tracer = tracer();
    trace::<accesskit::ScrollUnit>(&mut tracer);
    trace::<accesskit::ScrollHint>(&mut tracer);
    trace::<accesskit::ActionData>(&mut tracer);
    let own = tracer.registry().expect("ActionData traces whole alone");
    let clash: BTreeSet<String> = own
        .iter()
        .filter(|&(name, container)| registry.get(name).is_some_and(|tree| tree != container))
        .map(|(name, _)| name.clone())
        .filter(|name| name != "ActionData")
        .collect();
    for (name, mut container) in own {
        container
            .visit_mut(&mut |format| {
                if let Format::TypeName(name) = format
                    && clash.contains(name)
                {
                    *name = format!("accesskit::{name}");
                }
                Ok(())
            })
            .unwrap();
        let name = match clash.contains(&name) {
            true => format!("accesskit::{name}"),
            false => name,
        };
        registry.insert(name, container);
    }
}

/// gpui's `Background`, written once per tag: its tag is crate-private to
/// gpui, so no trace can name it on its own, and a variant written before
/// the trace reaches it is one the trace takes as whole.
fn backgrounds(tracer: &mut Tracer) -> Format {
    let color = gpui::black();
    let stop = gpui::linear_color_stop(color, 0.);
    let mut samples = Samples::new();
    for background in [
        gpui::solid_background(color),
        gpui::linear_gradient(0., stop, stop),
        gpui::pattern_slash(color, 1., 1.),
        gpui::checkerboard(color, 1.),
    ] {
        tracer.trace_value(&mut samples, &background).unwrap();
    }
    trace::<gpui::Background>(tracer)
}

/// What the tree's trace starts from: the stand-ins that go first (see
/// [`tree`]), what crosses, and every enum of view-wire's and accesskit's
/// under it.
const TREE: &[Trace] = &[
    trace::<Range<usize>>,
    trace::<Grid>,
    trace::<Frame>,
    trace::<Event>,
    trace::<WidgetCommand>,
    trace::<methods::Call>,
    trace::<Node>,
    trace::<Patch>,
    trace::<Result<Vec<u8>, view_wire::Error>>,
    trace::<view_wire::ElementIdWire>,
    trace::<view_wire::Anchor>,
    trace::<view_wire::AnchoredFitMode>,
    trace::<view_wire::AnchoredPositionMode>,
    trace::<view_wire::CanvasCommand>,
    trace::<view_wire::CanvasLineCap>,
    trace::<view_wire::CanvasLineJoin>,
    trace::<view_wire::CanvasSegment>,
    trace::<view_wire::CanvasShape>,
    trace::<view_wire::DispatchPhase>,
    trace::<view_wire::EditorDecision>,
    trace::<view_wire::EditorEditKind>,
    trace::<view_wire::EditorFault>,
    trace::<view_wire::EditorHistoryEffect>,
    trace::<view_wire::EditorRequestInput>,
    trace::<view_wire::EditorTransactionEvent>,
    trace::<view_wire::ElementIdAtom>,
    trace::<view_wire::HoverListenerMode>,
    trace::<view_wire::ImageData>,
    trace::<view_wire::ImageObjectFit>,
    trace::<view_wire::ListAlignment>,
    trace::<view_wire::ListCommand>,
    trace::<view_wire::ListSizingBehavior>,
    trace::<view_wire::RichTextRuns>,
    trace::<view_wire::SvgSource>,
    trace::<view_wire::click::Click>,
    trace::<view_wire::click::KeyboardButton>,
    trace::<view_wire::click::MouseButton>,
    trace::<view_wire::editor_document::EditorDocumentMessage>,
    trace::<view_wire::editor_document::EditorTransfer>,
    trace::<view_wire::editor_document::EditorTransferError>,
    trace::<view_wire::EditorInteraction>,
    trace::<view_wire::interactivity::PressureStage>,
    trace::<view_wire::interactivity::TouchPhase>,
    trace::<view_wire::keyboard::Key>,
    trace::<view_wire::keyboard::Location>,
    trace::<view_wire::keyboard::Named>,
    trace::<view_wire::keyboard::NativeCode>,
    trace::<view_wire::keyboard::Physical>,
    trace::<view_wire::list::UniformListHorizontalSizing>,
    trace::<view_wire::list::UniformListScrollStrategy>,
    trace::<view_wire::list::UniformListSizing>,
    trace::<view_wire::mouse::Cursor>,
    trace::<view_wire::mouse::ScrollDelta>,
    trace::<accesskit::Action>,
    trace::<accesskit::AriaCurrent>,
    trace::<accesskit::HasPopup>,
    trace::<accesskit::Invalid>,
    trace::<accesskit::Live>,
    trace::<accesskit::Orientation>,
    trace::<accesskit::Role>,
    trace::<accesskit::Toggled>,
];

/// gpui's enums under the tree and the style, after its backgrounds.
const GPUI: &[Trace] = &[
    backgrounds,
    trace::<gpui::AlignContent>,
    trace::<gpui::AlignItems>,
    trace::<gpui::BorderStyle>,
    trace::<gpui::ColorSpace>,
    trace::<gpui::CursorStyle>,
    trace::<gpui::Display>,
    trace::<gpui::Fill>,
    trace::<gpui::FlexDirection>,
    trace::<gpui::FlexWrap>,
    trace::<gpui::FontStyle>,
    trace::<gpui::GridPlacement>,
    trace::<gpui::GridTemplateMinSize>,
    trace::<gpui::Overflow>,
    trace::<gpui::Position>,
    trace::<gpui::TextAlign>,
    trace::<gpui::TextOverflow>,
    trace::<gpui::Visibility>,
    trace::<gpui::WhiteSpace>,
];

/// gpui's grid placement, as the tree's trace takes it: see [`tree`].
#[derive(serde::Deserialize)]
#[serde(rename = "GridLocation")]
#[allow(dead_code)]
struct Grid {
    row: Range<usize>,
    column: Range<usize>,
}

/// Every container the tree reaches, and gpui's `GridLocation` apart. A
/// trace names a container by its serde name alone, and the tree's text
/// ranges are `Range<usize>` where a style's grid placement is a
/// `Range<GridPlacement>`: so the tree takes `Range` first, with a stand-in
/// `GridLocation` of it that cuts the style's own on the next pass, and a
/// trace of `StyleRefinement` alone answers for the style's.
pub(super) fn tree() -> (Registry, Registry) {
    let mut tree = registry(TREE);
    let text = ContainerFormat::Struct(vec![
        Named {
            name: "start".into(),
            value: Format::U64,
        },
        Named {
            name: "end".into(),
            value: Format::U64,
        },
    ]);
    assert_eq!(tree["Range"], text, "the tree's Range is the text range");
    tree.remove("GridLocation");
    let mut grid = registry(&[trace::<StyleRefinement>]);
    grid.retain(|name, container| tree.get(name) != Some(container));
    (tree, grid)
}

/// The program a node method addresses, as [`Module`] declares it: each of
/// its types is a marker, so the schema reads `ModuleQuery` where a view's
/// program puts its own.
struct P;
impl Module for P {
    const NAME: &'static str = "P";
    type Op = ModuleOp;
    type Query = ModuleQuery;
    type Reply = ModuleReply;
}
#[derive(Debug, BorshSerialize, BorshDeserialize, BorshSchema)]
struct ModuleOp;
#[derive(Debug, BorshSerialize, BorshDeserialize, BorshSchema)]
struct ModuleQuery;
#[derive(Debug, BorshSerialize, BorshDeserialize, BorshSchema)]
struct ModuleReply;

type Definitions = BTreeMap<Declaration, Definition>;

/// A borsh method: its kind, target, request and reply.
fn borsh<M: Method>(definitions: &mut Definitions) -> (&'static str, String)
where
    M::Request: BorshSchema,
    M::Reply: BorshSchema,
{
    M::Request::add_definitions_recursively(definitions);
    M::Reply::add_definitions_recursively(definitions);
    let line = format!(
        "{} {:?} {} -> {}",
        M::KIND,
        M::TARGET,
        M::Request::declaration(),
        M::Reply::declaration()
    );
    (M::KIND, line)
}

/// A method on the tree's side of the codec rule, its request and reply as
/// serde traces them.
fn tree_method<M: Method>() -> (&'static str, String)
where
    M::Request: DeserializeOwned,
    M::Reply: DeserializeOwned,
{
    let mut tracer = tracer();
    let request = trace::<M::Request>(&mut tracer);
    let reply = trace::<M::Reply>(&mut tracer);
    let line = format!(
        "{} {:?} {request:?} -> {reply:?} named MessagePack",
        M::KIND,
        M::TARGET
    );
    (M::KIND, line)
}

/// The whole text: the methods, the borsh definitions they reach, and the
/// tree's containers.
fn schema() -> String {
    use methods::*;
    let mut definitions = Definitions::new();
    let d = &mut definitions;
    let methods: BTreeMap<&str, String> = [
        borsh::<Query<P>>(d),
        borsh::<Submit<P>>(d),
        borsh::<Changes<P>>(d),
        tree_method::<HostWidget>(),
        borsh::<ChainStatus>(d),
        borsh::<InviteCreate>(d),
        borsh::<ChainBlocks>(d),
        borsh::<ChainBlock>(d),
        borsh::<ChainNetwork>(d),
        borsh::<BlobGet>(d),
        borsh::<HostSession>(d),
        borsh::<HostVisible>(d),
        borsh::<HostBadge>(d),
        borsh::<LinkOpen>(d),
        borsh::<HostRoute>(d),
        borsh::<HostId>(d),
        borsh::<ClockTicks>(d),
        borsh::<HostLog>(d),
        borsh::<ClipboardRead>(d),
        borsh::<ClipboardWrite>(d),
        borsh::<NotifyPost>(d),
        borsh::<NotifySeen>(d),
        borsh::<StoreGet>(d),
        borsh::<StoreSet>(d),
        borsh::<ChainHeads>(d),
        borsh::<ModuleDescribe>(d),
        borsh::<HostOffset>(d),
    ]
    .into();
    let kinds: BTreeSet<&str> = methods.keys().copied().collect();
    let all: BTreeSet<&str> = ALL.iter().copied().collect();
    assert_eq!(kinds, all, "one line per method in ALL");

    let (tree, grid) = tree();
    let mut text = String::from("# methods: kind, target, request -> reply; borsh unless named\n");
    for line in methods.values() {
        writeln!(text, "{line}").unwrap();
    }
    text.push_str("\n# borsh: every definition the methods reach\n");
    for (declaration, definition) in &definitions {
        writeln!(text, "{declaration} = {definition:?}").unwrap();
    }
    text.push_str("\n# tree: named MessagePack, every container by name\n");
    render(&mut text, &tree);
    text.push_str("\n# tree: gpui's GridLocation, whose Range is not the tree's\n");
    render(&mut text, &grid);
    text
}

/// A registry as text: a line per container, and under it one per field or
/// variant (and a variant's fields), in the order they are written.
fn render(text: &mut String, registry: &Registry) {
    let fields = |text: &mut String, fields: &[Named<Format>], indent: &str| {
        for field in fields {
            writeln!(text, "{indent}{}: {:?}", field.name, field.value).unwrap();
        }
    };
    for (name, container) in registry {
        match container {
            ContainerFormat::Struct(named) => {
                writeln!(text, "{name} struct").unwrap();
                fields(text, named, "  ");
            }
            ContainerFormat::Enum(variants) => {
                writeln!(text, "{name} enum").unwrap();
                for (index, variant) in variants {
                    match &variant.value {
                        VariantFormat::Struct(named) => {
                            writeln!(text, "  {index} {} struct", variant.name).unwrap();
                            fields(text, named, "    ");
                        }
                        value => writeln!(text, "  {index} {}: {value:?}", variant.name).unwrap(),
                    }
                }
            }
            other => writeln!(text, "{name} {other:?}").unwrap(),
        }
    }
}

const MESSAGE: &str = "the wire's shape changed: if intended, regenerate with \
     WIRE_GOLDEN_WRITE=1 (this changes WIRE_ID)";

#[test]
fn the_wire_has_the_committed_shape() {
    let path = golden("schema.txt");
    let built = schema();
    if std::env::var_os("WIRE_GOLDEN_WRITE").is_some() {
        std::fs::write(&path, &built).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}; {MESSAGE}", path.display()));
    if let Some((line, (was, is))) = committed
        .lines()
        .zip(built.lines())
        .enumerate()
        .find(|(_, (was, is))| was != is)
    {
        panic!(
            "{MESSAGE} (schema.txt line {}: `{was}` is now `{is}`)",
            line + 1
        );
    }
    assert!(committed == built, "{MESSAGE} (schema.txt grew or shrank)");
}

/// The files `build.rs` hashes into `WIRE_ID`, in its order.
const HASHED: [&str; 3] = ["frame.bin", "methods.bin", "schema.txt"];

/// The gap the schema closes, held generally: every name the wire has — a
/// container, a field, a variant, a method kind — is in what `WIRE_ID`
/// hashes, so none comes or goes without moving it. The bytes alone fail
/// this: `methods.bin` samples one `WidgetCommand` of fourteen, and every
/// fixture style is empty, so no `StyleRefinement` field is in `frame.bin`.
#[test]
fn wire_id_hashes_every_name_on_the_wire() {
    let hashed: Vec<u8> = HASHED
        .iter()
        .flat_map(|name| std::fs::read(golden(name)).unwrap())
        .collect();
    let id = hashed.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    assert_eq!(
        format!("{id:016x}"),
        view_wire::WIRE_ID,
        "build.rs hashes {HASHED:?}"
    );

    let hashed = String::from_utf8_lossy(&hashed);
    let (tree, grid) = tree();
    let mut names: BTreeSet<&str> = methods::ALL.iter().copied().collect();
    for (name, container) in tree.iter().chain(&grid) {
        names.insert(name);
        match container {
            ContainerFormat::Struct(fields) => names.extend(fields.iter().map(|f| f.name.as_str())),
            ContainerFormat::Enum(variants) => {
                for variant in variants.values() {
                    names.insert(&variant.name);
                    if let VariantFormat::Struct(fields) = &variant.value {
                        names.extend(fields.iter().map(|f| f.name.as_str()));
                    }
                }
            }
            _ => {}
        }
    }
    let missing: Vec<_> = names
        .into_iter()
        .filter(|name| !hashed.contains(name))
        .collect();
    assert!(missing.is_empty(), "WIRE_ID does not hash {missing:?}");
}
