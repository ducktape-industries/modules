use super::*;
use std::borrow::Cow;

fn assert_element<T: Element>() {}

#[test]
fn authoring_associated_types_follow_gpui() {
    fn string() -> <String as IntoElement>::Element {
        String::from("string").into_element()
    }
    fn text() -> <&'static str as IntoElement>::Element {
        "text".into_element()
    }
    fn shared() -> <SharedString as IntoElement>::Element {
        SharedString::from("shared").into_element()
    }
    fn borrowed() -> <Cow<'static, str> as IntoElement>::Element {
        Cow::Borrowed("borrowed").into_element()
    }

    assert_element::<SharedString>();
    assert_element::<&'static str>();
    assert_element::<Div>();
    assert_eq!(string().to_string(), "string");
    assert_eq!((*text()).to_owned(), "text");
    assert_eq!(shared().to_string(), "shared");
    assert_eq!(borrowed().to_string(), "borrowed");
}

#[test]
fn a_field_marked_invalid_required_and_read_only_says_so_and_why() {
    let lower = |input: Input| {
        let mut app = App::for_driver();
        let mut window = app.window();
        Lowering::new(&mut window, &mut app).lower(input)
    };
    let email = || {
        Input::new("email", &crate::TextField::default(), "Email")
            .invalid(gpui::accesskit::Invalid::True)
            .required(true)
            .read_only(true)
    };
    let wire::Node::Field { options, .. } = lower(email()) else {
        panic!("an input")
    };
    assert_eq!(options.invalid, Some(gpui::accesskit::Invalid::True));
    assert!(options.required && options.read_only);
    let faults = |node: &wire::Node| {
        wire::audit(node)
            .into_iter()
            .map(|fault| fault.kind)
            .collect::<Vec<_>>()
    };
    assert_eq!(faults(&lower(email())), [wire::FaultKind::ErrorNoText]);
    assert_eq!(
        faults(&lower(email().description("An address has an @"))),
        []
    );
}

/// A builder method takes its element by value and hands it back, and at
/// the size the views are built for that call is not inlined: it moves
/// the element, and the guest pays fuel for every byte moved. So an
/// element is small, and what makes it large (a style refinement, its
/// interactivity) sits behind a pointer. Each element is held to the
/// size it has today, so a field added by value fails here, under the
/// name of the element it grew: put it behind the element's pointer, or
/// raise that element's line knowing what each of its builder calls now
/// moves. The elements a tree is made of (a div is ten moves) are the
/// small ones; an input or a text with runs is a few per view.
#[test]
#[cfg(target_pointer_width = "64")]
fn an_element_a_builder_method_moves_is_small() {
    macro_rules! grown {
        ($($element:ty: $most:literal),+ $(,)?) => {
            [$((stringify!($element), size_of::<$element>(), $most)),+]
                .into_iter()
                .filter(|(_, size, most)| size > most)
                .collect::<Vec<_>>()
        };
    }
    let grown = grown![
        Div: 32,
        crate::Stateful<Div>: 32,
        crate::Img: 80,
        crate::Stateful<crate::Img>: 80,
        crate::Svg: 64,
        crate::Stateful<crate::Svg>: 64,
        UniformList: 56,
        crate::Stateful<UniformList>: 56,
        crate::ResizeHandle: 48,
        crate::Sensor: 80,
        crate::ModalOverlay: 136,
        Input: 296,
        Textarea: 296,
        crate::List: 80,
        crate::StyledText: 104,
        crate::InteractiveText: 216,
        crate::Canvas: 32,
        crate::Anchored: 72,
        crate::Deferred: 24,
        AnyElement: 16,
    ];
    assert!(
        grown.is_empty(),
        "(element, bytes, the most it may be): {grown:?}"
    );
}
