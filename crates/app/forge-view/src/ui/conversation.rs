//! A change's conversation: its body, its reviews, and the replies in
//! chat's hidden channel beneath them, with a composer at the end.
use ducktape_view_guest::EditorElement;
use ducktape_view_guest::design;
use ducktape_view_guest::prelude::*;

use crate::Forge;
use crate::state::verdict_verb;
use crate::ui::components::{
    badge, button, empty_state, id, path_text, quiet, ref_label, short_hex,
};
use crate::ui::scroller;
use ducktape_view_guest::view::Loadable;
use forge::ChangeState;

/// A review's body and comments start under its author's name, past the
/// avatar and the gap beside it.
const UNDER_NAME: Pixels = px(design::height::AVATAR as f32 + design::spacing::SM as f32);

pub(crate) fn render(forge: &Forge, cx: &mut Context<Forge>, theme: &Theme) -> AnyElement {
    let Some((change, _, _, reviews)) = forge.change() else {
        return div().into_any_element();
    };
    let mut column = scroller("forge-conversation")
        .px(crate::ui::PAGE_X)
        .py(design::space::BLOCK)
        .gap(design::space::LG);
    if !change.body.trim().is_empty() {
        column = column.child(
            div()
                .id(id("forge-change-body"))
                .pb(design::space::LG)
                .border_b_1()
                .border_color(theme.border)
                .child(crate::ui::markdown::render(
                    "forge-change-body-text",
                    &change.body,
                    theme,
                    &crate::ui::code::links(Vec::new(), cx),
                )),
        );
    }
    // Until chat answers, the reviews stand on their own; once it has, each
    // sits in the timeline where forge posted its line.
    let placed = |review: &forge::Review| match forge.messages.get(&change.channel) {
        Some(Loadable::Ready(rows)) => rows.iter().any(|row| row.message_id == review.message_id),
        _ => false,
    };
    for review in reviews.items.iter().filter(|review| !placed(review)) {
        column = column.child(review_card(forge, review, theme));
    }
    column = column.child(messages(forge, theme));
    column.child(composer(forge, cx, theme)).into_any_element()
}

/// One review as a timeline event: who, what they concluded (the verdict
/// is the verb), the block it landed in, then what it says and the line
/// comments it carried, quoted.
fn review_card(forge: &Forge, review: &forge::Review, theme: &Theme) -> AnyElement {
    let author = forge.principal_name(&review.author);
    let outdated = forge.outdated(&review.draft.commit_oid);
    let mut head = timeline_head(
        &author,
        Some(author.clone()),
        verdict_verb(review.draft.verdict),
        review.height,
        theme,
    );
    if outdated {
        head = head.child(badge(
            id(format!("forge-review-outdated-{}", review.id)),
            "outdated",
            theme.warning,
            theme.warning_soft,
        ));
    }
    let mut card = div()
        .id(id(format!("forge-review-{}", review.id)))
        .flex()
        .flex_col()
        .gap(design::space::XS)
        .child(head);
    if !review.draft.body.trim().is_empty() {
        card = card.child(div().pl(UNDER_NAME).child(review.draft.body.clone()));
    }
    for comment in &review.draft.comments {
        card = card.child(
            div().pl(UNDER_NAME).child(
                div()
                    .pl(design::space::MD)
                    .border_l_1()
                    .border_color(theme.border_strong)
                    .child(
                        design::mono(format!(
                            "{}:{} — {}",
                            path_text(&comment.path),
                            comment.line,
                            comment.body
                        ))
                        .text_size(design::text::CAPTION)
                        .text_color(theme.muted),
                    ),
            ),
        );
    }
    card.into_any_element()
}

/// A timeline line's head: the avatar, the name (bold), what they did
/// (quiet), and the block it landed in on the right.
fn timeline_head(
    avatar: &str,
    who: Option<String>,
    what: &str,
    height: u64,
    theme: &Theme,
) -> ducktape_view_guest::Div {
    div()
        .flex()
        .items_center()
        .gap(design::space::SM)
        .child(
            design::avatar(avatar, design::size::AVATAR, theme)
                .border_1()
                .border_color(theme.border)
                .font_weight(ducktape_view_guest::FontWeight::SEMIBOLD),
        )
        .children(who.map(crate::ui::bold))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .child(quiet(what.to_owned(), theme)),
        )
        .child(
            design::mono(format!("block {}", design::grouped(height)))
                .text_size(design::text::CAPTION)
                .text_color(theme.faint),
        )
}

/// One line of the change's history: who did it, if forge recorded it,
/// what happened, and the block it landed in.
fn event(key: String, who: Option<String>, what: String, height: u64, theme: &Theme) -> AnyElement {
    let avatar = who.clone().unwrap_or_default();
    div()
        .id(id(format!("forge-event-{key}")))
        .child(timeline_head(&avatar, who, &what, height, theme))
        .into_any_element()
}

/// A line forge itself posted, read from forge's records rather than its
/// text: the first opened the change, a review's names that review, and
/// the one other is how the change ended.
fn forge_line(
    forge: &Forge,
    row: &chat::MsgRow,
    opened: bool,
    theme: &Theme,
) -> Option<AnyElement> {
    let (change, _, _, reviews) = forge.change()?;
    if let Some(review) = reviews
        .items
        .iter()
        .find(|r| r.message_id == row.message_id)
    {
        return Some(review_card(forge, review, theme));
    }
    let key = row.message_id.clone();
    if opened {
        let author = forge.principal_name(&change.author);
        return Some(event(
            key,
            Some(author),
            "opened this change".into(),
            row.height,
            theme,
        ));
    }
    // a line matching no review yet may be one still paging in
    if reviews.next.is_some() {
        return None;
    }
    let actor = |principal: &Option<forge::Principal>| {
        principal
            .as_ref()
            .map(|principal| forge.principal_name(principal))
    };
    match (change.state, &change.merge_oid) {
        (ChangeState::Merged, Some(oid)) => Some(event(
            key,
            actor(&change.merged_by),
            format!(
                "merged into {} as {}",
                ref_label(&change.into),
                short_hex(oid)
            ),
            row.height,
            theme,
        )),
        (ChangeState::Closed, _) => Some(event(
            key,
            actor(&change.closed_by),
            "closed this change".into(),
            row.height,
            theme,
        )),
        _ => None,
    }
}

/// forge's own line: written in its room by the account identity names
/// forge's ([`chat::program_author`]).
fn is_forge(forge: &Forge, row: &chat::MsgRow) -> bool {
    let module = forge
        .names
        .ready()
        .and_then(|names| names.module(&row.author));
    chat::program_author(row, module) == Some(forge::MODULE)
}

/// The hidden chat channel of this change, in chat's row shape.
fn messages(forge: &Forge, theme: &Theme) -> AnyElement {
    let Some((change, _, _, _)) = forge.change() else {
        return div().into_any_element();
    };
    // forge's own lines are told by their author, whom the roster names
    let naming = forge.names.is_idle() || forge.names.is_loading();
    match forge.messages.get(&change.channel) {
        None | Some(Loadable::Idle) | Some(Loadable::Loading(_)) => {
            quiet("Reading the conversation…", theme)
        }
        Some(Loadable::Ready(_)) if naming => quiet("Reading the conversation…", theme),
        Some(Loadable::Failed(refusal)) => div()
            .id(id("forge-conversation-refused"))
            .p_2()
            .bg(theme.danger_soft)
            .text_size(design::text::SECONDARY)
            .child(refusal.message.clone())
            .into_any_element(),
        Some(Loadable::Ready(rows)) if rows.is_empty() => empty_state(
            id("forge-conversation-empty"),
            "No replies yet",
            "This change's channel is quiet.",
            theme,
        )
        .into_any_element(),
        Some(Loadable::Ready(rows)) => {
            let opened = rows
                .iter()
                .find(|row| is_forge(forge, row))
                .map(|row| row.seq);
            let mut column = div()
                .id(id("forge-conversation-messages"))
                .flex()
                .flex_col()
                .gap(design::space::BLOCK);
            for message in rows {
                if is_forge(forge, message) {
                    if let Some(line) =
                        forge_line(forge, message, opened == Some(message.seq), theme)
                    {
                        column = column.child(line);
                    }
                    continue;
                }
                let author = forge.principal_name(&message.author);
                column = column.child(
                    div()
                        .id(id(format!("forge-message-{}", message.message_id)))
                        .flex()
                        .flex_col()
                        .gap(design::space::XS)
                        .child(timeline_head(
                            &author,
                            Some(author.clone()),
                            "replied",
                            message.height,
                            theme,
                        ))
                        .child(div().pl(UNDER_NAME).child(message.text.clone())),
                );
            }
            column.into_any_element()
        }
    }
}

fn composer(forge: &Forge, cx: &mut Context<Forge>, theme: &Theme) -> AnyElement {
    let send = cx.listener(|forge, _: &ClickEvent, window, cx| forge.post_reply(window, cx));
    div()
        .id(id("forge-composer"))
        .flex()
        .gap(design::space::SM)
        .items_start()
        .pt(design::space::SM)
        .border_t_1()
        .border_color(theme.border)
        .child(
            EditorElement::plain(
                id("forge-reply"),
                &forge.reply,
                "forge-reply",
                |forge: &mut Forge| Some(&mut forge.reply),
            )
            .min_h(design::size::CONTROL)
            .flex_1()
            .px_2()
            .py(design::space::XS)
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.background)
            .text_color(theme.foreground)
            .placeholder("Reply in this change")
            .label("Reply"),
        )
        .child(
            button(id("forge-reply-send"), "Send", theme, send)
                .kind(design::Kind::Outline)
                .enabled(forge.may_write() && !forge.reply.state_view().text.trim().is_empty()),
        )
        .into_any_element()
}
