//! The aria a guest writes: every list it carries is bounded at decode, and
//! `sanitize` pulls the rest into what the host maps.
use super::*;

fn with_aria(aria: Aria) -> Frame {
    Frame {
        root: Some(Node::Container(ContainerNode {
            id: None,
            style: gpui::StyleRefinement::default(),
            interactivity: Box::new(Interactivity {
                aria,
                ..Default::default()
            }),
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
fn sanitized(interactivity: Interactivity) -> Result<Interactivity, Refused> {
    let mut frame = Frame {
        root: Some(Node::Container(ContainerNode {
            id: None,
            style: gpui::StyleRefinement::default(),
            interactivity: Box::new(interactivity),
            children: Vec::new(),
        })),
        ..Frame::default()
    };
    sanitize(&mut frame)?;
    let Some(Node::Container(ContainerNode { interactivity, .. })) = frame.root else {
        unreachable!("a container stays a container")
    };
    Ok(*interactivity)
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
    for (focusable, focus_handle) in [(true, None), (false, Some(7)), (false, None)] {
        let node = sanitized(Interactivity {
            role: Some(gpui::Role::ListBoxOption),
            focusable,
            focus_handle,
            aria: Aria {
                active_descendant: true,
                ..Default::default()
            },
            ..Default::default()
        });
        assert_eq!(
            node.unwrap().aria.active_descendant,
            !focusable && focus_handle.is_none()
        );
    }
}

#[test]
fn only_the_first_active_descendant_in_a_frame_is_kept() {
    let option = |key: &str| {
        Node::Container(ContainerNode {
            id: Some(ElementIdWire::Name(key.into())),
            style: gpui::StyleRefinement::default(),
            interactivity: Box::new(Interactivity {
                role: Some(gpui::Role::ListBoxOption),
                aria: Aria {
                    active_descendant: true,
                    ..Default::default()
                },
                ..Default::default()
            }),
            children: Vec::new(),
        })
    };
    let mut frame = Frame {
        root: Some(Node::Container(ContainerNode {
            id: None,
            style: gpui::StyleRefinement::default(),
            interactivity: Default::default(),
            children: vec![option("first"), option("second")],
        })),
        ..Frame::default()
    };
    sanitize(&mut frame).unwrap();
    let claims: Vec<bool> = frame.root.unwrap().children()[..]
        .iter()
        .map(|child| match child {
            Node::Container(ContainerNode { interactivity, .. }) => {
                interactivity.aria.active_descendant
            }
            _ => unreachable!("a container stays a container"),
        })
        .collect();
    assert_eq!(claims, [true, false]);
}

/// Each focusable node restarts the budget: two composites on one screen
/// both keep their claim, as gpui counts claims per focused node.
#[test]
fn a_claim_under_each_focusable_ancestor_is_kept() {
    let option = |key: &str| {
        Node::Container(ContainerNode {
            id: Some(ElementIdWire::Name(key.into())),
            style: gpui::StyleRefinement::default(),
            interactivity: Box::new(Interactivity {
                role: Some(gpui::Role::ListBoxOption),
                aria: Aria {
                    active_descendant: true,
                    ..Default::default()
                },
                ..Default::default()
            }),
            children: Vec::new(),
        })
    };
    let list = |key: &str, rows: Vec<Node>| {
        Node::Container(ContainerNode {
            id: Some(ElementIdWire::Name(key.into())),
            style: gpui::StyleRefinement::default(),
            interactivity: Box::new(Interactivity {
                role: Some(gpui::Role::ListBox),
                focusable: true,
                ..Default::default()
            }),
            children: rows,
        })
    };
    let mut frame = Frame {
        root: Some(Node::Container(ContainerNode {
            id: None,
            style: gpui::StyleRefinement::default(),
            interactivity: Default::default(),
            children: vec![
                list("rooms", vec![option("general"), option("random")]),
                list("members", vec![option("minseo")]),
            ],
        })),
        ..Frame::default()
    };
    sanitize(&mut frame).unwrap();
    let mut claims = Vec::new();
    frame.root.unwrap().for_each_mut(&mut |node| {
        if let Node::Container(ContainerNode {
            id: Some(id),
            interactivity,
            ..
        }) = node
            && interactivity.role == Some(gpui::Role::ListBoxOption)
        {
            claims.push((id.clone(), interactivity.aria.active_descendant));
        }
    });
    let name = |key: &str| ElementIdWire::Name(key.into());
    assert_eq!(
        claims,
        [
            (name("general"), true),
            (name("random"), false),
            (name("minseo"), true)
        ]
    );
}

/// A focusable box with no role pushes no accessibility node, so gpui
/// counts a claim under it against the roled ancestor above: two such boxes
/// over two claims keep one claim, not two.
#[test]
fn a_roleless_focusable_box_does_not_restart_the_claim_budget() {
    let option = |key: &str| {
        Node::Container(ContainerNode {
            id: Some(ElementIdWire::Name(key.into())),
            style: gpui::StyleRefinement::default(),
            interactivity: Box::new(Interactivity {
                role: Some(gpui::Role::ListBoxOption),
                aria: Aria {
                    active_descendant: true,
                    ..Default::default()
                },
                ..Default::default()
            }),
            children: Vec::new(),
        })
    };
    let box_ = |key: &str, row: Node| {
        Node::Container(ContainerNode {
            id: Some(ElementIdWire::Name(key.into())),
            style: gpui::StyleRefinement::default(),
            interactivity: Box::new(Interactivity {
                focusable: true,
                ..Default::default()
            }),
            children: vec![row],
        })
    };
    let mut frame = Frame {
        root: Some(Node::Container(ContainerNode {
            id: Some(ElementIdWire::Name("pane".into())),
            style: gpui::StyleRefinement::default(),
            interactivity: Box::new(Interactivity {
                role: Some(gpui::Role::Group),
                focusable: true,
                ..Default::default()
            }),
            children: vec![
                box_("channels", option("general")),
                box_("people", option("minseo")),
            ],
        })),
        ..Frame::default()
    };
    sanitize(&mut frame).unwrap();
    let mut claims = Vec::new();
    frame.root.unwrap().for_each_mut(&mut |node| {
        if let Node::Container(ContainerNode { interactivity, .. }) = node
            && interactivity.role == Some(gpui::Role::ListBoxOption)
        {
            claims.push(interactivity.aria.active_descendant);
        }
    });
    assert_eq!(claims, [true, false]);
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
        assert_eq!(
            refused,
            Err(Refused::Invalid("focus-handle element IDs are host-local"))
        );
    }
    let deep = sanitized(Interactivity {
        aria: Aria {
            controls: vec![vec![ElementIdWire::Integer(1); MAX_DEPTH + 1]],
            ..Default::default()
        },
        ..Default::default()
    });
    assert_eq!(
        deep,
        Err(Refused::Invalid("aria relation target is too deep"))
    );
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
    // repeats spend no place: a new id after eight of one is kept
    let repeated = aria(Aria {
        custom_actions: [1; MAX_ARIA_CUSTOM_ACTIONS]
            .into_iter()
            .chain([2])
            .map(|id| (id, "Pin".into()))
            .collect(),
        ..Default::default()
    });
    assert_eq!(
        repeated
            .custom_actions
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        [1, 2]
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
        focus: Some(Box::new(gen_native_style(&mut Rng::new(5)))),
        ..Default::default()
    };
    let mut frame = Frame {
        root: Some(Node::ResizeHandle {
            id: ElementIdWire::Name("divider".into()),
            style: gpui::StyleRefinement::default(),
            interactivity: Box::new(hostile()),
            on_press: None,
            on_release: None,
            on_drag: None,
            cursor: None,
            content: Box::new(Node::List {
                id: ElementIdWire::ListState(1),
                path: vec![
                    ElementIdWire::Name("divider".into()),
                    ElementIdWire::ListState(1),
                ],
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
                interactivity: Box::new(hostile()),
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
