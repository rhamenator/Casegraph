#![forbid(unsafe_code)]

//! Persistence, artifact storage, extraction, and diagnostics adapters.

pub mod adapters;
pub mod artifact_store;
pub mod config;
pub mod diagnostics;
pub mod extractors;
pub mod migrations;
pub mod sqlite_repository;

pub use adapters::{Sha256IdGenerator, SystemClock};
pub use artifact_store::FilesystemArtifactStore;
pub use config::Config;
pub use diagnostics::{DiagnosticEvent, DiagnosticOutcome};
pub use extractors::CoreDeterministicExtractor;
pub use sqlite_repository::SqliteEvidenceRepository;
