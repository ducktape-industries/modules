use super::*;

#[test]
fn owner_allows_its_owner_accounts_add_the_listed_and_anyone_allows_all() {
    let listed = Ownership::Accounts(vec![4]);
    assert!(Ownership::Owner.allows(Some(1), 1));
    assert!(!Ownership::Owner.allows(Some(1), 4));
    assert!(!Ownership::Owner.allows(None, 1));
    assert!(listed.allows(Some(1), 1));
    assert!(listed.allows(Some(1), 4));
    assert!(listed.allows(None, 4));
    assert!(!listed.allows(Some(1), 9));
    assert!(Ownership::Anyone.allows(None, 9));
}

#[test]
fn a_settled_list_holds_each_account_once_never_the_owner_and_empty_is_owner() {
    let settled = Ownership::Accounts(vec![9, 1, 4, 9]).settled(Some(1));
    assert_eq!(settled, Ownership::Accounts(vec![4, 9]));
    assert_eq!(settled.listed(), [4, 9]);
    assert_eq!(
        Ownership::Accounts(vec![1]).settled(Some(1)),
        Ownership::Owner
    );
    assert_eq!(Ownership::Anyone.settled(Some(1)), Ownership::Anyone);
}
