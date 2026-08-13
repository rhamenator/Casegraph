#![forbid(unsafe_code)]

//! Domain-neutral canonical types and invariants. This crate has no adapter dependencies.

mod error;
mod id;
mod model;
mod temporal;
mod value;

pub use error::DomainError;
pub use id::RecordId;
pub use model::*;
pub use temporal::{Date, TemporalPrecision, TemporalValue, TimestampMs};
pub use value::{Decimal, KnowledgeValue, MaterialValue, Money};

/// Current canonical schema version understood by the domain layer.
pub const CANONICAL_SCHEMA_VERSION: u32 = 1;
