//! The phone's design (`ui/phone/`), as the Expo app draws it: each state
//! of the Account screen and of the Agents pane, and each action through
//! its sheet, under a finger at 390 wide. The sweep holds them all at 360,
//! 390 and 430.
use super::*;

/// The fixture's `state` on a phone: a finger, 390 wide.
fn phone(state: &str) -> TestAppContext {
    let mut cx = fixture(state, false);
    on_phone(&mut cx);
    cx
}

fn on_phone(cx: &mut TestAppContext) {
    cx.simulate_touch(true);
    cx.simulate_resize(390., 844.);
}

/// Maya on her phone, the Agents pane open.
fn agents() -> TestAppContext {
    let mut cx = phone("ready");
    cx.simulate_click("settings/strip/agents");
    cx
}

/// Identity's live head moves on, and the account is read again.
fn identity_moves(cx: &mut TestAppContext, height: u64) {
    let heads = cx.host().stream::<Changes<Identity>>();
    heads.send(Some(Change {
        height,
        keys: Vec::new(),
    }));
    cx.run_until_parked();
}

fn has_all(cx: &TestAppContext, texts: &[&str]) {
    for text in texts {
        assert!(cx.has_text(text), "{text}: {:?}", cx.texts());
    }
}

/// The sweep (SPEC §8.1d-1): every pane of the strip and every mode of the
/// sheets, every frame audited and fitted at each phone width. It lives
/// here, not in `tests/sweep.rs`, because the fixture does.
#[test]
fn sweep() {
    let mut cx = fixture("ready", false);
    view::testing::sweep(&mut cx, |cx| {
        cx.simulate_click("settings/strip/account");
        assert_eq!(cx.back_handler(), None);
        cx.simulate_click("settings/strip/agents");
        cx.simulate_click("settings/agents/12");
        cx.simulate_click("settings/agents/12/rename/open");
        cx.simulate_click("settings/agents/12/rename-cancel");
        cx.simulate_click("settings/agents/12/answers/open");
        cx.simulate_click("settings/agents/12/answers/specific");
        cx.simulate_click("settings/agents/12/answers/done");
        cx.simulate_dismiss("settings/agents/12/actions");
        cx.simulate_click("settings/agents/create/open");
        cx.simulate_click("settings/agents/create/submit-cancel");
        cx.simulate_click("settings/agents/key/open");
        cx.simulate_back();
        cx.simulate_click("settings/strip/invites");
        assert_eq!(cx.back_handler(), None);
    });
}

/// The pointer picks the design, not the width: a finger gets the phone's
/// screen on a wide window, a mouse the desktop's menu on a narrow one.
#[test]
fn the_pointer_picks_the_design_not_the_width() {
    let mut cx = fixture("ready", false);
    cx.simulate_touch(true);
    cx.simulate_resize(820., 800.);
    assert!(cx.find("settings/strip/account").is_some());
    assert!(cx.find("settings/nav").is_none());
    cx.simulate_touch(false);
    cx.simulate_resize(360., 800.);
    assert!(cx.find("settings/strip/account").is_none());
    assert!(cx.find("settings/nav").is_some());
}

#[test]
fn a_person_reads_who_they_are_and_each_key() {
    let cx = phone("ready");
    has_all(
        &cx,
        &[
            "Maya",
            "account 7 · Person · 1 key",
            "Keys",
            "Laptop key",
            "abcd · fingerprint 123D 4C7E",
            "Validator",
        ],
    );
    // a person who manages agents has the Agents pane
    for pane in ["account", "agents", "invites"] {
        assert!(
            cx.find(&format!("settings/strip/{pane}")).is_some(),
            "{pane}"
        );
    }
    // the desktop's words are not the phone's
    assert!(!cx.has_text("This device"));
    assert!(!cx.has_text("account 7 · User"));
}

#[test]
fn the_account_says_it_is_being_read() {
    assert!(phone("loading").has_text("Reading your account…"));
}

#[test]
fn a_refused_read_says_why_and_reads_again() {
    let mut cx = phone("refused");
    assert!(cx.has_text("Account query refused."));
    respond(&cx);
    cx.simulate_click("settings/account-retry");
    cx.run_until_parked();
    assert!(cx.has_text("Maya"), "{:?}", cx.texts());
}

#[test]
fn a_phone_with_no_key_has_no_account() {
    let cx = phone("empty");
    has_all(
        &cx,
        &[
            "No account",
            "No host key is selected. Sign in with a key to create an account.",
        ],
    );
}

#[test]
fn an_account_identity_does_not_hold_says_so() {
    let mut cx = phone("refused");
    cx.host().handle::<Query<Identity>>(|q| match q {
        identity::Query::Get { number: 7 } => Ok(identity::Reply::Account(None)),
        q => panic!("unexpected query: {q:?}"),
    });
    cx.simulate_click("settings/account-retry");
    cx.run_until_parked();
    assert!(
        cx.has_text("Identity holds no account 7."),
        "{:?}",
        cx.texts()
    );
}

#[test]
fn an_unregistered_key_creates_its_account() {
    let mut cx = phone("unregistered");
    has_all(
        &cx,
        &[
            "Unregistered key",
            "no account yet",
            "Phone",
            "abcd · fingerprint 123D 4C7E",
            "Create an account",
        ],
    );
    assert!(
        cx.find("settings/strip/agents").is_none(),
        "a bare key manages nothing"
    );
    cx.simulate_click("settings/account/create/submit");
    cx.run_until_parked();
    assert!(cx.has_text("Enter an account name."));
    assert!(sent(&cx).is_empty());

    cx.host()
        .handle::<Submit<Identity>>(|_| Ok(view::testing::applied(Vec::new())));
    cx.simulate_input("settings/account/create/name", "Maya");
    cx.simulate_click("settings/account/create/submit");
    cx.run_until_parked();
    assert!(matches!(
        sent(&cx)[..],
        [identity::Op::Create { ref name, scheme: abi::Scheme::Ed25519 }] if name == "Maya"
    ));
    // busy until the host's session names the new account
    assert!(cx.has_text("Creating…"), "{:?}", cx.texts());
}

#[test]
fn a_stopped_agents_key_says_why_it_does_not_act() {
    let cx = phone("suspended");
    has_all(
        &cx,
        &[
            "Scout",
            "a key of an agent",
            "Phone",
            "This key belongs to Scout, suspended by its manager.",
        ],
    );
    assert!(cx.find("settings/account/create").is_none());
}

#[test]
fn a_phone_says_who_mints_invites() {
    let mut cx = phone("ready");
    cx.simulate_click("settings/strip/invites");
    assert!(cx.has_text("Only the machine running this node can mint its invites."));
    assert!(cx.find("settings/invite/mint").is_none());
}

#[test]
fn a_person_reads_each_agent_and_whom_it_answers() {
    let cx = agents();
    has_all(
        &cx,
        &[
            "Agents act as accounts you manage. You answer for what they do.",
            "Scout",
            "active",
            "account 12 · 0 keys · answers only me",
            "Create an agent",
            "Add a key to an agent",
        ],
    );
}

#[test]
fn an_agents_row_opens_its_actions_and_back_closes_them() {
    let mut cx = agents();
    cx.simulate_click("settings/agents/12");
    assert!(
        matches!(cx.node("settings/agents/12/actions"), wire::Node::Sheet { label, .. }
            if label.as_deref() == Some("Scout"))
    );
    has_all(
        &cx,
        &[
            "account 12 · active",
            "Rename",
            "Who it answers",
            "Only me",
            "Suspend",
            "Revoke",
            "final",
        ],
    );
    cx.simulate_back();
    assert!(cx.find("settings/agents/12/actions").is_none());
    assert_eq!(cx.back_handler(), None);
}

#[test]
fn a_rename_that_lands_goes_back_to_the_agents_actions() {
    let mut cx = agents();
    cx.simulate_click("settings/agents/12");
    cx.simulate_click("settings/agents/12/rename/open");
    let Some(wire::Node::Field { value, .. }) = cx.find("settings/agents/12/name") else {
        panic!("the rename field: {:?}", cx.texts())
    };
    assert_eq!(value, "Scout");
    cx.host()
        .handle::<Submit<Identity>>(|_| Ok(view::testing::applied(Vec::new())));
    cx.simulate_input("settings/agents/12/name", "Scout II");
    cx.simulate_click("settings/agents/12/rename");
    cx.run_until_parked();
    assert_eq!(
        sent(&cx),
        [identity::Op::SetName {
            account: 12,
            name: "Scout II".into()
        }]
    );
    assert!(cx.find("settings/agents/12/name").is_none());
    assert!(cx.find("settings/agents/12/rename/open").is_some());
}

#[test]
fn closing_the_sheet_drops_a_rename_not_sent() {
    let mut cx = agents();
    cx.simulate_click("settings/agents/12");
    cx.simulate_click("settings/agents/12/rename/open");
    cx.simulate_input("settings/agents/12/name", "Half typed");
    cx.simulate_back();
    cx.simulate_click("settings/agents/12");
    assert!(
        cx.find("settings/agents/12/name").is_none(),
        "the sheet opens on its actions"
    );
    cx.simulate_click("settings/agents/12/rename/open");
    let Some(wire::Node::Field { value, .. }) = cx.find("settings/agents/12/name") else {
        panic!("the rename field: {:?}", cx.texts())
    };
    assert_eq!(value, "Scout");
}

/// Suspend and Revoke are one press each: the host confirms before the
/// key signs, not the view.
#[test]
fn suspend_and_revoke_are_one_press_each() {
    let mut cx = agents();
    cx.host()
        .handle::<Submit<Identity>>(|_| Ok(view::testing::applied(Vec::new())));
    cx.simulate_click("settings/agents/12");
    cx.simulate_click("settings/agents/12/suspend");
    cx.run_until_parked();
    cx.simulate_click("settings/agents/12/revoke");
    cx.run_until_parked();
    assert_eq!(
        sent(&cx),
        [
            identity::Op::Suspend { account: 12 },
            identity::Op::Revoke { account: 12 },
        ]
    );
}

#[test]
fn a_refused_suspend_says_why_under_the_actions() {
    let mut cx = agents();
    cx.host()
        .refuse::<Submit<Identity>>("unauthorized", "account 7 does not manage account 12");
    cx.simulate_click("settings/agents/12");
    cx.simulate_click("settings/agents/12/suspend");
    cx.run_until_parked();
    assert!(cx.has_text("That didn’t go through: account 7 does not manage account 12"));
    assert!(cx.find("settings/agents/12/suspend").is_some());
}

#[test]
fn a_suspended_agent_resumes_and_a_revoked_one_only_says_who_it_was() {
    let mut cx = agents();
    scout_is(&cx, identity::Life::Suspended { keys: Vec::new() });
    identity_moves(&mut cx, 43);
    cx.simulate_click("settings/agents/12");
    assert!(cx.find("settings/agents/12/resume").is_some());
    assert!(cx.find("settings/agents/12/suspend").is_none());

    scout_is(&cx, identity::Life::Revoked);
    identity_moves(&mut cx, 44);
    assert!(cx.has_text("account 12 · revoked"), "{:?}", cx.texts());
    for action in ["rename/open", "answers/open", "resume", "revoke"] {
        assert!(
            cx.find(&format!("settings/agents/12/{action}")).is_none(),
            "{action}"
        );
    }
}

#[test]
fn whom_an_agent_answers_is_picked_in_its_sheet() {
    let (mut cx, answers) = answering(identity::Audience::Manager);
    on_phone(&mut cx);
    cx.simulate_click("settings/agents/12");
    cx.simulate_click("settings/agents/12/answers/open");
    assert_eq!(picked(&cx), ["manager"]);

    cx.simulate_click("settings/agents/12/answers/anyone");
    cx.run_until_parked();
    assert_eq!(*answers.borrow(), identity::Audience::Anyone);
    assert!(cx.has_text(WARNING), "{:?}", cx.texts());

    cx.simulate_click("settings/agents/12/answers/specific");
    assert!(cx.has_text("Add people by name or account number."));
    cx.simulate_input("settings/agents/12/answers/name", "#9");
    cx.simulate_click("settings/agents/12/answers/add");
    cx.run_until_parked();
    assert_eq!(*answers.borrow(), identity::Audience::Accounts(vec![9]));
    assert!(cx.has_text("Eve"), "{:?}", cx.texts());

    // Done goes back to the actions, which now say whom it answers
    cx.simulate_click("settings/agents/12/answers/done");
    assert!(cx.find("settings/agents/12/answers/name").is_none());
    assert!(cx.has_text("Specific people"), "{:?}", cx.texts());
}

#[test]
fn a_refused_answer_says_why_in_the_sheet() {
    let (mut cx, answers) = answering(identity::Audience::Manager);
    on_phone(&mut cx);
    cx.host()
        .refuse::<Submit<Identity>>("unauthorized", "account 7 does not manage account 12");
    cx.simulate_click("settings/agents/12");
    cx.simulate_click("settings/agents/12/answers/open");
    cx.simulate_click("settings/agents/12/answers/anyone");
    cx.run_until_parked();
    assert_eq!(*answers.borrow(), identity::Audience::Manager);
    assert_eq!(picked(&cx), ["manager"]);
    assert!(!cx.has_text(WARNING));
    assert!(cx.has_text("That didn’t go through: account 7 does not manage account 12"));
}

#[test]
fn a_created_agent_closes_its_sheet() {
    let mut cx = agents();
    cx.simulate_click("settings/agents/create/open");
    cx.simulate_click("settings/agents/create/submit");
    assert!(cx.has_text("Enter an agent name."));
    cx.host()
        .handle::<Submit<Identity>>(|_| Ok(view::testing::applied(Vec::new())));
    cx.simulate_input("settings/agents/create/name", "Bot");
    cx.simulate_click("settings/agents/create/submit");
    cx.run_until_parked();
    assert_eq!(
        sent(&cx),
        [identity::Op::CreateAgent { name: "Bot".into() }]
    );
    assert!(cx.find("settings/agents/create/sheet").is_none());
}

#[test]
fn a_refused_create_keeps_its_sheet_and_says_why() {
    let mut cx = agents();
    cx.host()
        .refuse::<Submit<Identity>>("invalid", "a name is taken");
    cx.simulate_click("settings/agents/create/open");
    cx.simulate_input("settings/agents/create/name", "Bot");
    cx.simulate_click("settings/agents/create/submit");
    cx.run_until_parked();
    assert!(cx.find("settings/agents/create/sheet").is_some());
    assert!(cx.has_text("That didn’t go through: a name is taken"));
}

#[test]
fn an_added_key_closes_its_sheet() {
    let mut cx = agents();
    cx.simulate_click("settings/agents/key/open");
    cx.simulate_input("settings/agents/key/request", "zz");
    cx.simulate_click("settings/agents/key/submit");
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
    cx.host()
        .handle::<Submit<Identity>>(|_| Ok(view::testing::applied(Vec::new())));
    cx.simulate_input("settings/agents/key/request", &abi::hex(&abi::encode(&add)));
    cx.simulate_click("settings/agents/key/submit");
    cx.run_until_parked();
    assert_eq!(sent(&cx), [add]);
    assert!(cx.find("settings/agents/key/sheet").is_none());
}

/// Every phone field says what keyboard it wants (rule 7).
#[test]
fn the_phones_fields_carry_their_keyboard_hints() {
    let mut cx = agents();
    let hints = |cx: &TestAppContext, key: &str| {
        let Some(wire::Node::Field { options, .. }) = cx.find(key) else {
            panic!("no field {key}");
        };
        options.text_input
    };
    cx.simulate_click("settings/agents/key/open");
    let request = hints(&cx, "settings/agents/key/request");
    assert!(!request.autocorrect && !request.suggestions);
    assert_eq!(request.autocapitalize, wire::Autocapitalize::None);
    cx.simulate_back();
    cx.simulate_click("settings/agents/create/open");
    let name = hints(&cx, "settings/agents/create/name");
    assert_eq!(name.autocapitalize, wire::Autocapitalize::Words);
    cx.simulate_back();
    cx.simulate_click("settings/agents/12");
    cx.simulate_click("settings/agents/12/rename/open");
    let rename = hints(&cx, "settings/agents/12/name");
    assert_eq!(rename.autocapitalize, wire::Autocapitalize::Words);
    cx.simulate_click("settings/agents/12/rename-cancel");
    cx.simulate_click("settings/agents/12/answers/open");
    cx.simulate_click("settings/agents/12/answers/specific");
    let person = hints(&cx, "settings/agents/12/answers/name");
    assert!(!person.autocorrect);
    assert_eq!(person.autocapitalize, wire::Autocapitalize::None);
}
