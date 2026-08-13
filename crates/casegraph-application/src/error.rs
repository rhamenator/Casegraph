use casegraph_domain::DomainError;
use std::fmt::{Display, Formatter};

/// Stable application error categories for API/CLI mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    InvalidInput,
    NotFound,
    Conflict,
    Unsupported,
    TooLarge,
    Storage,
    Internal,
}

/// Sanitized application failure. Source contents and secrets must not be included.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppError {
    kind: ErrorKind,
    message: String,
}

impl AppError {
    /// Construct a sanitized error.
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// Stable category for delivery adapters.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Safe message suitable for a local client or failure record.
    pub fn safe_message(&self) -> &str {
        &self.message
    }
}

impl Display for AppError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AppError {}

impl From<DomainError> for AppError {
    fn from(value: DomainError) -> Self {
        Self::new(ErrorKind::InvalidInput, value.to_string())
    }
}
