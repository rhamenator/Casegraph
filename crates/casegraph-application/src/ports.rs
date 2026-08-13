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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ErrorKind, EvaluationBundle, RegisterRuleBundle};
    use casegraph_domain::{
        KnowledgeValue, MaterialValue, Rule, RuleCondition, RuleDefinition, RuleEvaluation,
        RuleInput, RuleResult, WorkflowEffect,
    };

    struct MinimalRepository;

    impl EvidenceRepository for MinimalRepository {
        fn create_case(&self, _bundle: &CreateCaseBundle) -> Result<Case, AppError> {
            unreachable!("unused mandatory operation")
        }

        fn get_case(&self, _case_id: &RecordId) -> Result<Option<Case>, AppError> {
            unreachable!("unused mandatory operation")
        }

        fn ingest(&self, _bundle: &IngestionBundle) -> Result<IngestionResult, AppError> {
            unreachable!("unused mandatory operation")
        }

        fn get_artifact_version(
            &self,
            _version_id: &RecordId,
        ) -> Result<Option<ArtifactVersion>, AppError> {
            unreachable!("unused mandatory operation")
        }

        fn list_artifact_versions(
            &self,
            _case_id: &RecordId,
        ) -> Result<Vec<ArtifactVersion>, AppError> {
            unreachable!("unused mandatory operation")
        }

        fn get_provenance(
            &self,
            _provenance_id: &RecordId,
        ) -> Result<Option<ProvenanceRecord>, AppError> {
            unreachable!("unused mandatory operation")
        }

        fn record_claim(&self, _bundle: &ClaimBundle) -> Result<ClaimResult, AppError> {
            unreachable!("unused mandatory operation")
        }

        fn get_claim(&self, _claim_id: &RecordId) -> Result<Option<Claim>, AppError> {
            unreachable!("unused mandatory operation")
        }

        fn list_claims(&self, _case_id: &RecordId) -> Result<Vec<Claim>, AppError> {
            unreachable!("unused mandatory operation")
        }

        fn list_contradictions(&self, _case_id: &RecordId) -> Result<Vec<Contradiction>, AppError> {
            unreachable!("unused mandatory operation")
        }

        fn review_claim(&self, _bundle: &ReviewBundle) -> Result<HumanReview, AppError> {
            unreachable!("unused mandatory operation")
        }

        fn correct_claim(&self, _bundle: &CorrectionBundle) -> Result<Correction, AppError> {
            unreachable!("unused mandatory operation")
        }

        fn list_audit_events(&self, _case_id: &RecordId) -> Result<Vec<AuditEvent>, AppError> {
            unreachable!("unused mandatory operation")
        }
    }

    fn id(value: &str) -> RecordId {
        RecordId::parse(value).expect("fixture id")
    }

    fn timestamp() -> TimestampMs {
        TimestampMs::new(1_000).expect("fixture timestamp")
    }

    fn rule_bundle() -> RegisterRuleBundle {
        let definition = RuleDefinition {
            all: vec![RuleCondition {
                subject_key: "subject:one".to_owned(),
                predicate: "status".to_owned(),
                expected: KnowledgeValue::Known(MaterialValue::Text("ready".to_owned())),
            }],
            effect: WorkflowEffect {
                obligation_kind: "respond".to_owned(),
                obligation_description: "Respond".to_owned(),
                deadline_anchor_predicate: "received_date".to_owned(),
                deadline_days_after: 1,
                task_title: "Respond".to_owned(),
            },
        };
        RegisterRuleBundle {
            rule: Rule {
                id: id("rule_1"),
                package_id: "fixture".to_owned(),
                stable_key: "fixture.rule".to_owned(),
                title: "Fixture rule".to_owned(),
                created_at: timestamp(),
            },
            version: RuleVersion {
                id: id("rule_version_1"),
                rule_id: id("rule_1"),
                version: 1,
                definition,
                definition_sha256: "a".repeat(64),
                effective_from: None,
                effective_until: None,
                created_at: timestamp(),
            },
            context: OperationContext {
                audit_id: id("audit_1"),
                actor: "test".to_owned(),
                correlation_id: id("correlation_1"),
                occurred_at: timestamp(),
                reason: None,
            },
        }
    }

    fn evaluation_bundle() -> EvaluationBundle {
        EvaluationBundle {
            materialization: WorkflowMaterialization {
                evaluation: RuleEvaluation {
                    id: id("rule_evaluation_1"),
                    case_id: id("case_1"),
                    rule_version_id: id("rule_version_1"),
                    inputs: vec![RuleInput {
                        claim_id: id("claim_1"),
                        evidence_ids: vec![id("evidence_1")],
                        subject_key: "subject:one".to_owned(),
                        predicate: "status".to_owned(),
                        value: KnowledgeValue::Known(MaterialValue::Text("ready".to_owned())),
                    }],
                    inputs_sha256: "b".repeat(64),
                    result: RuleResult::Indeterminate,
                    explanation: "fixture".to_owned(),
                    evaluated_at: timestamp(),
                    evaluator_version: "fixture-v1".to_owned(),
                    correlation_id: id("correlation_1"),
                },
                obligation: None,
                deadline: None,
                task: None,
            },
            context: OperationContext {
                audit_id: id("audit_2"),
                actor: "test".to_owned(),
                correlation_id: id("correlation_1"),
                occurred_at: timestamp(),
                reason: None,
            },
        }
    }

    #[test]
    fn optional_repository_capabilities_fail_closed_by_default() {
        let repository = MinimalRepository;
        assert_eq!(
            repository.register_rule(&rule_bundle()).unwrap_err().kind(),
            ErrorKind::Unsupported
        );
        assert_eq!(
            repository.get_rule_version(&id("rule_version_1")).unwrap(),
            None
        );
        assert_eq!(
            repository
                .list_grounded_claims(&id("case_1"))
                .unwrap_err()
                .kind(),
            ErrorKind::Unsupported
        );
        assert_eq!(
            repository
                .record_evaluation(&evaluation_bundle())
                .unwrap_err()
                .kind(),
            ErrorKind::Unsupported
        );
        assert!(repository.list_workflow(&id("case_1")).unwrap().is_empty());
    }
}
