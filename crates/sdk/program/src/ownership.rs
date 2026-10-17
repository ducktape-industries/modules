//! Who may act on something an account owns: its owner alone, its owner and the accounts
//! listed by number, or anyone. identity says so of whom an agent answers, files of who writes a path.
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};

use crate::AccountNumber;

#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum Ownership {
    #[default]
    Owner,
    Accounts(Vec<AccountNumber>),
    Anyone,
}

impl Ownership {
    pub fn allows(&self, owner: Option<AccountNumber>, account: AccountNumber) -> bool {
        let owns = owner == Some(account);
        match self {
            Ownership::Owner => owns,
            Ownership::Accounts(listed) => owns || listed.contains(&account),
            Ownership::Anyone => true,
        }
    }

    pub fn settled(self, owner: Option<AccountNumber>) -> Ownership {
        let Ownership::Accounts(mut listed) = self else {
            return self;
        };
        listed.sort_unstable();
        listed.dedup();
        listed.retain(|number| Some(*number) != owner);
        match listed.is_empty() {
            true => Ownership::Owner,
            false => Ownership::Accounts(listed),
        }
    }

    pub fn listed(&self) -> &[AccountNumber] {
        match self {
            Ownership::Accounts(listed) => listed,
            Ownership::Owner | Ownership::Anyone => &[],
        }
    }
}

#[cfg(test)]
mod tests;
