use std::fmt::{Display, Formatter};

/// A rejected domain value or invariant violation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DomainError {
    field: &'static str,
    message: String,
}

impl DomainError {
    /// Construct an error naming the rejected field and condition.
    pub fn new(field: &'static str, message: impl Into<String>) -> Self {
        Self {
            field,
            message: message.into(),
        }
    }

    /// Stable field name suitable for adapter error mapping.
    pub fn field(&self) -> &'static str {
        self.field
    }
}

impl Display for DomainError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid {}: {}", self.field, self.message)
    }
}

impl std::error::Error for DomainError {}

#[cfg(test)]
mod tests {
    use super::DomainError;

    #[test]
    fn error_exposes_stable_field_and_sanitized_display() {
        let error = DomainError::new("claim.predicate", "must not be empty");
        assert_eq!(error.field(), "claim.predicate");
        assert_eq!(
            error.to_string(),
            "invalid claim.predicate: must not be empty"
        );
    }
}
