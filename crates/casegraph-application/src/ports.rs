use crate::AppError;
use casegraph_domain::{
    Artifact, ArtifactVersion, AuditEvent, Case, Claim, ClaimState, Contradiction, Correction,
    Evidence, Fact, GroundedClaim, HumanReview, Observation, ProvenanceRecord, RecordId, Rule,
    RuleVersion, Source, TimestampMs, WorkflowMaterialization,
};
use serde::{Deserialize, Serialize};

/// Deterministic clock boundary.
pub trait Clock: Send + Sync {
    /// Current non-negative Unix timestamp.
    fn now(&self) -> Result<TimestampMs, AppError>;
}

/// Identifier generation boundary. IDs are opaque and must not encode sensitive content.
pub trait IdGenerator: Send + Sync {
    /// Produce a validated identifier with a non-sensitive kind prefix.
    fn next(&self, kind: &'static str) -> Result<RecordId, AppError>;
}

/// Result of a content-addressed byte write.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StoredArtifact {
    /// Lowercase SHA-256 of the exact source bytes.
    pub content_sha256: String,
    /// Number of exact source bytes.
    pub content_length: u64,
    /// Opaque store key; never a caller-supplied path.
    pub storage_key: String,
    /// Whether the exact content already existed and was verified.
    pub already_existed: bool,
}

/// Immutable artifact-byte storage boundary.
pub trait ArtifactStore: Send + Sync {
    /// Persist exact bytes using a cryptographic content address.
    fn put(&self, bytes: &[u8]) -> Result<StoredArtifact, AppError>;

    /// Recover exact bytes by validated internal storage key.
    fn read(&self, storage_key: &str) -> Result<Vec<u8>, AppError>;
}

/// Actor/correlation context for an audited state change.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationContext {
    pub audit_id: RecordId,
    pub actor: String,
    pub correlation_id: RecordId,
    pub occurred_at: TimestampMs,
    pub reason: Option<String>,
}

/// Atomic case-creation persistence request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateCaseBundle {
    pub case_record: Case,
    pub context: OperationContext,
}

/// Atomic ingestion persistence request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IngestionBundle {
    pub source: Source,
    pub artifact: Artifact,
    pub artifact_version: ArtifactVersion,
    pub context: OperationContext,
}

/// Whether ingestion created evidence identity/version or reused an exact duplicate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestionDisposition {
    NewArtifact,
    NewVersion,
    ExactDuplicate,
}

/// Persisted ingestion result using authoritative IDs/version numbers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IngestionResult {
    pub artifact: Artifact,
    pub artifact_version: ArtifactVersion,
    pub disposition: IngestionDisposition,
}

/// Atomic externally-derived assertion persistence request.
#[derive(Clone, Debug, PartialEq)]
pub struct ClaimBundle {
    pub provenance: ProvenanceRecord,
    pub observation: Observation,
    pub claim: Claim,
    pub evidence: Evidence,
    pub evidence_edge_id: RecordId,
    pub context: OperationContext,
}

/// Contradiction/corroboration side effects produced without deleting either claim.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClaimResult {
    pub claim: Claim,
    pub contradictions: Vec<Contradiction>,
    pub corroborates: Vec<RecordId>,
}

/// Atomic human review persistence request.
#[derive(Clone, Debug, PartialEq)]
pub struct ReviewBundle {
    pub review: HumanReview,
    pub state_change_id: RecordId,
    pub state: ClaimState,
    pub fact: Option<Fact>,
    pub context: OperationContext,
}

/// Atomic correction persistence request.
#[derive(Clone, Debug, PartialEq)]
pub struct CorrectionBundle {
    pub provenance: ProvenanceRecord,
    pub corrected_claim: Claim,
    pub review: HumanReview,
    pub correction: Correction,
    pub original_state_change_id: RecordId,
    pub corrected_state_change_id: RecordId,
    pub context: OperationContext,
}

/// Atomic rule identity/version registration.
#[derive(Clone, Debug, PartialEq)]
pub struct RegisterRuleBundle {
    pub rule: Rule,
    pub version: RuleVersion,
    pub context: OperationContext,
}

/// Atomic evaluation and optional workflow materialization.
#[derive(Clone, Debug, PartialEq)]
pub struct EvaluationBundle {
    pub materialization: WorkflowMaterialization,
    pub context: OperationContext,
}

/// Durable evidence repository. Each mutating method is one database transaction.
pub trait EvidenceRepository: Send + Sync {
    fn create_case(&self, bundle: &CreateCaseBundle) -> Result<Case, AppError>;
    fn get_case(&self, case_id: &RecordId) -> Result<Option<Case>, AppError>;
    fn ingest(&self, bundle: &IngestionBundle) -> Result<IngestionResult, AppError>;
    fn get_artifact_version(
        &self,
        version_id: &RecordId,
    ) -> Result<Option<ArtifactVersion>, AppError>;
    fn list_artifact_versions(&self, case_id: &RecordId) -> Result<Vec<ArtifactVersion>, AppError>;
    fn get_provenance(
        &self,
        provenance_id: &RecordId,
    ) -> Result<Option<ProvenanceRecord>, AppError>;
    fn record_claim(&self, bundle: &ClaimBundle) -> Result<ClaimResult, AppError>;
    fn get_claim(&self, claim_id: &RecordId) -> Result<Option<Claim>, AppError>;
    fn list_claims(&self, case_id: &RecordId) -> Result<Vec<Claim>, AppError>;
    fn list_contradictions(&self, case_id: &RecordId) -> Result<Vec<Contradiction>, AppError>;
    fn review_claim(&self, bundle: &ReviewBundle) -> Result<HumanReview, AppError>;
    fn correct_claim(&self, bundle: &CorrectionBundle) -> Result<Correction, AppError>;
    fn list_audit_events(&self, case_id: &RecordId) -> Result<Vec<AuditEvent>, AppError>;
    fn register_rule(&self, _bundle: &RegisterRuleBundle) -> Result<RuleVersion, AppError> {
        Err(AppError::new(
            crate::ErrorKind::Unsupported,
            "repository does not support rules",
        ))
    }
    fn get_rule_version(
        &self,
        _rule_version_id: &RecordId,
    ) -> Result<Option<RuleVersion>, AppError> {
        Ok(None)
    }
    fn list_grounded_claims(&self, _case_id: &RecordId) -> Result<Vec<GroundedClaim>, AppError> {
        Err(AppError::new(
            crate::ErrorKind::Unsupported,
            "repository does not support grounded claims",
        ))
    }
    fn record_evaluation(
        &self,
        _bundle: &EvaluationBundle,
    ) -> Result<WorkflowMaterialization, AppError> {
        Err(AppError::new(
            crate::ErrorKind::Unsupported,
            "repository does not support rule evaluations",
        ))
    }
    fn list_workflow(&self, _case_id: &RecordId) -> Result<Vec<WorkflowMaterialization>, AppError> {
        Ok(Vec::new())
    }
}
