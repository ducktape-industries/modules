//! What `Send` checks and does: only a program sends, within the caps; each recipient account
//! becomes its Ed25519 keys; the body goes out as one blob through the egress role, and the
//! service's answer is the frame's outcome.
use ::program::{
    AccountNumber, Ctx, Error, HashKind, Principal, ProgramId, QueryCtx, Scheme, invalid,
    unexpected_reply, wrong_state,
};
use abi::role::egress::{self, http};
use serde_json::{Value, json};

use crate::{KIND, MAX_NOTICES, MAX_RECIPIENTS, MAX_TEXT, Notice, SERVICE, TARGET};

pub(crate) async fn send(ctx: &Ctx, notices: Vec<Notice>) -> Result<(), Error> {
    let program = ctx.env().sending_program()?;
    checked(&notices)?;
    let egress = ctx.env().role(abi::role::EGRESS)?.clone();
    let identity = ctx.env().role(abi::role::IDENTITY)?.clone();
    let acting = ctx.env().sender.as_ref().and_then(Principal::account);
    let addressed = addressed(&notices, acting, |number| keys(ctx, &identity, number))?;
    if addressed.is_empty() {
        return Ok(());
    }
    let body = json!({ "program": program, "notices": addressed }).to_string();
    let part = ctx.blob_put(HashKind::Sha256, KIND, body.into_bytes())?;
    let request = egress::Op::Request {
        request: egress::Request::Http {
            service: SERVICE.into(),
            head: http::Head {
                method: http::Method::Post,
                target: TARGET.into(),
                headers: vec![("content-type".into(), "application/json".into())],
            },
            body: vec![part],
        },
        wait: None,
    };
    let output = ctx.call_raw(egress, abi::encode(&request)).await?;
    answered(&output)
}

pub(crate) fn checked(notices: &[Notice]) -> Result<(), Error> {
    if notices.len() > MAX_NOTICES {
        return Err(invalid(format!(
            "a send carries at most {MAX_NOTICES} notices"
        )));
    }
    for notice in notices {
        if notice.to.len() > MAX_RECIPIENTS {
            return Err(invalid(format!(
                "a notice is for at most {MAX_RECIPIENTS} accounts"
            )));
        }
        if notice.title.len() > MAX_TEXT || notice.body.len() > MAX_TEXT {
            return Err(invalid(format!(
                "a notice's title and body hold at most {MAX_TEXT} bytes each: push::Notice::new \
                 clips them"
            )));
        }
    }
    Ok(())
}

/// Each notice as the service reads it, its accounts turned into their keys by `keys`: never the
/// account the frame acts as, and a notice no key is left for drops out.
pub(crate) fn addressed(
    notices: &[Notice],
    acting: Option<AccountNumber>,
    mut keys: impl FnMut(AccountNumber) -> Result<Vec<String>, Error>,
) -> Result<Vec<Value>, Error> {
    let mut out = Vec::new();
    for notice in notices {
        let mut to = notice.to.clone();
        to.sort_unstable();
        to.dedup();
        let mut held = Vec::new();
        for number in to.into_iter().filter(|number| Some(*number) != acting) {
            held.extend(keys(number)?);
        }
        if held.is_empty() {
            continue;
        }
        out.push(json!({
            "keys": held,
            "title": notice.title,
            "body": notice.body,
            "tag": notice.tag,
            "link": notice.link,
        }));
    }
    Ok(out)
}

/// An account's Ed25519 keys, hex; none for a number no account has.
fn keys(ctx: &QueryCtx, identity: &ProgramId, number: AccountNumber) -> Result<Vec<String>, Error> {
    let asked = identity::Query::Get { number };
    match ctx.query::<identity::Query, identity::Reply>(identity, &asked)? {
        identity::Reply::Account(account) => Ok(account
            .iter()
            .flat_map(|account| account.keys())
            .filter(|key| key.scheme == Scheme::Ed25519)
            .map(|key| abi::hex(&key.key))
            .collect()),
        other => Err(unexpected_reply(identity, "Get", &other)),
    }
}

/// The frame's outcome from the service's answer: Ok on a 2xx, else what it said.
pub(crate) fn answered(output: &[u8]) -> Result<(), Error> {
    let response: http::Response = abi::decode(output)
        .map_err(|_| wrong_state("the push service's answer is not an HTTP response"))?;
    if (200..300).contains(&response.status) {
        return Ok(());
    }
    let said = &response.body[..response.body.len().min(200)];
    Err(wrong_state(format!(
        "the push service answered {}: {}",
        response.status,
        String::from_utf8_lossy(said)
    )))
}
