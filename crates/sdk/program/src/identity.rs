//! What the identity role says about an account a program is asked to name.

use abi::role::identity::{Kind, Query, Reply, Standing};

use crate::{AccountNumber, Error, QueryCtx, invalid, unexpected_reply, wrong_state};

impl QueryCtx {
    /// Refused unless the identity role profiles `number` as a user or a
    /// program account that acts: no absent account, no system account,
    /// no program account suspended or revoked.
    pub fn require_user_or_program_account(&self, number: AccountNumber) -> Result<(), Error> {
        let asked = Query::Profile(number);
        let identity = self.env().role(abi::role::IDENTITY)?.clone();
        let Reply::Profile(profile) = self.query::<Query, Reply>(&identity, &asked)? else {
            return Err(unexpected_reply(&identity, "Profile", &"another reply"));
        };
        let Some(profile) = profile else {
            return Err(invalid(format!("there is no account {number}")));
        };
        match profile.kind {
            Kind::User
            | Kind::Program {
                standing: Standing::Active,
                ..
            } => Ok(()),
            Kind::Program {
                standing: Standing::Suspended,
                ..
            } => Err(wrong_state(format!(
                "account {number} is suspended: only users and program accounts that act are named"
            ))),
            Kind::Program {
                standing: Standing::Revoked,
                ..
            } => Err(wrong_state(format!(
                "account {number} is revoked: only users and program accounts that act are named"
            ))),
            Kind::System(program) => Err(invalid(format!(
                "account {number} is program {program}'s own: only users and program accounts are named"
            ))),
        }
    }
}
