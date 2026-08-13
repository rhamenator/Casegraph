//! Staged deterministic extraction contracts and orchestration.

use crate::{AppError, CasegraphService, ErrorKind, RecordExternalClaimRequest};
use casegraph_domain::{Confidence, KnowledgeValue, RecordId, TemporalValue};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Supported deterministic artifact classifications.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactFormat {
    PlainText,
    Json,
    Csv,
}

impl ArtifactFormat {
    /// Classify a bounded set of exact media types. Unsupported input remains ingested evidence.
    pub fn classify(media_type: &str) -> Result<Self, PipelineError> {
        match media_type.split(';').next().map(str::trim) {
            Some("text/plain") => Ok(Self::PlainText),
            Some("application/json") => Ok(Self::Json),
            Some("text/csv") | Some("application/csv") => Ok(Self::Csv),
            _ => Err(PipelineError::new(
                PipelineFailureKind::UnsupportedFormat,
                "artifact format is not supported by deterministic extraction",
            )),
        }
    }
}

/// Explicit extraction stages, preserved in execution results and diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStage {
    Classification,
    RawExtraction,
    StructuralExtraction,
    SemanticExtraction,
    Normalization,
    Validation,
    EvidenceCreation,
}

/// Safe failure categories; messages never contain source contents.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineFailureKind {
    UnsupportedFormat,
    UnreadableArtifact,
    MalformedInput,
    NoObservations,
    ValidationRejected,
    Internal,
}

/// Inspectable pipeline failure with safe retry semantics.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PipelineError {
    pub kind: PipelineFailureKind,
    pub safe_message: String,
    pub retryable: bool,
}

impl PipelineError {
    /// Construct a non-retryable deterministic failure.
    pub fn new(kind: PipelineFailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            safe_message: message.into(),
            retryable: false,
        }
    }
}

impl From<AppError> for PipelineError {
    fn from(error: AppError) -> Self {
        Self {
            kind: if error.kind() == ErrorKind::InvalidInput {
                PipelineFailureKind::ValidationRejected
            } else {
                PipelineFailureKind::Internal
            },
            safe_message: error.safe_message().to_owned(),
            retryable: error.kind() == ErrorKind::Storage,
        }
    }
}

/// Precise source location emitted by deterministic structural extraction.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExtractedLocation {
    pub source_field: Option<String>,
    pub paragraph_number: Option<u32>,
    pub text_span_start: Option<u64>,
    pub text_span_end: Option<u64>,
    pub row_number: Option<u32>,
    pub column_number: Option<u32>,
}

/// Validated material assertion candidate. This is not authoritative until persisted as a claim.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtractedField {
    pub subject_key: String,
    pub predicate: String,
    pub original_value: String,
    pub normalized_value: KnowledgeValue,
    pub temporal: Option<TemporalValue>,
    pub location: ExtractedLocation,
    pub extraction_confidence: Option<Confidence>,
}

/// Pluggable deterministic extractor. Extractors do not persist or generate identifiers.
pub trait DeterministicExtractor: Send + Sync {
    /// Stable implementation name for provenance.
    fn name(&self) -> &'static str;
    /// Stable implementation version for reproducibility.
    fn version(&self) -> &'static str;
    /// Formats accepted by this implementation.
    fn supports(&self, format: ArtifactFormat) -> bool;
    /// Raw -> structural -> semantic -> normalized validated candidates.
    fn extract(
        &self,
        format: ArtifactFormat,
        bytes: &[u8],
    ) -> Result<Vec<ExtractedField>, PipelineError>;
}

/// Request to run deterministic extraction for one immutable artifact version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtractArtifactRequest {
    pub case_id: RecordId,
    pub artifact_version_id: RecordId,
    pub media_type: String,
    pub connector: Option<String>,
    pub actor: String,
    pub correlation_id: Option<RecordId>,
}

/// Staged result with claims created through the shared evidence service.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtractionResult {
    pub stages: Vec<PipelineStage>,
    pub claims: Vec<casegraph_domain::Claim>,
    pub warnings: Vec<String>,
}

/// Extraction orchestrator. AI is intentionally absent from this deterministic path.
#[derive(Clone)]
pub struct ExtractionPipeline {
    service: CasegraphService,
    extractors: Vec<Arc<dyn DeterministicExtractor>>,
}

impl ExtractionPipeline {
    /// Create a pipeline from an explicit extractor registry.
    pub fn new(
        service: CasegraphService,
        extractors: Vec<Arc<dyn DeterministicExtractor>>,
    ) -> Self {
        Self {
            service,
            extractors,
        }
    }

    /// Run every explicit stage and persist evidence only after validation.
    pub fn extract(
        &self,
        request: ExtractArtifactRequest,
    ) -> Result<ExtractionResult, PipelineError> {
        let format = ArtifactFormat::classify(&request.media_type)?;
        let bytes = self
            .service
            .read_artifact_version(&request.artifact_version_id)?;
        let extractor = self
            .extractors
            .iter()
            .find(|extractor| extractor.supports(format))
            .ok_or_else(|| {
                PipelineError::new(
                    PipelineFailureKind::UnsupportedFormat,
                    "no registered deterministic extractor supports this format",
                )
            })?;
        let fields = extractor.extract(format, &bytes)?;
        if fields.is_empty() {
            return Err(PipelineError::new(
                PipelineFailureKind::NoObservations,
                "deterministic extraction produced no observations",
            ));
        }

        let mut claims = Vec::with_capacity(fields.len());
        for field in fields {
            let result = self
                .service
                .record_external_claim(RecordExternalClaimRequest {
                    case_id: request.case_id.clone(),
                    artifact_version_id: request.artifact_version_id.clone(),
                    subject_id: None,
                    subject_key: field.subject_key,
                    predicate: field.predicate,
                    original_value: field.original_value,
                    normalized_value: field.normalized_value,
                    temporal: field.temporal,
                    connector: request.connector.clone(),
                    endpoint: None,
                    external_record_id: None,
                    source_field: field.location.source_field,
                    page_number: None,
                    paragraph_number: field.location.paragraph_number,
                    text_span_start: field.location.text_span_start,
                    text_span_end: field.location.text_span_end,
                    table_number: None,
                    row_number: field.location.row_number,
                    column_number: field.location.column_number,
                    bounding_region_json: None,
                    extraction_method: "deterministic".to_owned(),
                    extractor_name: extractor.name().to_owned(),
                    extractor_version: extractor.version().to_owned(),
                    model_provider: None,
                    model_name: None,
                    model_version: None,
                    model_configuration_json: None,
                    extraction_confidence: field.extraction_confidence,
                    interpretation_confidence: None,
                    actor: request.actor.clone(),
                    correlation_id: request.correlation_id.clone(),
                })?;
            claims.push(result.claim);
        }

        Ok(ExtractionResult {
            stages: vec![
                PipelineStage::Classification,
                PipelineStage::RawExtraction,
                PipelineStage::StructuralExtraction,
                PipelineStage::SemanticExtraction,
                PipelineStage::Normalization,
                PipelineStage::Validation,
                PipelineStage::EvidenceCreation,
            ],
            claims,
            warnings: Vec::new(),
        })
    }
}
