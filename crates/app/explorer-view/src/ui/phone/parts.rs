//! Expo `src/network/explorer/parts.tsx` and the `src/kit` pieces the
//! explorer's screens share: the rows (a block, a run of empty blocks, a
//! transaction), `Notice`, `Quiet`, `EmptyState`, `SectionHead`, `Facts`,
//! `KeyValue`, `Row`, `Button small`, `Avatar`, `Linked` and `CopyLink`,
//! at the Expo kit's sizes. Each draws what it is handed; a press goes
//! through the model's actions (`go`, `copy`, `read_all`).
//!
//! The Expo line numbers below are mobile `explorer-detail` b2de83e.
use super::*;
use crate::decode::{clip, short, sized};

/// A section head's least height (Expo `src/kit/Row.tsx:86`).
const SECTION_H: Pixels = px(40.);
/// A caption's line (Expo `src/design/theme.ts:47`): the box a section
/// head's link draws in.
const CAPTION_LINE: Pixels = px(16.);
/// A small text's line (Expo `src/design/theme.ts:44`): the box a link
/// draws in.
const SMALL_LINE: Pixels = px(19.);
/// A notice's action: secondary's 18 line (`theme.ts:46`) and `gap.xs`
/// over and under (Expo `src/kit/Notice.tsx:60`).
const NOTICE_ACTION_H: Pixels = px(30.);
/// An empty state's insets, over and under, and each side (Expo
/// `src/kit/States.tsx:26`).
const EMPTY_INSET_Y: Pixels = px(40.);
const EMPTY_INSET_X: Pixels = px(32.);
/// An empty state's icon (Expo `src/design/theme.ts:87` `icon.empty`).
const EMPTY_ICON: Pixels = px(28.);
/// An empty state's line length (Expo `src/kit/States.tsx:31`).
const EMPTY_LINE_W: Pixels = px(280.);
/// The room between a row's title and its caption (Expo
/// `src/kit/Row.tsx:58`).
const TITLE_GAP: Pixels = px(1.);
/// A row with a caption under its title (Expo `theme.ts` `rowTall`).
const ROW_TALL: Pixels = px(design::height::touch::ROW_TALL as f32);
/// A key's column in a key/value row (Expo `src/kit/Facts.tsx:64`).
const KEY_W: Pixels = px(96.);
/// A small button (Expo `src/design/theme.ts:76` `size.small`).
const SMALL_BUTTON_H: Pixels = px(36.);
/// A transaction's outcome mark, each way (Expo
/// `src/network/explorer/parts.tsx:139`).
const MARK: Pixels = px(8.);
/// A transaction row's program, before its title (Expo
/// `src/network/explorer/parts.tsx:151`).
const TARGET_TEXT: Pixels = px(12.);

/// A press handed to a piece that draws it.
pub(super) type Press = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

// ---------- pages ----------

/// A page's scroller (Expo `pages.tsx` `Page`): what is under the title
/// row and the notice, its foot `gap.block` clear.
pub(super) fn scroller(id: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .flex_1()
        .min_h(px(0.))
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .pb(space::BLOCK)
}

/// A page over the root (Expo `Header back`): its title row with Back,
/// and the notice under it.
pub(super) fn over(
    id: &'static str,
    title: impl Into<SharedString>,
    view: &Explorer,
    cx: Cx,
    k: &Kit,
) -> design::Page {
    let page = design::page(id, title, Some(back(cx)), &k.ladder, &k.theme);
    match note(view, cx, k) {
        Some(note) => page.child(note),
        None => page,
    }
}

/// Copy link in a page's title row (Expo `CopyLink`): absent while the
/// session names no chain to mint the link in.
pub(super) fn copy_link(view: &Explorer, route: &Route, cx: Cx, k: &Kit) -> Option<AnyElement> {
    let link = view.link(route)?;
    let copy = cx.listener(move |view: &mut Explorer, _: &ClickEvent, _, cx| {
        view.copy(link.clone(), "the link", cx)
    });
    Some(
        design::icon_button(
            "explorer-copy-link",
            design::icon("link", k.ladder.icon),
            "Copy link",
            &k.ladder,
            &k.theme,
            copy,
        )
        .into_any_element(),
    )
}

// ---------- states ----------

/// A notice's tone (Expo `Notice kind`).
#[derive(Clone, Copy)]
pub(super) enum Tone {
    Quiet,
    Success,
    Danger,
}

/// What a search, a link, a copy or a write came to, under the title row
/// (Expo `Notice` with Dismiss): a write that went through in success,
/// one that did not in danger, the rest quiet.
pub(super) fn note(view: &Explorer, cx: Cx, k: &Kit) -> Option<Stateful<Div>> {
    let note = view.note.as_ref()?;
    let tone = match note {
        Note::Sent(_) => Tone::Success,
        Note::Refused(_) => Tone::Danger,
        Note::NotFound(_) | Note::NoSuchHash { .. } | Note::Unlinked(_) | Note::Copied(_) => {
            Tone::Quiet
        }
    };
    let dismiss = cx.listener(|view: &mut Explorer, _: &ClickEvent, _, cx| {
        view.note = None;
        cx.notify();
    });
    Some(notice(
        "explorer-note",
        worded(note),
        tone,
        Some(("Dismiss", Box::new(dismiss))),
        k,
    ))
}

/// A strip above content (Expo `Notice`): its words in the tone on the
/// tone's soft ground, `gap.md` over and under, the gutter in; one action
/// at the right, its press reaching past it to `ladder.hit`.
pub(super) fn notice(
    id: &'static str,
    text: impl Into<SharedString>,
    tone: Tone,
    action: Option<(&'static str, Press)>,
    k: &Kit,
) -> Stateful<Div> {
    let (theme, ladder) = (&k.theme, &k.ladder);
    let (ground, ink) = match tone {
        Tone::Quiet => (theme.surface, theme.muted),
        Tone::Success => (theme.success_soft, theme.success),
        Tone::Danger => (theme.danger_soft, theme.danger),
    };
    let action = action.map(|(label, press)| {
        div()
            .relative()
            .flex_none()
            .h(NOTICE_ACTION_H)
            .flex()
            .items_center()
            .pl(space::SM)
            .text_type(ladder.secondary)
            .font_weight(FontWeight::MEDIUM)
            .text_color(ink)
            .child(label)
            .child(
                design::hit(
                    SharedString::from(format!("{id}-{}", label.to_lowercase())),
                    ladder.hit,
                    NOTICE_ACTION_H,
                    ladder,
                )
                .role(Role::Button)
                .aria_label(label)
                .focusable()
                .on_click(press),
            )
    });
    div()
        .id(id)
        .flex_none()
        .flex()
        .items_center()
        .gap(space::MD)
        .py(space::MD)
        .px(space::BLOCK)
        .bg(ground)
        .when(matches!(tone, Tone::Quiet), |strip| {
            strip.border_b_1().border_color(theme.border)
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_type(ladder.secondary)
                .text_color(ink)
                .child(text.into()),
        )
        .children(action)
}

/// A refused read (Expo `Refused`): its sentence as a danger notice, and
/// Retry, which reads the view's data again.
pub(super) fn refused(sentence: &str, cx: Cx, k: &Kit) -> AnyElement {
    let retry = cx.listener(|view: &mut Explorer, _: &ClickEvent, _, cx| view.read_all(cx));
    notice(
        "explorer-refused",
        sentence.to_owned(),
        Tone::Danger,
        Some(("Retry", Box::new(retry))),
        k,
    )
    .into_any_element()
}

/// One muted status line, the gutter in (Expo `Quiet`).
pub(super) fn quiet(id: &'static str, text: impl Into<SharedString>, k: &Kit) -> AnyElement {
    design::quiet(text, &k.theme)
        .id(id)
        .text_type(k.ladder.secondary)
        .p(space::BLOCK)
        .into_any_element()
}

/// Nothing to show (Expo `EmptyState`): its icon, what is missing, why.
pub(super) fn empty(
    id: &'static str,
    icon: &str,
    title: impl Into<SharedString>,
    body: impl Into<SharedString>,
    k: &Kit,
) -> AnyElement {
    div()
        .id(id)
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(space::SM)
        .py(EMPTY_INSET_Y)
        .px(EMPTY_INSET_X)
        .child(design::icon(icon, EMPTY_ICON).text_color(k.theme.faint))
        .child(
            div()
                .text_type(k.ladder.body)
                .font_weight(FontWeight::MEDIUM)
                .text_center()
                .child(title.into()),
        )
        .child(
            div()
                .max_w(EMPTY_LINE_W)
                .text_type(k.ladder.secondary)
                .text_color(k.theme.muted)
                .text_center()
                .child(body.into()),
        )
        .into_any_element()
}

// ---------- sections and facts ----------

/// A section's head (Expo `SectionHead`): its label in caption medium, and
/// what sits beside it (a count, a sentence, a link).
pub(super) fn section(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    aside: Option<AnyElement>,
    k: &Kit,
) -> Stateful<Div> {
    let id = id.into();
    div()
        .id(id.clone())
        .flex_none()
        .min_h(SECTION_H)
        .flex()
        .items_center()
        .justify_between()
        .gap(space::SM)
        .pt(space::LG)
        .pb(space::XXS)
        .px(space::BLOCK)
        .text_type(k.ladder.caption)
        .text_color(k.theme.muted)
        .child(
            div()
                .id(SharedString::from(format!("{id}-title")))
                .font_weight(FontWeight::MEDIUM)
                .role(Role::Heading)
                .aria_level(2)
                .child(label.into()),
        )
        .children(aside)
}

/// Words beside a section head's label, cut short where they run out.
pub(super) fn aside(text: impl Into<SharedString>) -> AnyElement {
    div().truncate().child(text.into()).into_any_element()
}

/// A section head's link ("All blocks →", Expo `pages.tsx` `More`): caption
/// words whose press reaches past them to `ladder.hit`.
pub(super) fn more_link(
    id: &'static str,
    label: &'static str,
    route: Route,
    cx: Cx,
    k: &Kit,
) -> AnyElement {
    let go =
        cx.listener(move |view: &mut Explorer, _: &ClickEvent, _, cx| view.go(route.clone(), cx));
    div()
        .relative()
        .flex_none()
        .h(CAPTION_LINE)
        .flex()
        .items_center()
        .child(label)
        .child(
            design::hit(id, k.ladder.hit, CAPTION_LINE, &k.ladder)
                .role(Role::Link)
                .aria_label(label)
                .focusable()
                .on_click(go),
        )
        .into_any_element()
}

/// One figure of [`facts`]: an eyebrow, a value, a note under it.
pub(super) struct Fact {
    /// its element's key under the facts' own
    pub(super) key: &'static str,
    pub(super) label: &'static str,
    pub(super) value: String,
    pub(super) caption: Option<String>,
}

/// Two columns of figures (Expo `Facts`): half the width each, hairlines
/// between.
pub(super) fn facts(id: &'static str, facts: Vec<Fact>, k: &Kit) -> Stateful<Div> {
    div().id(id).flex_none().flex().flex_wrap().children(
        facts
            .into_iter()
            .enumerate()
            .map(|(index, fact)| figure(id, index, fact, k)),
    )
}

/// One figure: its eyebrow in the data face, uppercase, its value in the
/// heading role, its note; the left column draws the hairline between.
fn figure(id: &str, index: usize, fact: Fact, k: &Kit) -> Stateful<Div> {
    let (theme, ladder) = (&k.theme, &k.ladder);
    let left = index.is_multiple_of(2);
    div()
        .id(SharedString::from(format!("{id}-{}", fact.key)))
        .w_1_2()
        .flex()
        .flex_col()
        .gap(space::HAIR)
        .py(space::LG)
        .px(space::BLOCK)
        .border_b_1()
        .when(left, |figure| figure.border_r_1())
        .border_color(theme.border)
        .child(
            mono(fact.label.to_uppercase())
                .text_type(ladder.tiny)
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.muted)
                .mb(space::XXS),
        )
        .child(
            div()
                .text_type(ladder.heading)
                .font_weight(FontWeight::MEDIUM)
                .child(fact.value),
        )
        .children(fact.caption.map(|caption| {
            div()
                .text_type(ladder.caption)
                .text_color(theme.muted)
                .child(caption)
        }))
}

/// A label and its value (Expo `KeyValue`): the label small and muted in
/// its column, the value small, wrapping, level with it. Its key is
/// `explorer-field-<label>`.
pub(super) fn key_value(label: &str, value: impl IntoElement, k: &Kit) -> Stateful<Div> {
    div()
        .id(SharedString::from(format!(
            "explorer-field-{}",
            label.to_lowercase().replace(' ', "-")
        )))
        .flex_none()
        .min_h(k.ladder.control)
        .flex()
        .items_start()
        .gap(space::LG)
        .py(space::MD)
        .px(space::BLOCK)
        .border_b_1()
        .border_color(k.theme.border)
        .child(
            div()
                .w(KEY_W)
                .flex_none()
                .text_type(k.ladder.small)
                .text_color(k.theme.muted)
                .child(label.to_owned()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_type(k.ladder.small)
                .child(value),
        )
}

/// Text in the data face that wraps where it must: a 64-digit hash.
pub(super) fn data(text: impl Into<SharedString>) -> Div {
    div()
        .font_family(design::fonts::FAMILY_MONO)
        .child(text.into())
}

/// A date in the data face, and how long ago it was, faint (Expo's Time
/// rows).
pub(super) fn time_and_age(time: u64, now: u64, k: &Kit) -> Div {
    div()
        .flex()
        .flex_wrap()
        .child(data(format!("{} ", date(time))))
        .child(data(format!("{} ago", ago(now, time))).text_color(k.theme.faint))
}

/// A link inside a page (Expo `Linked`): small, foreground, underlined; its
/// press reaches past its line to `ladder.hit`. A caller wanting the data
/// face sets it on what this returns.
pub(super) fn linked(
    id: impl Into<SharedString>,
    text: impl Into<SharedString>,
    route: Route,
    cx: Cx,
    k: &Kit,
) -> Div {
    let text = text.into();
    let go =
        cx.listener(move |view: &mut Explorer, _: &ClickEvent, _, cx| view.go(route.clone(), cx));
    div()
        .relative()
        .flex_none()
        .h(SMALL_LINE)
        .flex()
        .items_center()
        .text_type(k.ladder.small)
        .text_color(k.theme.foreground)
        .underline()
        .whitespace_nowrap()
        .child(text.clone())
        .child(
            design::hit(id.into(), k.ladder.hit, SMALL_LINE, &k.ladder)
                .role(Role::Link)
                .aria_label(text)
                .focusable()
                .on_click(go),
        )
}

// ---------- rows ----------

/// A list row with a caption under its title (Expo `Row`): the kit's frame
/// at `rowTall`.
pub(super) fn list_row(id: impl Into<ElementId>, k: &Kit) -> Stateful<Div> {
    design::row(id, &k.ladder, &k.theme).min_h(ROW_TALL)
}

/// A row whose press opens `route` (Expo `Row onPress`): a button named
/// `label`, the whole row the press.
pub(super) fn opens(
    row: Stateful<Div>,
    label: impl Into<SharedString>,
    route: Route,
    cx: Cx,
) -> Stateful<Div> {
    let go =
        cx.listener(move |view: &mut Explorer, _: &ClickEvent, _, cx| view.go(route.clone(), cx));
    row.role(Role::Button)
        .aria_label(label.into())
        .focusable()
        .on_click(go)
}

/// A row's title over its caption.
pub(super) fn title_and_caption(title: impl IntoElement, caption: impl IntoElement) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(TITLE_GAP)
        .child(title)
        .child(caption)
}

/// A caption under a row's title, muted, cut short where it runs out.
pub(super) fn caption(text: impl Into<SharedString>, k: &Kit) -> Div {
    div()
        .truncate()
        .text_type(k.ladder.caption)
        .text_color(k.theme.muted)
        .child(text.into())
}

/// The chevron a row that opens a page ends with (Expo `Row chevron`).
pub(super) fn chevron(k: &Kit) -> impl IntoElement {
    design::icon("chevron-right", k.ladder.icon_row).text_color(k.theme.faint)
}

/// What ends a row (Expo `Row tail`): a caption in the data face, muted.
pub(super) fn tail(text: impl Into<SharedString>, k: &Kit) -> Div {
    mono(text)
        .text_type(k.ladder.caption)
        .text_color(k.theme.muted)
}

/// A list drawn whole in a page that scrolls.
pub(super) fn column(id: &'static str, rows: Vec<Stateful<Div>>) -> Stateful<Div> {
    div().id(id).flex_none().flex().flex_col().children(rows)
}

/// A small button (Expo `Button small`): [`SMALL_BUTTON_H`] drawn, its
/// press reaching past it to `ladder.hit`. `Quiet` has no edge and muted
/// words, `Outline` a strong hairline and medium words; with no press it
/// is disabled, faint.
pub(super) fn small(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    kind: design::Kind,
    press: Option<Press>,
    k: &Kit,
) -> Stateful<Div> {
    let theme = &k.theme;
    let (id, label) = (id.into(), label.into());
    let outline = kind == design::Kind::Outline;
    let (edge, ink) = match (&press, outline) {
        (None, _) => (theme.border, theme.faint),
        (Some(_), true) => (theme.border_strong, theme.foreground),
        (Some(_), false) => (Hsla::transparent_black(), theme.muted),
    };
    let hit = design::hit(id.clone(), k.ladder.hit, SMALL_BUTTON_H, &k.ladder)
        .role(Role::Button)
        .aria_label(label.clone());
    let hit = match press {
        Some(press) => hit.focusable().on_click(press),
        None => hit.aria_disabled(true),
    };
    div()
        .id(SharedString::from(format!("{id}-box")))
        .relative()
        .flex_none()
        .h(SMALL_BUTTON_H)
        .px(space::LG)
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(edge)
        .text_type(k.ladder.secondary)
        .text_color(ink)
        .when(outline, |button| button.font_weight(FontWeight::MEDIUM))
        .whitespace_nowrap()
        .child(label)
        .child(hit)
}

/// An account's face (Expo `Avatar` with `words.ts` `faceOf`): a system
/// program's is square with a mono letter, a managed account's in the
/// agent tint, a suspended or revoked one at half strength.
pub(super) fn face(account: &identity::Account, size: Face, k: &Kit) -> Div {
    let kind = account.kind();
    let off = kind.note().is_some();
    let avatar = match kind {
        identity::Kind::System(_) => module_face(&account.card.name, size, k),
        identity::Kind::Program { .. } => design::avatar(&account.card.name, size, &k.theme)
            .bg(k.theme.agent_soft)
            .text_color(k.theme.agent),
        identity::Kind::User => design::avatar(&account.card.name, size, &k.theme),
    };
    avatar.when(off, |avatar| avatar.opacity(0.5))
}

/// A program's face: square, a mono letter (Expo `Avatar module`).
pub(super) fn module_face(name: &str, size: Face, k: &Kit) -> Div {
    design::avatar(name, size, &k.theme)
        .rounded(px(0.))
        .font_family(design::fonts::FAMILY_MONO)
}

// ---------- the explorer's rows ----------

/// Up to `most` lines of the window, newest first (Expo `BlockLines`): a
/// block, or a run of empty ones folded.
pub(super) fn block_lines(view: &Explorer, most: usize, cx: Cx, k: &Kit) -> Vec<Stateful<Div>> {
    let now = view.chain.now();
    lines(&view.chain.blocks, most)
        .into_iter()
        .map(|line| match line {
            Line::Block(block) => block_line(block, now, cx, k),
            Line::Empty { newest, oldest } => empty_run(newest, oldest, k),
        })
        .collect()
}

/// A block (Expo `BlockLine`): its height, medium when it carries
/// transactions and faint when not, its short id, its count and age.
fn block_line(block: &BlockRow, now: u64, cx: Cx, k: &Kit) -> Stateful<Div> {
    let height = block.height;
    let carries = block.txs > 0;
    let row = list_row(SharedString::from(format!("explorer-block-{height}")), k);
    opens(row, format!("Block {height}"), Route::Block(height), cx)
        .child(title_and_caption(
            mono(grouped(height))
                .text_type(k.ladder.body)
                .when(carries, |height| height.font_weight(FontWeight::MEDIUM))
                .text_color(match carries {
                    true => k.theme.foreground,
                    false => k.theme.faint,
                }),
            tail(short(&block.id), k),
        ))
        .child(tail(
            format!("{} tx · {}", block.txs, ago(now, block.time)),
            k,
        ))
        .child(chevron(k))
}

/// A run of empty blocks folded into one quiet line (Expo `EmptyRun`).
fn empty_run(newest: u64, oldest: u64, k: &Kit) -> Stateful<Div> {
    div()
        .id(SharedString::from(format!("explorer-empty-{newest}")))
        .flex_none()
        .min_h(k.ladder.row)
        .flex()
        .items_center()
        .px(space::BLOCK)
        .border_b_1()
        .border_color(k.theme.border)
        .child(
            mono(format!(
                "{}–{} · {} empty blocks",
                grouped(oldest),
                grouped(newest),
                grouped(newest - oldest + 1)
            ))
            .text_type(k.ladder.secondary)
            .text_color(k.theme.faint),
        )
}

/// How a run ended, as the mark in a row's lead (Expo `Mark`): success,
/// danger, an empty lead where the node kept no receipt.
fn mark(applied: Option<bool>, k: &Kit) -> Div {
    design::lead(div().children(applied.map(|applied| {
        let (_, color, _) = verdict(applied, &k.theme);
        div().size(MARK).bg(color)
    })))
}

/// Who signed: the account holding the key as `name #n`, or the key
/// (Expo `signerWords`).
fn signer_words(view: &Explorer, key: &[u8]) -> String {
    match view.holder(key) {
        Some((account, _)) => format!("{} #{}", account.card.name, account.number),
        None => short(key),
    }
}

/// A row's title (Expo `titleOf`): its op's, once its program has
/// described it, else its program and size.
pub(super) fn title_of(tx: &TxRow) -> String {
    match tx.op() {
        Some(op) => op.title.clone(),
        None => sized(&tx.target, u64::from(tx.size)),
    }
}

/// A transaction in the window (Expo `TxLine`): its mark, its program and
/// its op's title, its hash and signer, its age. Drawing it asks its
/// program to describe it, once.
pub(super) fn tx_line(view: &Explorer, tx: &TxRow, cx: Cx, k: &Kit) -> Stateful<Div> {
    view.describe(tx, cx);
    let (height, position) = (tx.height, tx.index);
    let applied = tx
        .receipt
        .as_ref()
        .map(|run| matches!(run.outcome, Outcome::Applied { .. }));
    let label = match applied {
        Some(applied) => activity::view::label(&tx.target, applied, &grouped(height), position),
        None => format!("{}, block {} · {position}", tx.target, grouped(height)),
    };
    let title = div()
        .truncate()
        .text_type(k.ladder.body)
        .child(
            mono(format!("{} ", tx.target))
                .text_size(TARGET_TEXT)
                .text_color(k.theme.muted),
        )
        .child(clip(&title_of(tx)));
    let row = list_row(
        SharedString::from(format!("explorer-tx-{height}-{position}")),
        k,
    );
    opens(row, label, Route::Tx { height, position }, cx)
        .child(mark(applied, k))
        .child(title_and_caption(
            title,
            caption(
                format!("{} · {}", short(&tx.hash), signer_words(view, &tx.signer)),
                k,
            ),
        ))
        .child(tail(ago(view.chain.now(), tx.time), k))
        .child(chevron(k))
}

/// A transaction an account signed, as activity recorded it (Expo
/// `TxLine who={false}`): a frame carries no hash and no title, so its
/// program is the title and its block the caption.
pub(super) fn frame_line(
    view: &Explorer,
    frame: &activity::Frame,
    cx: Cx,
    k: &Kit,
) -> Stateful<Div> {
    let (height, position) = (frame.height, frame.position);
    let label = activity::view::label(&frame.target, frame.applied, &grouped(height), position);
    let row = list_row(
        SharedString::from(format!("explorer-tx-{height}-{position}")),
        k,
    );
    opens(row, label, Route::Tx { height, position }, cx)
        .child(mark(Some(frame.applied), k))
        .child(title_and_caption(
            div().truncate().child(frame.target.clone()),
            caption(format!("block {}", grouped(height)), k),
        ))
        .child(tail(ago(view.chain.now(), frame.time), k))
        .child(chevron(k))
}
