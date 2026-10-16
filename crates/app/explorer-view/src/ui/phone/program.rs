//! Expo `app/(tabs)/explorer/program/[name].tsx` (mobile `explorer-detail`
//! b2de83e, no screenshot yet): one program, or one view-only entry, over
//! the root. Its hook and role, what its code's view asks of the host,
//! the code and its size, the params, who maintains it, what the view
//! declares, what is scheduled for it, the way to its transactions. The
//! reader's presses stay on the Programs page's sheet. Reads the programs
//! role, the code's manifest (`manifest`), the window; writes the route
//! and the clipboard (Copy).
use super::*;
use crate::Network;
use crate::decode::{preview, size};
use crate::state::{Declared, blob_ref};
use crate::ui::programs::{allows, name_of, role_of};
use view::methods::Manifested;

pub(super) fn program(view: &Explorer, name: &str, cx: Cx, k: &Kit) -> AnyElement {
    let code = view.code_of(name);
    let copy = copy_link(view, &Route::Program(name.to_owned()), cx, k);
    let page = over("explorer-program", name.to_owned(), view, cx, k);
    let page = match code.is_some() {
        true => page.caption(match view.runs(name) {
            Some(_) => "program",
            None => "view only",
        }),
        false => page,
    };
    let page = match copy {
        Some(copy) => page.trailing(copy),
        None => page,
    };
    let body = match (&view.network, code) {
        (Loadable::Failed(refusal), _) => refused(&refusal.message, cx, k),
        (Loadable::Idle | Loadable::Loading(_), _) => {
            quiet("explorer-program-loading", "Reading the programs role…", k)
        }
        (Loadable::Ready(_) | Loadable::Reloading(..), None) => empty(
            "explorer-no-program",
            "search-x",
            format!("Nothing called {name}"),
            "The programs role runs and lists nothing by this name.",
            k,
        ),
        (Loadable::Ready(network) | Loadable::Reloading(network, _), Some(code)) => {
            detail(view, network, name, &code, cx, k)
        }
    };
    page.child(body).into_any_element()
}

/// The page under its title row (Expo `Detail`).
fn detail(
    view: &Explorer,
    network: &Network,
    name: &str,
    code: &abi::BlobId,
    cx: Cx,
    k: &Kit,
) -> AnyElement {
    let listed = view.runs(name).is_none();
    scroller("explorer-page")
        .children(standing(network, name, k))
        .children(capabilities(view, listed, cx, k))
        .child(section("explorer-about-header", "About", None, k))
        .children(about(view, network, name, code, cx, k))
        .children(scheduled(view, network, name, k))
        .children((!listed).then(|| transactions(view, name, cx, k)))
        .into_any_element()
}

/// The hook and the role it fills, as badges; neither, nothing.
fn standing(network: &Network, name: &str, k: &Kit) -> Option<Div> {
    let hook = network.hooks.iter().any(|hook| hook == name);
    let role = network
        .roles
        .as_ref()
        .and_then(|roles| role_of(roles, name));
    if !hook && role.is_none() {
        return None;
    }
    let badge = |id: &'static str, label: String| {
        design::badge(id, label, &k.ladder, k.theme.muted, k.theme.surface_raised)
    };
    Some(
        div()
            .flex()
            .gap(space::SM)
            .px(space::BLOCK)
            .pt(space::MD)
            .children(hook.then(|| badge("explorer-hook", "hook".into())))
            .children(role.map(|role| badge("explorer-role", role.to_owned()))),
    )
}

/// What the code's view asks of the host, a row each, under its head; or
/// why none is shown (Expo `Capabilities`).
fn capabilities(view: &Explorer, listed: bool, cx: Cx, k: &Kit) -> Vec<AnyElement> {
    let declared = view.declared(listed);
    let count = match &declared {
        Declared::Asks(manifest) => Some(aside(plural(
            manifest.capabilities.len() as u64,
            "capability",
            "capabilities",
        ))),
        Declared::Reading | Declared::Refused(_) | Declared::Nothing(_) => None,
    };
    let head = section("explorer-capabilities-header", "Capabilities", count, k);
    let rows = match declared {
        Declared::Reading => vec![quiet("explorer-manifest-loading", "Reading the code…", k)],
        Declared::Refused(sentence) => vec![refused(sentence, cx, k)],
        Declared::Nothing(why) => vec![quiet("explorer-unlisted", why.sentence(), k)],
        Declared::Asks(manifest) => manifest
            .capabilities
            .iter()
            .map(|capability| {
                key_value(capability.as_str(), div().child(allows(*capability)), k)
                    .into_any_element()
            })
            .collect(),
    };
    std::iter::once(head.into_any_element())
        .chain(rows)
        .collect()
}

/// The code and Copy, its size, the params, who maintains it, and what its
/// view declares: name, min width, the programs it talks to, its wire.
fn about(
    view: &Explorer,
    network: &Network,
    name: &str,
    code: &abi::BlobId,
    cx: Cx,
    k: &Kit,
) -> Vec<AnyElement> {
    let blob = view.manifest.ready().and_then(Option::as_ref);
    let declared = blob
        .and_then(|blob| blob.view.as_ref())
        .and_then(|carried| carried.manifest.as_ref());
    let entry = view.runs(name);
    let mut rows = vec![key_value("Code", code_and_copy(code, cx, k), k)];
    rows.extend(blob.map(|blob| key_value("Size", div().child(sizes(blob, entry.is_none())), k)));
    rows.extend(entry.map(|entry| key_value("Params", params(&entry.params), k)));
    rows.push(key_value(
        "Maintained by",
        div().child(maintained_by(view, network, name)),
        k,
    ));
    rows.extend(declared.into_iter().flat_map(|manifest| {
        let says = match manifest.description.is_empty() {
            true => manifest.name.clone(),
            false => format!("{} · {}", manifest.name, manifest.description),
        };
        let talks = (!manifest.targets.is_empty())
            .then(|| key_value("Talks to", data(manifest.targets.join(", ")), k));
        [
            Some(key_value("Declares", div().child(says), k)),
            Some(key_value(
                "Min width",
                div().child(format!("{} px", manifest.min_width)),
                k,
            )),
            talks,
            Some(key_value("Wire", data(manifest.wire_id.clone()), k)),
        ]
        .into_iter()
        .flatten()
    }));
    rows.into_iter()
        .map(IntoElement::into_any_element)
        .collect()
}

/// The code id in the data face, and a small Copy beside it.
fn code_and_copy(code: &abi::BlobId, cx: Cx, k: &Kit) -> Div {
    let id = blob_ref(code);
    let copy: Press = {
        let id = id.clone();
        Box::new(
            cx.listener(move |view: &mut Explorer, _: &ClickEvent, _, cx| {
                view.copy(id.clone(), "the code id", cx)
            }),
        )
    };
    div()
        .flex()
        .items_center()
        .gap(space::SM)
        .child(data(id).flex_1().min_w_0())
        .child(small(
            "explorer-copy-code",
            "Copy",
            design::Kind::Outline,
            Some(copy),
            k,
        ))
}

/// How big the code is, and how much of it the view, for a program that
/// carries one.
fn sizes(blob: &Manifested, listed: bool) -> String {
    match &blob.view {
        Some(carried) if !listed => format!(
            "{} of code, {} of it the view",
            size(blob.size),
            size(carried.size)
        ),
        _ => size(blob.size),
    }
}

/// The params it runs with: "none", or their length and ends in the data
/// face.
fn params(params: &[u8]) -> Div {
    match params.is_empty() {
        true => div().child("none"),
        false => data(preview(
            params.len() as u64,
            &params[..params.len().min(32)],
        )),
    }
}

/// Who changes it (Expo `maintainedBy`): the validators' vote for a role's
/// program; else each maintainer (you, an agent, marked so) and the
/// every-program ones; else the vote alone.
fn maintained_by(view: &Explorer, network: &Network, name: &str) -> String {
    let Some(maintainers) = network.maintainers(name) else {
        return "validators' vote".into();
    };
    let mut named: Vec<String> = maintainers
        .named
        .iter()
        .map(|&number| {
            let you = match view.acting.account == Some(number) {
                true => " (you)",
                false => "",
            };
            let agent = match view.account(number).map(|account| account.kind()) {
                Some(identity::Kind::Program { .. }) => " · agent",
                _ => "",
            };
            format!("{}{you}{agent}", name_of(view, number))
        })
        .collect();
    if maintainers.every {
        named.push("every-program maintainers".into());
    }
    match named.is_empty() {
        true => "no account: the validators' vote only".into(),
        false => named.join(", "),
    }
}

/// What is scheduled for it, under its head, each row as the Programs
/// page says it; none, a quiet line.
fn scheduled(view: &Explorer, network: &Network, name: &str, k: &Kit) -> Vec<AnyElement> {
    let head = section("explorer-program-scheduled-header", "Scheduled", None, k);
    let rows: Vec<AnyElement> = network
        .changes
        .iter()
        .enumerate()
        .filter(|(_, pending)| pending.change.program() == name)
        .map(|(index, pending)| {
            programs::scheduled_row(view, network, index, pending, k).into_any_element()
        })
        .collect();
    let none = rows.is_empty().then(|| {
        quiet(
            "explorer-program-no-changes",
            "Nothing is scheduled for it.",
            k,
        )
    });
    std::iter::once(head.into_any_element())
        .chain(rows)
        .chain(none)
        .collect()
}

/// The way to its transactions: how many the window holds, and over how
/// many blocks.
fn transactions(view: &Explorer, name: &str, cx: Cx, k: &Kit) -> Stateful<Div> {
    let blocks = view.chain.blocks.len() as u64;
    let sent = view.chain.txs.iter().filter(|tx| tx.target == name).count() as u64;
    let title = match blocks {
        0 => "Transactions →".to_owned(),
        _ => format!(
            "{} in the last {} →",
            plural(sent, "transaction", "transactions"),
            plural(blocks, "block", "blocks")
        ),
    };
    let row = design::row("explorer-program-txs", &k.ladder, &k.theme);
    opens(
        row,
        title.clone(),
        Route::Transactions(Some(name.to_owned())),
        cx,
    )
    .child(div().flex_1().truncate().child(title))
}
