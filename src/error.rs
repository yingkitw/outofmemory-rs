//! Error type — mirrors the HTTP status semantics of the TS API
//! (`400` validation, `404` missing, `409` conflict).

use std::fmt;

/// Which HTTP status the TS workbench would have returned — useful when this
/// crate backs a server, and a quick signal about the failure class otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// 400 — malformed input, failed validation.
    BadRequest,
    /// 404 — referenced entity does not exist.
    NotFound,
    /// 409 — duplicate apiName / duplicate link.
    Conflict,
}

#[derive(Debug)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
}

impl Error {
    pub(crate) fn bad(msg: impl Into<String>) -> Self {
        Error {
            kind: ErrorKind::BadRequest,
            message: msg.into(),
        }
    }
    pub(crate) fn gone(msg: impl Into<String>) -> Self {
        Error {
            kind: ErrorKind::NotFound,
            message: msg.into(),
        }
    }
    pub(crate) fn clash(msg: impl Into<String>) -> Self {
        Error {
            kind: ErrorKind::Conflict,
            message: msg.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::bad(format!("invalid JSON: {e}"))
    }
}

impl From<quick_xml::Error> for Error {
    fn from(e: quick_xml::Error) -> Self {
        Error::bad(format!("invalid XML: {e}"))
    }
}

pub type Result<T> = std::result::Result<T, Error>;
