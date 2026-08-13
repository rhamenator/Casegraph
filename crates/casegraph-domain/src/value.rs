use crate::{Date, DomainError, TemporalValue};
use serde::{Deserialize, Serialize};

/// Exact base-10 number represented without binary floating-point arithmetic.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Decimal {
    /// Signed coefficient; 142700 with scale 2 represents 1427.00.
    pub coefficient: i64,
    /// Number of decimal fractional digits.
    pub scale: u8,
}

impl Decimal {
    /// Construct a bounded fixed-point number.
    pub fn new(coefficient: i64, scale: u8) -> Result<Self, DomainError> {
        if scale > 18 {
            return Err(DomainError::new("decimal.scale", "must not exceed 18"));
        }
        Ok(Self { coefficient, scale })
    }
}

/// Exact money value. The original representation remains in provenance/claim records.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Money {
    /// Signed minor-unit coefficient.
    pub amount: Decimal,
    /// ISO-style uppercase three-letter currency code.
    pub currency: String,
}

impl Money {
    /// Validate an exact monetary value.
    pub fn new(amount: Decimal, currency: impl Into<String>) -> Result<Self, DomainError> {
        let currency = currency.into();
        if currency.len() != 3 || !currency.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(DomainError::new(
                "money.currency",
                "must be three uppercase ASCII letters",
            ));
        }
        Ok(Self { amount, currency })
    }
}

/// Domain-neutral normalized value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum MaterialValue {
    Text(String),
    Integer(i64),
    Decimal(Decimal),
    Money(Money),
    Boolean(bool),
    Date(Date),
    Temporal(TemporalValue),
}

/// Epistemic state distinct from a known material value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "knowledge", content = "value", rename_all = "snake_case")]
pub enum KnowledgeValue {
    Known(MaterialValue),
    Unknown,
    NotApplicable,
    NotEvaluated,
}

#[cfg(test)]
mod tests {
    use super::{Decimal, KnowledgeValue, MaterialValue, Money};

    #[test]
    fn money_is_exact_and_currency_is_validated() {
        let amount = Decimal::new(142_700, 2).expect("decimal");
        let money = Money::new(amount, "USD").expect("money");
        assert_eq!(money.amount.coefficient, 142_700);
        assert!(Money::new(amount, "usd").is_err());
        assert!(Money::new(amount, "US").is_err());
        assert!(Money::new(amount, "US1").is_err());
        assert!(Decimal::new(1, 19).is_err());
    }

    #[test]
    fn unknown_is_not_known_false() {
        assert_ne!(
            KnowledgeValue::Unknown,
            KnowledgeValue::Known(MaterialValue::Boolean(false))
        );
        assert_ne!(KnowledgeValue::NotApplicable, KnowledgeValue::NotEvaluated);
    }
}
