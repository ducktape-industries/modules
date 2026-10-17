//! Who may act on something an account owns: its owner alone, its owner and the parties
//! listed (accounts by number, programs by id), or anyone. identity says so of whom an agent
//! answers, files of who writes a path.
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};

use crate::{AccountNumber, ProgramId};

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    BorshSerialize,
    BorshDeserialize,
    BorshSchema,
)]
pub enum Party {
    Account(AccountNumber),
    Program(ProgramId),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum Ownership {
    #[default]
    Owner,
    Listed(Vec<Party>),
    Anyone,
}

impl Ownership {
    pub fn allows(&self, owner: Option<AccountNumber>, caller: &[Party]) -> bool {
        let owns = owner.is_some_and(|owner| caller.contains(&Party::Account(owner)));
        match self {
            Ownership::Owner => owns,
            Ownership::Listed(listed) => owns || listed.iter().any(|party| caller.contains(party)),
            Ownership::Anyone => true,
        }
    }

    pub fn settled(self, owner: Option<AccountNumber>) -> Ownership {
        let Ownership::Listed(mut listed) = self else {
            return self;
        };
        listed.sort_unstable();
        listed.dedup();
        listed.retain(|party| owner.is_none_or(|owner| *party != Party::Account(owner)));
        match listed.is_empty() {
            true => Ownership::Owner,
            false => Ownership::Listed(listed),
        }
    }

    pub fn listed(&self) -> &[Party] {
        match self {
            Ownership::Listed(listed) => listed,
            Ownership::Owner | Ownership::Anyone => &[],
        }
    }

    pub fn accounts(&self) -> impl Iterator<Item = AccountNumber> + '_ {
        self.listed().iter().filter_map(Party::account)
    }

    pub fn programs(&self) -> impl Iterator<Item = &ProgramId> {
        self.listed().iter().filter_map(Party::program)
    }
}

impl Party {
    pub fn account(&self) -> Option<AccountNumber> {
        match self {
            Party::Account(number) => Some(*number),
            Party::Program(_) => None,
        }
    }

    pub fn program(&self) -> Option<&ProgramId> {
        match self {
            Party::Account(_) => None,
            Party::Program(program) => Some(program),
        }
    }
}

#[cfg(test)]
mod tests;
