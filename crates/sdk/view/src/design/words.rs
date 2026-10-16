//! The words every view says of a host's answer: a refusal, and a write
//! that did not land. One table, so no view words them its own way (the
//! Expo app's `src/write/words.ts:72-85`).

use crate::host::Error;
use crate::methods::{Submitted, refusal};

/// A refusal as a person reads it: the program's or the node's own
/// sentence, but a program the network does not run comes back as its bare
/// name (`unknown_program` carries only that), so it is said in full
/// (`crates/mobile-core/src/lib.rs:104-118`): "This network runs no program
/// called activity."
pub fn sentence(error: &Error) -> String {
    match error.code.as_str() {
        ::error::code::UNKNOWN_PROGRAM => {
            format!("This network runs no program called {}.", error.message)
        }
        _ => error.message.clone(),
    }
}

/// What a write that did not land says, or `None` when it landed
/// (`src/write/words.ts:77-85`): refused, in the program's or the node's
/// words; in doubt, since it may have landed; not sent, because no node
/// was reached, no node is connected, or the host refused it before the
/// send. A view shows it where the write was asked for.
pub fn failure(answer: &Result<Submitted, Error>) -> Option<String> {
    match answer {
        Ok(Submitted::Applied { .. }) => None,
        Ok(Submitted::Refused { refusal, .. }) => Some(refused(refusal)),
        Ok(Submitted::InDoubt { .. }) => Some(IN_DOUBT.into()),
        Err(error) => Some(unsent(error)),
    }
}

/// A write in doubt. The Expo app follows the colon with the transport's
/// own words (`words.ts:80`); `Submitted::InDoubt` carries none, so it says
/// what is known: no answer came back.
const IN_DOUBT: &str = "This may have gone through: no answer came back.";

/// A write the program or the node refused (`words.ts:79`).
fn refused(error: &Error) -> String {
    format!("That didn’t go through: {}", sentence(error))
}

/// A write the host answered with an error of its own, or passed on the
/// node's refusal. The host's codes ([`refusal`]) say it was never sent
/// (`words.ts:82-84`), but two say it may have run: the node failed, or no
/// answer came back.
fn unsent(error: &Error) -> String {
    match error.code.as_str() {
        refusal::RPC_CLIENT => {
            "Not sent: the node can’t be reached. Check the connection, then try again.".into()
        }
        refusal::NOT_CONNECTED => "Not sent: not connected to a node.".into(),
        refusal::NODE_FAILED | refusal::REQUEST_CLOSED => IN_DOUBT.into(),
        code if HOST.contains(&code) => format!("Not sent: {}", sentence(error)),
        _ => refused(error),
    }
}

/// The host's own refusals that leave a write unsent ([`refusal`]). A
/// hand copy: `refusal` lists no set to walk, so a code added there must be
/// added here, or a write it refuses is worded as the program's refusal.
const HOST: [&str; 13] = [
    refusal::UNKNOWN_REQUEST,
    refusal::UNDECLARED_CAPABILITY,
    refusal::UNDECLARED_TARGET,
    refusal::CONSENT_REFUSED,
    refusal::MALFORMED_REQUEST,
    refusal::STALE_CONNECTION,
    refusal::SESSION_LOCKED,
    refusal::HOST_FAULT,
    refusal::INVALID_WIDGET_COMMAND,
    refusal::WIDGET_COMMAND_FAILED,
    refusal::WIDGET_UNMOUNTED,
    refusal::NEEDS_GESTURE,
    refusal::INVITE_UNSUPPORTED,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn error(code: &str, message: &str) -> Error {
        Error::new(code, message)
    }

    #[test]
    fn a_refusal_says_its_own_sentence_but_a_missing_program_in_full() {
        assert_eq!(
            sentence(&error("not_found", "No such room.")),
            "No such room."
        );
        assert_eq!(
            sentence(&error("unknown_program", "activity")),
            "This network runs no program called activity."
        );
    }

    #[test]
    fn a_write_that_did_not_land_says_why_by_what_is_known_of_it() {
        let frame = [0; 32];
        assert_eq!(
            failure(&Ok(Submitted::Applied {
                frame,
                until: 9,
                output: Vec::new(),
            })),
            None
        );
        let said = |answer| failure(&answer).unwrap();
        assert_eq!(
            said(Ok(Submitted::Refused {
                frame,
                until: 9,
                refusal: error("wrong_state", "The room is closed."),
            })),
            "That didn’t go through: The room is closed."
        );
        assert_eq!(
            said(Ok(Submitted::InDoubt { frame, until: 9 })),
            "This may have gone through: no answer came back."
        );
        assert_eq!(
            said(Err(error(refusal::RPC_CLIENT, "connection refused"))),
            "Not sent: the node can’t be reached. Check the connection, then try again."
        );
        assert_eq!(
            said(Err(error(refusal::NOT_CONNECTED, "no node"))),
            "Not sent: not connected to a node."
        );
        assert_eq!(
            said(Err(error(refusal::SESSION_LOCKED, "Unlock a key first."))),
            "Not sent: Unlock a key first."
        );
        assert_eq!(
            said(Err(error(refusal::REQUEST_CLOSED, "closed"))),
            "This may have gone through: no answer came back."
        );
        assert_eq!(
            said(Err(error("unknown_program", "activity"))),
            "That didn’t go through: This network runs no program called activity."
        );
    }
}
