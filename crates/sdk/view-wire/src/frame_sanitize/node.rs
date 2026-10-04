use super::*;

/// One node and, within what is left of the budgets, everything under it.
/// The walk per node: its typed id claimed in its scope and checked
/// (`row` is its index when it is a row of a list: [`identity::segment`]),
/// its interactivity bounded, its own fields bounded by
/// [`sanitize_fields`], then its children.
pub(super) fn sanitize_node(
    node: &mut Node,
    depth: usize,
    budgets: &mut Budgets,
    scopes: &mut identity::Scopes,
    row: Option<usize>,
) -> Result<(), Refused> {
    // The caller guarantees one node of budget; a node too deep spends it
    // on the empty node that stands in for it.
    budgets.nodes -= 1;
    if depth >= MAX_DEPTH {
        let cut = std::mem::replace(node, Node::empty());
        budgets.cut(|cuts| &mut cuts.depth, usize::from(cut != Node::empty()));
        return Ok(());
    }
    let entered = scopes.enter(identity::segment(node.identity().cloned(), row))?;
    if let Node::Container(crate::ContainerNode { interactivity, .. })
    | Node::UniformList { interactivity, .. }
    | Node::List { interactivity, .. }
    | Node::ResizeHandle { interactivity, .. }
    | Node::Image { interactivity, .. }
    | Node::Svg { interactivity, .. } = node
    {
        sanitize_interactivity(interactivity, budgets)?;
        // gpui panics (debug) on a second claim under one focused node;
        // the first in tree order keeps it. The budget restarts under
        // every node with a role that takes focus (`sanitize_children`),
        // as gpui counts claims per nearest focusable ancestor and a
        // roleless focusable pushes no node there.
        let aria = &mut interactivity.aria;
        if aria.active_descendant {
            aria.active_descendant = !std::mem::replace(&mut budgets.active_descendant, true);
        }
        if let Some(tooltip) = &mut interactivity.tooltip {
            tooltip.delay_ms = tooltip.delay_ms.min(60_000);
        }
    }
    // Every id a node is filed under (its own, a row's index) is checked.
    if let Some(id) = entered.then(|| scopes.path().last()).flatten() {
        id.validate_host()?;
    }
    sanitize_fields(node, budgets, scopes.path())?;
    let takes_focus = node
        .interactivity()
        .is_some_and(|i| (i.focusable || i.focus_handle.is_some()) && i.role.is_some());
    let claimed_outside =
        takes_focus.then(|| std::mem::replace(&mut budgets.active_descendant, false));
    sanitize_children(node, depth, budgets, scopes)?;
    if let Some(claimed) = claimed_outside {
        budgets.active_descendant = claimed;
    }
    scopes.leave(entered);
    Ok(())
}

/// A node's own fields, one arm per kind; its id is already checked.
fn sanitize_fields(
    node: &mut Node,
    budgets: &mut Budgets,
    authored_path: &[ElementIdWire],
) -> Result<(), Refused> {
    match node {
        Node::Container(crate::ContainerNode { style, .. })
        | Node::ResizeHandle { style, .. }
        | Node::Sensor { style, .. } => budgets.style(*style)?,
        Node::Space => {}
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
            budgets.style(*style)?;
            uniform_list_path(id, path, authored_path)?;
            budgets.cut(
                |cuts| &mut cuts.lists,
                usize::from(*count > MAX_UNIFORM_LIST_COUNT),
            );
            *count = (*count).min(MAX_UNIFORM_LIST_COUNT);
            *measure_index = (*measure_index).min(count.saturating_sub(1));
            if let Some(request) = scroll_request {
                request.offset = request.offset.min(MAX_UNIFORM_LIST_COUNT);
            }
            let rows: usize = children.iter().map(Node::count).sum();
            uniform_list_rows(*count, indices, children);
            let kept: usize = children.iter().map(Node::count).sum();
            budgets.cut(|cuts| &mut cuts.nodes, rows - kept);
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
                return Err("list authored path is invalid".into());
            }
            for id in path.iter() {
                id.validate_host()?;
            }
            budgets.cut(
                |cuts| &mut cuts.lists,
                usize::from(*item_count > budgets.list_items),
            );
            *item_count = (*item_count).min(budgets.list_items);
            budgets.list_items -= *item_count;
            *overdraw = bounded(*overdraw).min(4096.0);
            budgets.style(*style)?;
            list_commands(commands, *item_count);
            *range_start = (*range_start).min(*item_count);
            cut_children(
                children,
                MAX_LIST_ROWS.min(item_count.saturating_sub(*range_start)),
                budgets,
            );
        }
        Node::Overlay {
            label,
            style,
            children,
            ..
        } => {
            if let Some(label) = label {
                cut_string(label, budgets);
            }
            budgets.style(*style)?;
            cut_children(children, 2, budgets);
        }
        Node::Canvas { style, commands } => {
            budgets.style(*style)?;
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
            ..
        } => {
            budgets.style(*style)?;
            rich_text::sanitize(text, runs, font_family_overrides, clickable_ranges, budgets);
        }
        Node::Text(crate::TextNode { style, content, .. }) => {
            budgets.style(*style)?;
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
            budgets.style(*style)?;
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
                SvgSource::Asset(path) | SvgSource::External(path) => cut_string(path, budgets),
                SvgSource::None => {}
            }
            for value in &mut transformation.scale {
                *value = signed_bounded(*value);
            }
            for value in &mut transformation.translate {
                *value = signed_bounded(*value);
            }
            transformation.rotate = signed_bounded(transformation.rotate);
            budgets.style(*style)?;
            if let Some(label) = label {
                spend_text(label, budgets);
            }
        }
        Node::Field {
            value,
            cursor,
            tokens,
            claims,
            options,
            placeholder,
            style,
            ..
        } => {
            // the value is the engine's to adopt whole: bounded, never shaped
            crate::validate_field(value, *cursor, tokens, claims)?;
            spend_text(placeholder, budgets);
            spend_text(&mut options.label, budgets);
            if let Some(description) = &mut options.description {
                spend_text(description, budgets);
            }
            budgets.style(*style)?;
        }
    }
    Ok(())
}

/// Keeps the first `keep` children and reports the rest as cut nodes.
fn cut_children(children: &mut Vec<Node>, keep: usize, budgets: &mut Budgets) {
    let dropped: usize = children.iter().skip(keep).map(Node::count).sum();
    budgets.cut(|cuts| &mut cuts.nodes, dropped);
    children.truncate(keep);
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

/// The children, in tree order, on what is left of the node budget; a
/// list's children each as its row.
fn sanitize_children(
    node: &mut Node,
    depth: usize,
    budgets: &mut Budgets,
    scopes: &mut identity::Scopes,
) -> Result<(), Refused> {
    // Children past the budget are dropped, not stood in for: a layout of
    // ten thousand rows becomes its first rows, which is what a host can
    // lay out, rather than ten thousand empty nodes it still has to walk.
    // A single slot (a wrapper's content) keeps its place as an empty
    // node, as does a uniform list's row, which its index names.
    let drops = matches!(
        node,
        Node::Container(_)
            | Node::List { .. }
            | Node::Overlay { .. }
            | Node::Anchored { .. }
            | Node::Image { .. }
    );
    let mut kept = 0;
    for child in 0..node.children().len() {
        if budgets.nodes == 0 {
            if drops {
                break;
            }
            let cut = std::mem::replace(&mut node.children_mut()[child], Node::empty());
            if cut != Node::empty() {
                budgets.at.push(child as u32);
                budgets.cut(|cuts| &mut cuts.nodes, cut.count());
                budgets.at.pop();
            }
            continue;
        }
        let row = identity::row(node, child);
        budgets.at.push(child as u32);
        sanitize_node(
            &mut node.children_mut()[child],
            depth + 1,
            budgets,
            scopes,
            row,
        )?;
        budgets.at.pop();
        kept += 1;
    }
    if !drops {
        return Ok(());
    }
    if let Some(children) = node.child_list_mut() {
        let dropped: usize = children[kept..].iter().map(Node::count).sum();
        if dropped > 0 {
            budgets.at.push(kept as u32);
            budgets.cut(|cuts| &mut cuts.nodes, dropped);
            budgets.at.pop();
        }
        children.truncate(kept);
    }
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
    Ok(())
}
