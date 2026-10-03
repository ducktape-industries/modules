use super::*;

// -------------------------------------------------------- bound assertions

/// The nesting depth `sanitize` would count for this node (root is 0, each
/// container-like or one-child node adds one) — the same metric
/// `decode`'s own depth budget counts, so it doubles as "was this tree
/// really over the budget" evidence when `decode` refuses one.
pub(super) fn tree_depth(node: &Node) -> usize {
    match node {
        Node::Sensor { child: content, .. }
        | Node::ResizeHandle { content, .. }
        | Node::Deferred { content, .. } => 1 + tree_depth(content),
        Node::Container(view_wire::ContainerNode { children, .. })
        | Node::Anchored { children, .. }
        | Node::Image {
            state_children: children,
            ..
        }
        | Node::Overlay { children, .. } => 1 + children.iter().map(tree_depth).max().unwrap_or(0),
        _ => 0,
    }
}

pub(super) fn check_string(text: &str, ctx: &str, field: &str) {
    assert!(
        text.len() <= MAX_STRING_BYTES,
        "{ctx}: {field} is {} bytes, over MAX_STRING_BYTES",
        text.len()
    );
    assert!(
        text.is_char_boundary(text.len()),
        "{ctx}: {field} does not end on a char boundary"
    );
}

/// Walks a sanitized tree asserting every post-condition `sanitize_node`
/// promises: depth within `MAX_DEPTH`, every string within
/// `MAX_STRING_BYTES` and on a char boundary, every key unique across the
/// whole tree, every size/colour/border field inside its own bound, and the
/// picture bytes the tree carries summed into `svg_bytes`.
pub(super) fn check_bounds(node: &Node, depth: usize, svg_bytes: &mut usize, ctx: &str) {
    assert!(
        depth <= MAX_DEPTH,
        "{ctx}: a node sits at depth {depth}, over MAX_DEPTH"
    );
    if let Some(id) = node.identity() {
        id.validate_host()
            .expect("sanitized typed identity is portable and bounded");
    }
    let mut sibling_ids = HashSet::new();
    for child in node.children() {
        if let Some(id) = child.identity() {
            assert!(
                sibling_ids.insert(id),
                "{ctx}: typed sibling identity aliases state"
            );
        }
    }
    match node {
        Node::Container(view_wire::ContainerNode {
            style,
            interactivity,
            children,
            ..
        }) => {
            check_native_style(style);
            for conditional in [
                interactivity.hover.as_ref(),
                interactivity.active.as_ref(),
                interactivity.group_hover.as_ref().map(|group| &group.style),
                interactivity
                    .group_active
                    .as_ref()
                    .map(|group| &group.style),
            ]
            .into_iter()
            .flatten()
            {
                check_native_style(conditional);
            }
            for child in children {
                check_bounds(child, depth + 1, svg_bytes, ctx);
            }
        }
        Node::UniformList { children, .. } => {
            for child in children {
                check_bounds(child, depth + 1, svg_bytes, ctx);
            }
        }
        Node::List {
            path,
            item_count,
            overdraw,
            commands,
            range_start,
            children,
            ..
        } => {
            assert!(path.len() <= view_wire::MAX_DEPTH);
            assert!(*item_count <= view_wire::MAX_LIST_ITEMS);
            assert!(overdraw.is_finite() && (0.0..=4096.0).contains(overdraw));
            assert!(commands.len() <= view_wire::MAX_LIST_COMMANDS);
            assert!(children.len() <= view_wire::MAX_LIST_ROWS);
            assert!(*range_start <= *item_count);
            assert!(children.len() <= item_count.saturating_sub(*range_start));
            for child in children {
                check_bounds(child, depth + 1, svg_bytes, ctx);
            }
        }
        Node::Sensor { style, child, .. } => {
            check_native_style(style);
            check_bounds(child, depth + 1, svg_bytes, ctx);
        }
        Node::ResizeHandle { content, .. } => {
            check_bounds(content, depth + 1, svg_bytes, ctx);
        }
        Node::RichText {
            text,
            runs,
            font_family_overrides,
            clickable_ranges,
            ..
        } => {
            check_string(text, ctx, "rich text");
            let valid = |range: &std::ops::Range<usize>| {
                range.start <= range.end
                    && range.end <= text.len()
                    && text.is_char_boundary(range.start)
                    && text.is_char_boundary(range.end)
            };
            match runs {
                view_wire::RichTextRuns::Highlights(highlights) => {
                    assert!(highlights.iter().all(|(range, _)| valid(range)));
                }
                view_wire::RichTextRuns::Runs(runs) => {
                    assert_eq!(runs.iter().map(|run| run.len).sum::<usize>(), text.len());
                }
            }
            assert!(font_family_overrides.iter().all(|(range, _)| valid(range)));
            assert!(clickable_ranges.iter().all(valid));
        }
        Node::Text(view_wire::TextNode { content, style, .. }) => {
            check_native_style(style);
            check_string(content, ctx, "text content");
        }
        Node::Image {
            data,
            label,
            style,
            state_children,
            ..
        } => {
            if let Some(data) = data {
                *svg_bytes += data.byte_len();
                assert!(data.valid_rgba(), "{ctx}: invalid RGBA");
            }
            if let Some(label) = label {
                check_string(label, ctx, "image label");
            }
            check_native_style(style);
            for child in state_children {
                check_bounds(child, depth + 1, svg_bytes, ctx);
            }
        }
        Node::Svg {
            source,
            transformation,
            label,
            style,
            interactivity,
            ..
        } => {
            if let SvgSource::Data { bytes, .. } = source {
                *svg_bytes += bytes.as_ref().map_or(0, Vec::len);
            }
            if let Some(label) = label {
                check_string(label, ctx, "picture label");
            }
            check_native_style(style);
            if let Some(hover) = &interactivity.hover {
                check_native_style(hover);
            }
            for value in transformation
                .scale
                .into_iter()
                .chain(transformation.translate)
                .chain([transformation.rotate])
            {
                assert!(value.is_finite() && (-MAX_PIXELS..=MAX_PIXELS).contains(&value));
            }
        }
        Node::Anchored {
            position,
            offset,
            children,
            ..
        } => {
            for value in position.iter().chain(offset).flatten() {
                assert!(value.is_finite() && (-MAX_PIXELS..=MAX_PIXELS).contains(value));
            }
            for child in children {
                check_bounds(child, depth + 1, svg_bytes, ctx);
            }
        }
        Node::Deferred { priority, content } => {
            assert!(*priority <= 16);
            check_bounds(content, depth + 1, svg_bytes, ctx);
        }
        Node::Field {
            id,
            options,
            placeholder,
            value,
            cursor,
            tokens,
            claims,
            ..
        } => {
            assert!(id.validate_host().is_ok(), "{ctx}: invalid field identity");
            check_string(placeholder, ctx, "placeholder");
            check_string(&options.label, ctx, "field label");
            if let Some(value) = &options.description {
                check_string(value, ctx, "field description");
            }
            // The value is the engine's: sanitize keeps a valid one whole,
            // and never spends the display budget on it.
            assert_eq!(
                view_wire::validate_field(value, *cursor, tokens, claims),
                Ok(()),
                "{ctx}: sanitize kept an invalid field"
            );
        }
        Node::Space { style } => check_native_style(style),
        Node::Overlay {
            label, children, ..
        } => {
            if let Some(label) = label {
                check_string(label, ctx, "accessible label");
            }
            assert!(children.len() <= 2, "{ctx}: overlay child count");
            for child in children {
                check_bounds(child, depth + 1, svg_bytes, ctx);
            }
        }
        Node::Canvas { style, commands } => {
            check_native_style(style);
            assert!(
                commands.len() <= view_wire::MAX_CANVAS_PARTS,
                "{ctx}: canvas command budget"
            );
        }
    }
}

/// Every field's text in the tree, in one fixed walk order, so the same
/// tree before and after `sanitize` compares element for element.
pub(super) fn field_texts(root: &Node) -> Vec<String> {
    let mut pending = vec![root];
    let mut texts = Vec::new();
    while let Some(node) = pending.pop() {
        if let Node::Field { value, .. } = node {
            texts.push(value.clone());
        }
        pending.extend(node.children());
    }
    texts
}

/// Every post-condition `sanitize` promises about a whole frame: the tree's
/// node count and every bound `check_bounds` covers, plus every request's
/// `kind`.
pub(super) fn check_frame(frame: &Frame, ctx: &str) {
    if let Some(root) = &frame.root {
        assert!(
            !has_duplicate_typed_siblings(root),
            "{ctx}: authored identity scope aliases state"
        );
    }

    if let Some(root) = &frame.root {
        assert!(
            root.count() <= MAX_NODES,
            "{ctx}: {} nodes, over MAX_NODES",
            root.count()
        );
        let mut svg_bytes = 0;
        check_bounds(root, 0, &mut svg_bytes, ctx);
        assert!(
            svg_bytes <= MAX_PICTURE_BYTES_PER_FRAME,
            "{ctx}: {svg_bytes} picture bytes, over MAX_PICTURE_BYTES_PER_FRAME"
        );
    }
    for request in &frame.requests {
        check_string(&request.kind, ctx, "request kind");
    }
}

pub(super) fn has_duplicate_typed_siblings(node: &Node) -> bool {
    fn walk<'a>(node: &'a Node, scope: &mut HashSet<&'a ElementIdWire>) -> bool {
        if let Some(id) = node.identity() {
            if !scope.insert(id) {
                return true;
            }
            let mut child_scope = HashSet::new();
            node.children()
                .iter()
                .any(|child| walk(child, &mut child_scope))
        } else {
            node.children().iter().any(|child| walk(child, scope))
        }
    }
    walk(node, &mut HashSet::new())
}
