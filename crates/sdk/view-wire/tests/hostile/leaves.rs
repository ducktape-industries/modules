//! The random nodes a hostile tree is made of: every value drawn to land
//! on or past a bound `sanitize` promises.
use super::*;

/// A hostile f32: sometimes a normal-looking value, sometimes one of the
/// exact values `sanitize` exists to handle (NaN, both infinities, the
/// float extremes, and out-of-range negatives).
pub(super) fn gen_f32(rng: &mut Rng) -> f32 {
    match rng.next_range(20) {
        0 => f32::NAN,
        1 => f32::INFINITY,
        2 => f32::NEG_INFINITY,
        3 => f32::MAX,
        4 => f32::MIN,
        5 => -(rng.skewed(1_000_000, 2) as f32),
        _ => rng.skewed(20_000, 2) as f32 - 5_000.0,
    }
}

/// A node's typed id; from a [`Rng::poisoning_ids`] generator, one in
/// eight is a focus handle, which never crosses the wire.
pub(super) fn gen_id(rng: &mut Rng) -> ElementIdWire {
    if rng.poisons_ids() && rng.next_range(8) == 0 {
        return ElementIdWire::FocusHandle(rng.next_u64());
    }
    ElementIdWire::Name(gen_key(rng).into())
}

/// A key drawn from a small fixed pool: with only five options across a
/// whole tree, collisions are the common case rather than the exception.
pub(super) fn gen_key(rng: &mut Rng) -> String {
    const POOL: [&str; 5] = ["App/a", "App/b", "dup", "x", "same-key"];
    (*rng.choose(&POOL)).to_string()
}

/// A string built from single-, two-, three- and four-byte UTF-8
/// characters. Most calls stay small so a tree of thousands of leaves
/// stays cheap to build; roughly one in three hundred goes hostile and
/// targets up to `3 * MAX_STRING_BYTES`, which is what actually exercises
/// `truncate`'s char-boundary walk.
pub(super) fn gen_string(rng: &mut Rng) -> String {
    const POOL: [char; 6] = ['a', 'Z', 'é', '한', '😀', '\n'];
    let (cap, exponent) = match rng.next_range(300) {
        0 => (3 * MAX_STRING_BYTES, 8),
        _ => (48, 2),
    };
    let target = rng.skewed(cap, exponent);
    let mut s = String::new();
    while s.len() < target {
        s.push(*rng.choose(&POOL));
    }
    s
}

pub(super) fn gen_input(rng: &mut Rng) -> Node {
    Node::Input {
        options: InputOptions {
            label: gen_string(rng),
            description: Some(gen_string(rng)),
            disabled: rng.next_bool(),
        },
        id: gen_id(rng),
        placeholder: gen_string(rng),
        value: gen_string(rng),
        on_input: rng.next_bool().then(|| rng.next_u64() as u32),
        on_submit: rng.next_bool().then(|| rng.next_u64() as u32),
        secure: rng.next_bool(),
        style: gpui::StyleRefinement::default(),
    }
}

/// One logical document per identifier, so every reference the tree makes to
/// the same document is exactly the one `validate_editor_document_refs`
/// requires. Byte lengths stay small: a projection is charged per binding, and
/// the fuzz is about tree shape, not the aggregate byte ceilings the focused
/// `editor_document` tests already pin.
pub(super) fn gen_document(rng: &mut Rng) -> editor_document::EditorDocumentRef {
    const POOL: [&str; 5] = ["app:draft", "app:notes", "dup", "x", "app:same"];
    let index = rng.next_range(POOL.len());
    let byte_len = (index * 37) as u32;
    editor_document::EditorDocumentRef {
        document: POOL[index].to_string(),
        reset: index as u64,
        text_revision: 2 * index as u64,
        revision: 3 * index as u64,
        cursor: EditorCursor {
            position: EditorPosition {
                line: 0,
                column: byte_len,
            },
            selection: None,
        },
        byte_len,
    }
}

pub(super) fn gen_editor(rng: &mut Rng) -> Node {
    Node::Editor {
        document: gen_document(rng),
        on_document: rng.next_u64() as u32,
        editable: rng.next_bool(),
        options: Box::new(EditorOptions {
            rich: None,
            presentation: None,
            binding: None,
        }),
        id: gen_id(rng),
        style: gpui::StyleRefinement::default(),
        placeholder: gen_string(rng),
        label: rng.next_bool().then(|| gen_string(rng)),
    }
}

pub(super) fn gen_text(rng: &mut Rng) -> Node {
    Node::Text(view_wire::TextNode {
        id: None,
        style: if rng.next_range(8) == 0 {
            gen_native_style(rng)
        } else {
            gpui::StyleRefinement::default()
        },
        content: gen_string(rng),
        // 0 and 7 are outside 1..=6, for the sanitizer to drop.
        heading: rng.next_bool().then(|| rng.next_range(8) as u8),
        live: rng
            .next_bool()
            .then(|| *rng.choose(&[Live::Polite, Live::Assertive])),
    })
}

/// A picture whose bytes cross about half the time, and about one time in
/// sixteen run past `MAX_PICTURE_BYTES_PER_FRAME` on their own.
pub(super) fn gen_svg(rng: &mut Rng) -> Node {
    let bytes = rng.next_bool().then(|| {
        let len = match rng.next_range(16) {
            0 => MAX_PICTURE_BYTES_PER_FRAME + 1 + rng.next_range(64),
            _ => rng.skewed(4096, 2),
        };
        vec![b'<'; len]
    });
    if rng.next_bool() {
        let data = bytes.map(|bytes| {
            if rng.next_bool() {
                ImageData::Encoded(bytes)
            } else {
                ImageData::Rgba {
                    width: rng.next_u64() as u32,
                    height: rng.next_u64() as u32,
                    pixels: bytes,
                }
            }
        });
        return Node::Image {
            id: Some(gen_id(rng)),
            hash: rng.next_u64(),
            data,
            label: rng.next_bool().then(|| gen_string(rng)),
            image_style: ImageStyle {
                grayscale: rng.next_bool(),
                object_fit: ImageObjectFit::Contain,
            },
            loading: false,
            fallback: false,
            state_children: Vec::new(),
            style: gen_native_style(rng),
            interactivity: Interactivity::default(),
        };
    }
    Node::Svg {
        id: Some(gen_id(rng)),
        source: SvgSource::Data {
            hash: rng.next_u64(),
            bytes,
        },
        transformation: SvgTransformation {
            scale: [gen_f32(rng), gen_f32(rng)],
            translate: [gen_f32(rng), gen_f32(rng)],
            rotate: gen_f32(rng),
        },
        label: rng.next_bool().then(|| gen_string(rng)),
        style: gen_native_style(rng),
        interactivity: Interactivity {
            hover: Some(gen_native_style(rng)),
            ..Default::default()
        },
    }
}

/// A leaf with no children, for filling out a wide container: every leaf
/// variant except `Space` carries a string, a colour or a number worth
/// pulling into range.
pub(super) fn gen_leaf(rng: &mut Rng) -> Node {
    match rng.next_range(5) {
        0 => gen_text(rng),
        1 => Node::Space {
            style: gen_native_style(rng),
        },
        2 => gen_input(rng),
        3 => gen_svg(rng),
        _ => gen_editor(rng),
    }
}

/// A current wire node holding a child list, around `children`. One in
/// eight carries a hostile base style and every conditional refinement, so
/// the bounds are exercised on the node kind views style most.
pub(super) fn gen_container(rng: &mut Rng, children: Vec<Node>) -> Node {
    let styled = rng.next_range(8) == 0;
    let refinement = |rng: &mut Rng| styled.then(|| gen_native_style(rng));
    let group = |rng: &mut Rng| {
        refinement(rng).map(|style| GroupRefinement {
            group: "row".into(),
            style,
        })
    };
    Node::Container(view_wire::ContainerNode {
        id: None,
        style: refinement(rng).unwrap_or_default(),
        interactivity: Interactivity {
            hover: refinement(rng),
            active: refinement(rng),
            group_hover: group(rng),
            group_active: group(rng),
            ..Default::default()
        },
        children,
    })
}

/// A node holding a child list: an overlay about one time in seven, a
/// container otherwise. Two draws, in this order, so every seed's tree is
/// the one it was.
pub(super) fn gen_list(rng: &mut Rng, children: Vec<Node>) -> Node {
    let overlay = rng.next_range(3) != 0 && rng.next_range(5) == 1;
    if overlay {
        Node::Overlay {
            id: gen_id(rng),
            label: rng.next_bool().then(|| gen_string(rng)),
            on_dismiss: Some(rng.next_u64() as u32),
            children,
            style: gpui::StyleRefinement::default(),
        }
    } else {
        gen_container(rng, children)
    }
}
