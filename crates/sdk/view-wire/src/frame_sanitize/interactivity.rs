use super::*;

pub(super) fn sanitize_interactivity(
    interactivity: &mut Interactivity,
    budgets: &Budgets,
) -> Result<(), &'static str> {
    interactivity.aria.sanitize();
    interactivity.role = crate::aria::view_role(interactivity.role);
    let aria = &mut interactivity.aria;
    if interactivity.role == Some(gpui::Role::Heading)
        && aria.level.is_some_and(|level| !(1..=6).contains(&level))
    {
        aria.level = None;
    }
    // gpui panics (debug) on an active descendant that is the focused
    // node, and a tracked focus handle makes a node focusable there.
    aria.active_descendant &= !interactivity.focusable && interactivity.focus_handle.is_none();
    for style in interactivity.styles() {
        budgets.style(style)?;
    }
    // a key cut short, or one gpui cannot read, would cross and stop nothing
    let keys = &interactivity.consumes_keys;
    if keys.len() > crate::interactivity::MAX_CONSUMED_KEYS {
        return Err("too many consumed keys");
    }
    if keys.iter().any(|key| {
        key.len() > crate::interactivity::MAX_KEYSTROKE_BYTES
            || gpui::Keystroke::parse(key).is_err()
    }) {
        return Err("a consumed key gpui cannot read");
    }
    // the host stops a consumed click in the node's click listener; a node
    // with none would cross and stop nothing
    if interactivity.consumes_click && interactivity.on_click.is_none() {
        return Err("consumes a click it does not take");
    }
    for group in [
        &mut interactivity.group_hover,
        &mut interactivity.group_active,
    ]
    .into_iter()
    .flatten()
    {
        let mut name = group.group.to_string();
        truncate_string(&mut name);
        group.group = name.into();
    }
    if let Some(group) = &mut interactivity.group {
        let mut name = group.to_string();
        truncate_string(&mut name);
        *group = name.into();
    }
    Ok(())
}
