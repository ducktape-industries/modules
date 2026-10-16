//! Paths as files spells them (NFC, absolute, `/`-separated) and the one check of who
//! may write a path: the nearest write list at or above it, else an account under
//! `/shared/` and its own home, `/home/<its identity name>/`; the chain anywhere.
use ::program::{
    AccountNumber, Error, Ownership, Principal, QueryCtx, invalid, unauthorized, unexpected_reply,
};
use unicode_normalization::UnicodeNormalization;

use crate::Writers;
use crate::objects::Access;

pub fn segments(path: &str) -> Result<Vec<String>, Error> {
    if !path.starts_with('/') {
        return Err(invalid(format!(
            "{path:?} is not absolute: a path starts with /"
        )));
    }
    if path.contains('\0') {
        return Err(invalid("a path holds no NUL byte"));
    }
    let composed = path.nfc().eq(path.chars());
    if !composed {
        return Err(invalid(format!("{path:?} is not in NFC form")));
    }
    if path == "/" {
        return Ok(Vec::new());
    }
    let mut segments = Vec::new();
    for segment in path[1..].split('/') {
        let navigates = segment.is_empty() || segment == "." || segment == "..";
        if navigates {
            return Err(invalid(format!("{path:?} has an empty, . or .. segment")));
        }
        segments.push(segment.to_owned());
    }
    Ok(segments)
}

pub fn join(segments: &[String]) -> String {
    format!("/{}", segments.join("/"))
}

pub fn is_namespace_root(segments: &[String]) -> bool {
    match segments {
        [root] => root == "home" || root == "shared",
        [root, _] => root == "home",
        _ => false,
    }
}

pub enum Writer {
    Chain,
    Account {
        number: AccountNumber,
        label: Option<String>,
    },
}

impl Writer {
    pub fn number(&self) -> Option<AccountNumber> {
        match self {
            Writer::Chain => None,
            Writer::Account { number, .. } => Some(*number),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Write,
    SetOwnership,
}

pub fn writer(ctx: &QueryCtx, principal: &Principal) -> Result<Writer, Error> {
    use abi::role::identity::{Query, Reply};
    let Principal::Account(number) = *principal else {
        return Ok(Writer::Chain);
    };
    let identity = ctx.env().role(abi::role::IDENTITY)?.clone();
    let label = match ctx.query::<Query, Reply>(&identity, &Query::Profile(number))? {
        Reply::Profile(profile) => profile.map(|profile| profile.name),
        other => return Err(unexpected_reply(&identity, "Profile", &other)),
    };
    Ok(Writer::Account { number, label })
}

pub fn writers(segments: &[String], along: &[Option<Access>], act: Act) -> Writers {
    let nearest = along.iter().enumerate().rev().find_map(|(depth, row)| {
        let row = row.as_ref()?;
        let write = row.write.clone()?;
        Some(Writers::Listed {
            path: join(&segments[..=depth]),
            creator: row.creator,
            write,
        })
    });
    if let Some(listed) = nearest {
        return listed;
    }
    match (segments, act) {
        ([root, _, ..], _) if root == "shared" => Writers::Shared,
        ([root, home, _, ..], _) if root == "home" => Writers::Home(home.clone()),
        ([root, home], Act::SetOwnership) if root == "home" => Writers::Home(home.clone()),
        _ => Writers::Chain,
    }
}

pub fn writable(
    writer: &Writer,
    segments: &[String],
    along: &[Option<Access>],
    act: Act,
) -> Result<(), Error> {
    let Writer::Account { number, label } = writer else {
        return Ok(());
    };
    let label = label.as_deref();
    allowed(*number, label, segments, &writers(segments, along, act))?;
    match act {
        Act::Write => Ok(()),
        Act::SetOwnership => sets_list(*number, label, segments, along),
    }
}

fn allowed(
    number: AccountNumber,
    label: Option<&str>,
    segments: &[String],
    writers: &Writers,
) -> Result<(), Error> {
    match writers {
        Writers::Listed {
            path,
            creator,
            write,
        } => listed(number, segments, path, *creator, write),
        Writers::Home(home) => owned(number, label, home, segments),
        Writers::Shared => Ok(()),
        Writers::Chain => Err(unauthorized(format!(
            "{} is outside /shared/ and an account's own home, /home/<its name>/",
            join(segments)
        ))),
    }
}

fn listed(
    number: AccountNumber,
    segments: &[String],
    path: &str,
    creator: Option<AccountNumber>,
    write: &Ownership,
) -> Result<(), Error> {
    if write.allows(creator, number) {
        return Ok(());
    }
    Err(unauthorized(format!(
        "account {number} may not write {}: the write list on {path} allows {}",
        join(segments),
        allows(creator, write)
    )))
}

pub fn allows(creator: Option<AccountNumber>, write: &Ownership) -> String {
    let accounts = |listed: &[AccountNumber]| {
        listed
            .iter()
            .map(|number| format!("account {number}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    match (write, creator) {
        (Ownership::Owner, Some(creator)) => format!("only its creator, account {creator}"),
        (Ownership::Owner, None) => "no account: no account created it".into(),
        (Ownership::Accounts(listed), Some(creator)) => {
            format!("its creator, account {creator}, and {}", accounts(listed))
        }
        (Ownership::Accounts(listed), None) => accounts(listed),
        (Ownership::Anyone, _) => "any account".into(),
    }
}

fn owned(
    number: AccountNumber,
    label: Option<&str>,
    home: &str,
    segments: &[String],
) -> Result<(), Error> {
    let own_home = label == Some(home);
    if own_home {
        return Ok(());
    }
    let whose = format!(
        "{} is in /home/{home}/, the home of the account named {home}",
        join(segments)
    );
    let mine = match label {
        Some(label) => format!(
            "account {number} is named {label} and writes under /home/{label}/ and /shared/"
        ),
        None => format!("account {number} has no name in identity and writes under /shared/ only"),
    };
    Err(unauthorized(format!("{whose}: {mine}")))
}

fn sets_list(
    number: AccountNumber,
    label: Option<&str>,
    segments: &[String],
    along: &[Option<Access>],
) -> Result<(), Error> {
    let path = join(segments);
    if let [root, home] = segments
        && root == "home"
    {
        let named = label == Some(home.as_str());
        if named {
            return Ok(());
        }
        return Err(unauthorized(format!(
            "the write list on {path} is set by the account named {home}: account {number} is not"
        )));
    }
    let creator = along
        .last()
        .and_then(|row| row.as_ref())
        .and_then(|row| row.creator);
    let creates = creator == Some(number);
    if creates {
        return Ok(());
    }
    match creator {
        Some(creator) => Err(unauthorized(format!(
            "the write list on {path} is set by its creator, account {creator}: account {number} is not"
        ))),
        None => Err(unauthorized(format!(
            "no account created {path}, so no account sets its write list"
        ))),
    }
}

pub fn change_path(path: &str) -> Result<Vec<String>, Error> {
    let segments = segments(path)?;
    if segments.is_empty() {
        return Err(invalid("the root is not a path a change names"));
    }
    Ok(segments)
}

#[cfg(test)]
mod tests;
