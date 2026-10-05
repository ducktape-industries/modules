//! The host's sanitizer drops the bytes of every picture past
//! `MAX_PICTURE_BYTES_PER_FRAME` in one frame. These drive a view against
//! a host that does with each frame what the app's does — sanitize it or
//! apply its patches, then hold each picture's bytes by hash and keep the
//! tree without them — and count the frames until every picture drawn is
//! held.
use super::*;
use std::collections::HashSet;

const KIB: usize = 1 << 10;

/// One picture per size, each its own bytes.
#[derive(Default, Serialize, Deserialize)]
struct Gallery(Vec<usize>);
impl View for Gallery {
    const NAME: &'static str = "Gallery";
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self::default()
    }
}
impl Render for Gallery {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("gallery")
            .children(self.0.iter().enumerate().map(|(index, size)| {
                img(Arc::new(Image::from_bytes(
                    ImageFormat::Png,
                    vec![index as u8; *size],
                )))
            }))
    }
}

/// The app's runtime as far as pictures go.
#[derive(Default)]
struct Host {
    tree: Option<wire::Node>,
    styles: wire::Styles,
    held: HashSet<u64>,
    drawn: HashSet<u64>,
    /// Picture bytes a frame carried that the sanitizer dropped.
    dropped: usize,
}

impl Host {
    fn take(&mut self, frame: &wire::Frame) -> bool {
        let mut carried = 0;
        let mut count = |node: &wire::Node| {
            node.clone()
                .for_each_mut(&mut |node| carried += payload(node).map_or(0, |bytes| bytes.len()))
        };
        frame.root.iter().for_each(&mut count);
        frame.patches.iter().for_each(|patch| match patch {
            wire::Patch::Replace { node, .. }
            | wire::Patch::Insert { node, .. }
            | wire::Patch::Props { node, .. } => count(node),
            wire::Patch::Remove { .. } | wire::Patch::Move { .. } => {}
        });
        let mut taken = wire::Frame {
            root: frame.root.clone(),
            styles: frame.styles.clone(),
            ..Default::default()
        };
        wire::sanitize(&mut taken, &mut self.styles).expect("the host takes the frame");
        if taken.root.is_some() {
            self.tree = taken.root;
        } else if !frame.patches.is_empty() {
            let tree = self.tree.as_mut().expect("a patch frame has a tree");
            wire::apply(tree, frame.patches.clone(), &self.styles)
                .expect("the host takes the patches");
        }
        // Adopt: the bytes move into the store, the tree keeps the hash.
        let (held, drawn) = (&mut self.held, &mut self.drawn);
        let mut kept = 0;
        drawn.clear();
        if let Some(tree) = &mut self.tree {
            tree.for_each_mut(&mut |node| {
                let Some(hash) = picture_hash(node) else {
                    return;
                };
                drawn.insert(hash);
                if let Some(bytes) = take_payload(node) {
                    kept += bytes;
                    held.insert(hash);
                }
            });
        }
        self.dropped += carried - kept;
        frame.busy
    }

    fn missing(&self) -> usize {
        self.drawn.difference(&self.held).count()
    }

    /// The host evicted what it held and dropped its tree.
    fn resync(&mut self) -> Vec<wire::Event> {
        self.tree = None;
        self.held.clear();
        vec![wire::Event::Resync]
    }
}

fn picture_hash(node: &wire::Node) -> Option<u64> {
    match node {
        wire::Node::Image { hash, .. }
        | wire::Node::Svg {
            source: wire::SvgSource::Data { hash, .. },
            ..
        } => Some(*hash),
        _ => None,
    }
}

fn payload(node: &wire::Node) -> Option<&[u8]> {
    match node {
        wire::Node::Image {
            data: Some(wire::ImageData::Encoded(bytes)),
            ..
        }
        | wire::Node::Svg {
            source: wire::SvgSource::Data {
                bytes: Some(bytes), ..
            },
            ..
        } => Some(bytes),
        _ => None,
    }
}

fn take_payload(node: &mut wire::Node) -> Option<usize> {
    let len = payload(node)?.len();
    match node {
        wire::Node::Image { data, .. } => *data = None,
        wire::Node::Svg {
            source: wire::SvgSource::Data { bytes, .. },
            ..
        } => *bytes = None,
        _ => unreachable!(),
    }
    Some(len)
}

/// Ticks the way the host does — again while the frame says busy — and
/// answers how many frames it took to park.
fn frames_to_park<V: View>(
    driver: &mut Driver<V>,
    host: &mut Host,
    events: Vec<wire::Event>,
) -> usize {
    let mut events = events;
    for frames in 1..=64 {
        if !driver.tick_with(std::mem::take(&mut events), |frame| host.take(frame)) {
            return frames;
        }
    }
    panic!("the view still asks for frames after 64");
}

fn show(driver: &mut Driver<Gallery>, sizes: Vec<usize>) {
    driver.entity().update(driver.app_mut(), |view, cx| {
        view.0 = sizes;
        cx.notify();
    });
}

/// 3 MiB of pictures, two to a frame: ceil(3 MiB / 1 MiB) + 1 frames.
#[test]
fn every_picture_drawn_reaches_the_host_within_the_frame_budget() {
    let mut driver = Driver::<Gallery>::new();
    let mut host = Host::default();
    show(&mut driver, vec![384 * KIB; 8]);
    let frames = frames_to_park(&mut driver, &mut host, vec![]);
    assert_eq!(host.drawn.len(), 8);
    assert_eq!(
        host.missing(),
        0,
        "pictures drawn that the host does not hold"
    );
    assert_eq!(
        host.dropped, 0,
        "picture bytes the host's sanitizer dropped"
    );
    assert!(
        frames <= 4,
        "every picture held after {frames} frames, not 4"
    );
}

/// Pictures that came in one at a time go out again together after a
/// `Resync`, which is what the host sends when it evicts.
#[test]
fn every_picture_drawn_is_held_again_after_a_resync() {
    let mut driver = Driver::<Gallery>::new();
    let mut host = Host::default();
    let mut sizes = Vec::new();
    for _ in 0..4 {
        sizes.push(768 * KIB);
        show(&mut driver, sizes.clone());
        frames_to_park(&mut driver, &mut host, vec![]);
    }
    assert_eq!((host.drawn.len(), host.missing()), (4, 0));
    let events = host.resync();
    let frames = frames_to_park(&mut driver, &mut host, events);
    assert_eq!(host.drawn.len(), 4);
    assert_eq!(
        host.missing(),
        0,
        "pictures drawn that the host does not hold"
    );
    assert_eq!(
        host.dropped, 0,
        "picture bytes the host's sanitizer dropped"
    );
    assert!(
        frames <= 4,
        "every picture held again after {frames} frames, not 4"
    );
}

/// A picture larger than a frame's whole budget can never reach the host:
/// it goes by hash alone and is not owed, so the view parks.
#[test]
fn a_picture_past_the_frame_budget_is_never_sent_and_never_owed() {
    let mut driver = Driver::<Gallery>::new();
    let mut host = Host::default();
    show(
        &mut driver,
        vec![wire::MAX_PICTURE_BYTES_PER_FRAME + 1, 64 * KIB],
    );
    let frames = frames_to_park(&mut driver, &mut host, vec![]);
    assert_eq!(frames, 1, "nothing is owed, so the first frame parks");
    assert_eq!(
        host.dropped, 0,
        "no frame carries bytes the host drops whole"
    );
    assert_eq!(
        (host.drawn.len(), host.held.len()),
        (2, 1),
        "the small picture is held, the oversized one is not"
    );
}

/// A picture inside a kept grandchild that the frame had no budget for is
/// owed by its owner: the root's picture, lowered first, takes the budget;
/// the next frame renders the grandchild (and the cached parents above it,
/// not its sibling), and the bytes go.
#[test]
fn an_owed_picture_inside_a_kept_child_is_sent() {
    use super::cached::{Leaf, Shell};
    let mut driver = Driver::<Shell>::new();
    let mut host = Host::default();
    frames_to_park(&mut driver, &mut host, vec![]);
    let (a, b) = driver.entity().update(driver.app_mut(), |shell, _| {
        (shell.a.clone().unwrap(), shell.b.clone().unwrap())
    });
    let g = a.update(driver.app_mut(), |a, cx| {
        let g = cx.new(|_| Leaf::new("g"));
        a.inner = Some(g.clone());
        cx.notify();
        g
    });
    frames_to_park(&mut driver, &mut host, vec![]);
    let lowered = |driver: &mut Driver<Shell>, id: u64| {
        driver
            .app_mut()
            .inner
            .lowered
            .borrow()
            .get(&id)
            .copied()
            .unwrap_or(0)
    };
    let before = [a.id, b.id, g.id].map(|id| lowered(&mut driver, id));
    g.update(driver.app_mut(), |g, cx| {
        g.picture = Some(vec![1; 768 * KIB]);
        cx.notify();
    });
    driver.entity().update(driver.app_mut(), |shell, cx| {
        shell.picture = Some(vec![2; 768 * KIB]);
        cx.notify();
    });
    let busy = driver.tick_with(vec![], |frame| host.take(frame));
    assert!(busy, "a picture is owed");
    assert_eq!(host.drawn.len(), 2);
    assert_eq!(
        host.missing(),
        1,
        "the root's picture went, the grandchild's is owed"
    );
    assert_eq!(host.dropped, 0);
    let after = [a.id, b.id, g.id].map(|id| lowered(&mut driver, id));
    assert_eq!(
        after,
        [before[0] + 1, before[1], before[2] + 1],
        "g's notify reached a and the root"
    );
    let busy = driver.tick_with(vec![], |frame| host.take(frame));
    assert!(!busy, "every picture drawn is held");
    assert_eq!(host.missing(), 0);
    assert_eq!(host.dropped, 0);
    let owed = [a.id, b.id, g.id].map(|id| lowered(&mut driver, id));
    assert_eq!(
        owed,
        [after[0] + 1, after[1], after[2] + 1],
        "the owner and its parents, not the sibling"
    );
}
