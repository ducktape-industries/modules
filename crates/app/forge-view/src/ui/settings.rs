//! Repository settings: the default head, the force/delete flags and who
//! may write, as setting rows. Nothing the contract does not expose
//! appears here.
use ducktape_view_guest::design;
use ducktape_view_guest::prelude::*;
use ducktape_view_guest::{Div, FontWeight, MouseDownEvent, Stateful};

use crate::Forge;
use crate::state::{Menu, SettingsForm};
use crate::ui::changes::people_picker;
use crate::ui::components::{button, dropdown, id, menu_item, ref_label};
use crate::ui::{PAGE_X, pending, scroller, staged};
use forge::Reply;

/// The widest the rows run.
const ROWS_W: Pixels = px(664.);
/// The grant field.
const GRANT_W: Pixels = px(220.);

pub(crate) fn render(forge: &Forge, cx: &mut Context<Forge>, theme: &Theme) -> AnyElement {
    let column = scroller("forge-settings")
        .px(PAGE_X + design::space::SM)
        .py(design::space::LG)
        .gap_0()
        .child(pending(forge, "settings", theme));
    let reply = match staged(
        forge,
        &forge.repo_query(),
        "forge-settings-repo",
        "Reading this repository…",
        cx,
        theme,
    ) {
        Ok(reply) => reply,
        Err(state) => return column.child(state).into_any_element(),
    };
    let Reply::Repo { repo, writers, .. } = reply else {
        return column.into_any_element();
    };
    let Some(form) = forge.repo_settings.clone() else {
        return column.into_any_element();
    };
    let save = cx.listener(|forge, _: &ClickEvent, _, cx| forge.configure(cx));
    let rows = div()
        .id(id("forge-settings-rows"))
        .max_w(ROWS_W)
        .flex()
        .flex_col()
        .child(group("forge-settings-title", "Refs", theme))
        .child(design::setting_row(
            id("forge-settings-heads"),
            "Default head",
            "What a clone checks out and where changes point by default.",
            heads(forge, &form, cx, theme),
            theme,
        ))
        .children(flags(forge, &form, cx, theme))
        .child(
            div().flex().justify_end().py(design::space::LG).child(
                button(id("forge-settings-save"), "Save", theme, save)
                    .kind(design::Kind::Primary)
                    .enabled(forge.owns_repo()),
            ),
        )
        .child(group("forge-settings-access-title", "Access", theme))
        .child(access_row(
            "owner",
            forge.principal_name(&repo.repo.owner),
            "Owner",
            None,
            theme,
        ))
        .children(writer_rows(forge, &writers.items, cx, theme))
        .child(grant_field(forge, &form, cx, theme));
    column.child(rows).into_any_element()
}

/// A group's heading, a hairline under it.
fn group(key: &str, title: &str, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id(key.to_owned()))
        .pt(design::space::LG)
        .pb(design::space::XS)
        .border_b_1()
        .border_color(theme.border)
        .text_size(design::text::SECONDARY)
        .font_weight(FontWeight::SEMIBOLD)
        .role(Role::Heading)
        .aria_level(2)
        .child(title.to_owned())
}

/// The branches a default head may name, as a dropdown.
fn heads(forge: &Forge, form: &SettingsForm, cx: &mut Context<Forge>, theme: &Theme) -> AnyElement {
    let open = forge.menu == Some(Menu::Head);
    let items = match open {
        true => forge
            .branches()
            .into_iter()
            .map(|name| {
                let pick = cx.listener({
                    let name = name.clone();
                    move |forge, _: &ClickEvent, _, cx| {
                        if let Some(form) = &mut forge.repo_settings {
                            form.head = name.clone();
                        }
                        forge.open_menu(None, cx);
                    }
                });
                menu_item(
                    id(format!("forge-settings-head-{}", ref_label(&name))),
                    ref_label(&name),
                    None,
                    form.head == name,
                    theme,
                    pick,
                )
            })
            .collect(),
        false => Vec::new(),
    };
    dropdown(
        "forge-settings-head",
        ref_label(&form.head),
        open,
        items,
        theme,
        cx.listener(|forge, _: &ClickEvent, _, cx| forge.open_menu(Some(Menu::Head), cx)),
        cx.listener(|forge, _: &MouseDownEvent, _, cx| forge.open_menu(None, cx)),
    )
}

/// The force and delete switches.
fn flags(
    forge: &Forge,
    form: &SettingsForm,
    cx: &mut Context<Forge>,
    theme: &Theme,
) -> [Stateful<Div>; 2] {
    let force = cx.listener(|forge, _: &ClickEvent, _, cx| {
        if let Some(form) = &mut forge.repo_settings {
            form.allow_force = !form.allow_force;
        }
        cx.notify();
    });
    let delete = cx.listener(|forge, _: &ClickEvent, _, cx| {
        if let Some(form) = &mut forge.repo_settings {
            form.allow_delete = !form.allow_delete;
        }
        cx.notify();
    });
    let owns = forge.owns_repo();
    [
        design::setting_row(
            id("forge-settings-force-row"),
            "Allow force pushes",
            "Let writers rewrite a branch's history.",
            design::switch(
                id("forge-settings-force"),
                "Allow force pushes",
                form.allow_force,
                owns,
                theme,
                force,
            ),
            theme,
        ),
        design::setting_row(
            id("forge-settings-delete-row"),
            "Allow ref deletion",
            "Let writers delete branches and tags.",
            design::switch(
                id("forge-settings-delete"),
                "Allow ref deletion",
                form.allow_delete,
                owns,
                theme,
                delete,
            ),
            theme,
        ),
    ]
}

/// One person with access: avatar, name over what they may do, and what
/// the owner can take back.
fn access_row(
    key: &str,
    name: String,
    role: &str,
    action: Option<AnyElement>,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id(format!("forge-writer-{key}")))
        .flex()
        .items_center()
        .gap(design::space::LG)
        .py(design::space::MD)
        .border_b_1()
        .border_color(theme.border)
        .child(
            design::avatar(&name, design::size::AVATAR, theme)
                .border_1()
                .border_color(theme.border)
                .font_weight(FontWeight::SEMIBOLD),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .child(div().font_weight(FontWeight::MEDIUM).child(name))
                .child(
                    div()
                        .text_size(design::text::SECONDARY)
                        .text_color(theme.muted)
                        .child(role.to_owned()),
                ),
        )
        .children(action)
}

/// Who to grant write access to, and Grant: an account number typed, or a
/// name searched and picked from the roster, which fills in its number.
fn grant_field(
    forge: &Forge,
    form: &SettingsForm,
    cx: &mut Context<Forge>,
    theme: &Theme,
) -> Stateful<Div> {
    let typed = cx.listener(|forge, text: &String, _, cx| {
        if let Some(form) = &mut forge.repo_settings {
            form.grant = text.clone();
        }
        cx.notify();
    });
    let grant = cx.listener(|forge, _: &ClickEvent, _, cx| forge.grant(cx));
    // a typed number needs no search; a name does
    let needle = form.grant.trim();
    let searching = !needle.is_empty() && forge::Principal::parse(needle).is_none();
    let matches = match searching {
        true => {
            let fill = |forge: &mut Forge, person: forge::Principal| {
                if let (Some(form), Some(number)) = (&mut forge.repo_settings, person.account()) {
                    form.grant = number.to_string();
                }
            };
            people_picker(
                forge,
                "forge-grant-pick",
                needle,
                |_| false,
                fill,
                cx,
                theme,
            )
        }
        false => Vec::new(),
    };
    let control = div()
        .flex()
        .gap(design::space::SM)
        .items_center()
        .child(
            Input::new(id("forge-settings-grant-input"), "Grant write access")
                .h(design::size::CONTROL)
                .w(GRANT_W)
                .px_2()
                .border_1()
                .border_color(theme.border_strong)
                .bg(theme.background)
                .text_color(theme.foreground)
                .value(form.grant.clone())
                .placeholder("Search members")
                .on_input(typed),
        )
        .child(
            button(id("forge-settings-grant"), "Grant", theme, grant)
                .kind(design::Kind::Outline)
                .enabled(forge.owns_repo()),
        );
    div()
        .id(id("forge-settings-access"))
        .flex()
        .flex_col()
        .child(design::setting_row(
            id("forge-settings-grant-row"),
            "Grant write access",
            "Pick a member. They can push and merge here.",
            control,
            theme,
        ))
        .when(!matches.is_empty(), |access| {
            access.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .py(design::space::SM)
                    .children(matches),
            )
        })
}

/// One row per writer with its Revoke.
fn writer_rows(
    forge: &Forge,
    writers: &[forge::Principal],
    cx: &mut Context<Forge>,
    theme: &Theme,
) -> Vec<Stateful<Div>> {
    writers
        .iter()
        .map(|key| {
            let revoke = cx.listener({
                let key = key.clone();
                move |forge, _: &ClickEvent, _, cx| forge.revoke(key.clone(), cx)
            });
            let owns = forge.owns_repo();
            let danger = theme.danger;
            let action = div()
                .id(id(format!("forge-settings-revoke-{}", principal_id(key))))
                .px(design::space::SM)
                .py(design::space::XXS)
                .text_size(design::text::SECONDARY)
                .text_color(if owns { danger } else { theme.muted })
                .role(Role::Button)
                .when(!owns, |action| action.aria_disabled(true))
                .when(owns, |action| {
                    action
                        .hover(|style| style.text_decoration_1())
                        .focusable()
                        .on_click(revoke)
                })
                .child("Revoke");
            access_row(
                &principal_id(key),
                forge.principal_name(key),
                "Can write",
                Some(action.into_any_element()),
                theme,
            )
        })
        .collect()
}

/// A writer's element id: `acct-<n>` for an account.
fn principal_id(principal: &forge::Principal) -> String {
    match principal {
        forge::Principal::Account(number) => format!("acct-{number}"),
        forge::Principal::Root => "system".into(),
    }
}
