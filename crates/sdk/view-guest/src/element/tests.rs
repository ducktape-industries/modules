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
        Input::new("email", "Email")
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
