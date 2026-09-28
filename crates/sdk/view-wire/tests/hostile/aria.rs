//! The aria a guest writes: every list it carries is bounded at decode, and
//! `sanitize` pulls the rest into what the host maps.
use super::*;

fn with_aria(aria: Aria) -> Frame {
    Frame {
        root: Some(Node::Container(ContainerNode {
            id: None,
            style: gpui::StyleRefinement::default(),
            interactivity: Interactivity {
                aria,
                ..Default::default()
            },
            children: Vec::new(),
        })),
        ..Frame::default()
    }
}

fn target() -> Vec<ElementIdWire> {
    vec![ElementIdWire::Name("caption".into())]
}

/// At the bound the frame decodes; one past it, `decode` names the list.
fn refused_past(at: impl Fn(usize) -> Aria, bound: usize, message: &str) {
    assert!(decode::<Frame>(&encode(&with_aria(at(bound)))).is_ok());
    let refused = decode::<Frame>(&encode(&with_aria(at(bound + 1)))).unwrap_err();
    assert!(refused.contains(message), "{refused}");
}

#[test]
fn decode_refuses_more_relations_than_a_list_holds() {
    for relation in 0..3 {
        refused_past(
            |len| {
                let targets = vec![target(); len];
                match relation {
                    0 => Aria {
                        labelled_by: targets,
                        ..Default::default()
                    },
                    1 => Aria {
                        described_by: targets,
                        ..Default::default()
                    },
                    _ => Aria {
                        controls: targets,
                        ..Default::default()
                    },
                }
            },
            MAX_ARIA_RELATIONS,
            "too many aria relations",
        );
    }
    refused_past(
        |depth| Aria {
            labelled_by: vec![vec![ElementIdWire::Integer(1); depth]],
            ..Default::default()
        },
        MAX_DEPTH,
        "aria relation target is too deep",
    );
    refused_past(
        |depth| Aria {
            error_message: Some(vec![ElementIdWire::Integer(1); depth]),
            ..Default::default()
        },
        MAX_DEPTH,
        "aria relation target is too deep",
    );
}

#[test]
fn decode_refuses_more_actions_than_a_node_advertises() {
    refused_past(
        |len| Aria {
            actions: vec![(Action::Increment, 1); len],
            ..Default::default()
        },
        MAX_ARIA_ACTIONS,
        "too many aria actions",
    );
}

#[test]
fn decode_refuses_more_custom_actions_than_a_node_offers() {
    refused_past(
        |len| Aria {
            custom_actions: (0..len as i32).map(|id| (id, "Pin".into())).collect(),
            ..Default::default()
        },
        MAX_ARIA_CUSTOM_ACTIONS,
        "too many aria custom actions",
    );
}

/// One container carrying `interactivity`, sanitized; what survives.
fn sanitized(interactivity: Interactivity) -> Result<Interactivity, &'static str> {
    let mut frame = Frame {
        root: Some(Node::Container(ContainerNode {
            id: None,
            style: gpui::StyleRefinement::default(),
            interactivity,
            children: Vec::new(),
        })),
        ..Frame::default()
    };
    sanitize(&mut frame)?;
    let Some(Node::Container(ContainerNode { interactivity, .. })) = frame.root else {
        unreachable!("a container stays a container")
    };
    Ok(interactivity)
}

fn aria(aria: Aria) -> Aria {
    sanitized(Interactivity {
        aria,
        ..Default::default()
    })
    .unwrap()
    .aria
}

#[test]
fn a_role_that_says_nothing_or_is_the_windows_is_dropped() {
    use gpui::Role::*;
    for role in [
        GenericContainer,
        Unknown,
        Window,
        Application,
        RootWebArea,
        Pane,
        Iframe,
        IframePresentational,
        WebView,
        TitleBar,
    ] {
        let kept = sanitized(Interactivity {
            role: Some(role),
            ..Default::default()
        });
        assert_eq!(kept.unwrap().role, None, "{role:?}");
    }
    let button = sanitized(Interactivity {
        role: Some(Button),
        ..Default::default()
    });
    assert_eq!(button.unwrap().role, Some(Button));
}

#[test]
fn a_heading_level_outside_1_to_6_is_no_level() {
    for (level, kept) in [
        (0, None),
        (1, Some(1)),
        (6, Some(6)),
        (7, None),
        (usize::MAX, None),
    ] {
        let heading = sanitized(Interactivity {
            role: Some(gpui::Role::Heading),
            aria: Aria {
                level: Some(level),
                ..Default::default()
            },
            ..Default::default()
        });
        assert_eq!(heading.unwrap().aria.level, kept, "level {level}");
    }
    // A tree item's level is its depth, not a heading's.
    let item = sanitized(Interactivity {
        role: Some(gpui::Role::TreeItem),
        aria: Aria {
            level: Some(usize::MAX),
            ..Default::default()
        },
        ..Default::default()
    });
    assert_eq!(item.unwrap().aria.level, Some(1_000_000));
}

#[test]
fn a_focusable_node_is_not_its_own_active_descendant() {
    for focusable in [true, false] {
        let node = sanitized(Interactivity {
            role: Some(gpui::Role::ListBoxOption),
            focusable,
            aria: Aria {
                active_descendant: true,
                ..Default::default()
            },
            ..Default::default()
        });
        assert_eq!(node.unwrap().aria.active_descendant, !focusable);
    }
}

#[test]
fn live_off_is_no_live_region() {
    let off = aria(Aria {
        live: Some(Live::Off),
        ..Default::default()
    });
    assert_eq!(off.live, None);
    let polite = aria(Aria {
        live: Some(Live::Polite),
        ..Default::default()
    });
    assert_eq!(polite.live, Some(Live::Polite));
}

#[test]
fn relations_are_cut_to_the_bound_and_each_target_is_checked() {
    let many = vec![target(); MAX_ARIA_RELATIONS + 4];
    let cut = aria(Aria {
        labelled_by: many.clone(),
        described_by: many.clone(),
        controls: many,
        ..Default::default()
    });
    for relation in [cut.labelled_by, cut.described_by, cut.controls] {
        assert_eq!(relation.len(), MAX_ARIA_RELATIONS);
    }
    let host_local = vec![ElementIdWire::FocusHandle(1)];
    for aria in [
        Aria {
            labelled_by: vec![host_local.clone()],
            ..Default::default()
        },
        Aria {
            error_message: Some(host_local),
            ..Default::default()
        },
    ] {
        let refused = sanitized(Interactivity {
            aria,
            ..Default::default()
        });
        assert_eq!(refused, Err("focus-handle element IDs are host-local"));
    }
    let deep = sanitized(Interactivity {
        aria: Aria {
            controls: vec![vec![ElementIdWire::Integer(1); MAX_DEPTH + 1]],
            ..Default::default()
        },
        ..Default::default()
    });
    assert_eq!(deep, Err("aria relation target is too deep"));
}

#[test]
fn actions_the_host_owns_or_repeats_are_dropped() {
    let kept = aria(Aria {
        actions: [
            Action::Click,
            Action::Increment,
            Action::Focus,
            Action::Blur,
            Action::SetValue,
            Action::ReplaceSelectedText,
            Action::SetTextSelection,
            Action::Increment,
            Action::Decrement,
        ]
        .into_iter()
        .zip(1..)
        .collect(),
        ..Default::default()
    });
    assert_eq!(
        kept.actions,
        vec![(Action::Increment, 2), (Action::Decrement, 9)]
    );
}

#[test]
fn custom_actions_are_cut_to_the_bound_unique_and_short() {
    let kept = aria(Aria {
        custom_actions: (0..MAX_ARIA_CUSTOM_ACTIONS as i32 + 4)
            .map(|id| (id % 6, "é".repeat(MAX_ARIA_TEXT_BYTES)))
            .collect(),
        ..Default::default()
    });
    assert_eq!(
        kept.custom_actions
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 4, 5]
    );
    for (_, description) in &kept.custom_actions {
        assert_eq!(description.len(), MAX_ARIA_TEXT_BYTES);
    }
    let unique = aria(Aria {
        custom_actions: (0..MAX_ARIA_CUSTOM_ACTIONS as i32 + 4)
            .map(|id| (id, "Pin".into()))
            .collect(),
        ..Default::default()
    });
    assert_eq!(
        unique
            .custom_actions
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        (0..MAX_ARIA_CUSTOM_ACTIONS as i32).collect::<Vec<_>>()
    );
}

#[test]
fn aria_strings_are_cut_to_the_bound() {
    let long: gpui::SharedString = "x".repeat(MAX_ARIA_TEXT_BYTES + 1).into();
    let kept = aria(Aria {
        label: Some(long.clone()),
        description: Some(long.clone()),
        keyshortcuts: Some(long.clone()),
        value: Some(long.clone()),
        placeholder: Some(long.clone()),
        author_id: Some(long),
        ..Default::default()
    });
    for text in [
        kept.label,
        kept.description,
        kept.keyshortcuts,
        kept.value,
        kept.placeholder,
        kept.author_id,
    ] {
        assert_eq!(text.unwrap().len(), MAX_ARIA_TEXT_BYTES);
    }
}

/// A list's and a divider's interactivity are walked like a container's.
#[test]
fn list_and_resize_handle_interactivity_is_sanitized() {
    let hostile = || Interactivity {
        role: Some(gpui::Role::GenericContainer),
        aria: Aria {
            live: Some(Live::Off),
            ..Default::default()
        },
        focus: Some(gen_native_style(&mut Rng::new(5))),
        ..Default::default()
    };
    let mut frame = Frame {
        root: Some(Node::ResizeHandle {
            id: ElementIdWire::Name("divider".into()),
            style: gpui::StyleRefinement::default(),
            interactivity: hostile(),
            on_press: None,
            on_release: None,
            on_drag: None,
            cursor: None,
            content: Box::new(Node::List {
                state: 1,
                path: vec![ElementIdWire::Name("divider".into())],
                item_count: 0,
                alignment: ListAlignment::Top,
                overdraw: 0.,
                sizing: ListSizingBehavior::Infer,
                following_tail: false,
                revision: 0,
                commands: Vec::new(),
                request_handler: 1,
                scroll_handler: None,
                range_start: 0,
                style: gpui::StyleRefinement::default(),
                interactivity: hostile(),
                children: Vec::new(),
            }),
        }),
        ..Frame::default()
    };
    sanitize(&mut frame).unwrap();
    let Some(Node::ResizeHandle {
        interactivity: handle,
        content,
        ..
    }) = frame.root
    else {
        unreachable!()
    };
    let Node::List {
        interactivity: list,
        ..
    } = *content
    else {
        unreachable!()
    };
    for interactivity in [handle, list] {
        assert_eq!(interactivity.role, None);
        assert_eq!(interactivity.aria.live, None);
        check_native_style(interactivity.focus.as_ref().unwrap());
    }
}
