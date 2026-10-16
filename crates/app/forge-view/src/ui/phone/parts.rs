//! What more than one forge phone screen draws, in one home: the Expo
//! app's `src/forge/parts.tsx` (the change row, the commit row, the ref
//! sheet, the outbox's rows of a scope), the pieces of
//! `src/forge/repo.tsx` its screens share (`FileBar`, `sizeOf`, `Stage`),
//! and the kit pieces of `src/kit/*.tsx` the SDK does not ship yet (the
//! small button, chips, the strip, the notice, key/value, the empty
//! state). Each kit piece is marked `SDK gap`: every view needs it, so it
//! lands once in the SDK (`feat/phone-kit`) and this copy goes. Each part
//! reads the model and draws; none writes but through the press it is
//! handed.
use std::ops::Range;
use std::rc::Rc;

use view::design::{Kind, Ladder, TextType};
use view::prelude::*;
use view::{Div, FontWeight, HighlightStyle, Stateful, StyledText, accesskit};

use crate::id;
use crate::list;
use crate::select::revision_name;
use crate::state::{Menu, Progress};
use crate::words::{self, ref_label};
use crate::{Forge, Stage};
use forge::{ChangeState, ChangeSummary, CommitInfo, Judgment, Query, Reply, Revision};

/// A press, as a row or a control takes it.
pub(crate) type Press = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
/// A press that carries what was pressed: a row's index, a ref's name.
pub(crate) type Route<T> = Rc<dyn Fn(T, &mut Window, &mut App)>;
/// A page's Back, when it has one.
pub(crate) type Back = Option<Press>;

/// The kit's small button (`theme.ts` `size.small`).
pub(crate) const SMALL: Pixels = px(36.);
/// A small button's icon (`Button.tsx`: 16 small, 18 otherwise).
const SMALL_ICON: Pixels = px(16.);
/// A filter chip's height (`theme.ts` `size.chip`).
pub(crate) const CHIP: Pixels = px(32.);
/// The label column of a key/value row (`Facts.tsx` `KeyValue`).
const KEY_W: Pixels = px(96.);
/// An empty state's icon (`theme.ts` `size.icon.empty`), its padding and
/// its body's measure (`States.tsx` `EmptyState`).
const EMPTY_ICON: Pixels = px(28.);
const EMPTY_Y: Pixels = px(40.);
const EMPTY_X: Pixels = px(32.);
const EMPTY_MEASURE: Pixels = px(280.);
/// A change row's state dot (`parts.tsx` `StateDot`).
const DOT: Pixels = px(8.);
/// The underline under the chosen tab of a strip (`Choice.tsx` `Strip`).
const UNDERLINE: Pixels = px(2.);
/// The gap between a row's title and its caption (`Row.tsx`).
const ROW_LINES_GAP: Pixels = px(1.);
/// A checkbox's or a radio's box, its edge, and what it draws when on
/// (`Toggle.tsx` `Checkbox`, `Radio`).
const TICK_BOX: Pixels = px(20.);
const TICK_EDGE: Pixels = px(1.5);
const TICK_ICON: Pixels = px(14.);
const RADIO_DOT: Pixels = px(9.);

// ------------------------------------------------------------------ text

/// One line of caption text, muted (the kit's `Text role="caption"`):
/// plain words, or runs of their own (a `+a −d` delta).
// SDK gap: quiet text at the ladder's sizes, swap when feat/phone-kit lands
pub(crate) fn caption(text: impl IntoElement, ladder: &Ladder, theme: &Theme) -> Div {
    div()
        .text_type(ladder.caption)
        .text_color(theme.muted)
        .truncate()
        .child(text)
}

/// `text` then `tail` in muted, one run that truncates as one
/// (`<Text>{title} <Text tone="muted">#n</Text></Text>`).
pub(crate) fn muted_tail(text: &str, tail: &str, theme: &Theme) -> StyledText {
    let whole = format!("{text}{tail}");
    tinted(
        whole,
        vec![(text.len()..text.len() + tail.len(), theme.muted)],
    )
}

/// `whole` with each range in its colour.
fn tinted(whole: String, runs: Vec<(Range<usize>, Hsla)>) -> StyledText {
    let highlights = runs
        .into_iter()
        .map(|(range, color)| {
            let style = HighlightStyle {
                color: Some(color),
                ..Default::default()
            };
            (range, style)
        })
        .collect::<Vec<_>>();
    StyledText::new(whole).with_highlights(highlights)
}

/// `{before}+a −d` with `+a` in success and `−d` in danger
/// (`DiffList.tsx` `Delta`): a file that only adds or only removes shows
/// its one side.
pub(crate) fn delta(before: &str, additions: u64, deletions: u64, theme: &Theme) -> StyledText {
    let mut whole = before.to_owned();
    let mut runs = Vec::new();
    if additions > 0 || deletions == 0 {
        let start = whole.len();
        whole.push_str(&format!("+{additions}"));
        runs.push((start..whole.len(), theme.success));
    }
    if additions > 0 && deletions > 0 {
        whole.push(' ');
    }
    if deletions > 0 {
        let start = whole.len();
        whole.push_str(&format!("−{deletions}"));
        runs.push((start..whole.len(), theme.danger));
    }
    tinted(whole, runs)
}

/// `212 B`, `6.8 KB`, `12 KB`, `1.4 MB` (`repo.tsx` `sizeOf`).
pub(crate) fn size_of(bytes: u64) -> String {
    const KB: f64 = 1024.;
    let value = bytes as f64;
    if value < KB {
        return format!("{bytes} B");
    }
    let (amount, unit) = match value < KB * KB {
        true => (value / KB, "KB"),
        false => (value / (KB * KB), "MB"),
    };
    match amount < 10. {
        true => format!("{amount:.1} {unit}"),
        false => format!("{} {unit}", amount.round()),
    }
}

/// `<source> → <target>` of a change, short names (`parts.tsx`
/// `endpoints`).
pub(crate) fn endpoints(from: &Revision, into: &[u8]) -> String {
    format!("{} → {}", ref_label(&revision_name(from)), ref_label(into))
}

// ------------------------------------------------------------------ rows

/// A Lucide icon leading a row, muted.
pub(crate) fn lead_icon(name: &str, ladder: &Ladder, theme: &Theme) -> AnyElement {
    design::icon(name, ladder.icon_row)
        .text_color(theme.muted)
        .into_any_element()
}

/// A list row (`Row.tsx`): at least a row tall (56 with a caption), the
/// lead in its 24 column, the title over its caption, what trails it, a
/// chevron when it pushes a screen. The caller makes it a press.
// SDK gap: the row's title over caption, and its chevron, swap when feat/phone-kit lands
pub(crate) struct Row {
    id: ElementId,
    title: AnyElement,
    lead: Option<AnyElement>,
    caption: Option<AnyElement>,
    tail: Vec<AnyElement>,
    chevron: bool,
    chosen: bool,
}

impl Row {
    /// A row whose title is `title` as it is drawn.
    pub(crate) fn new(id: impl Into<ElementId>, title: impl IntoElement) -> Row {
        Row {
            id: id.into(),
            title: title.into_any_element(),
            lead: None,
            caption: None,
            tail: Vec::new(),
            chevron: false,
            chosen: false,
        }
    }

    /// A row titled in body text on one line.
    pub(crate) fn titled(
        id: impl Into<ElementId>,
        title: impl Into<SharedString>,
        ladder: &Ladder,
    ) -> Row {
        Row::new(
            id,
            div().text_type(ladder.body).truncate().child(title.into()),
        )
    }

    /// A row titled in the data face, at the secondary size (`mono`).
    pub(crate) fn data(
        id: impl Into<ElementId>,
        title: impl Into<SharedString>,
        ladder: &Ladder,
    ) -> Row {
        Row::new(
            id,
            div()
                .font_family(design::fonts::FAMILY_MONO)
                .text_type(ladder.secondary)
                .truncate()
                .child(title.into()),
        )
    }

    pub(crate) fn lead(mut self, lead: impl IntoElement) -> Row {
        self.lead = Some(lead.into_any_element());
        self
    }

    pub(crate) fn caption(mut self, caption: impl IntoElement) -> Row {
        self.caption = Some(caption.into_any_element());
        self
    }

    pub(crate) fn tail(mut self, tail: impl IntoElement) -> Row {
        self.tail.push(tail.into_any_element());
        self
    }

    /// The row pushes a screen.
    pub(crate) fn chevron(mut self) -> Row {
        self.chevron = true;
        self
    }

    /// The row is the picked one: on `accent_soft`.
    pub(crate) fn chosen(mut self, chosen: bool) -> Row {
        self.chosen = chosen;
        self
    }

    pub(crate) fn draw(self, ladder: &Ladder, theme: &Theme) -> Stateful<Div> {
        let tall = px(design::height::touch::ROW_TALL as f32);
        let captioned = self.caption.is_some();
        let lines = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap(ROW_LINES_GAP)
            .child(self.title)
            .children(self.caption);
        design::row(self.id, ladder, theme)
            .when(captioned, |row| row.min_h(tall))
            .when(self.chosen, |row| row.bg(theme.accent_soft))
            .children(self.lead.map(design::lead))
            .child(lines)
            .when(!self.tail.is_empty(), |row| {
                row.child(
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap(design::space::SM)
                        .children(self.tail),
                )
            })
            .when(self.chevron, |row| {
                row.child(design::icon("chevron-right", ladder.icon_row).text_color(theme.faint))
            })
    }
}

/// A list's rows as one Tab stop (`components::list`), each row an option
/// the arrows walk; `open` presses row `index`.
pub(crate) fn rows_list(
    list_id: &'static str,
    label: &str,
    rows: Vec<(Row, Press)>,
    open: impl Fn(&mut Forge, usize, &mut Window, &mut Context<Forge>) + 'static,
    forge: &Forge,
    cx: &mut Context<Forge>,
    (ladder, theme): (&Ladder, &Theme),
) -> Stateful<Div> {
    let (at, _) = list::cursor(forge, list_id);
    let walked = list::walked(forge, list_id);
    let at = at.min(rows.len().saturating_sub(1));
    let mut list = list::list(list_id, label, rows.len(), None, forge, cx, open).flex_none();
    for (index, (row, press)) in rows.into_iter().enumerate() {
        let row = row.draw(ladder, theme).on_click(press);
        list = list.child(option(row, walked && index == at));
    }
    list
}

/// A row as an option of a list (`design::item`), selected while the
/// arrows are on it.
pub(crate) fn option(row: Stateful<Div>, active: bool) -> Stateful<Div> {
    design::item(row.aria_selected(active), Role::ListBoxOption, active)
}

/// A virtual list of `count` rows as one Tab stop ([`list::list`]),
/// named `label`: the arrows walk them, the list scrolled to the active
/// one; `paint` draws row `index` (and whether it is the active one) as
/// it comes into view, `open` presses it.
pub(crate) fn virtual_list(
    (list_id, label): (&'static str, &str),
    count: usize,
    scroll: &UniformListScrollHandle,
    open: impl Fn(&mut Forge, usize, &mut Window, &mut Context<Forge>) + 'static,
    paint: impl Fn(usize, bool) -> AnyElement + 'static,
    forge: &Forge,
    cx: &mut Context<Forge>,
) -> Stateful<Div> {
    let (at, _) = list::cursor(forge, list_id);
    let walked = list::walked(forge, list_id);
    let at = at.min(count.saturating_sub(1));
    let rows = list::window(
        &format!("{list_id}-rows"),
        count,
        None,
        Some(scroll),
        move |range, _, _| {
            range
                .map(|index| paint(index, walked && index == at))
                .collect()
        },
    );
    list::list(list_id, label, count, Some(scroll), forge, cx, open).child(rows)
}

/// A key/value row (`Facts.tsx` `KeyValue`): at least a control tall, the
/// label `small` muted in its column, the value `small` beside it.
// SDK gap: key/value, swap when feat/phone-kit lands
pub(crate) fn key_value(
    label: &str,
    value: impl IntoElement,
    ladder: &Ladder,
    theme: &Theme,
) -> Div {
    div()
        .w_full()
        .min_h(ladder.control)
        .flex()
        .items_start()
        .gap(design::space::LG)
        .py(design::space::MD)
        .px(design::space::BLOCK)
        .border_b_1()
        .border_color(theme.border)
        .text_type(ladder.small)
        .child(
            div()
                .w(KEY_W)
                .flex_none()
                .text_color(theme.muted)
                .child(label.to_owned()),
        )
        .child(div().flex_1().min_w(px(0.)).child(value))
}

/// A one-line field with its lines (`Field.tsx`): the kit's box, edged in
/// danger while something is wrong. What is wrong marks the input invalid
/// and is its description, so a screen reader hears why (rule 18); with
/// nothing wrong its rule is.
// SDK gap: the labelled field with its problem, swap when feat/phone-kit lands
pub(crate) fn input_field(
    label: Option<&str>,
    input: Input,
    (problem, rule): (&str, Option<&str>),
    ladder: &Ladder,
    theme: &Theme,
) -> Div {
    let wrong = !problem.is_empty();
    let said = if wrong { Some(problem) } else { rule };
    let input = input
        .when(wrong, |input| input.invalid(accesskit::Invalid::True))
        .when_some(said, |input, said| input.description(said));
    let field = design::field(input, ladder, theme)
        .w_full()
        .when(wrong, |field| field.border_color(theme.danger));
    field_lines(label, field, (problem, rule), ladder, theme)
}

/// A field of many lines with its lines ([`text_area`] in [`field_lines`]).
// SDK gap: `Textarea` takes no `invalid` or `description`, so what is
// wrong is drawn under it but not said to a screen reader; swap when
// feat/phone-kit lands
pub(crate) fn text_area_field(
    label: Option<&str>,
    area: Textarea,
    problem: &str,
    ladder: &Ladder,
    theme: &Theme,
) -> Div {
    field_lines(
        label,
        text_area(area, ladder, theme),
        (problem, None),
        ladder,
        theme,
    )
}

/// A field with its lines (`Field.tsx`): its label over it, caption
/// muted, and under it what is wrong in danger, else its rule muted.
fn field_lines(
    label: Option<&str>,
    field: impl IntoElement,
    (problem, rule): (&str, Option<&str>),
    ladder: &Ladder,
    theme: &Theme,
) -> Div {
    let under = match problem.is_empty() {
        false => Some((problem.to_owned(), theme.danger)),
        true => rule.map(|rule| (rule.to_owned(), theme.muted)),
    };
    div()
        .flex()
        .flex_col()
        .children(label.map(|label| {
            div()
                .mb(design::space::XS)
                .text_type(ladder.caption)
                .text_color(theme.muted)
                .child(label.to_owned())
        }))
        .child(field)
        .children(under.map(|(text, colour)| {
            div()
                .mt(design::space::XS)
                .text_type(ladder.caption)
                .text_color(colour)
                .child(text)
        }))
}

/// A value in the data face: a hash, a ref, a number.
// SDK gap: mono text at the ladder's sizes, swap when feat/phone-kit lands
pub(crate) fn data(text: impl Into<SharedString>) -> Div {
    div()
        .font_family(design::fonts::FAMILY_MONO)
        .child(text.into())
}

// ------------------------------------------------------------------ states

/// One muted status line, 16 all round (`States.tsx` `Quiet`).
// SDK gap: quiet text at the ladder's sizes, swap when feat/phone-kit lands
pub(crate) fn quiet(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .p(design::space::BLOCK)
        .text_type(ladder.secondary)
        .text_color(theme.muted)
        .child(text.into())
}

/// Nothing to show yet (`States.tsx` `EmptyState`): the icon faint, what
/// is missing in medium, what would fill it muted, all centred.
// SDK gap: the empty state at touch sizes, swap when feat/phone-kit lands
pub(crate) fn empty_state(
    id: impl Into<ElementId>,
    icon: &str,
    title: impl Into<SharedString>,
    body: impl Into<SharedString>,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(design::space::SM)
        .py(EMPTY_Y)
        .px(EMPTY_X)
        .child(design::icon(icon, EMPTY_ICON).text_color(theme.faint))
        .child(
            div()
                .text_type(ladder.body)
                .font_weight(FontWeight::MEDIUM)
                .text_center()
                .child(title.into()),
        )
        .child(
            div()
                .max_w(EMPTY_MEASURE)
                .text_type(ladder.secondary)
                .text_color(theme.muted)
                .text_center()
                .child(body.into()),
        )
}

/// A notice's tone (`Notice.tsx` `NoticeKind`): its ground and its words'
/// colour.
#[derive(Clone, Copy)]
// SDK gap: the notice, swap when feat/phone-kit lands
pub(crate) enum Tone {
    /// a refusal
    Danger,
    /// a gate
    Warning,
    /// a status
    Quiet,
}

impl Tone {
    fn colours(self, theme: &Theme) -> (Hsla, Hsla) {
        match self {
            Tone::Danger => (theme.danger_soft, theme.danger),
            Tone::Warning => (theme.warning_soft, theme.warning),
            Tone::Quiet => (theme.surface, theme.muted),
        }
    }
}

/// A strip above or under content (`Notice.tsx`): edge to edge, 10 over
/// and under, 16 in, secondary text in its tone on its ground; a quiet one
/// keeps a hairline under it.
// SDK gap: the notice, swap when feat/phone-kit lands
pub(crate) fn notice(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    tone: Tone,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    let (ground, words) = tone.colours(theme);
    div()
        .id(id)
        .w_full()
        .flex_none()
        .flex()
        .items_center()
        .gap(design::space::MD)
        .py(design::space::MD)
        .px(design::space::BLOCK)
        .bg(ground)
        .when(matches!(tone, Tone::Quiet), |strip| {
            strip.border_b_1().border_color(theme.border)
        })
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .text_type(ladder.secondary)
                .text_color(words)
                .child(text.into()),
        )
}

/// A notice's one action at its right (`Notice.tsx`): medium in the
/// notice's tone, its press reaching a finger's box.
// SDK gap: the notice's action, swap when feat/phone-kit lands
pub(crate) fn notice_action(
    id: impl Into<ElementId>,
    label: &'static str,
    tone: Tone,
    ladder: &Ladder,
    theme: &Theme,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let (_, words) = tone.colours(theme);
    let height = ladder.secondary.line.unwrap_or(ladder.secondary.size);
    div()
        .relative()
        .flex_none()
        .h(height)
        .pl(design::space::SM)
        .text_type(ladder.secondary)
        .font_weight(FontWeight::MEDIUM)
        .text_color(words)
        .child(label)
        .child(
            design::hit(id, ladder.hit, height, ladder)
                .role(Role::Button)
                .aria_label(label)
                .focusable()
                .on_click(click),
        )
}

/// A read as a screen shows it (`States.tsx` `ReadView`): the reply once
/// it landed, else the quiet line `loading` says, or the refusal and
/// Retry in its place (`{id}-loading`, `{id}-refused`, `{id}-retry`).
pub(crate) fn stage<'a>(
    forge: &'a Forge,
    query: &Query,
    element_id: &str,
    loading: &str,
    cx: &mut Context<Forge>,
    (ladder, theme): (&Ladder, &Theme),
) -> Result<&'a Reply, AnyElement> {
    match forge.stage(query) {
        Stage::Ready(reply) => Ok(reply),
        Stage::Loading => Err(quiet(
            id(format!("{element_id}-loading")),
            loading.to_owned(),
            ladder,
            theme,
        )
        .into_any_element()),
        Stage::Failed(refusal) => {
            let query = query.clone();
            let retry =
                cx.listener(move |forge, _: &ClickEvent, _, cx| forge.retry(query.clone(), cx));
            Err(refused(
                element_id,
                refusal.message.clone(),
                retry,
                ladder,
                theme,
            ))
        }
    }
}

/// A refused read (`States.tsx` `Refused`): the sentence verbatim on the
/// danger strip, and Retry.
// SDK gap: the refusal and its Retry, swap when feat/phone-kit lands
pub(crate) fn refused(
    element_id: &str,
    sentence: String,
    retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ladder: &Ladder,
    theme: &Theme,
) -> AnyElement {
    notice(
        id(format!("{element_id}-refused")),
        sentence,
        Tone::Danger,
        ladder,
        theme,
    )
    .role(Role::Alert)
    .aria_live(view::accesskit::Live::Assertive)
    .child(notice_action(
        id(format!("{element_id}-retry")),
        "Retry",
        Tone::Danger,
        ladder,
        theme,
        retry,
    ))
    .into_any_element()
}

/// The writes of `scope` still on their way (`parts.tsx` `PendingRows`):
/// each a quiet strip saying where it is, or a danger strip with why it
/// was refused.
pub(crate) fn pending(
    forge: &Forge,
    scope: &str,
    ladder: &Ladder,
    theme: &Theme,
) -> Vec<AnyElement> {
    forge
        .pending_in(scope)
        .into_iter()
        .map(|op| {
            let (tone, said) = match &op.progress {
                Progress::Submitting => (Tone::Quiet, words::SUBMITTING.to_owned()),
                Progress::Accepted => (Tone::Quiet, words::WAITING_FOR_BLOCK.to_owned()),
                Progress::Refused(sentence) => (Tone::Danger, sentence.clone()),
            };
            notice(
                id(format!("forge-pending-{}", op.id)),
                format!("{} · {said}", op.label),
                tone,
                ladder,
                theme,
            )
            .into_any_element()
        })
        .collect()
}

/// Whether a write of the open change (a review among them) is on its
/// way: a second built from what is on screen could land twice.
pub(crate) fn sending(forge: &Forge) -> bool {
    forge.review_key().is_some_and(|key| {
        forge
            .pending_in(&key)
            .iter()
            .any(|op| !matches!(op.progress, Progress::Refused(_)))
    })
}

/// What every screen says over its body: the last refusal, and why the
/// write controls are off while the key holds no account.
pub(crate) fn notices(forge: &Forge, ladder: &Ladder, theme: &Theme) -> Vec<AnyElement> {
    let refusal = (!forge.notice.is_empty()).then(|| {
        notice(
            id("forge-notice"),
            forge.notice.clone(),
            Tone::Danger,
            ladder,
            theme,
        )
        .role(Role::Alert)
        .aria_live(view::accesskit::Live::Assertive)
        .into_any_element()
    });
    let seated = forge.session.connected && forge.me_principal().is_none();
    let no_account = seated.then(|| {
        notice(
            id("forge-no-account"),
            "To create repositories, push, open changes or review, create or join an account in \
             Account. You can read every repository without an account.",
            Tone::Warning,
            ladder,
            theme,
        )
        .into_any_element()
    });
    refusal.into_iter().chain(no_account).collect()
}

// ------------------------------------------------------------------ controls

/// The kit's small button (`Button.tsx` `small`): 36 tall, 12 in, an
/// optional icon at 16, the label secondary (medium but for a quiet one);
/// its press reaches past its edges to a finger's box.
// SDK gap: the small button, swap when feat/phone-kit lands
pub(crate) struct SmallButton {
    id: ElementId,
    label: SharedString,
    icon: Option<&'static str>,
    kind: Kind,
    enabled: bool,
    ladder: Ladder,
    theme: Theme,
    click: Press,
}

pub(crate) fn small_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    ladder: &Ladder,
    theme: &Theme,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> SmallButton {
    SmallButton {
        id: id.into(),
        label: label.into(),
        icon: None,
        kind: Kind::Outline,
        enabled: true,
        ladder: *ladder,
        theme: *theme,
        click: Box::new(click),
    }
}

impl SmallButton {
    pub(crate) fn icon(mut self, name: &'static str) -> Self {
        self.icon = Some(name);
        self
    }
    pub(crate) fn kind(mut self, kind: Kind) -> Self {
        self.kind = kind;
        self
    }
    pub(crate) fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// (edge, words) as the kit's outline, quiet and primary draw them;
    /// a disabled one faint.
    fn colours(&self) -> (Hsla, Hsla) {
        let theme = &self.theme;
        match (self.kind, self.enabled) {
            (_, false) => (theme.border, theme.faint),
            (Kind::Primary, true) => (theme.primary, theme.primary_foreground),
            (Kind::Quiet, true) => (hsla(0., 0., 0., 0.), theme.muted),
            (Kind::Plain | Kind::Outline, true) => (theme.border_strong, theme.foreground),
        }
    }
}

impl IntoElement for SmallButton {
    type Element = Div;

    fn into_element(self) -> Div {
        let (edge, words) = self.colours();
        let ink = self.kind == Kind::Primary && self.enabled;
        let press = design::hit(self.id, self.ladder.hit, SMALL, &self.ladder)
            .role(Role::Button)
            .aria_label(self.label.clone());
        let press = match self.enabled {
            true => press.focusable().on_click(self.click),
            false => press.aria_disabled(true),
        };
        div()
            .relative()
            .h(SMALL)
            .px(design::space::LG)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(design::space::SM)
            .border_1()
            .border_color(edge)
            .when(ink, |button| button.bg(self.theme.primary))
            .text_color(words)
            .text_type(self.ladder.secondary)
            .when(self.kind != Kind::Quiet, |button| {
                button.font_weight(FontWeight::MEDIUM)
            })
            .whitespace_nowrap()
            .children(self.icon.map(|name| design::icon(name, SMALL_ICON)))
            .child(self.label)
            .child(press)
    }
}

/// A field with the kit's lead icon inside its edge (`Field.tsx` `lead`):
/// the box is `design::field`'s, the icon 12 in, the text 8 after it.
// SDK gap: the search field, swap when feat/phone-kit lands
pub(crate) fn search_field(input: Input, icon: &str, ladder: &Ladder, theme: &Theme) -> Div {
    let inset = design::space::LG + ladder.icon_row + design::space::SM;
    div()
        .relative()
        .w_full()
        .child(design::field(input, ladder, theme).w_full().pl(inset))
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(design::space::LG)
                .flex()
                .items_center()
                .child(design::icon(icon, ladder.icon_row).text_color(theme.muted)),
        )
}

/// A field of many lines in the kit's box (`Field` `multiline`): at
/// least a control tall, a `border_strong` hairline, body text 12 in and
/// 10 over and under; it grows with what is typed.
// SDK gap: the many-line field's box, swap when feat/phone-kit lands
pub(crate) fn text_area(field: Textarea, ladder: &Ladder, theme: &Theme) -> Textarea {
    field
        .min_h(ladder.control)
        .px(design::space::LG)
        .py(design::space::MD)
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.background)
        .text_color(theme.foreground)
        .text_type(ladder.body)
}

/// A button taking `share` parts of its row (`Button` `wide` with
/// `flex: share`): it grows from nothing by its share of the room the
/// row has, its box stretched across it.
// SDK gap: the wide button's share of its row, swap when feat/phone-kit lands
pub(crate) fn share(share: f32, button: impl IntoElement) -> Div {
    let mut cell = div().min_w(px(0.)).flex().flex_col().child(button);
    let style = cell.style();
    style.flex_grow = Some(share);
    style.flex_basis = Some(px(0.).into());
    cell
}

/// A checkbox (`Toggle.tsx` `Checkbox`): a 20 box, a `border_strong`
/// edge, ink with a check once ticked; its press reaches a finger's box
/// and stops there, so a row around it does not open too.
// SDK gap: the checkbox, swap when feat/phone-kit lands
pub(crate) fn checkbox(
    id: impl Into<ElementId>,
    on: bool,
    label: impl Into<SharedString>,
    ladder: &Ladder,
    theme: &Theme,
    toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let edge = if on {
        theme.primary
    } else {
        theme.border_strong
    };
    let press = design::hit(id, TICK_BOX, TICK_BOX, ladder)
        .role(Role::CheckBox)
        .aria_label(label.into())
        .aria_toggled(on.into())
        .focusable()
        .consumes_click()
        .on_click(toggle);
    div()
        .relative()
        .size(TICK_BOX)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .border(TICK_EDGE)
        .border_color(edge)
        .when(on, |tick| {
            tick.bg(theme.primary)
                .child(design::icon("check", TICK_ICON).text_color(theme.primary_foreground))
        })
        .child(press)
}

/// A radio's mark (`Toggle.tsx` `Radio`): a 20 ring, ink with a 9 dot
/// once picked. The row it sits in is the radio: it carries the press
/// and the state.
// SDK gap: the radio mark, swap when feat/phone-kit lands
pub(crate) fn radio_mark(on: bool, theme: &Theme) -> Div {
    let edge = if on {
        theme.foreground
    } else {
        theme.border_strong
    };
    div()
        .size(TICK_BOX)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border(TICK_EDGE)
        .border_color(edge)
        .when(on, |ring| {
            ring.child(div().size(RADIO_DOT).rounded_full().bg(theme.foreground))
        })
}

/// One filter chip (`Choice.tsx` `Chips`): its label, and its count in
/// the data face.
// SDK gap: chips, swap when feat/phone-kit lands
pub(crate) struct Chip {
    pub(crate) id: ElementId,
    pub(crate) label: &'static str,
    pub(crate) count: Option<usize>,
}

/// Filter chips (`Choice.tsx` `Chips`): 32 tall outline boxes 8 apart,
/// the chosen one in ink, scrolling sideways; each press reaches a
/// finger's box.
pub(crate) fn chips(
    row_id: &str,
    chips: Vec<Chip>,
    chosen: usize,
    ladder: &Ladder,
    theme: &Theme,
    pick: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let pick = std::rc::Rc::new(pick);
    let theme = *theme;
    let row = div()
        .id(id(row_id.to_owned()))
        .flex()
        .flex_none()
        .gap(design::space::SM)
        .py(design::space::SM)
        .px(design::space::BLOCK)
        .overflow_x_scroll();
    row.children(chips.into_iter().enumerate().map(|(index, chip)| {
        let pick = pick.clone();
        let click =
            move |_: &ClickEvent, window: &mut Window, app: &mut App| pick(index, window, app);
        chip_box(chip, index == chosen, ladder, &theme, click)
    }))
}

fn chip_box(
    chip: Chip,
    chosen: bool,
    ladder: &Ladder,
    theme: &Theme,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let (ground, edge, words, count) = match chosen {
        true => (
            Some(theme.primary),
            theme.primary,
            theme.primary_foreground,
            theme.primary_foreground,
        ),
        false => (None, theme.border_strong, theme.foreground, theme.muted),
    };
    div()
        .relative()
        .h(CHIP)
        .flex()
        .flex_none()
        .items_center()
        .gap(design::space::XS)
        .px(design::space::MD)
        .border_1()
        .border_color(edge)
        .when_some(ground, |chip, ground| chip.bg(ground))
        .text_type(ladder.secondary)
        .text_color(words)
        .whitespace_nowrap()
        .child(chip.label)
        .children(chip.count.map(|count_| {
            div()
                .font_family(design::fonts::FAMILY_MONO)
                .text_size(ladder.tiny.size)
                .text_color(count)
                .child(count_.to_string())
        }))
        .child(
            design::hit(chip.id, ladder.hit, CHIP, ladder)
                .role(Role::Button)
                .aria_label(chip.label)
                .aria_toggled(chosen.into())
                .focusable()
                .on_click(click),
        )
}

/// A page's tabs (`Choice.tsx` `Strip`) on `bar`, a `TabList` composite:
/// a control tall, a hairline under it, 8 in, scrolling sideways.
// SDK gap: the strip, swap when feat/phone-kit lands
pub(crate) fn strip(bar: Stateful<Div>, ladder: &Ladder, theme: &Theme) -> Stateful<Div> {
    bar.flex()
        .flex_none()
        .items_center()
        .h(ladder.control)
        .px(design::space::SM)
        .border_b_1()
        .border_color(theme.border)
        .overflow_x_scroll()
}

/// One tab of a [`strip`]: 10 in, the label `small`, the chosen one
/// medium in the foreground over a 2 px underline as wide as its label;
/// its count after it in the data face, faint. `(chosen, active)`: it is
/// the open one, and the arrows are on it.
// SDK gap: the strip's tab, swap when feat/phone-kit lands
pub(crate) fn strip_tab(
    element_id: ElementId,
    label: &str,
    count: Option<u64>,
    (chosen, active): (bool, bool),
    ladder: &Ladder,
    theme: &Theme,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let words = div()
        .flex()
        .items_baseline()
        .text_type(ladder.small)
        .text_color(if chosen {
            theme.foreground
        } else {
            theme.muted
        })
        .when(chosen, |words| words.font_weight(FontWeight::MEDIUM))
        .child(label.to_owned())
        .children(count.map(|count| {
            div()
                .ml(design::space::XXS)
                .font_family(design::fonts::FAMILY_MONO)
                .text_size(ladder.tiny.size)
                .font_weight(FontWeight::NORMAL)
                .text_color(theme.faint)
                .child(count.to_string())
        }));
    let underline = div()
        .absolute()
        .left(design::space::MD)
        .right(design::space::MD)
        .bottom_0()
        .h(UNDERLINE)
        .bg(theme.foreground);
    let tab = div()
        .id(element_id)
        .relative()
        .h_full()
        .flex_none()
        .flex()
        .items_center()
        .px(design::space::MD)
        .whitespace_nowrap()
        .active(|style| style.opacity(0.6))
        .role(Role::Tab)
        .aria_selected(chosen)
        .on_click(click)
        .child(words)
        .when(chosen, |tab| tab.child(underline));
    design::item(tab, Role::Tab, active).into_any_element()
}

/// The head of a sheet (`Sheet.tsx` `SheetHead`): its title in `title`
/// medium.
// SDK gap: a form sheet's head, swap when feat/phone-kit lands
pub(crate) fn sheet_head(key: &str, title: &str, ladder: &Ladder, theme: &Theme) -> Stateful<Div> {
    design::sheet_head(id(key.to_owned()), theme).child(
        div()
            .id(id(format!("{key}-text")))
            .text_type(ladder.title)
            .font_weight(FontWeight::MEDIUM)
            .role(Role::Heading)
            .aria_level(2)
            .child(title.to_owned()),
    )
}

/// The strip over a document or a folder (`repo.tsx` `FileBar`): at least
/// a control tall on `surface`, 16 in, 6 over and under, a hairline under
/// it; the title secondary in the data face at the left, `right` a caption
/// at the right.
pub(crate) fn file_bar(
    key: &str,
    title: impl Into<SharedString>,
    right: impl Into<SharedString>,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    let title = div()
        .font_family(design::fonts::FAMILY_MONO)
        .text_type(ladder.secondary)
        .child(title.into());
    let right: SharedString = right.into();
    bar(key, title, caption(right, ladder, theme), ladder, theme)
}

/// A [`file_bar`] holding `left` (truncating) and `right` as they are
/// drawn: a file's facts beside its Source and Wrap.
pub(crate) fn bar(
    key: &str,
    left: impl IntoElement,
    right: impl IntoElement,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id(key.to_owned()))
        .w_full()
        .min_h(ladder.control)
        .flex()
        .flex_none()
        .items_center()
        .gap(design::space::SM)
        .px(design::space::BLOCK)
        .py(design::space::XS)
        .bg(theme.surface)
        .border_b_1()
        .border_color(theme.border)
        .child(div().flex_1().min_w(px(0.)).truncate().child(left))
        .child(div().flex_none().child(right))
}

// ------------------------------------------------------------------ forge rows

/// A badge's tint (`Badge.tsx` `Tint`).
#[derive(Clone, Copy)]
// SDK gap: the badge's tints, swap when feat/phone-kit lands
pub(crate) enum Tint {
    /// on `surface_raised`, muted
    Plain,
    Ok,
    Warn,
    Danger,
    Agent,
}

/// A state word on the kit's 20 tall tag (`Badge.tsx`), in its tint.
// SDK gap: the badge's tints, swap when feat/phone-kit lands
pub(crate) fn badge(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    tint: Tint,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    let (words, ground) = match tint {
        Tint::Plain => (theme.muted, theme.surface_raised),
        Tint::Ok => (theme.success, theme.success_soft),
        Tint::Warn => (theme.warning, theme.warning_soft),
        Tint::Danger => (theme.danger, theme.danger_soft),
        Tint::Agent => (theme.agent, theme.agent_soft),
    };
    design::badge(id, label, ladder, words, ground)
}

/// A change's state as a badge (`parts.tsx` `StateBadge`): open ok,
/// merged agent, closed plain.
pub(crate) fn state_badge(
    state: ChangeState,
    key: &str,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    let tint = match state {
        ChangeState::Open => Tint::Ok,
        ChangeState::Merged => Tint::Agent,
        ChangeState::Closed => Tint::Plain,
    };
    badge(
        id(key.to_owned()),
        words::change_state(state),
        tint,
        ladder,
        theme,
    )
}

/// The dot a change row leads with (`parts.tsx` `StateDot`).
fn state_dot(state: ChangeState, theme: &Theme) -> Div {
    let colour = match state {
        ChangeState::Open => theme.success,
        ChangeState::Merged => theme.agent,
        ChangeState::Closed => theme.faint,
    };
    div().size(DOT).rounded_full().bg(colour)
}

/// One change (`parts.tsx` `ChangeRow`): `<title> #n`, the caption
/// `<source> → <target> · <author> · N comments`; listed by judgment,
/// what it asks of me. On the root (`across`) it leads with a branch and
/// names its repository first, not in the data face.
pub(crate) fn change_row(
    summary: &ChangeSummary,
    judgment: Option<&Judgment>,
    across: bool,
    forge: &Forge,
    ladder: &Ladder,
    theme: &Theme,
) -> Row {
    let author = forge.principal_name(&summary.author);
    let asks = judgment.into_iter().flat_map(|judgment| {
        let requested = judgment
            .requested
            .then_some(words::REVIEW_REQUESTED.to_owned());
        let replied = judgment
            .replies
            .is_some()
            .then(|| words::NEW_REPLY.to_owned());
        requested.into_iter().chain(replied)
    });
    let comments = words::comments(summary.comment_count);
    let words: Vec<String> = across
        .then(|| summary.repo.clone())
        .into_iter()
        .chain([endpoints(&summary.from, &summary.into), author])
        .chain((!across).then_some(comments))
        .chain(asks)
        .collect();
    let caption_line = caption(words.join(" · "), ladder, theme)
        .when(!across, |line| line.font_family(design::fonts::FAMILY_MONO));
    let title = div().text_type(ladder.body).truncate().child(muted_tail(
        &summary.title,
        &format!(" #{}", summary.n),
        theme,
    ));
    let lead = match across {
        true => lead_icon("git-branch", ladder, theme),
        false => state_dot(summary.state, theme).into_any_element(),
    };
    Row::new(
        id(format!("forge-change-{}-{}", summary.repo, summary.n)),
        title,
    )
    .lead(lead)
    .caption(caption_line)
    .chevron()
}

/// A commit's first line (`parts.tsx` `summaryOf`).
pub(crate) fn summary_of(commit: &CommitInfo) -> String {
    String::from_utf8_lossy(&commit.message)
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// A git time as `3 Oct 2026, 4:00 AM` (`parts.tsx` `when`).
pub(crate) fn when(seconds: i64) -> String {
    // a time before 1970 reads as 1970
    let at = u64::try_from(seconds).unwrap_or_default() * 1000;
    format!("{}, {}", design::day(at), design::clock(at))
}

/// One commit (`parts.tsx` `CommitRow`): its title, `<short> · <author> ·
/// <date, time>`, and `· N parents` for a merge.
pub(crate) fn commit_row(commit: &CommitInfo, ladder: &Ladder, theme: &Theme) -> Row {
    let short = design::short_hex(&commit.oid);
    let merge = match commit.parents.len() {
        0 | 1 => String::new(),
        parents => format!(" · {parents} parents"),
    };
    let title = match summary_of(commit) {
        summary if summary.is_empty() => short.clone(),
        summary => summary,
    };
    let author = String::from_utf8_lossy(&commit.author.name);
    let facts = format!("{short} · {author} · {}{merge}", when(commit.author.time));
    Row::titled(id(format!("forge-commit-{}", commit.oid)), title, ladder)
        .lead(lead_icon("git-commit-horizontal", ladder, theme))
        .caption(caption(facts, ladder, theme))
        .chevron()
}

// ------------------------------------------------------------------ the ref sheet

/// The ref sheet (`parts.tsx` `RefSheet`) for `menu`: Branches | Tags,
/// the filter, a row per ref with `default` on the default head and a
/// check on the picked one; a pick closes it.
pub(crate) fn ref_sheet(
    menu: Menu,
    title: &str,
    forge: &Forge,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> AnyElement {
    let close =
        cx.listener(move |forge, _: &(), window, cx| forge.close_dropdown(menu, window, cx));
    let key = menu.key();
    let body = div()
        .id(id(format!("{key}-rows")))
        .size_full()
        .flex()
        .flex_col()
        .child(sheet_head(&format!("{key}-head"), title, ladder, theme))
        .child(ref_sheet_controls(menu, forge, cx, ladder, theme))
        .child(ref_sheet_list(menu, forge, cx, ladder, theme));
    design::sheet(id(format!("{key}-sheet")), title.to_owned(), ladder, theme)
        .detents([0.6, 0.92])
        .on_dismiss(close)
        .child(body)
        .into_any_element()
}

/// The sheet's Branches | Tags and its filter.
fn ref_sheet_controls(
    menu: Menu,
    forge: &Forge,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> Div {
    let key = menu.key();
    let choices = [("branches", "Branches"), ("tags", "Tags")]
        .map(|(slug, word)| (id(format!("{key}-{slug}")), SharedString::from(word)));
    let pick = cx.processor(move |forge, index: usize, _, cx| {
        forge.menu_tags = index == 1;
        forge.menu_step(0, cx);
    });
    let kinds = (menu != Menu::Head).then(|| {
        design::segmented(
            id(format!("{key}-kind")),
            words::BRANCHES_OR_TAGS,
            ladder,
            theme,
            Some(usize::from(forge.menu_tags)),
            choices,
            pick,
        )
    });
    let filter = Input::new(id(format!("{key}-filter")), &forge.menu_search, "Filter")
        .autocorrect(false)
        .autocapitalize(Autocapitalize::None)
        .virtual_keyboard(VirtualKeyboard::Search)
        .placeholder("Filter")
        .on_change(cx.listener(|forge, _: &view::wire::TextChange, _, cx| forge.menu_step(0, cx)));
    div()
        .flex()
        .flex_col()
        .gap(design::space::SM)
        .p(design::space::BLOCK)
        .children(kinds)
        .child(search_field(filter, "search", ladder, theme))
}

/// The refs the sheet lists, a row each, as one `Menu` composite the
/// arrows walk (`{key}-menu`); virtual, as a repository can hold many.
fn ref_sheet_list(
    menu: Menu,
    forge: &Forge,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> AnyElement {
    let names = Rc::new(forge.menu_list(menu));
    let count = names.len();
    let active = forge.menu_cursor.min(count.saturating_sub(1));
    // the default head is saved as it is picked (`SettingsTab`)
    let pick = Rc::new(cx.processor(move |forge, name: Vec<u8>, window, cx| {
        forge.menu_pick(menu, name, window, cx);
        if menu == Menu::Head {
            forge.configure(cx);
        }
    }));
    let paint = ref_rows(
        menu,
        names.clone(),
        (forge.default_head(), forge.menu_picked(menu)),
        active,
        pick.clone(),
        (*ladder, *theme),
    );
    let list = uniform_list(id(format!("{}-items", menu.key())), count, paint)
        .track_scroll(&forge.menu_scroll)
        .size_full();
    design::composite(id(format!("{}-menu", menu.key())), Role::Menu, "Refs")
        .active(active, count)
        .track_scroll(&forge.menu_scroll)
        .on_move(cx.processor(|forge, index: usize, _, cx| {
            forge.menu_cursor = index;
            cx.notify();
        }))
        .on_press(move |index, window, app| pick(names[index].clone(), window, app))
        .build()
        .flex_1()
        .min_h(px(0.))
        .child(list)
        .into_any_element()
}

/// Paints the ref sheet's rows in view: the name in the data face,
/// `default` on the default head, a check on the picked one.
fn ref_rows(
    menu: Menu,
    names: Rc<Vec<Vec<u8>>>,
    (head, picked): (Vec<u8>, Vec<u8>),
    active: usize,
    pick: Route<Vec<u8>>,
    (ladder, theme): (Ladder, Theme),
) -> impl Fn(Range<usize>, &mut Window, &mut App) -> Vec<AnyElement> + 'static {
    move |range, _, _| {
        range
            .map(|index| {
                let name = &names[index];
                let mut row = Row::data(
                    id(format!("forge-ref-{}", crate::words::path_text(name))),
                    ref_label(name),
                    &ladder,
                );
                if *name == head {
                    let badge = design::badge(
                        id(format!("{}-default", menu.key())),
                        "default",
                        &ladder,
                        theme.muted,
                        theme.surface_raised,
                    );
                    row = row.tail(badge);
                }
                if *name == picked {
                    row = row
                        .tail(design::icon("check", ladder.icon_row).text_color(theme.foreground));
                }
                let (pick, name) = (pick.clone(), name.clone());
                let row = row.draw(&ladder, &theme).on_click(
                    move |_: &ClickEvent, window: &mut Window, app: &mut App| {
                        pick(name.clone(), window, app)
                    },
                );
                design::item(row, Role::MenuItem, index == active).into_any_element()
            })
            .collect()
    }
}

// ------------------------------------------------------------------ the people sheet

/// What the people sheet is for: its id, title, the line under it, and
/// who it leaves out.
pub(crate) struct PeopleAsk<'a> {
    pub(crate) key: &'static str,
    pub(crate) title: &'static str,
    pub(crate) caption: Option<&'static str>,
    pub(crate) skip: &'a [forge::Principal],
}

/// The people sheet (`parts.tsx` `PeopleSheet`): the roster's people
/// whose name or number holds the filter, a row each (avatar, name,
/// `account N`); a pick closes it.
pub(crate) fn people_sheet(
    ask: PeopleAsk<'_>,
    pick: impl Fn(&mut Forge, forge::Principal, &mut Context<Forge>) + Clone + 'static,
    forge: &Forge,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> AnyElement {
    let close = cx.listener(|forge, _: &(), _, cx| forge.close_sheet(cx));
    let filter = Input::new(
        id(format!("{}-filter", ask.key)),
        &forge.people_search,
        words::SEARCH_MEMBERS,
    )
    .autocorrect(false)
    .autocapitalize(Autocapitalize::None)
    .virtual_keyboard(VirtualKeyboard::Search)
    .placeholder(words::SEARCH_MEMBERS);
    let head = sheet_head(&format!("{}-head", ask.key), ask.title, ladder, theme)
        .children(ask.caption.map(|said| caption(said, ladder, theme)));
    let body = div()
        .id(id(format!("{}-body", ask.key)))
        .size_full()
        .flex()
        .flex_col()
        .child(head)
        .child(
            div()
                .p(design::space::BLOCK)
                .child(search_field(filter, "search", ladder, theme)),
        )
        .child(people_rows(&ask, pick, forge, cx, ladder, theme));
    design::sheet(id(ask.key), ask.title, ladder, theme)
        .detents([0.6, 0.92])
        .on_dismiss(close)
        .child(body)
        .into_any_element()
}

fn people_rows(
    ask: &PeopleAsk<'_>,
    pick: impl Fn(&mut Forge, forge::Principal, &mut Context<Forge>) + Clone + 'static,
    forge: &Forge,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> AnyElement {
    let Some(names) = forge.names.ready() else {
        return quiet(
            id(format!("{}-reading", ask.key)),
            words::READING_ROSTER,
            ladder,
            theme,
        )
        .into_any_element();
    };
    let typed = forge.people_search.text().trim().to_lowercase();
    let people: Vec<forge::Principal> = names
        .people()
        .map(forge::Principal::Account)
        .filter(|person| !ask.skip.contains(person))
        .filter(|person| {
            let number = person.account().unwrap_or_default().to_string();
            let name = names.name(person).unwrap_or_default().to_lowercase();
            typed.is_empty() || number == typed || name.contains(&typed)
        })
        .collect();
    if people.is_empty() {
        return quiet(
            id(format!("{}-none", ask.key)),
            "No member reads like that.",
            ladder,
            theme,
        )
        .into_any_element();
    }
    let rows = people.into_iter().map(|person| {
        let name = names.member(&person);
        let number = person.account().unwrap_or_default();
        let pick = pick.clone();
        let chosen = person.clone();
        let press = cx.listener(move |forge, _: &ClickEvent, _, cx| {
            forge.close_sheet(cx);
            pick(forge, chosen.clone(), cx);
        });
        Row::titled(id(format!("{}-{number}", ask.key)), name.clone(), ladder)
            .lead(design::avatar(&name, ladder.avatar, theme))
            .caption(caption(format!("account {number}"), ladder, theme))
            .draw(ladder, theme)
            .role(Role::Button)
            .focusable()
            .on_click(press)
    });
    div()
        .id(id(format!("{}-rows", ask.key)))
        .flex_1()
        .min_h(px(0.))
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .children(rows)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_size_reads_as_the_expo_app_says_it() {
        for (bytes, said) in [
            (38, "38 B"),
            (1023, "1023 B"),
            (6963, "6.8 KB"),
            (12 * 1024, "12 KB"),
            (1_468_006, "1.4 MB"),
        ] {
            assert_eq!(super::size_of(bytes), said);
        }
    }
}
