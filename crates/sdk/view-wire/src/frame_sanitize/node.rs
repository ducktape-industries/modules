use super::*;

/// One node and, within what is left of the budgets, everything under it.
/// The walk per node: its typed id claimed in its scope and checked, its
/// interactivity (and a tooltip's content) bounded, its own fields bounded
/// by [`sanitize_fields`], then its children.
pub(super) fn sanitize_node(
    node: &mut Node,
    depth: usize,
    budgets: &mut Budgets,
    identity_scopes: &mut IdentityScopes,
    authored_path: &mut Vec<ElementIdWire>,
) -> Result<(), &'static str> {
    // The caller guarantees one node of budget; a node too deep spends it
    // on the empty node that stands in for it.
    budgets.nodes -= 1;
    if depth >= MAX_DEPTH {
        *node = Node::empty();
        return Ok(());
    }
    let typed_id = node.identity().cloned();
    let typed_scope_started = claim_typed_scope(node, identity_scopes)?;
    if let Some(id) = &typed_id {
        authored_path.push(id.clone());
    }
    if let Node::Container(crate::ContainerNode { interactivity, .. })
    | Node::UniformList { interactivity, .. }
    | Node::List { interactivity, .. }
    | Node::ResizeHandle { interactivity, .. }
    | Node::Image { interactivity, .. }
    | Node::Svg { interactivity, .. } = node
    {
        sanitize_interactivity(interactivity)?;
        if let Some(tooltip) = &mut interactivity.tooltip {
            tooltip.delay_ms = tooltip.delay_ms.min(60_000);
            sanitize_tooltip_content(&mut tooltip.content, depth, budgets)?;
        }
    }
    // Every variant that carries an id (`Node::identity`) has it checked.
    if let Some(id) = &typed_id {
        id.validate_host()?;
    }
    sanitize_fields(node, depth, budgets, authored_path)?;
    sanitize_children(node, depth, budgets, identity_scopes, authored_path)?;
    finish_typed_scope(identity_scopes, typed_scope_started);
    if typed_scope_started {
        authored_path.pop();
    }
    Ok(())
}

/// A tooltip's content is a tree of its own, in a fresh identity scope, on
/// the frame's node budget; with none left it is dropped.
fn sanitize_tooltip_content(
    content: &mut Option<Box<Node>>,
    depth: usize,
    budgets: &mut Budgets,
) -> Result<(), &'static str> {
    if budgets.nodes == 0 {
        *content = None;
    } else if let Some(content) = content {
        sanitize_node(
            content,
            depth + 1,
            budgets,
            &mut vec![std::collections::HashSet::new()],
            &mut Vec::new(),
        )?;
    }
    Ok(())
}

/// A node's own fields, one arm per kind; its id is already checked.
fn sanitize_fields(
    node: &mut Node,
    depth: usize,
    budgets: &mut Budgets,
    authored_path: &[ElementIdWire],
) -> Result<(), &'static str> {
    match node {
        Node::Container(crate::ContainerNode { style, .. })
        | Node::ResizeHandle { style, .. }
        | Node::Sensor { style, .. }
        | Node::Space { style } => style_sanitize::sanitize(style),
        Node::UniformList {
            id,
            path,
            style,
            count,
            measure_index,
            scroll_request,
            indices,
            children,
            ..
        } => {
            style_sanitize::sanitize(style);
            uniform_list_path(id, path, authored_path)?;
            *count = (*count).min(MAX_UNIFORM_LIST_COUNT);
            *measure_index = (*measure_index).min(count.saturating_sub(1));
            if let Some(request) = scroll_request {
                request.offset = request.offset.min(MAX_UNIFORM_LIST_COUNT);
            }
            uniform_list_rows(*count, indices, children);
        }
        Node::List {
            path,
            item_count,
            overdraw,
            style,
            commands,
            range_start,
            children,
            ..
        } => {
            if path != authored_path {
                return Err("list authored path is invalid");
            }
            for id in path.iter() {
                id.validate_host()?;
            }
            *item_count = (*item_count).min(budgets.list_items);
            budgets.list_items -= *item_count;
            *overdraw = bounded(*overdraw).min(4096.0);
            style_sanitize::sanitize(style);
            list_commands(commands, *item_count);
            *range_start = (*range_start).min(*item_count);
            children.truncate(MAX_LIST_ROWS.min(item_count.saturating_sub(*range_start)));
        }
        Node::Overlay {
            label,
            style,
            children,
            ..
        } => {
            label.iter_mut().for_each(truncate_string);
            style_sanitize::sanitize(style);
            children.truncate(2);
        }
        Node::Canvas { style, commands } => {
            style_sanitize::sanitize(style);
            canvas::sanitize(commands, budgets);
        }
        Node::Anchored {
            fit,
            position,
            offset,
            ..
        } => {
            for point in [position, offset].into_iter().flatten() {
                for value in point {
                    *value = signed_bounded(*value);
                }
            }
            if let AnchoredFitMode::SnapToWindowWithMargin(edges) = fit {
                for edge in edges {
                    *edge = bounded(*edge);
                }
            }
        }
        Node::Deferred { priority, .. } => *priority = (*priority).min(16),
        Node::RichText {
            style,
            text,
            runs,
            font_family_overrides,
            clickable_ranges,
            tooltip,
            ..
        } => {
            style_sanitize::sanitize(style);
            rich_text::sanitize(text, runs, font_family_overrides, clickable_ranges, budgets);
            if let Some(tooltip) = tooltip {
                sanitize_tooltip_content(&mut tooltip.content, depth, budgets)?;
            }
        }
        Node::Text(crate::TextNode { style, content, .. }) => {
            style_sanitize::sanitize(style);
            spend_text(content, budgets);
        }
        Node::Image {
            data,
            label,
            style,
            loading,
            fallback,
            state_children,
            ..
        } => {
            ImageData::sanitize(data, budgets);
            style_sanitize::sanitize(style);
            if let Some(label) = label {
                spend_text(label, budgets);
            }
            let expected = usize::from(*loading) + usize::from(*fallback);
            state_children.truncate(expected);
            if state_children.len() < expected {
                *loading = false;
                *fallback = false;
                state_children.clear();
            }
        }
        Node::Svg {
            source,
            transformation,
            label,
            style,
            ..
        } => {
            match source {
                SvgSource::Data { bytes, .. } => spend_svg(bytes, budgets),
                SvgSource::Asset(path) | SvgSource::External(path) => truncate_string(path),
                SvgSource::None => {}
            }
            for value in &mut transformation.scale {
                *value = signed_bounded(*value);
            }
            for value in &mut transformation.translate {
                *value = signed_bounded(*value);
            }
            transformation.rotate = signed_bounded(transformation.rotate);
            style_sanitize::sanitize(style);
            if let Some(label) = label {
                spend_text(label, budgets);
            }
        }
        Node::Input {
            placeholder,
            value,
            options,
            style,
            ..
        } => {
            spend_text(placeholder, budgets);
            spend_text(value, budgets);
            spend_text(&mut options.label, budgets);
            if let Some(description) = &mut options.description {
                spend_text(description, budgets);
            }
            style_sanitize::sanitize(style);
        }
        Node::Editor {
            options,
            style,
            placeholder,
            label,
            ..
        } => {
            style_sanitize::sanitize(style);
            if let Some(presentation) = &mut options.presentation {
                presentation.sanitize();
            }
            spend_text(placeholder, budgets);
            if let Some(label) = label {
                spend_text(label, budgets);
            }
        }
    }
    Ok(())
}

/// A uniform list names the path of ids that authored it, ending in its own,
/// and it must be the path the walk actually took to reach it.
fn uniform_list_path(
    id: &ElementIdWire,
    path: &[ElementIdWire],
    authored_path: &[ElementIdWire],
) -> Result<(), &'static str> {
    if path.is_empty() || path.len() > 64 || path.last() != Some(id) || path != authored_path {
        return Err("uniform-list authored path is invalid");
    }
    for ancestor in path {
        ancestor.validate_host()?;
    }
    Ok(())
}

/// Keeps the rows whose index is inside `count`, at most
/// [`MAX_UNIFORM_LIST_ROWS`] of them.
fn uniform_list_rows(count: usize, indices: &mut Vec<u32>, children: &mut Vec<Node>) {
    let mut kept_indices = Vec::with_capacity(indices.len().min(MAX_UNIFORM_LIST_ROWS));
    let mut kept_children = Vec::with_capacity(children.len().min(MAX_UNIFORM_LIST_ROWS));
    for (index, child) in indices.drain(..).zip(children.drain(..)) {
        if (index as usize) < count && kept_indices.len() < MAX_UNIFORM_LIST_ROWS {
            kept_indices.push(index);
            kept_children.push(child);
        }
    }
    *indices = kept_indices;
    *children = kept_children;
}

/// A list's commands, at most [`MAX_LIST_COMMANDS`], each held inside the
/// list's (already bounded) item count.
fn list_commands(commands: &mut Vec<ListCommand>, item_count: usize) {
    commands.truncate(MAX_LIST_COMMANDS);
    for command in commands {
        match command {
            ListCommand::Reset { count } => *count = (*count).min(MAX_LIST_ITEMS),
            ListCommand::Splice { start, end, count } => {
                *start = (*start).min(MAX_LIST_ITEMS);
                *end = (*end).clamp(*start, MAX_LIST_ITEMS);
                *count = (*count).min(MAX_LIST_ITEMS);
            }
            ListCommand::Remeasure { start, end } => {
                *start = (*start).min(item_count);
                *end = (*end).clamp(*start, item_count);
            }
            ListCommand::ScrollTo(offset) => {
                offset.item_ix = offset.item_ix.min(item_count);
                offset.offset_in_item = bounded(offset.offset_in_item);
            }
            ListCommand::ScrollToRevealItem(index) => {
                *index = (*index).min(item_count.saturating_sub(1));
            }
            ListCommand::ScrollToEnd
            | ListCommand::SetFollowMode { .. }
            | ListCommand::PauseFollowingTail => {}
        }
    }
}

/// The children, in tree order, on what is left of the node budget.
fn sanitize_children(
    node: &mut Node,
    depth: usize,
    budgets: &mut Budgets,
    identity_scopes: &mut IdentityScopes,
    authored_path: &mut Vec<ElementIdWire>,
) -> Result<(), &'static str> {
    // Children past the budget are dropped, not stood in for: a layout of
    // ten thousand rows becomes its first rows, which is what a host can
    // lay out, rather than ten thousand empty nodes it still has to walk.
    if let Node::Container(crate::ContainerNode { children, .. })
    | Node::List { children, .. }
    | Node::Overlay { children, .. }
    | Node::Anchored { children, .. }
    | Node::Image {
        state_children: children,
        ..
    } = node
    {
        let mut kept = 0;
        for child in children.iter_mut() {
            if budgets.nodes == 0 {
                break;
            }
            sanitize_node(child, depth + 1, budgets, identity_scopes, authored_path)?;
            kept += 1;
        }
        children.truncate(kept);
        if let Node::Image {
            loading,
            fallback,
            state_children,
            ..
        } = node
            && state_children.len() < usize::from(*loading) + usize::from(*fallback)
        {
            *loading = false;
            *fallback = false;
            state_children.clear();
        }
        return Ok(());
    }
    // A single slot (a wrapper's content) keeps its place as an empty node.
    for child in node.children_mut() {
        if budgets.nodes == 0 {
            *child = Node::empty();
            continue;
        }
        sanitize_node(child, depth + 1, budgets, identity_scopes, authored_path)?;
    }
    Ok(())
}
