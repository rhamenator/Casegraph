#![forbid(unsafe_code)]

//! Canonical domain types and invariants. This crate has no adapter dependencies.

/// Current canonical schema version understood by the domain layer.
pub const CANONICAL_SCHEMA_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::CANONICAL_SCHEMA_VERSION;

    #[test]
    fn schema_version_is_explicit() {
        assert_eq!(CANONICAL_SCHEMA_VERSION, 1);
    }
}
