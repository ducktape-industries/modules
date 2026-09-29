//! Every `WidgetCommand` variant once: the payload of `host.widget`, but
//! also its own tree-side (named MessagePack) type, sampled here the same
//! way `nodes.rs` samples `Node` so its bytes are pinned too.
use super::*;

/// Every `ElementIdAtom` variant once, wrapped in a `NamedChild` (the one
/// place an atom crosses): a target list otherwise unremarkable.
fn every_atom_child() -> Vec<ElementIdWire> {
    let atoms = [
        ElementIdAtom::View(1),
        ElementIdAtom::Integer(2),
        ElementIdAtom::Name("atom".into()),
        ElementIdAtom::Uuid([1; 16]),
        ElementIdAtom::FocusHandle(3),
        ElementIdAtom::NamedInteger("atom".into(), 4),
        ElementIdAtom::Path(b"a/b".to_vec()),
        ElementIdAtom::CodeLocation {
            file: "widget.rs".into(),
            line: 1,
            column: 2,
        },
        ElementIdAtom::OpaqueId([2; 20]),
    ];
    atoms
        .into_iter()
        .map(|base| ElementIdWire::NamedChild {
            base,
            names: vec!["child".into()],
        })
        .collect()
}

pub fn every_widget_command() -> Vec<WidgetCommand> {
    let target = |name: &str| vec![id(name)];
    vec![
        WidgetCommand::EditorAction {
            target: target("editor"),
            tag: "send".into(),
        },
        WidgetCommand::FocusPrevious,
        WidgetCommand::FocusNext,
        WidgetCommand::Focus {
            target: every_atom_child(),
        },
        WidgetCommand::FocusHandle { handle: 7 },
        WidgetCommand::CursorFront {
            target: target("editor"),
        },
        WidgetCommand::CursorEnd {
            target: target("editor"),
        },
        WidgetCommand::Cursor {
            target: target("editor"),
            position: 3,
        },
        WidgetCommand::SelectAll {
            target: target("editor"),
        },
        WidgetCommand::Select {
            target: target("editor"),
            start: 0,
            end: 3,
        },
        WidgetCommand::Snap {
            target: target("handle"),
            x: 0.25,
            y: 0.75,
        },
        WidgetCommand::SnapEnd {
            target: target("handle"),
        },
        WidgetCommand::ScrollTo {
            target: target("list"),
            x: 10.0,
            y: 20.0,
        },
        WidgetCommand::ScrollBy {
            target: target("list"),
            x: 1.0,
            y: -1.0,
        },
    ]
}
