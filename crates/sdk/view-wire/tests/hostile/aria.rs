//! The aria a guest writes: every list it carries is bounded at decode.
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
