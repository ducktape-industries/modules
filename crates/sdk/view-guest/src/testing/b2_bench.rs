//! hx/b2 measurement (prototype branch only): how much of a frame is style,
//! how many distinct styles it carries, and what an interning key costs.
use crate::wire::{self, Node};
use gpui::StyleRefinement;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

fn style_mut(node: &mut Node) -> Option<&mut StyleRefinement> {
    match node {
        Node::RichText { style, .. }
        | Node::UniformList { style, .. }
        | Node::List { style, .. }
        | Node::ResizeHandle { style, .. }
        | Node::Sensor { style, .. }
        | Node::Image { style, .. }
        | Node::Svg { style, .. }
        | Node::Input { style, .. }
        | Node::Editor { style, .. }
        | Node::Space { style }
        | Node::Overlay { style, .. }
        | Node::Canvas { style, .. } => Some(style),
        Node::Container(c) => Some(&mut c.style),
        Node::Text(t) => Some(&mut t.style),
        Node::Anchored { .. } | Node::Deferred { .. } => None,
    }
}

fn interactivity_mut(node: &mut Node) -> Option<&mut wire::Interactivity> {
    match node {
        Node::UniformList { interactivity, .. }
        | Node::List { interactivity, .. }
        | Node::ResizeHandle { interactivity, .. }
        | Node::Image { interactivity, .. }
        | Node::Svg { interactivity, .. } => Some(interactivity),
        Node::Container(c) => Some(&mut c.interactivity),
        _ => None,
    }
}

fn ms(f: &mut dyn FnMut()) -> f64 {
    let n = 30;
    f();
    let t = Instant::now();
    for _ in 0..n {
        f();
    }
    t.elapsed().as_secs_f64() * 1e3 / n as f64
}

/// Prints one `B2` line per aspect of `root`; `B2_DUMP=<dir>` also writes
/// the frame bytes and the distinct styles there.
pub fn b2_report(label: &str, root: &Node) {
    let frame = |root: &Node| wire::Frame {
        root: Some(root.clone()),
        ..Default::default()
    };
    let full = frame(root);
    let full_bytes = wire::encode(&full);
    let nodes = root.count();

    let mut distinct = HashSet::new();
    let mut all_styles: Vec<StyleRefinement> = Vec::new();
    let mut style_bytes = 0usize;
    let mut styled = 0usize;
    let mut cond_bytes = 0usize;
    let mut cond_distinct = HashSet::new();
    let mut cond = 0usize;
    let mut stripped = root.clone();
    stripped.for_each_mut(&mut |node| {
        if let Some(style) = style_mut(node) {
            let bytes = wire::encode(style);
            if bytes.len() > 1 {
                styled += 1;
            }
            style_bytes += bytes.len();
            distinct.insert(bytes);
            all_styles.push(std::mem::take(style));
        }
        if let Some(i) = interactivity_mut(node) {
            for s in [
                &mut i.hover,
                &mut i.active,
                &mut i.focus,
                &mut i.in_focus,
                &mut i.focus_visible,
            ] {
                if let Some(s) = s.take() {
                    cond += 1;
                    let bytes = wire::encode(&s);
                    cond_bytes += bytes.len();
                    cond_distinct.insert(bytes);
                }
            }
            for g in [&mut i.group_hover, &mut i.group_active] {
                if let Some(g) = g.take() {
                    cond += 1;
                    let bytes = wire::encode(&g.style);
                    cond_bytes += bytes.len();
                    cond_distinct.insert(bytes);
                }
            }
        }
    });
    let stripped_frame = frame(&stripped);
    let stripped_bytes = wire::encode(&stripped_frame);
    let distinct_total: usize = distinct.iter().map(Vec::len).sum();

    let t_encode = ms(&mut || {
        std::hint::black_box(wire::encode(&full));
    });
    let t_encode_stripped = ms(&mut || {
        std::hint::black_box(wire::encode(&stripped_frame));
    });
    let t_clone = ms(&mut || {
        std::hint::black_box(root.clone());
    });
    let t_clone_stripped = ms(&mut || {
        std::hint::black_box(stripped.clone());
    });
    let (mut a, mut b) = (root.clone(), root.clone());
    let t_diff_same = ms(&mut || {
        std::hint::black_box(wire::diff(&mut a, &mut b));
    });
    let (mut a, mut b) = (stripped.clone(), stripped.clone());
    let t_diff_same_stripped = ms(&mut || {
        std::hint::black_box(wire::diff(&mut a, &mut b));
    });
    let t_decode = ms(&mut || {
        std::hint::black_box(wire::decode::<wire::Frame>(&full_bytes).unwrap());
    });
    let t_decode_stripped = ms(&mut || {
        std::hint::black_box(wire::decode::<wire::Frame>(&stripped_bytes).unwrap());
    });
    // Interning keys over every style the tree carries (empties included).
    let t_key_bytes = ms(&mut || {
        let mut table: HashMap<Vec<u8>, u32> = HashMap::new();
        let mut buf = Vec::with_capacity(512);
        for style in &all_styles {
            buf.clear();
            buf.extend_from_slice(&wire::encode(style));
            if !table.contains_key(buf.as_slice()) {
                table.insert(buf.clone(), table.len() as u32);
            }
        }
        std::hint::black_box(table.len());
    });
    let t_key_scan = ms(&mut || {
        let mut table: Vec<StyleRefinement> = Vec::new();
        for style in &all_styles {
            if !table.iter().any(|s| s == style) {
                table.push(style.clone());
            }
        }
        std::hint::black_box(table.len());
    });
    let t_style_clone = ms(&mut || {
        for style in &all_styles {
            std::hint::black_box(style.clone());
        }
    });
    println!(
        "B2 {label}: nodes {nodes} bytes {} stripped {} (style share {:.1}%) | base styles: {} slots, {styled} non-empty, {} distinct ({distinct_total} B together), {style_bytes} B on the wire | conditional: {cond} refs, {} distinct, {cond_bytes} B | size_of Node {} StyleRefinement {}",
        full_bytes.len(),
        stripped_bytes.len(),
        100. * (full_bytes.len() - stripped_bytes.len()) as f64 / full_bytes.len() as f64,
        all_styles.len(),
        distinct.len(),
        cond_distinct.len(),
        std::mem::size_of::<Node>(),
        std::mem::size_of::<StyleRefinement>(),
    );
    println!(
        "B2 {label} ms: encode {t_encode:.3} / stripped {t_encode_stripped:.3} | tree clone {t_clone:.3} / stripped {t_clone_stripped:.3} | diff-same {t_diff_same:.3} / stripped {t_diff_same_stripped:.3} | decode {t_decode:.3} / stripped {t_decode_stripped:.3} | intern key: bytes {t_key_bytes:.3} scan {t_key_scan:.3} | style clones {t_style_clone:.3}"
    );
    if let Some(dir) = std::env::var_os("B2_DUMP") {
        let dir = std::path::Path::new(&dir);
        std::fs::write(dir.join(format!("{label}.frame.bin")), &full_bytes).unwrap();
        let mut sizes: Vec<&Vec<u8>> = distinct.iter().collect();
        sizes.sort();
        let lines: Vec<String> = sizes
            .iter()
            .map(|b| {
                let s: StyleRefinement = wire::decode(b).unwrap();
                format!("{} {:?}", b.len(), s)
            })
            .collect();
        std::fs::write(dir.join(format!("{label}.styles.txt")), lines.join("\n")).unwrap();
    }
}
