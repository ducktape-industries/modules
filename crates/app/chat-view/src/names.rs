//! What the network calls a principal, as the composer offers and inserts it.
//! The roster itself is [`chat::view::Names`], shared with every view that
//! names chat principals.
use std::collections::BTreeMap;

use chat::view::Names;
use chat::{Kind, Principal};
use identity::Audience;

/// Autocomplete candidates: every user and program account that acts
/// ([`Names::people`]), then the room's other such members, labelled
/// without the `@`.
pub fn mention_choices(names: &Names, members: &[Principal]) -> Vec<MentionChoice> {
    let accounts = names.people().map(Principal::Account);
    let members = members.iter().filter(|principal| names.pickable(principal));
    let mut choices: Vec<MentionChoice> = Vec::new();
    for principal in accounts.chain(members.cloned()) {
        if !choices.iter().any(|choice| choice.principal == principal) {
            choices.push(MentionChoice {
                label: names.mention(&principal)[1..].to_string(),
                principal,
            });
        }
    }
    choices.sort_by_key(|choice| choice.label.to_lowercase());
    choices
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionChoice {
    pub label: String,
    pub principal: Principal,
}

/// The canonical token the composer inserts: `<@account>`.
pub fn mention_token(principal: &Principal) -> String {
    match principal {
        Principal::Account(account) => format!("<@{account}>"),
        Principal::Root => String::new(),
    }
}

/// Why `agent` would give `reader` no answer, by the rule harness answers
/// by: its manager always gets one, anyone else by its audience. None when
/// it answers them, for anyone not an agent, and while its audience is
/// unread.
pub fn unanswered(
    names: &Names,
    audiences: &BTreeMap<u64, Audience>,
    agent: &Principal,
    reader: u64,
) -> Option<&'static str> {
    let Some(Kind::Managed { manager, .. }) = names.kind(agent) else {
        return None;
    };
    if *manager == reader {
        return None;
    }
    match audiences.get(&agent.account()?)? {
        Audience::Manager => Some("its manager only"),
        Audience::Accounts(listed) if !listed.contains(&reader) => Some("listed people only"),
        Audience::Accounts(_) | Audience::Anyone => None,
    }
}

/// Whom an agent answers, in words: its manager, and whom its audience
/// adds.
pub fn answers(names: &Names, audience: &Audience) -> String {
    match audience {
        Audience::Manager => "Answers its manager only".into(),
        Audience::Accounts(listed) => {
            let listed: Vec<String> = listed
                .iter()
                .map(|number| names.member(&Principal::Account(*number)))
                .collect();
            format!("Answers its manager and {}", listed.join(", "))
        }
        Audience::Anyone => "Answers anyone".into(),
    }
}

/// The other account of a dm room `mine` is in.
pub fn dm_peer_of(mine: u64, channel_id: &str) -> Option<u64> {
    let (a, b) = chat::dm_peers(channel_id)?;
    (mine == a).then_some(b).or((mine == b).then_some(a))
}

#[cfg(test)]
mod tests {
    use chat::{Category, Kind, Principal, Profile, Standing};

    use super::*;

    fn profile(number: u64, name: &str, kind: Kind) -> Profile {
        Profile {
            number,
            name: name.into(),
            kind,
        }
    }

    fn agent(manager: u64, standing: Standing) -> Kind {
        Kind::Program {
            script: None,
            manager,
            category: Category::Agent,
            standing,
        }
    }

    /// Mentions and reviewers offer users and program accounts that act:
    /// no system account, no suspended or revoked one. Each wears the one
    /// badge and note its kind gives it.
    #[test]
    fn the_pickers_offer_people_and_live_agents_only() {
        let names = Names::from_roster([
            profile(1, "ada", Kind::User),
            profile(2, "scout", agent(1, Standing::Active)),
            profile(3, "forge", Kind::System("forge".into())),
            profile(4, "idle", agent(1, Standing::Suspended)),
            profile(5, "gone", agent(1, Standing::Revoked)),
        ]);
        assert_eq!(names.people().collect::<Vec<_>>(), [1, 2]);
        let members = [3, 4, 9].map(Principal::Account);
        let labels: Vec<_> = mention_choices(&names, &members)
            .into_iter()
            .map(|choice| choice.label)
            .collect();
        assert_eq!(labels, ["account-9", "ada", "scout"]);
        let account = Principal::Account;
        assert_eq!(names.member(&account(4)), "idle (suspended)");
        assert_eq!(names.member(&account(5)), "gone (revoked)");
        assert_eq!(names.badge(&account(1)), None);
        assert_eq!(
            names.badge(&account(2)).as_deref(),
            Some("Agent · managed by ada")
        );
        assert_eq!(names.badge(&account(3)).as_deref(), Some("Program · forge"));
        assert_eq!(names.program(&account(3)), Some("forge"));
        assert!(crate::message::agent(&names, &account(2)));
        assert!(!crate::message::agent(&names, &account(3)));
    }

    /// An agent answers its manager always, and anyone else as its
    /// audience says; nothing is said of a person, a program, or an agent
    /// whose audience is unread.
    #[test]
    fn who_an_agent_leaves_unanswered() {
        let names = Names::from_roster([
            profile(1, "ada", Kind::Person),
            profile(2, "scout", agent(1, Standing::Active)),
            profile(3, "forge", Kind::Program("forge".into())),
            profile(4, "bo", Kind::Person),
        ]);
        let scout = Principal::Account(2);
        let heard = |audience: Audience, reader| {
            let audiences = BTreeMap::from([(2, audience)]);
            unanswered(&names, &audiences, &scout, reader)
        };
        assert_eq!(heard(Audience::Manager, 1), None, "its manager");
        assert_eq!(heard(Audience::Manager, 4), Some("its manager only"));
        assert_eq!(heard(Audience::Accounts(vec![4]), 4), None);
        assert_eq!(
            heard(Audience::Accounts(vec![4]), 9),
            Some("listed people only")
        );
        assert_eq!(heard(Audience::Anyone, 9), None);
        let none = BTreeMap::new();
        assert_eq!(unanswered(&names, &none, &scout, 4), None, "unread");
        for other in [1, 3] {
            let audiences = BTreeMap::from([(other, Audience::Manager)]);
            assert_eq!(
                unanswered(&names, &audiences, &Principal::Account(other), 4),
                None
            );
        }
        assert_eq!(
            answers(&names, &Audience::Accounts(vec![4, 9])),
            "Answers its manager and bo, account 9"
        );
    }
}
