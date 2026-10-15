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

pub(super) fn gen_string(rng: &mut Rng) -> String {
    const POOL: [char; 6] = ['a', 'Z', 'é', '한', '😀', '\n'];
    let (cap, exponent) = match rng.next_range(300) {
        0 => (3 * LONG_STRING, 8),
        _ => (48, 2),
    };
    let target = rng.skewed(cap, exponent);
    let mut s = String::new();
    while s.len() < target {
        s.push(*rng.choose(&POOL));
    }
    s
}

/// A field whose cursor and tokens sit on its own text: the host engine's
/// copy to adopt, which `sanitize` keeps whole. One in four draws a cursor
/// or a token off the text, or a claim on an engine key, which is refused.
pub(super) fn gen_field(rng: &mut Rng) -> Node {
    let value = gen_string(rng);
    let boundary = |rng: &mut Rng, text: &str| {
        let mut at = rng.next_range(text.len() + 1);
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        at
    };
    let hostile = rng.next_range(4) == 0;
    let cursor = match hostile {
        true => TextRange::caret(value.len() + 1),
        false => {
            let (a, b) = (boundary(rng, &value), boundary(rng, &value));
            TextRange::from(a.min(b)..a.max(b))
        }
    };
    let mut tokens = Vec::new();
    if !hostile && rng.next_bool() && !value.is_empty() {
        let end = boundary(rng, &value).max(value.chars().next().map_or(0, char::len_utf8));
        tokens.push(TextToken {
            range: TextRange::from(0..end),
            id: "<@1>".into(),
        });
    }
    let mut claims = Vec::new();
    if rng.next_bool() {
        claims.push(KeyClaim {
            key: keyboard::Key::Named(match hostile {
                true => keyboard::Named::Backspace,
                false => keyboard::Named::Enter,
            }),
            modifiers: Default::default(),
            command: false,
        });
    }
    Node::Field {
        id: gen_id(rng),
        multiline: rng.next_bool(),
        value,
        cursor,
        generation: rng.next_u64(),
        revision: rng.next_u64(),
        tokens: tokens.into(),
        claims: claims.into(),
        options: Box::new(InputOptions {
            label: gen_string(rng),
            description: Some(gen_string(rng)),
            disabled: rng.next_bool(),
            invalid: rng.next_bool().then_some(Invalid::True),
            required: rng.next_bool(),
            read_only: rng.next_bool(),
            text_input: TextInput {
                autocorrect: rng.next_bool(),
                autocapitalize: *rng.choose(&[
                    Autocapitalize::None,
                    Autocapitalize::Words,
                    Autocapitalize::Sentences,
                    Autocapitalize::Characters,
                ]),
                suggestions: rng.next_bool(),
                input_action: *rng.choose(&[TextInputAction::Unspecified, TextInputAction::Send]),
                virtual_keyboard: *rng.choose(&[
                    VirtualKeyboard::Text,
                    VirtualKeyboard::Url,
                    VirtualKeyboard::Numeric,
                ]),
            },
        }),
        placeholder: gen_string(rng),
        secure: rng.next_bool(),
        on_change: rng.next_bool().then(|| rng.next_u64() as u32),
        on_key: rng.next_bool().then(|| rng.next_u64() as u32),
        on_submit: rng.next_bool().then(|| rng.next_u64() as u32),
        style: PLAIN,
    }
}

pub(super) fn gen_text(rng: &mut Rng) -> Node {
    Node::Text(view_wire::TextNode {
        id: None,
        style: if rng.next_range(8) == 0 {
            gen_style(rng)
        } else {
            PLAIN
        },
        content: gen_string(rng),
    })
}

pub(super) fn gen_svg(rng: &mut Rng) -> Node {
    let bytes = rng.next_bool().then(|| {
        let len = match rng.next_range(16) {
            0 => BIG_PICTURE + 1 + rng.next_range(64),
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
            style: gen_style(rng),
            interactivity: Default::default(),
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
        style: gen_style(rng),
        interactivity: Some(Box::new(Interactivity {
            hover: Some(gen_style(rng)),
            ..Default::default()
        })),
    }
}

/// A leaf with no children, for filling out a wide container: every leaf
/// variant except `Space` carries a string, a colour or a number worth
/// pulling into range.
pub(super) fn gen_leaf(rng: &mut Rng) -> Node {
    match rng.next_range(5) {
        0 => gen_text(rng),
        1 => {
            // `Space` carried a style once: the draws it took still are,
            // so every seed's tree is the one it was
            gen_native_style(rng);
            Node::Space
        }
        2 => gen_svg(rng),
        3 => gen_web(rng),
        _ => gen_field(rng),
    }
}

fn gen_url(rng: &mut Rng) -> String {
    let mut url = String::from("https://");
    let target = rng.next_range(2 * LONG_URL);
    while url.len() < target {
        url.push(*rng.choose(&['a', '/', '한', '%']));
    }
    url
}

pub(super) fn gen_web(rng: &mut Rng) -> Node {
    let url = gen_url(rng);
    let commands = (0..=rng.next_range(WEB_COMMANDS))
        .map(|_| match rng.next_range(3) {
            0 => view_wire::WebCommand::Load(gen_url(rng)),
            1 => view_wire::WebCommand::Back,
            _ => view_wire::WebCommand::Reload,
        })
        .collect();
    Node::Web {
        id: gen_id(rng),
        url,
        style: PLAIN,
        revision: rng.next_u64(),
        commands,
        on_state: rng.next_bool().then(|| rng.next_u64() as u32),
    }
}

/// A current wire node holding a child list, around `children`. One in
/// eight carries a hostile base style and every conditional refinement, so
/// the bounds are exercised on the node kind views style most.
pub(super) fn gen_container(rng: &mut Rng, children: Vec<Node>) -> Node {
    let styled = rng.next_range(8) == 0;
    let refinement = |rng: &mut Rng| styled.then(|| gen_style(rng));
    let group = |rng: &mut Rng| {
        refinement(rng).map(|style| GroupRefinement {
            group: "row".into(),
            style,
        })
    };
    Node::Container(view_wire::ContainerNode {
        id: None,
        style: refinement(rng).unwrap_or(PLAIN),
        interactivity: Some(Box::new(Interactivity {
            hover: refinement(rng),
            active: refinement(rng),
            group_hover: group(rng),
            group_active: group(rng),
            on_back: (rng.next_range(8) == 0).then(|| rng.next_u64() as u32),
            ..Default::default()
        })),
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
            style: PLAIN,
        }
    } else {
        gen_container(rng, children)
    }
}
