//! One tree that breaks each rule, and one that keeps it and every other.
use super::*;
use crate::{ElementIdWire, Invalid, Live};
use FaultKind::*;

mod fixtures;
use fixtures::*;

fn kinds(tree: &Node) -> Vec<FaultKind> {
    audit(tree).faults.iter().map(|fault| fault.kind).collect()
}

fn fails(kind: FaultKind, tree: Node) {
    let kinds = kinds(&tree);
    assert!(kinds.contains(&kind), "{kind:?} not among {kinds:?}");
}

/// A rule looked at the tree, and nothing in it breaks any rule.
fn passes(tree: Node) {
    let report = audit(&tree);
    assert_eq!(report.faults, Vec::new());
    assert!(report.applicable > 0, "no rule selected the tree");
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
    let report = audit(&faulty);
    assert_eq!(
        report.faults,
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
    assert_eq!(report.applicable, 2);
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

#[test]
fn a_row_of_a_focused_composite_passes() {
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
            ..roled(Role::ListBox)
        },
        "Rooms",
    );
    passes(el(
        "rooms",
        list,
        vec![el("general", option, vec![text("general")])],
    ));
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
    let report = audit(&row);
    assert_eq!(
        report.faults,
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
}
