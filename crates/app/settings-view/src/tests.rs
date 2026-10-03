use crate::Settings;
use crate::api::*;
use ducktape_view_guest::methods::{Changes, ClipboardWrite, Query};
use ducktape_view_guest::testing::{StreamSender, TestAppContext};
use ducktape_view_guest::{Theme, wire};
use identity::Identity;
use valset::Valset;

fn account(number: u64, name: &str, control: identity::Control) -> identity::Account {
    identity::Account {
        number,
        card: identity::Card {
            name: name.into(),
            avatar: None,
            bio: None,
            updated_at: 1,
        },
        control,
    }
}
/// Maya's account: a person holding one key.
fn maya(number: u64, label: &str) -> identity::Account {
    let keys = vec![identity::Key {
        scheme: abi::Scheme::Ed25519,
        key: vec![0xab, 0xcd],
        label: Some(label.into()),
        added_at: 1,
    }];
    account(number, "Maya", identity::Control::Person { keys })
}
/// Scout, the agent Maya (7) manages, keyless.
fn scout() -> identity::Account {
    account(
        12,
        "Scout",
        identity::Control::Managed {
            manager: 7,
            category: identity::Category::Agent,
            life: identity::Life::Active { keys: Vec::new() },
        },
    )
}
fn respond(cx: &TestAppContext) {
    cx.host().handle::<Query<Identity>>(|q| {
        Ok(match q {
            identity::Query::Get { number } => {
                assert_eq!(number, 7);
                identity::Reply::Account(Some(maya(number, "Laptop key")))
            }
            identity::Query::Managed { by: 7, .. } => {
                identity::Reply::Accounts(identity::PageResponse {
                    height: 42,
                    items: vec![scout()],
                    next: None,
                })
            }
            q => panic!("unexpected query: {q:?}"),
        })
    });
    cx.host().handle::<Query<Valset>>(|q| {
        Ok(match q {
            valset::Query::Membership { key } => {
                valset::Reply::Membership(Some(valset::Membership {
                    key,
                    address: "127.0.0.1:19001".into(),
                    role: valset::Role::Validator,
                }))
            }
            q => panic!("unexpected query: {q:?}"),
        })
    });
}
fn fixture(state: &str, dark: bool) -> TestAppContext {
    seated(state, dark).0
}
/// The fixture, and the session feed the host speaks through.
fn seated(state: &str, dark: bool) -> (TestAppContext, StreamSender<HostSession>) {
    let mut cx = TestAppContext::new();
    let props = cx.host().stream::<HostSession>();
    respond(&cx);
    match state {
        "unregistered" => cx.host().handle::<Query<Identity>>(|q| match q {
            identity::Query::Resolve { references } => {
                Ok(identity::Reply::Resolved(vec![None; references.len()]))
            }
            q => panic!("an unregistered key asks identity who holds it, nothing more: {q:?}"),
        }),
        "suspended" => cx.host().handle::<Query<Identity>>(|q| {
            Ok(match q {
                identity::Query::Resolve { references } => {
                    assert_eq!(references, vec![identity::Reference::Key(vec![0xab, 0xcd])]);
                    identity::Reply::Resolved(vec![Some(12)])
                }
                identity::Query::Profile { number: 12 } => {
                    identity::Reply::Profile(Some(identity::Profile {
                        number: 12,
                        name: "Scout".into(),
                        kind: identity::Kind::Managed {
                            manager: 7,
                            category: identity::Category::Agent,
                            standing: identity::Standing::Suspended,
                        },
                    }))
                }
                q => panic!("unexpected query: {q:?}"),
            })
        }),
        "loading" => {
            cx.host().never::<Query<Identity>>();
        }
        "refused" => {
            cx.host()
                .refuse::<Query<Identity>>("unavailable", "Account query refused.");
        }
        _ => {}
    }
    cx.set_global(if dark { Theme::dark() } else { Theme::light() });
    cx.open::<Settings>();
    props.send(Session {
        signer: if state == "empty" {
            String::new()
        } else {
            "abcd".into()
        },
        account: (!matches!(state, "empty" | "unregistered" | "suspended")).then_some(7),
        endpoint: "http://127.0.0.1:19001".into(),
        ..Session::default()
    });
    cx.run_until_parked();
    if state.starts_with("invite") {
        cx.simulate_click("settings/nav/invites");
        cx.run_until_parked();
        match state {
            "invite-loading" => cx.host().never::<InviteCreate>(),
            "invite-refused" => cx
                .host()
                .refuse::<InviteCreate>("forbidden", "This node does not allow minting invites."),
            _ => cx.host().handle::<InviteCreate>(|request| {
                assert_eq!(request.ttl_days, 7);
                Ok(Invite {
                    invite: "duck-invite:workshop-loopback-example".into(),
                    notes: vec![ducktape_view_guest::host::Error {
                        code: "expires".into(),
                        message: "This invite expires in 7 days.".into(),
                    }],
                })
            }),
        }
        cx.simulate_click("settings/invite/mint");
        cx.run_until_parked();
    }
    (cx, props)
}
// at 480 the Agents page's "Create agent" and "Add key" are cut
#[test]
fn the_view_is_laid_out_from_560() {
    assert_eq!(
        <Settings as ducktape_view_guest::View>::MIN_WINDOW_WIDTH,
        560
    );
}

#[test]
fn four_states_are_honest() {
    assert!(fixture("loading", false).has_text("Reading your account…"));
    assert!(fixture("refused", false).has_text("Account query refused."));
    assert!(fixture("empty", false).has_text("No account"));
    let cx = fixture("ready", false);
    for text in [
        "Maya",
        "account 7 · Person",
        "Keys",
        "Laptop key",
        "abcd",
        "Validator",
    ] {
        assert!(cx.has_text(text), "{text}: {:?}", cx.texts());
    }
    // the node's status is the Nodes view's, the app's preferences the gear's
    assert!(!cx.has_text("Node"));
    assert!(cx.find("settings/app/body").is_none());
}

/// The left menu lists Agents only for an account that manages agents,
/// and each entry opens its pane.
#[test]
fn the_menu_opens_one_pane_at_a_time() {
    let mut cx = fixture("ready", false);
    for pane in ["account", "agents", "invites"] {
        assert!(cx.find(&format!("settings/nav/{pane}")).is_some(), "{pane}");
    }
    assert!(cx.find("settings/agents/12/suspend").is_none());
    cx.simulate_click("settings/nav/agents");
    cx.run_until_parked();
    assert!(cx.find("settings/agents/12/suspend").is_some());
    assert!(cx.find("settings/key/0").is_none());
    cx.simulate_click("settings/nav/invites");
    cx.run_until_parked();
    assert!(cx.find("settings/invite/mint").is_some());
    let cx = fixture("unregistered", false);
    assert!(cx.find("settings/nav/agents").is_none());
    assert!(cx.find("settings/nav/invites").is_some());
}
/// The menu is one tab list, a column: one Tab stop, ↓ opens the next
/// section (automatic), wrapping from the last to the first.
#[test]
fn an_arrow_on_the_menu_opens_the_next_section() {
    let mut cx = fixture("ready", false);
    let nav = cx.interactivity("settings/nav");
    assert_eq!(nav.role, Some(ducktape_view_guest::Role::TabList));
    assert_eq!(nav.aria.label.as_deref(), Some("Settings"));
    assert_eq!(
        nav.aria.orientation,
        Some(ducktape_view_guest::design::Orientation::Vertical)
    );
    assert!(nav.focusable && nav.tab_stop == Some(true));
    assert!(!cx.interactivity("settings/nav/account").focusable);
    assert!(
        cx.interactivity("settings/nav/account")
            .aria
            .active_descendant
    );
    cx.simulate_focus("settings/nav");
    cx.simulate_key_down("settings/nav", "down");
    cx.run_until_parked();
    assert!(cx.find("settings/agents/12/suspend").is_some());
    let agents = cx.interactivity("settings/nav/agents");
    assert_eq!(agents.aria.selected, Some(true));
    assert!(agents.aria.active_descendant);
    cx.simulate_key_down("settings/nav", "up");
    cx.simulate_key_down("settings/nav", "up");
    cx.run_until_parked();
    assert!(cx.find("settings/invite/mint").is_some());
    // the primary button's ring is drawn in the ink's foreground
    let mint = cx.interactivity("settings/invite/mint");
    let ring = mint.focus_visible.as_ref().expect("a focus ring");
    assert_eq!(ring.border_color, Some(Theme::light().primary_foreground));
    assert!(
        ring.box_shadow
            .as_ref()
            .is_some_and(|shadows| shadows.len() == 1 && shadows[0].inset)
    );
}

#[test]
fn invite_ttl_copy_and_refusal() {
    let mut cx = fixture("invite-ready", false);
    cx.host().handle::<ClipboardWrite>(|text| {
        assert_eq!(text, "duck-invite:workshop-loopback-example");
        Ok(())
    });
    cx.simulate_click("settings/invite/copy");
    cx.run_until_parked();
    assert!(cx.has_text("Copied"));
    cx.host().handle::<InviteCreate>(|r| {
        assert_eq!(r.ttl_days, 30);
        Ok(Invite {
            invite: "long-lived".into(),
            notes: vec![],
        })
    });
    cx.simulate_click("settings/ttl/30");
    cx.simulate_click("settings/invite/mint");
    cx.run_until_parked();
    assert!(cx.has_text("long-lived"));
    assert!(fixture("invite-refused", false).has_text("This node does not allow minting invites."));
    assert!(fixture("invite-loading", false).has_text("Minting invite…"));
}
/// The lifetime is a radio group: one Tab stop whose arrows check the next
/// choice, wrapping at the ends; its segments are never focusable.
#[test]
fn the_invite_lifetime_checks_the_next_choice_on_an_arrow() {
    let mut cx = fixture("invite-ready", false);
    let group = cx.interactivity("settings/ttl");
    assert_eq!(group.role, Some(ducktape_view_guest::Role::RadioGroup));
    assert!(group.focusable && group.tab_stop == Some(true));
    assert!(!cx.interactivity("settings/ttl/1").focusable);
    let checked = |cx: &TestAppContext| {
        [1, 7, 30].map(|days| {
            let segment = cx.interactivity(&format!("settings/ttl/{days}"));
            (
                segment.aria.toggled == Some(ducktape_view_guest::Toggled::True),
                segment.aria.active_descendant,
            )
        })
    };
    // the view opens on 7 days
    assert_eq!(checked(&cx), [(false, false), (true, true), (false, false)]);
    cx.simulate_focus("settings/ttl");
    cx.simulate_key_down("settings/ttl", "right");
    assert_eq!(checked(&cx), [(false, false), (false, false), (true, true)]);
    cx.simulate_key_down("settings/ttl", "right");
    assert_eq!(checked(&cx), [(true, true), (false, false), (false, false)]);
    cx.simulate_key_down("settings/ttl", "left");
    assert_eq!(checked(&cx), [(false, false), (false, false), (true, true)]);
}

#[test]
fn a_refusal_retries_and_a_snapshot_restores() {
    let mut cx = fixture("refused", false);
    respond(&cx);
    cx.simulate_click("settings/account-retry");
    cx.run_until_parked();
    assert!(cx.has_text("account 7 · Person"), "{:?}", cx.texts());
    let snapshot = cx.snapshot().unwrap();
    cx.restore::<Settings>(&snapshot).unwrap();
    cx.run_until_parked();
    assert!(cx.has_text("account 7 · Person"));
}
#[test]
fn an_account_changed_elsewhere_is_read_again() {
    let mut cx = TestAppContext::new();
    let accounts = cx.host().stream::<Changes<Identity>>();
    let props = cx.host().stream::<HostSession>();
    respond(&cx);
    cx.open::<Settings>();
    props.send(Session {
        signer: "abcd".into(),
        account: Some(7),
        ..Session::default()
    });
    cx.run_until_parked();
    cx.simulate_click("settings/nav/agents");
    cx.run_until_parked();
    assert!(cx.find("settings/agents/12/suspend").is_some());
    // Maya suspends Scout from another device
    cx.host().handle::<Query<Identity>>(|q| {
        Ok(match q {
            identity::Query::Get { number } => {
                identity::Reply::Account(Some(maya(number, "Laptop key")))
            }
            identity::Query::Managed { .. } => {
                let mut scout = scout();
                scout.control = identity::Control::Managed {
                    manager: 7,
                    category: identity::Category::Agent,
                    life: identity::Life::Suspended { keys: Vec::new() },
                };
                identity::Reply::Accounts(identity::PageResponse {
                    height: 43,
                    items: vec![scout],
                    next: None,
                })
            }
            q => panic!("unexpected query: {q:?}"),
        })
    });
    accounts.send(Some(43));
    cx.run_until_parked();
    assert!(
        cx.find("settings/agents/12/resume").is_some(),
        "{:?}",
        cx.texts()
    );
}
#[test]
fn export_settings_screens() {
    if std::env::var_os("SETTINGS_SCREEN_EXPORT").is_none() {
        return;
    }
    let out =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/settings/fixtures");
    std::fs::create_dir_all(&out).unwrap();
    let mut manifest = Vec::new();
    for (i, state) in [
        "loading",
        "refused",
        "empty",
        "ready",
        "agents",
        "invite-loading",
        "invite-refused",
        "invite-ready",
        "unregistered",
    ]
    .into_iter()
    .enumerate()
    {
        for dark in [false, true] {
            let cx = match state {
                "agents" => {
                    let mut cx = fixture("ready", dark);
                    cx.simulate_click("settings/nav/agents");
                    cx.run_until_parked();
                    cx
                }
                state => fixture(state, dark),
            };
            let theme = if dark { "dark" } else { "light" };
            let name = format!("{:02}-{state}-{theme}", i + 1);
            let node = cx.find("settings").expect("settings root");
            std::fs::write(
                out.join(format!("{name}.json")),
                serde_json::to_vec(node).unwrap(),
            )
            .unwrap();
            manifest.push(serde_json::json!({"name":name,"theme":theme,"width":820,"height":1100,"how":"TestAppContext + FakeHost"}));
        }
    }
    std::fs::write(
        out.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
}

#[test]
fn unregistered_key_keeps_its_standing() {
    let cx = fixture("unregistered", false);
    for text in ["Unregistered key", "Host key", "abcd", "Validator"] {
        assert!(cx.has_text(text), "{text}: {:?}", cx.texts());
    }
}

#[test]
fn no_key_offers_no_form() {
    let cx = fixture("empty", false);
    assert!(cx.has_text("No host key is selected. Sign in with a key to create an account."));
    assert!(cx.find("settings/account/create/name").is_none());
}

#[test]
fn a_suspended_agents_key_reads_as_such_and_creates_nothing() {
    let cx = fixture("suspended", false);
    assert!(cx.has_text("Scout"), "{:?}", cx.texts());
    assert!(cx.has_text("This key belongs to Scout, suspended by its manager."));
    assert!(cx.find("settings/account/create").is_none());
}
#[test]
fn unregistered_key_creates_an_account() {
    let (mut cx, props) = seated("unregistered", false);
    assert!(cx.has_text(
        "Your key isn't linked to an account yet. An account gives you a name others see."
    ));

    // Empty name never reaches the host: identity's own rule (a name is not
    // empty) is mirrored inline.
    cx.simulate_click("settings/account/create/submit");
    cx.run_until_parked();
    assert!(cx.has_text("Enter an account name."));
    assert!(cx.host().requests::<Submit<Identity>>().is_empty());

    // A refusal from the program lands as a human sentence, name kept.
    cx.host()
        .refuse::<Submit<Identity>>("invalid", "a name is not empty");
    cx.simulate_input("settings/account/create/name", "Maya");
    cx.simulate_submit("settings/account/create/name");
    cx.run_until_parked();
    assert!(cx.has_text("That didn’t go through: a name is not empty"));

    // Success holds the form busy until the host's session names the new
    // account, which is read: "Who I am" now carries the name.
    cx.host().handle::<Submit<Identity>>(|op| {
        assert!(matches!(
            op,
            identity::Op::Create { ref name, scheme: abi::Scheme::Ed25519 } if name == "Maya"
        ));
        Ok(Vec::new())
    });
    cx.host().handle::<Query<Identity>>(|q| {
        Ok(match q {
            identity::Query::Get { number } => {
                assert_eq!(number, 9);
                identity::Reply::Account(Some(maya(number, "Host key")))
            }
            identity::Query::Managed { .. } => identity::Reply::Accounts(identity::PageResponse {
                height: 42,
                items: vec![],
                next: None,
            }),
            q => panic!("unexpected query: {q:?}"),
        })
    });
    cx.simulate_click("settings/account/create/submit");
    cx.run_until_parked();
    assert!(cx.has_text("Creating…"), "{:?}", cx.texts());
    props.send(Session {
        signer: "abcd".into(),
        account: Some(9),
        endpoint: "http://127.0.0.1:19001".into(),
        ..Session::default()
    });
    cx.run_until_parked();
    assert!(cx.has_text("Maya"), "{:?}", cx.texts());
    assert!(cx.has_text("account 9 · Person"), "{:?}", cx.texts());
    assert!(cx.find("settings/account/create/name").is_none());
}

#[test]
fn create_account_disables_controls_while_busy() {
    let mut cx = fixture("unregistered", false);
    cx.host().never::<Submit<Identity>>();
    cx.simulate_input("settings/account/create/name", "Maya");
    cx.simulate_click("settings/account/create/submit");
    cx.run_until_parked();
    assert!(cx.has_text("Creating…"));
    let Some(wire::Node::Container(wire::ContainerNode { interactivity, .. })) =
        cx.find("settings/account/create/submit")
    else {
        panic!("settings/account/create/submit button")
    };
    assert_eq!(interactivity.aria.disabled, Some(true));
    assert!(interactivity.on_click.is_none());
    let Some(wire::Node::Input { options, .. }) = cx.find("settings/account/create/name") else {
        panic!("settings/account/create/name input")
    };
    assert!(options.disabled);
}

/// Every field says what it is for; the hint drawn in it stays a hint.
#[test]
fn a_field_is_named_apart_from_its_hint() {
    fn named(cx: &TestAppContext, key: &str) -> (String, String) {
        let Some(wire::Node::Input {
            options,
            placeholder,
            ..
        }) = cx.find(key)
        else {
            panic!("no field {key}");
        };
        (options.label.clone(), placeholder.clone())
    }
    let pair = |name: &str, hint: &str| (name.to_owned(), hint.to_owned());
    let cx = fixture("unregistered", false);
    assert_eq!(
        named(&cx, "settings/account/create/name"),
        pair("Name the new account", "Account name")
    );
    let mut cx = fixture("ready", false);
    cx.simulate_click("settings/nav/agents");
    cx.simulate_click("settings/agents/12/rename");
    cx.run_until_parked();
    for (key, name, hint) in [
        (
            "settings/agents/create/name",
            "Name the new agent",
            "Agent name",
        ),
        (
            "settings/agents/key/request",
            "Paste an agent's key request",
            "Agent key request",
        ),
        ("settings/agents/12/name", "Rename Scout", "New name"),
    ] {
        assert_eq!(named(&cx, key), pair(name, hint));
    }
}

#[test]
fn long_host_key_is_truncated_and_non_validator_standing_is_quiet() {
    let mut cx = TestAppContext::new();
    let props = cx.host().stream::<HostSession>();
    let long_key = vec![0x11; 32];
    let long_hex = abi::hex(&long_key);
    cx.host().handle::<Query<Valset>>(|q| {
        Ok(match q {
            valset::Query::Membership { .. } => valset::Reply::Membership(None),
            q => panic!("unexpected query: {q:?}"),
        })
    });
    cx.host().handle::<Query<Identity>>(|q| match q {
        identity::Query::Resolve { .. } => Ok(identity::Reply::Resolved(vec![None])),
        q => panic!("unexpected query: {q:?}"),
    });
    cx.set_global(Theme::light());
    cx.open::<Settings>();
    props.send(Session {
        signer: long_hex.clone(),
        endpoint: "http://127.0.0.1:19001".into(),
        ..Session::default()
    });
    cx.run_until_parked();
    let truncated = format!("{}…{}", &long_hex[..8], &long_hex[long_hex.len() - 4..]);
    assert!(cx.has_text("Host key"), "{:?}", cx.texts());
    assert!(cx.has_text(&truncated), "{:?}", cx.texts());
    assert!(!cx.has_text("Validator"));
}

#[test]
fn a_person_creates_an_agent_and_adds_its_key() {
    let mut cx = fixture("ready", false);
    cx.simulate_click("settings/nav/agents");
    cx.run_until_parked();
    for text in ["Scout", "active", "account 12 · 0 keys"] {
        assert!(cx.has_text(text), "{text}: {:?}", cx.texts());
    }
    cx.host().handle::<Submit<Identity>>(|op| {
        assert_eq!(op, identity::Op::CreateAgent { name: "Bot".into() });
        Ok(Vec::new())
    });
    cx.simulate_input("settings/agents/create/name", " Bot ");
    cx.simulate_click("settings/agents/create/submit");
    cx.run_until_parked();
    assert_eq!(cx.host().requests::<Submit<Identity>>().len(), 1);

    // a request that is not an AddKey for one of Maya's agents never leaves
    cx.simulate_input("settings/agents/key/request", "zz");
    cx.simulate_click("settings/agents/key/submit");
    cx.run_until_parked();
    assert!(cx.has_text("That isn’t a key request for one of your agents."));
    let add = identity::Op::AddKey {
        scheme: abi::Scheme::Ed25519,
        label: Some("sandbox".into()),
        consent: identity::Consent {
            key: vec![0x51; 32],
            account: 12,
            expires_at: 500,
            proof: vec![0x52; 64],
        },
    };
    let expected = add.clone();
    cx.host().handle::<Submit<Identity>>(move |op| {
        assert_eq!(op, expected);
        Ok(Vec::new())
    });
    cx.simulate_input("settings/agents/key/request", &abi::hex(&abi::encode(&add)));
    cx.simulate_click("settings/agents/key/submit");
    cx.run_until_parked();
    assert_eq!(cx.host().requests::<Submit<Identity>>().len(), 2);
    assert!(!cx.has_text("That isn’t a key request for one of your agents."));
}

/// The manager alone renames, suspends, resumes and revokes an agent from
/// its line; each is one op, and the agents are read again after it.
/// Revoke is one press: the host confirms it natively before signing.
#[test]
fn a_manager_renames_suspends_and_revokes_an_agent() {
    let mut cx = fixture("ready", false);
    cx.simulate_click("settings/nav/agents");
    cx.run_until_parked();
    let sent = |cx: &TestAppContext| cx.host().requests::<Submit<Identity>>().len();
    // Rename turns the name into a field holding it; Save sends it
    assert!(cx.find("settings/agents/12/name").is_none());
    cx.simulate_click("settings/agents/12/rename");
    cx.run_until_parked();
    let Some(wire::Node::Input { value, .. }) = cx.find("settings/agents/12/name") else {
        panic!("the rename field")
    };
    assert_eq!(value, "Scout");
    assert!(cx.has_text("Save"));
    cx.simulate_input("settings/agents/12/name", " ");
    cx.simulate_click("settings/agents/12/rename");
    cx.run_until_parked();
    assert!(
        cx.has_text("Enter the agent's new name."),
        "{:?}",
        cx.texts()
    );
    assert_eq!(sent(&cx), 0);
    cx.host().handle::<Submit<Identity>>(|op| {
        assert_eq!(
            op,
            identity::Op::SetName {
                account: 12,
                name: "Scout II".into()
            }
        );
        Ok(Vec::new())
    });
    cx.simulate_input("settings/agents/12/name", " Scout II ");
    cx.simulate_click("settings/agents/12/rename");
    cx.run_until_parked();
    assert_eq!(sent(&cx), 1);
    assert!(!cx.has_text("Enter the agent's new name."));
    assert!(
        cx.find("settings/agents/12/name").is_none(),
        "a rename that landed closes its field"
    );

    // active, its line offers Suspend; suspended, Resume; revoked, nothing
    assert!(cx.find("settings/agents/12/suspend").is_some());
    assert!(cx.find("settings/agents/12/resume").is_none());
    cx.host().handle::<Submit<Identity>>(|op| {
        assert_eq!(op, identity::Op::Suspend { account: 12 });
        Ok(Vec::new())
    });
    let suspended = |cx: &TestAppContext, life: identity::Life| {
        cx.host().handle::<Query<Identity>>(move |q| {
            let life = life.clone();
            Ok(match q {
                identity::Query::Get { number } => {
                    identity::Reply::Account(Some(maya(number, "Laptop key")))
                }
                identity::Query::Managed { by: 7, .. } => {
                    let identity::Control::Managed {
                        manager, category, ..
                    } = scout().control
                    else {
                        panic!()
                    };
                    let control = identity::Control::Managed {
                        manager,
                        category,
                        life,
                    };
                    identity::Reply::Accounts(identity::PageResponse {
                        height: 42,
                        items: vec![identity::Account { control, ..scout() }],
                        next: None,
                    })
                }
                q => panic!("unexpected query: {q:?}"),
            })
        });
    };
    suspended(&cx, identity::Life::Suspended { keys: Vec::new() });
    cx.simulate_click("settings/agents/12/suspend");
    cx.run_until_parked();
    assert_eq!(sent(&cx), 2);
    assert!(cx.has_text("suspended"), "{:?}", cx.texts());
    assert!(cx.find("settings/agents/12/resume").is_some());
    let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let logged = log.clone();
    cx.host().handle::<Submit<Identity>>(move |op| {
        logged.borrow_mut().push(op);
        Ok(Vec::new())
    });
    cx.simulate_click("settings/agents/12/resume");
    cx.run_until_parked();
    suspended(&cx, identity::Life::Revoked);
    cx.simulate_click("settings/agents/12/revoke");
    cx.run_until_parked();
    assert!(
        !cx.has_text("Revoke for good"),
        "the view asks no confirmation of its own: the host does"
    );
    assert_eq!(
        *log.borrow(),
        [
            identity::Op::Resume { account: 12 },
            identity::Op::Revoke { account: 12 },
        ]
    );
    assert!(cx.has_text("revoked"), "{:?}", cx.texts());
    for action in ["rename", "suspend", "resume", "revoke"] {
        assert!(cx.find(&format!("settings/agents/12/{action}")).is_none());
    }
}

/// Identity's live heads re-read the account in place: a rename made
/// elsewhere shows without the account leaving the screen first.
#[test]
fn an_identity_head_re_reads_the_account_in_place() {
    let mut cx = TestAppContext::new();
    let heads = cx.host().stream::<Changes<Identity>>();
    let props = cx.host().stream::<HostSession>();
    respond(&cx);
    cx.open::<Settings>();
    props.send(Session {
        signer: "abcd".into(),
        account: Some(7),
        ..Session::default()
    });
    cx.run_until_parked();
    assert!(cx.has_text("Maya"));
    cx.host().handle::<Query<Identity>>(|q| {
        Ok(match q {
            identity::Query::Get { number } => {
                let mut renamed = maya(number, "Laptop key");
                renamed.card.name = "Maya R".into();
                identity::Reply::Account(Some(renamed))
            }
            identity::Query::Managed { .. } => identity::Reply::Accounts(identity::PageResponse {
                height: 43,
                items: vec![],
                next: None,
            }),
            q => panic!("unexpected query: {q:?}"),
        })
    });
    heads.send(Some(43));
    cx.run_until_parked();
    assert!(cx.has_text("Maya R"), "{:?}", cx.texts());
    assert!(!cx.has_text("Reading your account…"));
}
