//! The request and answer every HTTP service speaks: a program puts the head in a
//! request's input and the body in its parts; the provider answers with a [`Response`].
use ::program::{Error, invalid};
use borsh::{BorshDeserialize, BorshSerialize};

pub const WITHHELD: [&str; 9] = [
    "authorization",
    "proxy-authorization",
    "x-api-key",
    "cookie",
    "set-cookie",
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Head {
    pub method: Method,
    pub path: String,
    pub headers: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Head {
    pub fn check(&self) -> Result<(), Error> {
        let path_fits =
            self.path.starts_with('/') && self.path.bytes().all(|byte| byte.is_ascii_graphic());
        if !path_fits {
            return Err(invalid("a path starts with / and is printable ASCII"));
        }
        check_headers(&self.headers)
    }
}

pub fn check_headers(headers: &[(String, String)]) -> Result<(), Error> {
    for (name, value) in headers {
        let named = !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
        if !named {
            return Err(invalid(format!(
                "header name {name:?} is lowercase letters, digits and -"
            )));
        }
        if WITHHELD.contains(&name.as_str()) {
            return Err(invalid(format!(
                "{name} is the provider's to set, never a request's"
            )));
        }
        let printable = value
            .bytes()
            .all(|byte| byte == b'\t' || (b' '..=b'~').contains(&byte));
        if !printable {
            return Err(invalid(format!("header {name} is printable ASCII")));
        }
    }
    Ok(())
}
