//! Places: where to start. The reader's home (or why there is none),
//! `/shared`, the whole tree, then the newest snapshots. Reads the reader
//! and the history; writes the route.
use super::*;
use crate::state::{Home, SHARED};

/// How many of the newest snapshots the page lists.
const LATEST: usize = 5;

pub(super) fn places(view: &Files, cx: Cx, k: &Kit) -> AnyElement {
    div()
        .id("files-places")
        .flex()
        .flex_col()
        .child(heading("files/places-head", words::PLACES, None, k))
        .child(home(view, cx, k))
        .child(place(
            "shared",
            "users",
            words::SHARED,
            SHARED,
            words::SHARED_CAPTION,
            cx,
            k,
        ))
        .child(place(
            "everything",
            "hard-drive",
            words::EVERYTHING,
            ROOT,
            words::EVERYTHING_CAPTION,
            cx,
            k,
        ))
        .child(heading("files/latest-head", words::LATEST, None, k))
        .child(latest(view, cx, k))
        .into_any_element()
}

/// The reader's home, a place like the others once identity names it.
fn home(view: &Files, cx: Cx, k: &Kit) -> AnyElement {
    let why = match view.reader.home() {
        Home::At(path) => {
            let caption = path.clone();
            return place("home", "house", words::HOME, &path, &caption, cx, k).into_any_element();
        }
        Home::SignedOut => words::SIGNED_OUT,
        Home::Asking => words::FINDING_HOME,
        Home::Unnamed => words::NO_NAME,
        Home::Refused(why) => why,
    };
    div()
        .id("files/place/home")
        .h(ROW_H)
        .flex()
        .items_center()
        .gap_3()
        .px_5()
        .border_b_1()
        .border_color(k.theme.border)
        .child(design::icon("house", k.ladder.icon_row).text_color(k.theme.faint))
        .child(div().text_color(k.theme.muted).child(words::HOME))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(design::text::SECONDARY)
                .text_color(k.theme.muted)
                .child(why.to_owned()),
        )
        .into_any_element()
}

fn place(
    key: &str,
    glyph: &'static str,
    label: &str,
    path: &str,
    caption: &str,
    cx: Cx,
    k: &Kit,
) -> Stateful<Div> {
    let route = Route::Folder(path.to_owned());
    let open =
        cx.listener(move |view: &mut Files, _: &ClickEvent, _, cx| view.open(route.clone(), cx));
    row(
        format!("files/place/{key}"),
        glyph,
        label.to_owned(),
        caption.to_owned(),
        k,
        open,
    )
}

/// The newest snapshots, each opening its page.
fn latest(view: &Files, cx: Cx, k: &Kit) -> AnyElement {
    let history = match &view.history {
        Loadable::Ready(history) | Loadable::Reloading(history, _) => history,
        Loadable::Failed(refusal) => return refused("files-history", refusal, cx, k),
        Loadable::Idle | Loadable::Loading(_) => {
            return reading("files-latest-reading", words::READING_HISTORY, k);
        }
    };
    if history.snapshots.is_empty() {
        return design::empty_state(
            "files-no-snapshots",
            words::NO_SNAPSHOTS,
            words::NO_SNAPSHOTS_DETAIL,
            &k.theme,
        )
        .into_any_element();
    }
    let rows = history
        .snapshots
        .iter()
        .take(LATEST)
        .map(|snapshot| history::snapshot_row(view, snapshot, cx, k));
    div().flex().flex_col().children(rows).into_any_element()
}
