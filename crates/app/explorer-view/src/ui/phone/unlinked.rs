//! Expo `app/(tabs)/explorer/[...rest].tsx`: a link whose path names no
//! page, over the root. Reads the note the link left (`Note::Unlinked`);
//! writes Back, which dismisses it.
use super::*;

pub(super) fn unlinked(note: &Note, cx: Cx, k: &Kit) -> AnyElement {
    gone("explorer-unlinked", "Explorer", "link-2-off", "Nothing to show", note, cx, k)
}
