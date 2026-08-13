use crate::DomainError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Display, Formatter};

/// Opaque, validated identifier shared by canonical records.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RecordId(String);

impl RecordId {
    /// Validate an adapter-supplied identifier.
    pub fn parse(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let valid_length = (3..=100).contains(&value.len());
        let valid_characters = value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'));
        if !valid_length || !valid_characters {
            return Err(DomainError::new(
                "id",
                "must be 3-100 ASCII letters, digits, underscores, or hyphens",
            ));
        }
        Ok(Self(value))
    }

    /// Borrow the stable external representation.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for RecordId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for RecordId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RecordId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(raw).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::RecordId;

    #[test]
    fn unsafe_identifier_characters_are_rejected_during_deserialization() {
        let error = serde_json::from_str::<RecordId>(r#""../../evidence""#)
            .expect_err("path-like identifier must fail");
        assert!(error.to_string().contains("ASCII"));
    }

    #[test]
    fn valid_identifier_round_trips() {
        let id = RecordId::parse("claim_0123-abcd").expect("valid identifier");
        let encoded = serde_json::to_string(&id).expect("serialize");
        assert_eq!(
            serde_json::from_str::<RecordId>(&encoded).expect("deserialize"),
            id
        );
    }
}
