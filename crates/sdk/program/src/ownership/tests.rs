use super::*;

fn account(number: AccountNumber) -> Party {
    Party::Account(number)
}

fn program(id: &str) -> Party {
    Party::Program(id.into())
}

#[test]
fn owner_allows_its_owner_listed_adds_its_parties_and_anyone_allows_all() {
    let listed = Ownership::Listed(vec![account(4), program("harness")]);
    assert!(Ownership::Owner.allows(Some(1), &[account(1)]));
    assert!(!Ownership::Owner.allows(Some(1), &[account(4)]));
    assert!(!Ownership::Owner.allows(None, &[account(1)]));
    assert!(listed.allows(Some(1), &[account(1)]));
    assert!(listed.allows(Some(1), &[account(4)]));
    assert!(listed.allows(None, &[account(4)]));
    assert!(!listed.allows(Some(1), &[account(9)]));
    assert!(Ownership::Anyone.allows(None, &[account(9)]));
    assert!(Ownership::Anyone.allows(None, &[]));
}

#[test]
fn a_listed_program_allows_a_caller_that_is_that_program_whatever_account_it_acts_as() {
    let listed = Ownership::Listed(vec![program("harness")]);
    assert!(listed.allows(Some(1), &[account(9), program("harness")]));
    assert!(!listed.allows(Some(1), &[account(9), program("chat")]));
    assert!(!listed.allows(Some(1), &[account(9)]));
    assert!(Ownership::Owner.allows(Some(9), &[account(9), program("chat")]));
    assert!(!Ownership::Owner.allows(Some(1), &[program("harness")]));
}

#[test]
fn a_settled_list_holds_each_party_once_never_the_owner_and_empty_is_owner() {
    let settled = Ownership::Listed(vec![
        program("harness"),
        account(9),
        account(1),
        account(4),
        account(9),
        program("harness"),
    ])
    .settled(Some(1));
    assert_eq!(
        settled,
        Ownership::Listed(vec![account(4), account(9), program("harness")])
    );
    assert_eq!(settled.accounts().collect::<Vec<_>>(), [4, 9]);
    assert_eq!(settled.programs().collect::<Vec<_>>(), ["harness"]);
    assert_eq!(
        Ownership::Listed(vec![account(1)]).settled(Some(1)),
        Ownership::Owner
    );
    assert_eq!(
        Ownership::Listed(vec![account(1)]).settled(None),
        Ownership::Listed(vec![account(1)])
    );
    assert_eq!(Ownership::Anyone.settled(Some(1)), Ownership::Anyone);
}
