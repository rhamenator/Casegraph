use crate::{DomainError, KnowledgeValue, RecordId, TemporalValue, TimestampMs};
use serde::{Deserialize, Serialize};

/// Explicit confidence only when a producer supplies a meaningful score.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Confidence(f64);

impl Confidence {
    /// Validate an inclusive probability-like score.
    pub fn new(value: f64) -> Result<Self, DomainError> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(DomainError::new(
                "confidence",
                "must be a finite number between 0 and 1",
            ));
        }
        Ok(Self(value))
    }

    /// Return the validated score.
    pub fn get(self) -> f64 {
        self.0
    }
}

/// Case lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseStatus {
    Open,
    Suspended,
    Closed,
}

/// A domain-neutral case container.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Case {
    pub id: RecordId,
    pub title: String,
    pub status: CaseStatus,
    pub created_at: TimestampMs,
    pub closed_at: Option<TimestampMs>,
}

impl Case {
    /// Validate title and lifecycle timestamps.
    pub fn validate(&self) -> Result<(), DomainError> {
        validate_text("case.title", &self.title, 1, 300)?;
        if self
            .closed_at
            .is_some_and(|closed| closed < self.created_at)
        {
            return Err(DomainError::new(
                "case.closed_at",
                "must not precede creation",
            ));
        }
        Ok(())
    }
}

/// Identity and retrieval metadata for an external source.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub id: RecordId,
    pub case_id: RecordId,
    pub connector: String,
    pub locator: String,
    pub external_record_id: Option<String>,
    pub endpoint: Option<String>,
    pub source_revision: Option<String>,
    pub retrieved_at: TimestampMs,
}

/// Stable artifact identity, distinct from source bytes and versions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Artifact {
    pub id: RecordId,
    pub case_id: RecordId,
    pub source_id: RecordId,
    pub source_key: String,
    pub created_at: TimestampMs,
}

/// Immutable source-byte version and content address.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArtifactVersion {
    pub id: RecordId,
    pub artifact_id: RecordId,
    pub version_number: u32,
    pub content_sha256: String,
    pub content_length: u64,
    pub media_type: String,
    pub storage_key: String,
    pub ingested_at: TimestampMs,
    pub received_at: Option<TimestampMs>,
    pub original_filename: Option<String>,
}

impl ArtifactVersion {
    /// Validate immutable artifact metadata.
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.version_number == 0 {
            return Err(DomainError::new(
                "artifact_version.version_number",
                "must be positive",
            ));
        }
        if self.content_sha256.len() != 64
            || !self
                .content_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(DomainError::new(
                "artifact_version.content_sha256",
                "must be 64 lowercase hexadecimal characters",
            ));
        }
        validate_text("artifact_version.media_type", &self.media_type, 1, 255)?;
        validate_text("artifact_version.storage_key", &self.storage_key, 1, 255)
    }
}

/// Source location and production method for a material assertion.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProvenanceRecord {
    pub id: RecordId,
    pub artifact_version_id: Option<RecordId>,
    pub connector: Option<String>,
    pub endpoint: Option<String>,
    pub external_record_id: Option<String>,
    pub source_field: Option<String>,
    pub page_number: Option<u32>,
    pub paragraph_number: Option<u32>,
    pub text_span_start: Option<u64>,
    pub text_span_end: Option<u64>,
    pub table_number: Option<u32>,
    pub row_number: Option<u32>,
    pub column_number: Option<u32>,
    pub bounding_region_json: Option<String>,
    pub extraction_method: String,
    pub extractor_name: String,
    pub extractor_version: String,
    pub model_provider: Option<String>,
    pub model_name: Option<String>,
    pub model_version: Option<String>,
    pub model_configuration_json: Option<String>,
    pub extracted_at: TimestampMs,
    pub confidence: Option<Confidence>,
    pub verification_state: VerificationState,
    pub original_representation: Option<String>,
    pub correlation_id: RecordId,
}

impl ProvenanceRecord {
    /// Validate location ordering and model attribution consistency.
    pub fn validate(&self) -> Result<(), DomainError> {
        validate_text(
            "provenance.extraction_method",
            &self.extraction_method,
            1,
            100,
        )?;
        validate_text("provenance.extractor_name", &self.extractor_name, 1, 200)?;
        validate_text(
            "provenance.extractor_version",
            &self.extractor_version,
            1,
            100,
        )?;
        if matches!((self.text_span_start, self.text_span_end), (Some(start), Some(end)) if end < start)
        {
            return Err(DomainError::new(
                "provenance.text_span",
                "end must not precede start",
            ));
        }
        if self.model_provider.is_none()
            && (self.model_name.is_some()
                || self.model_version.is_some()
                || self.model_configuration_json.is_some())
        {
            return Err(DomainError::new(
                "provenance.model_provider",
                "is required when model metadata is present",
            ));
        }
        Ok(())
    }
}

/// Human verification state independent of probabilistic confidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationState {
    NotReviewed,
    Verified,
    Rejected,
    Corrected,
}

/// Origin class used to enforce provenance on external assertions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssertionOrigin {
    External,
    Human,
    Rule,
    System,
}

/// Claim epistemic lifecycle. Presence in storage does not imply truth.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimState {
    Observed,
    Extracted,
    Inferred,
    Corroborated,
    Disputed,
    Contradicted,
    Superseded,
    Verified,
    Rejected,
    Unresolved,
}

/// Immutable observation produced from an external source location.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    pub id: RecordId,
    pub case_id: RecordId,
    pub subject_id: Option<RecordId>,
    pub predicate: String,
    pub original_value: String,
    pub normalized_value: Option<KnowledgeValue>,
    pub provenance_id: RecordId,
    pub extraction_confidence: Option<Confidence>,
    pub observed_at: TimestampMs,
}

/// A source assertion, inference, or human assertion. It is not automatically a fact.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Claim {
    pub id: RecordId,
    pub case_id: RecordId,
    pub subject_id: Option<RecordId>,
    pub subject_key: String,
    pub predicate: String,
    pub original_value: String,
    pub normalized_value: KnowledgeValue,
    pub origin: AssertionOrigin,
    pub initial_state: ClaimState,
    pub primary_provenance_id: Option<RecordId>,
    pub interpretation_confidence: Option<Confidence>,
    pub temporal: Option<TemporalValue>,
    pub created_at: TimestampMs,
}

impl Claim {
    /// Enforce the material-claim provenance invariant and bounded vocabulary fields.
    pub fn validate(&self) -> Result<(), DomainError> {
        validate_text("claim.subject_key", &self.subject_key, 1, 300)?;
        validate_text("claim.predicate", &self.predicate, 1, 300)?;
        if self.origin == AssertionOrigin::External && self.primary_provenance_id.is_none() {
            return Err(DomainError::new(
                "claim.primary_provenance_id",
                "externally derived claims require provenance",
            ));
        }
        if let Some(temporal) = &self.temporal {
            temporal.validate()?;
        }
        Ok(())
    }
}

/// Established value that references the verified claim it came from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fact {
    pub id: RecordId,
    pub case_id: RecordId,
    pub claim_id: RecordId,
    pub established_value: KnowledgeValue,
    pub established_at: TimestampMs,
    pub established_by: String,
}

/// An inspectable pair of incompatible claims.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Contradiction {
    pub id: RecordId,
    pub case_id: RecordId,
    pub claim_a_id: RecordId,
    pub claim_b_id: RecordId,
    pub status: ContradictionStatus,
    pub detection_method: DetectionMethod,
    pub rationale: Option<String>,
    pub resolution_claim_id: Option<RecordId>,
    pub adjudicated_by: Option<String>,
    pub created_at: TimestampMs,
    pub resolved_at: Option<TimestampMs>,
}

/// Contradiction lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContradictionStatus {
    Unresolved,
    Resolved,
    Superseded,
}

/// Whether a contradiction was safely detected or explicitly recorded by a human.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionMethod {
    Automatic,
    Human,
}

/// Human review outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Verified,
    Rejected,
    Corrected,
    NeedsMoreEvidence,
}

/// Append-only review record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HumanReview {
    pub id: RecordId,
    pub case_id: RecordId,
    pub target_kind: String,
    pub target_id: RecordId,
    pub decision: ReviewDecision,
    pub actor: String,
    pub rationale: Option<String>,
    pub reviewed_at: TimestampMs,
    pub correlation_id: RecordId,
}

/// History-preserving correction linking original and corrected claims.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Correction {
    pub id: RecordId,
    pub case_id: RecordId,
    pub original_claim_id: RecordId,
    pub corrected_claim_id: RecordId,
    pub review_id: RecordId,
    pub provenance_id: RecordId,
    pub original_value: KnowledgeValue,
    pub corrected_value: KnowledgeValue,
    pub actor: String,
    pub rationale: Option<String>,
    pub corrected_at: TimestampMs,
    pub affected_derivations: Vec<RecordId>,
}

/// Append-only operational audit record, separate from evidentiary provenance.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: RecordId,
    pub case_id: Option<RecordId>,
    pub operation: String,
    pub actor: String,
    pub target_kind: String,
    pub target_id: RecordId,
    pub previous_state_json: Option<String>,
    pub resulting_state_json: String,
    pub reason: Option<String>,
    pub occurred_at: TimestampMs,
    pub correlation_id: RecordId,
}

/// Minimal evidence record supporting a graph edge.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub id: RecordId,
    pub case_id: RecordId,
    pub evidence_type: EvidenceType,
    pub provenance_id: Option<RecordId>,
    pub description: String,
    pub created_at: TimestampMs,
}

/// Evidence origin class.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceType {
    ArtifactExcerpt,
    StructuredField,
    HumanAttestation,
    RuleResult,
}

/// How a claim/effect relates to evidence or another graph node.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceRelationship {
    SupportedBy,
    ContradictedBy,
    Corroborates,
    Contradicts,
    Supersedes,
    Involves,
    CreatedBy,
    AppliesTo,
    Satisfies,
    Produces,
}

fn validate_text(
    field: &'static str,
    value: &str,
    min: usize,
    max: usize,
) -> Result<(), DomainError> {
    let length = value.trim().chars().count();
    if !(min..=max).contains(&length) {
        return Err(DomainError::new(
            field,
            format!("trimmed length must be between {min} and {max}"),
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(DomainError::new(
            field,
            "control characters are not allowed",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{AssertionOrigin, Claim, ClaimState, Confidence};
    use crate::{KnowledgeValue, RecordId, TimestampMs};

    fn id(value: &str) -> RecordId {
        RecordId::parse(value).expect("fixture id")
    }

    #[test]
    fn external_material_claim_without_provenance_is_rejected() {
        let claim = Claim {
            id: id("claim_1"),
            case_id: id("case_1"),
            subject_id: None,
            subject_key: "subject".to_owned(),
            predicate: "amount".to_owned(),
            original_value: "$1,427.00".to_owned(),
            normalized_value: KnowledgeValue::Unknown,
            origin: AssertionOrigin::External,
            initial_state: ClaimState::Extracted,
            primary_provenance_id: None,
            interpretation_confidence: None,
            temporal: None,
            created_at: TimestampMs::new(1).expect("timestamp"),
        };
        assert!(claim.validate().is_err());
    }

    #[test]
    fn confidence_is_optional_but_never_out_of_range_or_nan() {
        assert!(Confidence::new(0.0).is_ok());
        assert!(Confidence::new(1.0).is_ok());
        assert!(Confidence::new(f64::NAN).is_err());
        assert!(Confidence::new(1.01).is_err());
    }
}
