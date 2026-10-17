//! Paths as files spells them (NFC, absolute, `/`-separated) and the one check of who
//! may write a path: the nearest write list at or above it, else anyone, beneath the namespace
//! roots (`/home`, `/shared`, each `/home/<name>`) that the chain alone writes; the chain anywhere.
use ::program::{
    AccountNumber, Error, Origin, Ownership, Party, Principal, QueryCtx, invalid, unauthorized,
    unexpected_reply,
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
        [folder @ .., _] => holds_namespace_roots(folder),
        [] => false,
    }
}

pub fn holds_namespace_roots(folder: &[String]) -> bool {
    matches!(folder, [root] if root == "home")
}

pub enum Writer {
    Chain,
    Account {
        number: AccountNumber,
        caller: Vec<Party>,
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
    let caller = caller(number, &ctx.env().origin);
    Ok(Writer::Account {
        number,
        caller,
        label,
    })
}

pub fn caller(number: AccountNumber, origin: &Origin) -> Vec<Party> {
    let account = Party::Account(number);
    match origin {
        Origin::Program(program) => vec![account, Party::Program(program.clone())],
        Origin::Signed(_) | Origin::Root => vec![account],
    }
}

pub fn writers(segments: &[String], along: &[Option<Access>]) -> Writers {
    let nearest = along.iter().enumerate().rev().find_map(|(depth, row)| {
        let row = row.as_ref()?;
        let write = row.write.clone()?;
        Some(Writers::Listed {
            path: join(&segments[..=depth]),
            creator: row.creator,
            write,
        })
    });
    nearest.unwrap_or(Writers::Anyone)
}

pub fn writable(
    writer: &Writer,
    segments: &[String],
    along: &[Option<Access>],
    act: Act,
) -> Result<(), Error> {
    let Writer::Account {
        number,
        caller,
        label,
    } = writer
    else {
        return Ok(());
    };
    let writers = writers(segments, along);
    match act {
        Act::Write => {
            beneath_the_roots(segments)?;
            allowed(*number, caller, segments, &writers)
        }
        Act::SetOwnership => {
            allowed(*number, caller, segments, &writers)?;
            sets_list(*number, label.as_deref(), segments, along)
        }
    }
}

fn beneath_the_roots(segments: &[String]) -> Result<(), Error> {
    if !is_namespace_root(segments) {
        return Ok(());
    }
    Err(unauthorized(format!(
        "{} is a namespace root: /home, /shared and each /home/<name> are written by the \
         chain alone, and anyone writes beneath them where no write list says otherwise",
        join(segments)
    )))
}

fn allowed(
    number: AccountNumber,
    caller: &[Party],
    segments: &[String],
    writers: &Writers,
) -> Result<(), Error> {
    let Writers::Listed {
        path,
        creator,
        write,
    } = writers
    else {
        return Ok(());
    };
    if write.allows(*creator, caller) {
        return Ok(());
    }
    Err(unauthorized(format!(
        "{} may not write {}: the write list on {path} allows {}",
        who(number, caller),
        join(segments),
        allows(*creator, write)
    )))
}

fn who(number: AccountNumber, caller: &[Party]) -> String {
    let program = caller.iter().find_map(Party::program);
    match program {
        Some(program) => format!("account {number}, sent by program {program},"),
        None => format!("account {number}"),
    }
}

pub fn allows(creator: Option<AccountNumber>, write: &Ownership) -> String {
    let parties = |listed: &[Party]| listed.iter().map(party).collect::<Vec<_>>().join(", ");
    match (write, creator) {
        (Ownership::Owner, Some(creator)) => format!("only its creator, account {creator}"),
        (Ownership::Owner, None) => "no account: no account created it".into(),
        (Ownership::Listed(listed), Some(creator)) => {
            format!("its creator, account {creator}, and {}", parties(listed))
        }
        (Ownership::Listed(listed), None) => parties(listed),
        (Ownership::Anyone, _) => "anyone".into(),
    }
}

fn party(party: &Party) -> String {
    match party {
        Party::Account(number) => format!("account {number}"),
        Party::Program(program) => format!("program {program}"),
    }
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
