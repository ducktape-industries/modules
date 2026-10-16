//! Paths as files spells them (NFC, absolute, `/`-separated) and who may write
//! where: an account under `/shared/` and its own home, `/home/<its identity name>/`;
//! the chain anywhere.
use ::program::{
    AccountNumber, Error, Principal, QueryCtx, invalid, unauthorized, unexpected_reply,
};
use unicode_normalization::UnicodeNormalization;

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

enum Area<'a> {
    Shared,
    Home(&'a str),
    Outside,
}

fn area(segments: &[String]) -> Area<'_> {
    match segments {
        [root, _, ..] if root == "shared" => Area::Shared,
        [root, home, _, ..] if root == "home" => Area::Home(home),
        _ => Area::Outside,
    }
}

pub fn writable(writer: &Writer, segments: &[String]) -> Result<(), Error> {
    let Writer::Account { number, label } = writer else {
        return Ok(());
    };
    match area(segments) {
        Area::Shared => Ok(()),
        Area::Home(home) => owned(*number, label.as_deref(), home, segments),
        Area::Outside => Err(unauthorized(format!(
            "{} is outside /shared/ and an account's own home, /home/<its name>/",
            join(segments)
        ))),
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

pub fn writable_path(writer: &Writer, path: &str) -> Result<Vec<String>, Error> {
    let segments = segments(path)?;
    if segments.is_empty() {
        return Err(invalid("the root is not a path a change names"));
    }
    writable(writer, &segments)?;
    Ok(segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_absolute_and_composed() {
        assert_eq!(segments("/").unwrap(), Vec::<String>::new());
        assert_eq!(segments("/shared/a").unwrap(), ["shared", "a"]);
        for bad in [
            "shared",
            "/shared//a",
            "/shared/a/",
            "/shared/./a",
            "/shared/../a",
            "/a\0b",
            "/cafe\u{301}",
        ] {
            assert_eq!(
                segments(bad).unwrap_err().code,
                ::program::code::INVALID_INPUT,
                "{bad:?}"
            );
        }
    }

    fn named(number: AccountNumber, label: &str) -> Writer {
        Writer::Account {
            number,
            label: Some(label.to_owned()),
        }
    }

    fn refusal(writer: &Writer, path: &str) -> ::program::Error {
        let error = writable_path(writer, path).unwrap_err();
        assert!(
            [
                ::program::code::UNAUTHORIZED,
                ::program::code::INVALID_INPUT
            ]
            .contains(&error.code.as_str()),
            "{path}"
        );
        error
    }

    #[test]
    fn an_account_writes_under_its_own_home_and_shared_and_the_chain_anywhere() {
        let ada = named(1, "ada");
        for ok in ["/home/ada/notes", "/home/ada/x/y", "/shared/a"] {
            assert!(writable_path(&ada, ok).is_ok(), "{ok}");
        }
        for refused in [
            "/home",
            "/home/ada",
            "/home/bo/x",
            "/home/Ada/x",
            "/shared",
            "/etc/passwd",
            "/x",
        ] {
            refusal(&ada, refused);
        }
        assert!(writable_path(&Writer::Chain, "/etc/passwd").is_ok());
        assert!(writable_path(&Writer::Chain, "/home/bo/x").is_ok());
        assert!(writable_path(&Writer::Chain, "/").is_err());
    }

    #[test]
    fn another_home_is_unauthorized_and_an_unnamed_account_has_none() {
        let refused = refusal(&named(1, "ada"), "/home/bo/agents/scout.md");
        assert_eq!(refused.code, ::program::code::UNAUTHORIZED);
        let unnamed = Writer::Account {
            number: 3,
            label: None,
        };
        assert_eq!(
            refusal(&unnamed, "/home/bo/x").code,
            ::program::code::UNAUTHORIZED
        );
        assert!(writable_path(&unnamed, "/shared/x").is_ok());
    }
}
