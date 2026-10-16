//! Forge on a phone: the Expo app's forge stack (`app/(tabs)/forge/**`,
//! `_layout.tsx`), a tree of its own that the view's root picks once
//! under a finger (`lib.rs`). It shares the model with the desktop
//! (`state`, `select`, `navigate`, `actions`) and draws with the SDK's
//! touch ladder; nothing in it asks which pointer it is under.
//!
//! One file per Expo screen, each naming its source: [`repos`] (Forge's
//! root), [`repo`] (a repository) with its tab bodies in [`tabs`],
//! [`change`] (a change), [`diff`] (one file of its diff), [`commit`],
//! [`code`] (one file of the Code tab), [`new_change`] (New or Edit
//! change) and [`review`] (Finish your review); [`document`] is the
//! markdown they render, [`lines`] the code and diff rows, [`parts`] what
//! several of them draw.
//!
//! Reads the route ([`Nav::pages`](crate::state::Nav::pages)), the open
//! sheet and the open form; writes Back.
mod change;
mod code;
mod commit;
mod diff;
mod document;
mod lines;
mod new_change;
mod parts;
mod repo;
mod repos;
mod review;
mod tabs;

use view::design::Ladder;
use view::prelude::*;

use crate::Forge;
use crate::state::{Dock, Menu, Page, Sheet};
use crate::ui::components::id;
use parts::Back;

pub(crate) use lines::Block;

/// The phone's screen: the top page of the route (or the change form
/// over it), the one sheet open over that, and Back while there is
/// somewhere to go back to (rule 13).
pub(crate) fn render(forge: &mut Forge, cx: &mut Context<Forge>) -> AnyElement {
    let theme = *cx.global::<Theme>();
    let ladder = &design::TOUCH;
    let page = page(forge, cx, ladder, &theme);
    let sheet = sheet(forge, cx, ladder, &theme);
    let somewhere = forge.nav().depth() > 0 || forge.sheet_open() || forge.change_form().is_some();
    div()
        .id(id("forge"))
        .size_full()
        .flex()
        .flex_col()
        .bg(theme.background)
        .text_color(theme.foreground)
        .text_type(ladder.body)
        .when(somewhere, |root| {
            root.on_back(cx.listener(|forge, _: &BackEvent, window, cx| forge.back(window, cx)))
        })
        .child(page)
        .children(sheet)
        .into_any_element()
}

/// The page on top: the change form while one is open, else the route's
/// top page.
fn page(forge: &Forge, cx: &mut Context<Forge>, ladder: &Ladder, theme: &Theme) -> AnyElement {
    if let Some(form) = forge.change_form() {
        return new_change::render(forge, form, cx, ladder, theme);
    }
    let back: Back = (forge.nav().depth() > 0).then(|| {
        Box::new(cx.listener(|forge, _: &ClickEvent, window, cx| forge.back(window, cx)))
            as parts::Press
    });
    match forge.nav().pages().last().copied().unwrap_or(Page::Repos) {
        Page::Repos => repos::render(forge, cx, ladder, theme),
        Page::Repo => repo::render(forge, back, cx, ladder, theme),
        Page::Change => change::render(forge, back, cx, ladder, theme),
        Page::File => diff::render(forge, back, cx, ladder, theme),
        Page::Commit => commit::render(forge, back, cx, ladder, theme),
        Page::Blob => code::render(forge, back, cx, ladder, theme),
    }
}

/// The one sheet open over the page, if any: a ref picker, a change's
/// actions, a member to grant or to ask for a review, About, New
/// repository, or Finish your review.
fn sheet(
    forge: &Forge,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> Option<AnyElement> {
    if let Some(menu) = forge.menu {
        let title = match menu {
            Menu::Ref => "Pick a ref",
            Menu::Head => "Default head",
        };
        return Some(parts::ref_sheet(menu, title, forge, cx, ladder, theme));
    }
    match forge.sheet {
        Some(Sheet::ChangeActions) => return Some(change::actions_sheet(forge, cx, ladder, theme)),
        Some(Sheet::Grant) => return Some(grant_sheet(forge, cx, ladder, theme)),
        Some(Sheet::Reviewers) => {
            return Some(new_change::reviewers_sheet(forge, cx, ladder, theme));
        }
        None => {}
    }
    if forge.nav().dock == Some(Dock::About) && forge.nav().change.is_none() {
        return Some(repo::about_sheet(forge, cx, ladder, theme));
    }
    if let Some(form) = &forge.new_repo {
        return Some(repos::new_repository(form, cx, ladder, theme));
    }
    let review = forge.review().filter(|review| review.finishing)?;
    Some(review::render(forge, review, cx, ladder, theme))
}

/// A member to grant write access to (`SettingsTab`'s `PeopleSheet`):
/// everyone but the owner and those who already write.
fn grant_sheet(
    forge: &Forge,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> AnyElement {
    let skip: Vec<forge::Principal> = forge
        .repo()
        .map(|(info, _, writers)| {
            std::iter::once(info.repo.owner.clone())
                .chain(writers.items.iter().cloned())
                .collect()
        })
        .unwrap_or_default();
    let ask = parts::PeopleAsk {
        key: "forge-grant-sheet",
        title: "Grant write access",
        caption: Some("Pick a member. They can push and merge here."),
        skip: &skip,
    };
    let grant = |forge: &mut Forge, person: forge::Principal, cx: &mut Context<Forge>| {
        if let Some(form) = &mut forge.repo_settings {
            form.grant
                .reset(person.account().unwrap_or_default().to_string());
        }
        forge.grant(cx);
    };
    parts::people_sheet(ask, grant, forge, cx, ladder, theme)
}
