//! Who may call, read off the env's [`Origin`]: the checks a program makes
//! before it trusts a frame's raw caller rather than its sender.

use crate::{Env, Error, Origin, ProgramId, unauthorized};

impl Env {
    /// The signing key of a signed frame; any other origin is refused.
    pub fn signer(&self) -> Result<Vec<u8>, Error> {
        match &self.origin {
            Origin::Signed(key) => Ok(key.clone()),
            other => Err(unauthorized(format!(
                "only a signed frame may do this, not {other:?}"
            ))),
        }
    }

    /// The program that sent this message; any other origin is refused.
    pub fn sending_program(&self) -> Result<ProgramId, Error> {
        match &self.origin {
            Origin::Program(program) => Ok(program.clone()),
            other => Err(unauthorized(format!(
                "only a program may do this, not {other:?}"
            ))),
        }
    }

    /// Refused unless the program bound to the validators role sent this:
    /// the validators' vote, enacted.
    pub fn authority(&self) -> Result<(), Error> {
        let by_validators =
            matches!(&self.origin, Origin::Program(sender) if *sender == self.roles.validators);
        if by_validators {
            return Ok(());
        }
        Err(unauthorized(format!(
            "only the validators' vote ({}) may do this, not {:?}",
            self.roles.validators, self.origin
        )))
    }

    pub fn kernel(&self) -> Result<(), Error> {
        if self.origin == Origin::Root {
            return Ok(());
        }
        Err(unauthorized(format!(
            "only the kernel may do this, not {:?}",
            self.origin
        )))
    }

    /// Refused unless `program` or the chain itself sent this.
    pub fn sent_by(&self, program: &str) -> Result<(), Error> {
        let by_program = matches!(&self.origin, Origin::Program(sender) if sender == program);
        if by_program || self.origin == Origin::Root {
            return Ok(());
        }
        Err(unauthorized(format!(
            "only {program} may do this, not {:?}",
            self.origin
        )))
    }
}

#[cfg(test)]
mod tests {
    use crate::{Env, MockHost, Origin, code};

    fn env(origin: Origin) -> Env {
        Env {
            origin,
            ..MockHost::env("m")
        }
    }

    #[test]
    fn each_check_admits_only_its_origin() {
        let signed = env(Origin::Signed(vec![1]));
        let program = env(Origin::Program("gov".into()));
        let root = env(Origin::Root);
        assert_eq!(signed.signer().unwrap(), [1]);
        assert_eq!(program.signer().unwrap_err().code, code::UNAUTHORIZED);
        assert_eq!(program.sending_program().unwrap(), "gov");
        assert_eq!(root.sending_program().unwrap_err().code, code::UNAUTHORIZED);
        assert!(program.sent_by("gov").is_ok() && root.sent_by("gov").is_ok());
        assert!(program.sent_by("other").is_err() && signed.sent_by("gov").is_err());
    }

    #[test]
    fn authority_admits_only_the_program_bound_to_the_validators_role() {
        let validators = MockHost::roles().validators;
        assert!(env(Origin::Program(validators)).authority().is_ok());
        for origin in [
            Origin::Signed(vec![1; 32]),
            Origin::Program("gov".into()),
            Origin::Program(MockHost::roles().programs),
            Origin::Root,
        ] {
            let refused = env(origin).authority().unwrap_err();
            assert_eq!(refused.code, code::UNAUTHORIZED);
        }
        let rebound = Env {
            roles: crate::Roles {
                validators: "gov".into(),
                ..MockHost::roles()
            },
            ..env(Origin::Program("gov".into()))
        };
        assert!(rebound.authority().is_ok());
    }
}
