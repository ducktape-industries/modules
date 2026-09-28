//! One tree that breaks each rule, and one that keeps it and every other.
use super::*;
use crate::{ElementIdWire, InputOptions, Invalid, Live};
use FaultKind::*;
use gpui::StyleRefinement;

fn text(content: &str) -> Node {
    Node::Text(TextNode {
        id: None,
        style: StyleRefinement::default(),
        content: content.into(),
    })
}

fn el(key: &str, interactivity: Interactivity, children: Vec<Node>) -> Node {
    Node::Container(ContainerNode {
        id: Some(ElementIdWire::Name(key.into())),
        style: StyleRefinement::default(),
        interactivity,
        children,
    })
}

fn roled(role: Role) -> Interactivity {
    Interactivity {
        role: Some(role),
        ..Default::default()
    }
}

/// Roled, focusable and answering a click.
fn control(role: Role) -> Interactivity {
    Interactivity {
        focusable: true,
        on_click: Some(1),
        ..roled(role)
    }
}

fn labelled(interactivity: Interactivity, label: &str) -> Interactivity {
    Interactivity {
        aria: Aria {
            label: Some(label.into()),
            ..interactivity.aria
        },
        ..interactivity
    }
}

fn button(key: &str, label: &str) -> Node {
    el(key, control(Role::Button), vec![text(label)])
}

fn input(label: &str) -> Node {
    Node::Input {
        options: InputOptions {
            label: label.into(),
            ..Default::default()
        },
        id: ElementIdWire::Name("name".into()),
        placeholder: String::new(),
        value: String::new(),
        on_input: Some(1),
        on_submit: None,
        secure: false,
        style: StyleRefinement::default(),
    }
}

/// A paragraph with one clickable byte range.
fn rich(text: &str, clickable: std::ops::Range<usize>) -> Node {
    Node::RichText {
        id: None,
        style: StyleRefinement::default(),
        text: text.into(),
        runs: Default::default(),
        font_family_overrides: Vec::new(),
        clickable_ranges: vec![clickable],
        on_click: Some(1),
        on_hover: None,
        tooltip: None,
    }
}

fn handle(interactivity: Interactivity) -> Node {
    Node::ResizeHandle {
        id: ElementIdWire::Name("divider".into()),
        style: StyleRefinement::default(),
        interactivity,
        on_press: None,
        on_release: None,
        on_drag: Some(1),
        cursor: None,
        content: Box::new(Node::empty()),
    }
}

fn image(interactivity: Interactivity, label: Option<&str>) -> Node {
    Node::Image {
        id: None,
        hash: 1,
        data: None,
        label: label.map(Into::into),
        image_style: crate::ImageStyle {
            grayscale: false,
            object_fit: crate::ImageObjectFit::Contain,
        },
        loading: false,
        fallback: false,
        state_children: Vec::new(),
        style: StyleRefinement::default(),
        interactivity,
    }
}

fn kinds(tree: &Node) -> Vec<FaultKind> {
    audit(tree).iter().map(|fault| fault.kind).collect()
}

fn fails(kind: FaultKind, tree: Node) {
    let kinds = kinds(&tree);
    assert!(kinds.contains(&kind), "{kind:?} not among {kinds:?}");
}

/// Nothing in the tree breaks any rule.
fn passes(tree: Node) {
    assert_eq!(audit(&tree), Vec::new());
}

#[test]
fn named_trees_pass_and_unlabeled_clickables_are_reported() {
    let faulty = el(
        "App",
        Interactivity::default(),
        vec![
            el(
                "App/open",
                Interactivity {
                    on_click: Some(1),
                    ..Default::default()
                },
                vec![text("Open")],
            ),
            text("a"),
            Node::Overlay {
                id: ElementIdWire::Name("ask".into()),
                label: Some(String::new()),
                style: Default::default(),
                on_dismiss: None,
                children: vec![],
            },
        ],
    );
    assert_eq!(
        audit(&faulty),
        vec![
            Fault {
                path: vec!["App".into(), "App/open".into()],
                kind: NoRole,
            },
            Fault {
                path: vec!["App".into(), "App/open".into()],
                kind: Unreachable,
            },
            Fault {
                path: vec!["App".into(), "ask".into()],
                kind: Unnamed,
            },
        ]
    );
    passes(el(
        "App",
        Interactivity::default(),
        vec![button("App/open", "Open")],
    ));
}

#[test]
fn an_interactive_node_of_any_kind_without_a_role_fails() {
    fails(
        NoRole,
        el(
            "open",
            Interactivity {
                focusable: true,
                ..Default::default()
            },
            vec![text("Open")],
        ),
    );
    fails(
        NoRole,
        image(
            Interactivity {
                on_click: Some(1),
                ..Default::default()
            },
            Some("Avatar"),
        ),
    );
    // A role the host drops is no role.
    fails(
        NoRole,
        el("open", control(Role::GenericContainer), vec![text("Open")]),
    );
}

#[test]
fn an_interactive_node_with_a_role_passes() {
    passes(button("open", "Open"));
}

#[test]
fn a_node_only_the_pointer_hears_needs_no_role() {
    passes(el(
        "row",
        Interactivity {
            on_hover: Some(1),
            on_mouse_move: Some(2),
            on_mouse_down_out: Some(3),
            on_scroll_wheel: Some(4),
            on_modifiers_changed: Some(5),
            ..Default::default()
        },
        vec![button("open", "Open")],
    ));
}

#[test]
fn a_roled_control_overlay_or_picture_nothing_names_fails() {
    fails(
        Unnamed,
        el("close", control(Role::Button), vec![text("  ")]),
    );
    fails(Unnamed, image(roled(Role::Image), Some(" ")));
    fails(
        Unnamed,
        Node::Overlay {
            id: ElementIdWire::Name("ask".into()),
            label: Some("  ".into()),
            style: Default::default(),
            on_dismiss: None,
            children: vec![],
        },
    );
}

#[test]
fn a_named_control_overlay_and_picture_pass() {
    passes(el(
        "close",
        labelled(control(Role::Button), "Close"),
        Vec::new(),
    ));
    passes(image(roled(Role::Image), Some("Avatar")));
    passes(Node::Overlay {
        id: ElementIdWire::Name("ask".into()),
        label: Some("Settings".into()),
        style: Default::default(),
        on_dismiss: Some(1),
        children: vec![text("base"), text("dialog")],
    });
}

#[test]
fn a_field_without_a_label_fails() {
    fails(UnlabeledInput, input("  "));
}

#[test]
fn a_labelled_field_passes() {
    passes(input("Display name"));
}

#[test]
fn a_name_of_glyphs_or_of_the_role_fails() {
    fails(GlyphName, button("close", "×"));
    fails(GlyphName, button("send", "button"));
}

#[test]
fn a_name_with_words_passes() {
    passes(button("close", "× Close"));
}

#[test]
fn aria_without_a_role_fails() {
    fails(
        OrphanAria,
        el(
            "chat",
            labelled(Interactivity::default(), "Chat"),
            Vec::new(),
        ),
    );
}

#[test]
fn aria_with_a_role_passes() {
    passes(el("chat", labelled(roled(Role::Group), "Chat"), Vec::new()));
}

#[test]
fn a_role_without_its_state_fails() {
    fails(
        MissingState,
        el("dark", labelled(control(Role::Switch), "Dark"), Vec::new()),
    );
    fails(
        MissingState,
        el(
            "room",
            labelled(control(Role::ComboBox), "Room"),
            Vec::new(),
        ),
    );
    fails(
        MissingState,
        el("title", roled(Role::Heading), vec![text("Inbox")]),
    );
    let tab = el("chat", control(Role::Tab), vec![text("Chat")]);
    fails(MissingState, el("tabs", roled(Role::TabList), vec![tab]));
}

#[test]
fn a_role_with_its_state_passes() {
    let with = |role, aria: Aria| {
        el(
            "dark",
            labelled(
                Interactivity {
                    aria,
                    ..control(role)
                },
                "Dark",
            ),
            Vec::new(),
        )
    };
    passes(with(
        Role::Switch,
        Aria {
            toggled: Some(true.into()),
            ..Default::default()
        },
    ));
    passes(with(
        Role::ComboBox,
        Aria {
            expanded: Some(false),
            ..Default::default()
        },
    ));
    let heading = Interactivity {
        aria: Aria {
            level: Some(1),
            ..Default::default()
        },
        ..roled(Role::Heading)
    };
    passes(el("title", heading, vec![text("Inbox")]));
    let tab = Interactivity {
        aria: Aria {
            selected: Some(true),
            ..Default::default()
        },
        ..control(Role::Tab)
    };
    passes(el(
        "tabs",
        roled(Role::TabList),
        vec![el("chat", tab, vec![text("Chat")])],
    ));
}

#[test]
fn a_disabled_control_that_answers_fails() {
    let disabled = Interactivity {
        aria: Aria {
            disabled: Some(true),
            ..Default::default()
        },
        ..control(Role::Button)
    };
    fails(DisabledButLive, el("send", disabled, vec![text("Send")]));
}

#[test]
fn a_disabled_control_without_a_route_passes() {
    let disabled = Interactivity {
        aria: Aria {
            disabled: Some(true),
            ..Default::default()
        },
        focusable: true,
        ..roled(Role::Button)
    };
    passes(el("send", disabled, vec![text("Send")]));
}

#[test]
fn a_control_inside_a_control_fails() {
    let card = el(
        "card",
        control(Role::Button),
        vec![text("Message"), button("remove", "Remove")],
    );
    fails(NestedInteractive, card);
}

#[test]
fn controls_side_by_side_pass() {
    let row = el(
        "row",
        roled(Role::Group),
        vec![button("open", "Open"), button("remove", "Remove")],
    );
    passes(row);
}

#[test]
fn a_click_no_key_reaches_fails() {
    let unfocusable = Interactivity {
        focusable: false,
        ..control(Role::Button)
    };
    fails(Unreachable, el("open", unfocusable, vec![text("Open")]));
}

/// A focusable list box of one pressable row; `keys` routes its arrows.
fn rooms(keys: Option<u32>) -> Node {
    let option = Interactivity {
        focusable: false,
        aria: Aria {
            selected: Some(false),
            ..Default::default()
        },
        ..control(Role::ListBoxOption)
    };
    let list = labelled(
        Interactivity {
            focusable: true,
            on_key_down: keys,
            ..roled(Role::ListBox)
        },
        "Rooms",
    );
    el(
        "rooms",
        list,
        vec![el("general", option, vec![text("general")])],
    )
}

#[test]
fn a_row_of_a_focused_composite_the_keys_walk_passes() {
    passes(rooms(Some(2)));
}

#[test]
fn a_row_of_a_focused_composite_no_key_walks_fails() {
    fails(Unreachable, rooms(None));
}

/// A list box whose options claim the active descendant as `claims` says.
fn claimed(claims: [Interactivity; 2]) -> Node {
    let list = labelled(
        Interactivity {
            focusable: true,
            on_key_down: Some(2),
            ..roled(Role::ListBox)
        },
        "Rooms",
    );
    let rows = claims
        .into_iter()
        .zip(["general", "random"])
        .map(|(row, name)| {
            let option = Interactivity {
                aria: Aria {
                    selected: Some(false),
                    ..row.aria
                },
                ..row
            };
            el(name, option, vec![text(name)])
        });
    el("rooms", list, rows.collect())
}

fn claim(focusable: bool, focus_handle: Option<u64>) -> Interactivity {
    Interactivity {
        focusable,
        focus_handle,
        aria: Aria {
            active_descendant: true,
            ..Default::default()
        },
        ..roled(Role::ListBoxOption)
    }
}

#[test]
fn a_state_its_role_does_not_read_fails() {
    let selected = |role| Interactivity {
        aria: Aria {
            selected: Some(true),
            ..Default::default()
        },
        ..control(role)
    };
    fails(
        UnreadState,
        el("general", selected(Role::Button), vec![text("general")]),
    );
    fails(
        UnreadState,
        el("row", selected(Role::ListItem), vec![text("src/lib.rs")]),
    );
    let toggled = Interactivity {
        aria: Aria {
            toggled: Some(gpui::Toggled::True),
            ..Default::default()
        },
        ..control(Role::Link)
    };
    fails(UnreadState, el("open", toggled, vec![text("Open")]));
    let heading = Interactivity {
        aria: Aria {
            level: Some(1),
            toggled: Some(gpui::Toggled::True),
            ..Default::default()
        },
        ..roled(Role::Heading)
    };
    fails(UnreadState, el("title", heading, vec![text("Inbox")]));
}

#[test]
fn a_state_its_role_reads_passes() {
    let tab = Interactivity {
        aria: Aria {
            selected: Some(true),
            ..Default::default()
        },
        ..control(Role::Tab)
    };
    let tabs = labelled(roled(Role::TabList), "Pages");
    passes(el("tabs", tabs, vec![el("home", tab, vec![text("Home")])]));
    let pressed = Interactivity {
        aria: Aria {
            toggled: Some(gpui::Toggled::False),
            ..Default::default()
        },
        ..control(Role::Button)
    };
    passes(el("bold", pressed, vec![text("Bold")]));
    // WAI-ARIA gives an option and a tree item aria-checked
    let checked = |role| Interactivity {
        aria: Aria {
            selected: Some(false),
            toggled: Some(gpui::Toggled::True),
            ..Default::default()
        },
        ..roled(role)
    };
    for (within, role) in [
        (Role::ListBox, Role::ListBoxOption),
        (Role::Tree, Role::TreeItem),
    ] {
        let item = el("src", checked(role), vec![text("src")]);
        passes(el("files", roled(within), vec![item]));
    }
}

#[test]
fn an_active_descendant_the_host_drops_fails() {
    let quiet = roled(Role::ListBoxOption);
    fails(
        ActiveDescendant,
        claimed([claim(true, None), quiet.clone()]),
    );
    fails(ActiveDescendant, claimed([claim(false, Some(7)), quiet]));
    fails(
        ActiveDescendant,
        claimed([claim(false, None), claim(false, None)]),
    );
}

#[test]
fn one_active_descendant_that_takes_no_focus_passes() {
    passes(claimed([claim(false, None), roled(Role::ListBoxOption)]));
    // a dropped claim spends nothing
    let quiet = claimed([claim(true, None), claim(false, None)]);
    assert_eq!(kinds(&quiet), [ActiveDescendant]);
}

#[test]
fn an_item_outside_its_container_fails() {
    fails(
        Orphan,
        el("copy", control(Role::MenuItem), vec![text("Copy")]),
    );
    fails(Orphan, el("row", roled(Role::Row), vec![text("alice")]));
}

#[test]
fn an_item_inside_its_container_passes() {
    let menu = labelled(roled(Role::Menu), "Actions");
    passes(el(
        "menu",
        menu,
        vec![el("copy", control(Role::MenuItem), vec![text("Copy")])],
    ));
}

#[test]
fn a_clickable_range_without_words_fails() {
    fails(RangeUnnamed, rich("see → here", 4..7));
    fails(RangeUnnamed, rich("see", 2..9));
}

#[test]
fn a_clickable_range_with_words_passes() {
    passes(rich("see → here", 8..12));
}

#[test]
fn a_bare_resize_handle_fails() {
    fails(BareHandle, handle(Interactivity::default()));
    fails(
        BareHandle,
        handle(labelled(
            Interactivity {
                focusable: true,
                ..roled(Role::Splitter)
            },
            "Resize",
        )),
    );
}

#[test]
fn a_named_focusable_splitter_that_moves_by_key_passes() {
    let splitter = Interactivity {
        focusable: true,
        on_key_down: Some(2),
        ..labelled(roled(Role::Splitter), "Resize sidebar")
    };
    passes(handle(splitter));
}

#[test]
fn an_action_the_node_cannot_answer_fails() {
    let pin = Interactivity {
        aria: Aria {
            custom_actions: vec![(1, "Pin".into())],
            ..Default::default()
        },
        ..control(Role::Button)
    };
    fails(ActionUnhandled, el("message", pin, vec![text("Message")]));
    let step = Interactivity {
        aria: Aria {
            actions: vec![(Action::Increment, 2)],
            ..Default::default()
        },
        ..labelled(control(Role::SpinButton), "Size")
    };
    fails(ActionUnhandled, el("size", step, Vec::new()));
    let expand = Interactivity {
        aria: Aria {
            actions: vec![(Action::Expand, 2)],
            ..Default::default()
        },
        ..control(Role::Button)
    };
    fails(ActionUnhandled, el("more", expand, vec![text("More")]));
    // the host keeps a click, and hears one listener per action
    for actions in [
        vec![(Action::Click, 2)],
        vec![(Action::Expand, 2), (Action::Expand, 3)],
    ] {
        let dropped = Interactivity {
            aria: Aria {
                actions,
                expanded: Some(false),
                ..Default::default()
            },
            ..control(Role::Button)
        };
        fails(ActionUnhandled, el("more", dropped, vec![text("More")]));
    }
}

#[test]
fn an_action_with_its_route_and_state_passes() {
    let pin = Interactivity {
        aria: Aria {
            custom_actions: vec![(1, "Pin".into())],
            actions: vec![(Action::CustomAction, 3), (Action::Expand, 4)],
            expanded: Some(false),
            ..Default::default()
        },
        ..control(Role::Button)
    };
    passes(el("message", pin, vec![text("Message")]));
}

#[test]
fn a_key_an_earlier_sibling_holds_fails() {
    let row = el(
        "row",
        roled(Role::Group),
        vec![button("open", "Open"), button("open", "Again")],
    );
    assert_eq!(
        audit(&row),
        vec![Fault {
            path: vec!["row".into(), "open".into()],
            kind: DuplicateKey,
        }]
    );
}

#[test]
fn distinct_keys_pass() {
    passes(el(
        "row",
        roled(Role::Group),
        vec![button("open", "Open"), button("again", "Again")],
    ));
}

#[test]
fn a_status_that_is_not_live_or_says_nothing_fails() {
    fails(
        StatusNotLive,
        el("status", roled(Role::Status), vec![text("Saved")]),
    );
    let silent = Interactivity {
        aria: Aria {
            live: Some(Live::Assertive),
            ..Default::default()
        },
        ..roled(Role::Alert)
    };
    fails(StatusNotLive, el("alert", silent, vec![text(" ")]));
}

#[test]
fn a_live_status_with_words_passes() {
    let status = Interactivity {
        aria: Aria {
            live: Some(Live::Polite),
            ..Default::default()
        },
        ..roled(Role::Status)
    };
    passes(el("status", status, vec![text("Saved")]));
}

#[test]
fn an_invalid_field_that_does_not_say_why_fails() {
    let field = Interactivity {
        aria: Aria {
            invalid: Some(Invalid::True),
            ..Default::default()
        },
        ..labelled(
            Interactivity {
                focusable: true,
                ..roled(Role::TextInput)
            },
            "Email",
        )
    };
    fails(ErrorNoText, el("email", field, Vec::new()));
    let mut input = input("Email");
    if let Node::Input { options, .. } = &mut input {
        options.invalid = Some(Invalid::True);
    }
    fails(ErrorNoText, input);
}

#[test]
fn an_invalid_field_that_says_why_passes() {
    let field = |aria: Aria| {
        let interactivity = Interactivity {
            aria: Aria {
                invalid: Some(Invalid::True),
                label: Some("Email".into()),
                ..aria
            },
            focusable: true,
            ..roled(Role::TextInput)
        };
        el("email", interactivity, Vec::new())
    };
    passes(field(Aria {
        description: Some("An address has an @".into()),
        ..Default::default()
    }));
    passes(field(Aria {
        error_message: Some(vec![ElementIdWire::Name("email-error".into())]),
        ..Default::default()
    }));
    let mut input = input("Email");
    if let Node::Input { options, .. } = &mut input {
        options.invalid = Some(Invalid::True);
        options.description = Some("An address has an @".into());
    }
    passes(input);
}
