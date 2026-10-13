//! Paths as files spells them (NFC, absolute, `/`-separated, bounded) and who may write
//! where: any account under `/home/<label>/` and `/shared/`, the chain anywhere.
use guest::{Error, Principal, invalid, unauthorized};
use unicode_normalization::UnicodeNormalization;

use crate::{MAX_DEPTH, MAX_NAME_BYTES, MAX_PATH_BYTES};

pub fn segments(path: &str) -> Result<Vec<String>, Error> {
    if !path.starts_with('/') {
        return Err(invalid(format!(
            "{path:?} is not absolute: a path starts with /"
        )));
    }
    if path.len() > MAX_PATH_BYTES {
        return Err(invalid(format!("a path is at most {MAX_PATH_BYTES} bytes")));
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
        if segment.len() > MAX_NAME_BYTES {
            return Err(invalid(format!("a name is at most {MAX_NAME_BYTES} bytes")));
        }
        segments.push(segment.to_owned());
    }
    if segments.len() > MAX_DEPTH {
        return Err(invalid(format!("a path is at most {MAX_DEPTH} deep")));
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

pub fn writable(writer: &Principal, segments: &[String]) -> Result<(), Error> {
    let Principal::Account(_) = writer else {
        return Ok(());
    };
    let under_a_home = segments.len() >= 3 && segments[0] == "home";
    let under_shared = segments.len() >= 2 && segments[0] == "shared";
    if under_a_home || under_shared {
        return Ok(());
    }
    Err(unauthorized(format!(
        "{} is outside /home/<name>/ and /shared/, which an account writes",
        join(segments)
    )))
}

pub fn writable_path(writer: &Principal, path: &str) -> Result<Vec<String>, Error> {
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
    fn a_path_is_absolute_composed_and_bounded() {
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
                guest::code::INVALID_INPUT,
                "{bad:?}"
            );
        }
        assert!(segments(&format!("/{}", "a".repeat(MAX_NAME_BYTES))).is_ok());
        assert!(segments(&format!("/{}", "a".repeat(MAX_NAME_BYTES + 1))).is_err());
        assert!(segments(&"/a".repeat(MAX_DEPTH)).is_ok());
        assert!(segments(&"/a".repeat(MAX_DEPTH + 1)).is_err());
    }

    #[test]
    fn an_account_writes_under_a_home_and_shared_and_the_chain_anywhere() {
        let ada = Principal::Account(1);
        for ok in ["/home/ada/notes", "/home/bo/x/y", "/shared/a"] {
            assert!(writable_path(&ada, ok).is_ok(), "{ok}");
        }
        for refused in ["/home", "/home/ada", "/shared", "/etc/passwd", "/x"] {
            let error = writable_path(&ada, refused).unwrap_err();
            assert!(
                [guest::code::UNAUTHORIZED, guest::code::INVALID_INPUT]
                    .contains(&error.code.as_str()),
                "{refused}"
            );
        }
        assert!(writable_path(&Principal::Root, "/etc/passwd").is_ok());
        assert!(writable_path(&Principal::Root, "/").is_err());
    }
}
