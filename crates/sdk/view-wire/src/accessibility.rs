//! The one rule function for what assistive technology cannot name, place
//! or reach: the view tests ask [`audit`] of every frame (`view-guest`
//! testing); the host may, on a decoded frame.

use crate::aria::{HOST_ACTIONS, view_role};
use crate::{Action, Aria, ContainerNode, Interactivity, Node, TextNode};
use gpui::Role;

/// A node assistive technology cannot name, place or reach.
#[derive(Clone, Debug, PartialEq)]
pub struct Fault {
    /// The `key`s from the root down to the node.
    pub path: Vec<String>,
    pub kind: FaultKind,
}

/// One per broken rule per node; the AX ids are `docs/ax.md`'s in the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaultKind {
    /// Interactive, and says nothing about what it is (AX-001).
    NoRole,
    /// An interactive node with a role, an overlay, or a roled picture,
    /// that nothing names (AX-002, AX-007, AX-008).
    Unnamed,
    /// A text field or editor without a label (AX-003).
    UnlabeledInput,
    /// A name with no letter or digit in it, or that only repeats the role
    /// (AX-006).
    GlyphName,
    /// Aria on a node without a role: nothing reads it.
    OrphanAria,
    /// A role without its state: a toggle's `toggled`, a tab's, tree item's
    /// or option's `selected`, a combo box's `expanded`, a heading's `level`
    /// (AX-009, AX-010, AX-101, AX-106).
    MissingState,
    /// Disabled, and still answers a click or a key (AX-011).
    DisabledButLive,
    /// Interactive inside a button, link, tab, menu item or toggle (AX-119).
    NestedInteractive,
    /// Answers a click, and no key takes the user there (AX-012, AX-107).
    Unreachable,
    /// A menu item, tab, radio button, option, tree item, row or cell
    /// outside the container that gives it its place (AX-105).
    Orphan,
    /// A rich text's clickable range with no words in it (AX-117).
    RangeUnnamed,
    /// A resize handle that is not a named, focusable splitter that moves
    /// by key (AX-118).
    BareHandle,
    /// An advertised action the node cannot answer: custom actions with
    /// no route, a step without a value, an expand without its state, an
    /// action the host keeps, or a second listener for one action; the
    /// host drops the last two (AX-116).
    ActionUnhandled,
    /// An earlier sibling already holds this `key` (AX-015).
    DuplicateKey,
    /// A status or alert that is not live, or has nothing to say (AX-102).
    StatusNotLive,
    /// An invalid text field, an input or a text-input role, that does
    /// not say why (AX-108).
    ErrorNoText,
    /// An active descendant the host drops, as gpui would panic on it: on
    /// a node that takes focus, or after an earlier claim in the frame.
    ActiveDescendant,
    /// A state its role does not carry, so AT reads nothing of it:
    /// `selected` outside a tab, tree item, option, row or cell, `toggled`
    /// outside a toggle or a button.
    UnreadState,
}

/// Every fault in the tree, depth first.
///
/// **Interactive**: a node whose [`Interactivity`] answers a click, an
/// aux click, a mouse down or a key, is `focusable`, or advertises
/// `aria.actions` (what AT can press or focus: a hover, a move or a wheel
/// alone is the pointer's); and
/// every [`Node::Input`], [`Node::Editor`] and [`Node::RichText`] with a
/// clickable range. **Named**: a non-blank `aria.label`, else a container's
/// descendant text, a picture's or overlay's `label`, a field's label; all
/// trimmed. A role is what the host keeps of it: the sanitizer drops
/// `GenericContainer`, `Unknown` and the window-level roles, and so does
/// the audit.
pub fn audit(root: &Node) -> Vec<Fault> {
    let mut faults = Vec::new();
    walk(root, None, false, &mut false, &mut faults);
    faults
}

/// A node and the ones above it, kept on the walk's own stack.
struct Step<'a> {
    node: &'a Node,
    parent: Option<&'a Step<'a>>,
}

impl<'a> Step<'a> {
    fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = std::iter::successors(Some(self), |step| step.parent)
            .map(|step| step.node.key().unwrap_or_default().to_owned())
            .collect();
        keys.reverse();
        keys
    }

    /// The interactivity of every node above this one, innermost first.
    fn ancestors(&self) -> impl Iterator<Item = &'a Interactivity> {
        std::iter::successors(self.parent, |step| step.parent)
            .filter_map(|step| interactivity(step.node))
    }

    /// Whether a node above this one holds a role in `roles`, and, when
    /// `keyed` asks, takes focus and hears the keys that walk its rows.
    fn inside(&self, roles: &[Role], keyed: bool) -> bool {
        self.ancestors().any(|above| {
            (!keyed
                || above.focusable
                    && (above.on_key_down.is_some() || above.capture_key_down.is_some()))
                && view_role(above.role).is_some_and(|role| roles.contains(&role))
        })
    }
}

/// `claimed`: a node earlier in the walk holds the active descendant.
fn walk(
    node: &Node,
    parent: Option<&Step<'_>>,
    duplicate: bool,
    claimed: &mut bool,
    faults: &mut Vec<Fault>,
) {
    let step = Step { node, parent };
    let rules = rules(&step, duplicate, claimed);
    let path = (!rules.faults.is_empty()).then(|| step.keys());
    for kind in rules.faults {
        faults.push(Fault {
            path: path.clone().unwrap_or_default(),
            kind,
        });
    }
    let children = node.children();
    for (index, child) in children.iter().enumerate() {
        // Quadratic in a node's children, which `MAX_NODES` bounds.
        let duplicate = child.key().is_some_and(|key| {
            children[..index]
                .iter()
                .any(|earlier| earlier.key() == Some(key))
        });
        walk(child, Some(&step), duplicate, claimed, faults);
    }
}

#[derive(Default)]
struct Rules {
    faults: Vec<FaultKind>,
}

impl Rules {
    /// One rule: `selected` is its antecedent, `fails` its violation.
    fn check(&mut self, selected: bool, kind: FaultKind, fails: impl FnOnce() -> bool) {
        if selected && fails() {
            self.faults.push(kind);
        }
    }
}

const TOGGLE: [Role; 5] = [
    Role::CheckBox,
    Role::Switch,
    Role::RadioButton,
    Role::MenuItemCheckBox,
    Role::MenuItemRadio,
];
const MENU_ITEM: [Role; 3] = [Role::MenuItem, Role::MenuItemCheckBox, Role::MenuItemRadio];
/// The roles that read `selected` (ARIA's aria-selected set).
const SELECTABLE: [Role; 9] = [
    Role::Tab,
    Role::TreeItem,
    Role::ListBoxOption,
    Role::MenuListOption,
    Role::Row,
    Role::Cell,
    Role::GridCell,
    Role::ColumnHeader,
    Role::RowHeader,
];
/// Nothing inside these may be interactive: AT reads each as one control.
const CONTROL: [Role; 9] = [
    Role::Button,
    Role::Link,
    Role::Tab,
    Role::MenuItem,
    Role::MenuItemCheckBox,
    Role::MenuItemRadio,
    Role::CheckBox,
    Role::Switch,
    Role::RadioButton,
];
/// A focused one of these with a key route reaches its rows by arrow keys,
/// whichever is active.
const COMPOSITE: [Role; 8] = [
    Role::Tree,
    Role::ListBox,
    Role::Menu,
    Role::MenuBar,
    Role::Grid,
    Role::EditableComboBox,
    Role::RadioGroup,
    Role::TabList,
];
const TEXT_INPUT: [Role; 8] = [
    Role::TextInput,
    Role::MultilineTextInput,
    Role::SearchInput,
    Role::EmailInput,
    Role::NumberInput,
    Role::PasswordInput,
    Role::PhoneNumberInput,
    Role::UrlInput,
];

fn rules(step: &Step<'_>, duplicate: bool, claimed: &mut bool) -> Rules {
    use FaultKind::*;
    let node = step.node;
    let mut rules = Rules::default();
    let interactivity = interactivity(node);
    let role = interactivity.and_then(|i| view_role(i.role));
    let interactive = interactive(node);
    let name = std::cell::OnceCell::new();
    let name = || name.get_or_init(|| name_of(node)).as_deref();
    let is = |roles: &[Role]| role.is_some_and(|role| roles.contains(&role));

    rules.check(interactive && interactivity.is_some(), NoRole, || {
        role.is_none()
    });
    let pictured = matches!(node, Node::Image { .. } | Node::Svg { .. });
    rules.check(
        ((interactive || pictured) && role.is_some()) || matches!(node, Node::Overlay { .. }),
        Unnamed,
        || name().is_none(),
    );
    rules.check(
        matches!(node, Node::Input { .. } | Node::Editor { .. }),
        UnlabeledInput,
        || name().is_none(),
    );
    rules.check(interactive && name().is_some(), GlyphName, || {
        let name = name().unwrap_or_default();
        !name.chars().any(char::is_alphanumeric)
            || role.is_some_and(|role| name.eq_ignore_ascii_case(&format!("{role:?}")))
    });
    if let Some(i) = interactivity {
        let aria = &i.aria;
        rules.check(*aria != Aria::default(), OrphanAria, || role.is_none());
        let state = match role {
            _ if is(&TOGGLE) => Some(aria.toggled.is_some()),
            Some(Role::Tab | Role::TreeItem | Role::ListBoxOption) => Some(aria.selected.is_some()),
            Some(Role::ComboBox | Role::EditableComboBox) => Some(aria.expanded.is_some()),
            Some(Role::Heading) => Some(aria.level.is_some()),
            _ => None,
        };
        rules.check(state.is_some(), MissingState, || state == Some(false));
        // a roleless node's state is OrphanAria's
        rules.check(role.is_some(), UnreadState, || {
            (aria.selected.is_some() && !is(&SELECTABLE))
                || (aria.toggled.is_some() && !is(&TOGGLE) && role != Some(Role::Button))
        });
        rules.check(aria.disabled == Some(true), DisabledButLive, || {
            i.on_click.is_some() || i.on_key_down.is_some()
        });
        // as the sanitizer keeps it: a focusable claim is dropped and
        // spends nothing
        rules.check(aria.active_descendant, ActiveDescendant, || {
            i.focusable || i.focus_handle.is_some() || std::mem::replace(claimed, true)
        });
    }
    rules.check(interactive, NestedInteractive, || {
        step.inside(&CONTROL, false)
    });
    if let Some(i) = interactivity {
        rules.check(i.on_click.is_some(), Unreachable, || {
            !i.focusable && !step.inside(&COMPOSITE, true)
        });
    }
    let place: &[Role] = match role {
        _ if is(&MENU_ITEM) => &[Role::Menu, Role::MenuBar],
        Some(Role::Tab) => &[Role::TabList],
        Some(Role::RadioButton) => &[Role::RadioGroup],
        Some(Role::ListBoxOption) => &[Role::ListBox],
        Some(Role::TreeItem) => &[Role::Tree],
        Some(Role::Row | Role::Cell) => &[Role::Table, Role::Grid],
        _ => &[],
    };
    rules.check(!place.is_empty(), Orphan, || !step.inside(place, false));
    if let Node::RichText {
        text,
        clickable_ranges,
        ..
    } = node
    {
        rules.check(!clickable_ranges.is_empty(), RangeUnnamed, || {
            clickable_ranges.iter().any(|range| {
                text.get(range.clone())
                    .is_none_or(|words| !words.chars().any(char::is_alphanumeric))
            })
        });
    }
    if let Node::ResizeHandle { interactivity, .. } = node {
        rules.check(true, BareHandle, || {
            role != Some(Role::Splitter)
                || name().is_none()
                || !interactivity.focusable
                || interactivity.on_key_down.is_none()
        });
    }
    if let Some(aria) = interactivity.map(|i| &i.aria) {
        let has = |action| {
            aria.actions
                .iter()
                .any(|(advertised, _)| *advertised == action)
        };
        let custom = !aria.custom_actions.is_empty();
        let steps = has(Action::Increment) || has(Action::Decrement);
        let expands = has(Action::Expand) || has(Action::Collapse);
        let dropped = aria.actions.iter().enumerate().any(|(index, (action, _))| {
            HOST_ACTIONS.contains(action)
                || aria.actions[..index]
                    .iter()
                    .any(|(earlier, _)| earlier == action)
        });
        rules.check(custom || !aria.actions.is_empty(), ActionUnhandled, || {
            (custom && !has(Action::CustomAction))
                || (steps && aria.numeric_value.is_none())
                || (expands && aria.expanded.is_none())
                || dropped
        });
    }
    rules.check(duplicate, DuplicateKey, || true);
    if let Node::Input { options, .. } = node {
        rules.check(options.invalid.is_some(), ErrorNoText, || {
            blank(options.description.as_deref())
        });
    }
    if let Some(aria) = interactivity.map(|i| &i.aria) {
        rules.check(is(&[Role::Status, Role::Alert]), StatusNotLive, || {
            aria.live.is_none()
                || (blank(aria.value.as_deref()) && descendant_text(node).is_empty())
        });
        rules.check(
            aria.invalid.is_some() && is(&TEXT_INPUT),
            ErrorNoText,
            || blank(aria.description.as_deref()) && aria.error_message.is_none(),
        );
    }
    rules
}

/// The six variants that carry listener routes, focus and aria.
fn interactivity(node: &Node) -> Option<&Interactivity> {
    match node {
        Node::Container(ContainerNode { interactivity, .. })
        | Node::UniformList { interactivity, .. }
        | Node::List { interactivity, .. }
        | Node::ResizeHandle { interactivity, .. }
        | Node::Image { interactivity, .. }
        | Node::Svg { interactivity, .. } => Some(interactivity),
        _ => None,
    }
}

fn interactive(node: &Node) -> bool {
    match node {
        Node::Input { .. } | Node::Editor { .. } => true,
        Node::RichText {
            clickable_ranges, ..
        } => !clickable_ranges.is_empty(),
        _ => interactivity(node)
            .is_some_and(|i| pressed(i) || i.focusable || !i.aria.actions.is_empty()),
    }
}

/// A route a press or a key reaches.
fn pressed(i: &Interactivity) -> bool {
    [
        i.on_click,
        i.on_aux_click,
        i.on_mouse_down,
        i.capture_mouse_down,
        i.on_key_down,
        i.capture_key_down,
        i.on_key_up,
        i.capture_key_up,
    ]
    .iter()
    .any(Option::is_some)
}

/// What the node is called, trimmed; `None` for nothing.
fn name_of(node: &Node) -> Option<String> {
    fn trimmed(text: &str) -> Option<&str> {
        Some(text.trim()).filter(|text| !text.is_empty())
    }
    let own = match node {
        Node::Container(_) => None,
        Node::Image { label, .. }
        | Node::Svg { label, .. }
        | Node::Overlay { label, .. }
        | Node::Editor { label, .. } => label.as_deref().and_then(trimmed),
        Node::Input { options, .. } => trimmed(&options.label),
        _ => None,
    };
    interactivity(node)
        .and_then(|i| i.aria.label.as_deref().and_then(trimmed))
        .or(own)
        .map(str::to_owned)
        .or_else(|| {
            matches!(node, Node::Container(_))
                .then(|| descendant_text(node))
                .filter(|text| !text.is_empty())
        })
}

/// Every [`Node::Text`] under the node, trimmed, joined by spaces: what
/// names a container drawn with text and no label (`#` + `general` is
/// "# general").
fn descendant_text(node: &Node) -> String {
    fn gather<'a>(node: &'a Node, words: &mut Vec<&'a str>) {
        match node {
            Node::Text(TextNode { content, .. }) if !content.trim().is_empty() => {
                words.push(content.trim())
            }
            _ => node
                .children()
                .iter()
                .for_each(|child| gather(child, words)),
        }
    }
    let mut words = Vec::new();
    gather(node, &mut words);
    words.join(" ")
}

fn blank(text: Option<&str>) -> bool {
    text.is_none_or(|text| text.trim().is_empty())
}

#[cfg(test)]
mod tests;
