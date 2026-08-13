//! Content-minimized structured operational diagnostics.

use casegraph_application::{AppError, ErrorKind};
use casegraph_domain::{RecordId, TimestampMs};
use serde::Serialize;

/// Bounded operation outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticOutcome {
    Started,
    Completed,
    CompletedWithWarnings,
    Failed,
}

/// Structured event suitable for tracing one correlation through pipeline stages.
/// It deliberately has no source-content or arbitrary metadata field.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DiagnosticEvent {
    pub event: String,
    pub correlation_id: RecordId,
    pub stage: String,
    pub outcome: DiagnosticOutcome,
    pub occurred_at: TimestampMs,
    pub duration_ms: Option<u64>,
    pub target_id: Option<RecordId>,
    pub failure_kind: Option<String>,
}

impl DiagnosticEvent {
    /// Render one JSON line without accepting source contents.
    pub fn to_json_line(&self) -> Result<String, AppError> {
        serde_json::to_string(self).map_err(|_| {
            AppError::new(
                ErrorKind::Internal,
                "could not serialize operational diagnostic",
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{DiagnosticEvent, DiagnosticOutcome};
    use casegraph_domain::{RecordId, TimestampMs};

    #[test]
    fn diagnostics_are_structured_correlated_and_have_no_content_field() {
        let event = DiagnosticEvent {
            event: "pipeline.stage".to_owned(),
            correlation_id: RecordId::parse("correlation_1").expect("id"),
            stage: "normalization".to_owned(),
            outcome: DiagnosticOutcome::Completed,
            occurred_at: TimestampMs::new(1).expect("time"),
            duration_ms: Some(4),
            target_id: Some(RecordId::parse("artifact_version_1").expect("id")),
            failure_kind: None,
        };
        let json = event.to_json_line().expect("serialize");
        assert!(json.contains("correlation_1"));
        assert!(json.contains("\"duration_ms\":4"));
        assert!(!json.contains("source_content"));
        assert!(!json.contains("document_text"));
    }
}
