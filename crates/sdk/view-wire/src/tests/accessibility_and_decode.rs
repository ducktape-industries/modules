use super::*;

/// A sensor recurses like a container and is diffed as a node with one
/// fixed child.
#[test]
fn a_sensor_round_trips_and_diffs_by_props() {
    let frame = Frame {
        root: Some(sensor("App/m", Some(0), text("inside"))),
        ..Frame::default()
    };
    assert_eq!(decode::<Frame>(&encode(&frame)).unwrap(), frame);
    assert_eq!(frame.root.as_ref().unwrap().count(), 2);

    // A changed route index is a `Props` patch that keeps the child.
    let mut old = sensor("App/m", Some(0), text("inside"));
    let mut new = sensor("App/m", Some(7), text("inside"));
    let patches = diff(&mut old, &mut new);
    assert!(
        matches!(patches.as_slice(), [Patch::Props { path, .. }] if path.is_empty()),
        "{patches:?}"
    );
    apply(&mut old, patches, &held()).unwrap();
    assert_eq!(old, new);
}

/// A tree nested past `MAX_DEPTH` is refused as the bytes are read, before
/// a level of it is built. The bug this guards: a chain of a few thousand
/// containers is a frame of ~100 KB — far inside any byte cap a host sets
/// — and decoding it walked a host thread off its stack, aborting the
/// process. A refusal is a message in one app's window; an overflow is
/// every window gone.
#[test]
fn a_tree_the_host_would_not_walk_is_refused_before_it_is_built() {
    assert!(decode::<Frame>(&deep_chain_bytes(MAX_DEPTH - 1)).is_ok());
    let refused = decode::<Frame>(&deep_chain_bytes(MAX_DEPTH + 1)).unwrap_err();
    assert!(
        refused.contains("deeper than the host renders"),
        "{refused}"
    );
    let bytes = deep_chain_bytes(5_000);
    assert!(bytes.len() < 1 << 20, "{} bytes", bytes.len());
    assert!(decode::<Frame>(&bytes).is_err());
}

/// Every lower, clone, diff, move and decode level pays for a node's
/// inline bytes, so the rarely set parts sit behind a pointer as gpui keeps
/// them: a node's interactivity and the conditional styles inside it. Inline
/// they made a node 6,112 bytes.
#[test]
fn a_node_keeps_its_rarely_set_parts_behind_a_pointer() {
    assert!(
        std::mem::size_of::<Node>() <= 1024,
        "{} bytes",
        std::mem::size_of::<Node>()
    );
}

#[test]
fn more_nodes_than_the_host_holds_is_refused() {
    let wide = column((0..MAX_DECODED_NODES + 2).map(|_| Node::empty()).collect());
    let bytes = encode(&Frame {
        root: Some(wide),
        ..Frame::default()
    });
    let refused = decode::<Frame>(&bytes).unwrap_err();
    assert!(
        refused.contains("more nodes than the host holds"),
        "{refused}"
    );
}

#[test]
fn uniform_list_path_must_match_its_typed_tree_ancestry() {
    let parent = ElementIdWire::Integer(7);
    let list = ElementIdWire::Name("list".into());
    let mut valid = Frame {
        root: Some(Node::Container(crate::ContainerNode {
            id: Some(parent.clone()),
            style: StyleId(0),
            interactivity: Default::default(),
            children: vec![uniform(vec![parent.clone(), list.clone()])],
        })),
        ..Default::default()
    };
    sanitize_plain(&mut valid).unwrap();

    let Node::Container(crate::ContainerNode { children, .. }) = valid.root.as_mut().unwrap()
    else {
        unreachable!()
    };
    let Node::UniformList { path, .. } = &mut children[0] else {
        unreachable!()
    };
    *path = vec![ElementIdWire::Name("forged-parent".into()), list];
    assert_eq!(
        sanitize_plain(&mut valid).unwrap_err(),
        Refused::Invalid("uniform-list authored path is invalid")
    );
}

/// A hostile screen cannot cause quadratic identity repair or alias state.
#[test]
fn a_screen_of_one_typed_id_is_refused_in_linear_time() {
    let mut frame = Frame {
        root: Some(column(
            (0..MAX_NODES - 1).map(|_| keyed("same", "x")).collect(),
        )),
        ..Default::default()
    };
    let started = std::time::Instant::now();
    assert!(matches!(
        sanitize_plain(&mut frame),
        Err(Refused::Duplicate(_))
    ));
    if cfg!(not(debug_assertions)) {
        assert!(started.elapsed() < std::time::Duration::from_millis(200));
    }
}
