use crate::{
    AppError, ArtifactStore, ClaimBundle, ClaimResult, Clock, CorrectionBundle, CreateCaseBundle,
    ErrorKind, EvidenceRepository, IdGenerator, IngestionBundle, IngestionResult, OperationContext,
    ReviewBundle,
};
use casegraph_domain::{
    Artifact, ArtifactVersion, AssertionOrigin, AuditEvent, Case, CaseStatus, Claim, ClaimState,
    Correction, Evidence, EvidenceType, Fact, HumanReview, KnowledgeValue, Observation,
    ProvenanceRecord, RecordId, ReviewDecision, Source, TemporalValue, VerificationState,
};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path};
use std::sync::Arc;

/// Shared application service used by every delivery adapter.
#[derive(Clone)]
pub struct CasegraphService {
    repository: Arc<dyn EvidenceRepository>,
    artifact_store: Arc<dyn ArtifactStore>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    max_artifact_bytes: u64,
}

impl CasegraphService {
    /// Compose shared use cases from explicitly supplied adapters.
    pub fn new(
        repository: Arc<dyn EvidenceRepository>,
        artifact_store: Arc<dyn ArtifactStore>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
        max_artifact_bytes: u64,
    ) -> Result<Self, AppError> {
        if max_artifact_bytes == 0 {
            return Err(AppError::new(
                ErrorKind::InvalidInput,
                "artifact size limit must be positive",
            ));
        }
        Ok(Self {
            repository,
            artifact_store,
            clock,
            ids,
            max_artifact_bytes,
        })
    }

    /// Create an empty domain-neutral case and its audit record.
    pub fn create_case(&self, request: CreateCaseRequest) -> Result<Case, AppError> {
        validate_text("title", &request.title, 1, 300)?;
        validate_text("actor", &request.actor, 1, 300)?;
        let now = self.clock.now()?;
        let case_record = Case {
            id: self.ids.next("case")?,
            title: request.title,
            status: CaseStatus::Open,
            created_at: now,
            closed_at: None,
        };
        case_record.validate()?;
        let context = self.operation_context(request.actor, request.correlation_id, None, now)?;
        self.repository.create_case(&CreateCaseBundle {
            case_record,
            context,
        })
    }

    /// Ingest exact bytes immutably. Content storage happens before the atomic metadata transaction;
    /// a failed metadata transaction may leave only a harmless unreferenced content-addressed blob.
    pub fn ingest_bytes(&self, request: IngestBytesRequest) -> Result<IngestionResult, AppError> {
        validate_text("source_key", &request.source_key, 1, 2048)?;
        validate_text("connector", &request.connector, 1, 100)?;
        validate_text("locator", &request.locator, 1, 2048)?;
        validate_text("media_type", &request.media_type, 1, 255)?;
        validate_filename(request.original_filename.as_deref())?;

        let length = u64::try_from(request.bytes.len()).map_err(|_| {
            AppError::new(ErrorKind::TooLarge, "artifact length cannot be represented")
        })?;
        if length > self.max_artifact_bytes {
            return Err(AppError::new(
                ErrorKind::TooLarge,
                format!(
                    "artifact is {length} bytes; configured limit is {} bytes",
                    self.max_artifact_bytes
                ),
            ));
        }

        if self.repository.get_case(&request.case_id)?.is_none() {
            return Err(AppError::new(ErrorKind::NotFound, "case was not found"));
        }

        let stored = self.artifact_store.put(&request.bytes)?;
        if stored.content_length != length {
            return Err(AppError::new(
                ErrorKind::Storage,
                "artifact store returned an inconsistent byte length",
            ));
        }

        let now = self.clock.now()?;
        let source = Source {
            id: self.ids.next("source")?,
            case_id: request.case_id.clone(),
            connector: request.connector,
            locator: request.locator,
            external_record_id: request.external_record_id,
            endpoint: request.endpoint,
            source_revision: request.source_revision,
            retrieved_at: now,
        };
        let artifact = Artifact {
            id: self.ids.next("artifact")?,
            case_id: request.case_id,
            source_id: source.id.clone(),
            source_key: request.source_key,
            created_at: now,
        };
        let artifact_version = ArtifactVersion {
            id: self.ids.next("artifact_version")?,
            artifact_id: artifact.id.clone(),
            version_number: 1,
            content_sha256: stored.content_sha256,
            content_length: stored.content_length,
            media_type: request.media_type,
            storage_key: stored.storage_key,
            ingested_at: now,
            received_at: request.received_at,
            original_filename: request.original_filename,
        };
        artifact_version.validate()?;
        let context = self.operation_context(request.actor, request.correlation_id, None, now)?;
        self.repository.ingest(&IngestionBundle {
            source,
            artifact,
            artifact_version,
            context,
        })
    }

    /// Persist an externally-derived claim only with precise recoverable provenance.
    pub fn record_external_claim(
        &self,
        request: RecordExternalClaimRequest,
    ) -> Result<ClaimResult, AppError> {
        validate_text("subject_key", &request.subject_key, 1, 300)?;
        validate_text("predicate", &request.predicate, 1, 300)?;
        validate_text("original_value", &request.original_value, 1, 1_000_000)?;
        validate_text("actor", &request.actor, 1, 300)?;
        if self
            .repository
            .get_artifact_version(&request.artifact_version_id)?
            .is_none()
        {
            return Err(AppError::new(
                ErrorKind::NotFound,
                "artifact version was not found",
            ));
        }

        let now = self.clock.now()?;
        let correlation_id = self.correlation_id(request.correlation_id)?;
        let provenance = ProvenanceRecord {
            id: self.ids.next("provenance")?,
            artifact_version_id: Some(request.artifact_version_id),
            connector: request.connector,
            endpoint: request.endpoint,
            external_record_id: request.external_record_id,
            source_field: request.source_field,
            page_number: request.page_number,
            paragraph_number: request.paragraph_number,
            text_span_start: request.text_span_start,
            text_span_end: request.text_span_end,
            table_number: request.table_number,
            row_number: request.row_number,
            column_number: request.column_number,
            bounding_region_json: request.bounding_region_json,
            extraction_method: request.extraction_method,
            extractor_name: request.extractor_name,
            extractor_version: request.extractor_version,
            model_provider: request.model_provider,
            model_name: request.model_name,
            model_version: request.model_version,
            model_configuration_json: request.model_configuration_json,
            extracted_at: now,
            confidence: request.extraction_confidence,
            verification_state: VerificationState::NotReviewed,
            original_representation: Some(request.original_value.clone()),
            correlation_id: correlation_id.clone(),
        };
        provenance.validate()?;
        let observation = Observation {
            id: self.ids.next("observation")?,
            case_id: request.case_id.clone(),
            subject_id: request.subject_id.clone(),
            predicate: request.predicate.clone(),
            original_value: request.original_value.clone(),
            normalized_value: Some(request.normalized_value.clone()),
            provenance_id: provenance.id.clone(),
            extraction_confidence: request.extraction_confidence,
            observed_at: now,
        };
        let claim = Claim {
            id: self.ids.next("claim")?,
            case_id: request.case_id.clone(),
            subject_id: request.subject_id,
            subject_key: request.subject_key,
            predicate: request.predicate.clone(),
            original_value: request.original_value,
            normalized_value: request.normalized_value,
            origin: AssertionOrigin::External,
            initial_state: ClaimState::Extracted,
            primary_provenance_id: Some(provenance.id.clone()),
            interpretation_confidence: request.interpretation_confidence,
            temporal: request.temporal,
            created_at: now,
        };
        claim.validate()?;
        let evidence = Evidence {
            id: self.ids.next("evidence")?,
            case_id: request.case_id,
            evidence_type: EvidenceType::ArtifactExcerpt,
            provenance_id: Some(provenance.id.clone()),
            description: format!(
                "Artifact excerpt supporting predicate {}",
                request.predicate
            ),
            created_at: now,
        };
        let context = OperationContext {
            audit_id: self.ids.next("audit")?,
            actor: request.actor,
            correlation_id,
            occurred_at: now,
            reason: None,
        };
        self.repository.record_claim(&ClaimBundle {
            provenance,
            observation,
            claim,
            evidence,
            evidence_edge_id: self.ids.next("edge")?,
            context,
        })
    }

    /// Verify, reject, or defer a claim without mutating the original assertion.
    pub fn review_claim(&self, request: ReviewClaimRequest) -> Result<HumanReview, AppError> {
        validate_text("actor", &request.actor, 1, 300)?;
        let claim = self
            .repository
            .get_claim(&request.claim_id)?
            .ok_or_else(|| AppError::new(ErrorKind::NotFound, "claim was not found"))?;
        let now = self.clock.now()?;
        let correlation_id = self.correlation_id(request.correlation_id)?;
        let review = HumanReview {
            id: self.ids.next("review")?,
            case_id: claim.case_id.clone(),
            target_kind: "claim".to_owned(),
            target_id: claim.id.clone(),
            decision: request.decision,
            actor: request.actor.clone(),
            rationale: request.rationale.clone(),
            reviewed_at: now,
            correlation_id: correlation_id.clone(),
        };
        let (state, fact) = match request.decision {
            ReviewDecision::Verified => (
                ClaimState::Verified,
                Some(Fact {
                    id: self.ids.next("fact")?,
                    case_id: claim.case_id.clone(),
                    claim_id: claim.id.clone(),
                    established_value: claim.normalized_value.clone(),
                    established_at: now,
                    established_by: request.actor.clone(),
                }),
            ),
            ReviewDecision::Rejected => (ClaimState::Rejected, None),
            ReviewDecision::NeedsMoreEvidence => (ClaimState::Unresolved, None),
            ReviewDecision::Corrected => {
                return Err(AppError::new(
                    ErrorKind::InvalidInput,
                    "use correct_claim when the decision supplies a corrected value",
                ));
            }
        };
        let context = OperationContext {
            audit_id: self.ids.next("audit")?,
            actor: request.actor,
            correlation_id,
            occurred_at: now,
            reason: request.rationale,
        };
        self.repository.review_claim(&ReviewBundle {
            review,
            state_change_id: self.ids.next("claim_state")?,
            state,
            fact,
            context,
        })
    }

    /// Correct a claim by appending a human claim, review, correction, and state transitions.
    pub fn correct_claim(&self, request: CorrectClaimRequest) -> Result<Correction, AppError> {
        validate_text("actor", &request.actor, 1, 300)?;
        validate_text(
            "corrected_original_representation",
            &request.corrected_original_representation,
            1,
            1_000_000,
        )?;
        let original = self
            .repository
            .get_claim(&request.claim_id)?
            .ok_or_else(|| AppError::new(ErrorKind::NotFound, "claim was not found"))?;
        let now = self.clock.now()?;
        let correlation_id = self.correlation_id(request.correlation_id)?;
        let provenance = ProvenanceRecord {
            id: self.ids.next("provenance")?,
            artifact_version_id: None,
            connector: None,
            endpoint: None,
            external_record_id: None,
            source_field: None,
            page_number: None,
            paragraph_number: None,
            text_span_start: None,
            text_span_end: None,
            table_number: None,
            row_number: None,
            column_number: None,
            bounding_region_json: None,
            extraction_method: "human_correction".to_owned(),
            extractor_name: request.actor.clone(),
            extractor_version: "human".to_owned(),
            model_provider: None,
            model_name: None,
            model_version: None,
            model_configuration_json: None,
            extracted_at: now,
            confidence: None,
            verification_state: VerificationState::Corrected,
            original_representation: Some(request.corrected_original_representation.clone()),
            correlation_id: correlation_id.clone(),
        };
        provenance.validate()?;
        let corrected_claim = Claim {
            id: self.ids.next("claim")?,
            case_id: original.case_id.clone(),
            subject_id: original.subject_id.clone(),
            subject_key: original.subject_key.clone(),
            predicate: original.predicate.clone(),
            original_value: request.corrected_original_representation,
            normalized_value: request.corrected_value.clone(),
            origin: AssertionOrigin::Human,
            initial_state: ClaimState::Verified,
            primary_provenance_id: Some(provenance.id.clone()),
            interpretation_confidence: None,
            temporal: request.corrected_temporal,
            created_at: now,
        };
        corrected_claim.validate()?;
        let review = HumanReview {
            id: self.ids.next("review")?,
            case_id: original.case_id.clone(),
            target_kind: "claim".to_owned(),
            target_id: original.id.clone(),
            decision: ReviewDecision::Corrected,
            actor: request.actor.clone(),
            rationale: request.rationale.clone(),
            reviewed_at: now,
            correlation_id: correlation_id.clone(),
        };
        let correction = Correction {
            id: self.ids.next("correction")?,
            case_id: original.case_id.clone(),
            original_claim_id: original.id,
            corrected_claim_id: corrected_claim.id.clone(),
            review_id: review.id.clone(),
            provenance_id: provenance.id.clone(),
            original_value: original.normalized_value,
            corrected_value: request.corrected_value,
            actor: request.actor.clone(),
            rationale: request.rationale.clone(),
            corrected_at: now,
            affected_derivations: Vec::new(),
        };
        let context = OperationContext {
            audit_id: self.ids.next("audit")?,
            actor: request.actor,
            correlation_id,
            occurred_at: now,
            reason: request.rationale,
        };
        self.repository.correct_claim(&CorrectionBundle {
            provenance,
            corrected_claim,
            review,
            correction,
            original_state_change_id: self.ids.next("claim_state")?,
            corrected_state_change_id: self.ids.next("claim_state")?,
            context,
        })
    }

    /// Retrieve claims without adapter-specific behavior.
    pub fn list_claims(&self, case_id: &RecordId) -> Result<Vec<Claim>, AppError> {
        self.repository.list_claims(case_id)
    }

    /// Retrieve a case through the shared service boundary.
    pub fn get_case(&self, case_id: &RecordId) -> Result<Case, AppError> {
        self.repository
            .get_case(case_id)?
            .ok_or_else(|| AppError::new(ErrorKind::NotFound, "case was not found"))
    }

    /// Retrieve immutable artifact-version metadata through the shared service boundary.
    pub fn get_artifact_version(&self, version_id: &RecordId) -> Result<ArtifactVersion, AppError> {
        self.repository
            .get_artifact_version(version_id)?
            .ok_or_else(|| AppError::new(ErrorKind::NotFound, "artifact version was not found"))
    }

    /// List immutable artifact versions for a case.
    pub fn list_artifact_versions(
        &self,
        case_id: &RecordId,
    ) -> Result<Vec<ArtifactVersion>, AppError> {
        self.repository.list_artifact_versions(case_id)
    }

    /// Recover the exact source bytes referenced by an immutable artifact version.
    pub fn read_artifact_version(&self, version_id: &RecordId) -> Result<Vec<u8>, AppError> {
        let version = self
            .repository
            .get_artifact_version(version_id)?
            .ok_or_else(|| AppError::new(ErrorKind::NotFound, "artifact version was not found"))?;
        self.artifact_store.read(&version.storage_key)
    }

    /// Recover precise provenance for a material assertion.
    pub fn get_provenance(&self, provenance_id: &RecordId) -> Result<ProvenanceRecord, AppError> {
        self.repository
            .get_provenance(provenance_id)?
            .ok_or_else(|| AppError::new(ErrorKind::NotFound, "provenance record was not found"))
    }

    /// Retrieve contradictions without resolving or hiding them.
    pub fn list_contradictions(
        &self,
        case_id: &RecordId,
    ) -> Result<Vec<casegraph_domain::Contradiction>, AppError> {
        self.repository.list_contradictions(case_id)
    }

    /// Retrieve operational audit history separately from evidentiary provenance.
    pub fn list_audit_events(&self, case_id: &RecordId) -> Result<Vec<AuditEvent>, AppError> {
        self.repository.list_audit_events(case_id)
    }

    fn correlation_id(&self, provided: Option<RecordId>) -> Result<RecordId, AppError> {
        provided.map_or_else(|| self.ids.next("correlation"), Ok)
    }

    fn operation_context(
        &self,
        actor: String,
        correlation: Option<RecordId>,
        reason: Option<String>,
        occurred_at: casegraph_domain::TimestampMs,
    ) -> Result<OperationContext, AppError> {
        Ok(OperationContext {
            audit_id: self.ids.next("audit")?,
            actor,
            correlation_id: self.correlation_id(correlation)?,
            occurred_at,
            reason,
        })
    }
}

/// Case creation input shared by CLI and API adapters.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateCaseRequest {
    pub title: String,
    pub actor: String,
    pub correlation_id: Option<RecordId>,
}

/// Exact-byte ingestion input. Bytes are never included in routine errors or audit events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IngestBytesRequest {
    pub case_id: RecordId,
    pub source_key: String,
    pub connector: String,
    pub locator: String,
    pub external_record_id: Option<String>,
    pub endpoint: Option<String>,
    pub source_revision: Option<String>,
    pub media_type: String,
    pub original_filename: Option<String>,
    pub received_at: Option<casegraph_domain::TimestampMs>,
    pub bytes: Vec<u8>,
    pub actor: String,
    pub correlation_id: Option<RecordId>,
}

/// External assertion plus exact source location and extraction provenance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecordExternalClaimRequest {
    pub case_id: RecordId,
    pub artifact_version_id: RecordId,
    pub subject_id: Option<RecordId>,
    pub subject_key: String,
    pub predicate: String,
    pub original_value: String,
    pub normalized_value: KnowledgeValue,
    pub temporal: Option<TemporalValue>,
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
    pub extraction_confidence: Option<casegraph_domain::Confidence>,
    pub interpretation_confidence: Option<casegraph_domain::Confidence>,
    pub actor: String,
    pub correlation_id: Option<RecordId>,
}

/// Append-only human review input.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReviewClaimRequest {
    pub claim_id: RecordId,
    pub decision: ReviewDecision,
    pub actor: String,
    pub rationale: Option<String>,
    pub correlation_id: Option<RecordId>,
}

/// History-preserving correction input.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CorrectClaimRequest {
    pub claim_id: RecordId,
    pub corrected_original_representation: String,
    pub corrected_value: KnowledgeValue,
    pub corrected_temporal: Option<TemporalValue>,
    pub actor: String,
    pub rationale: Option<String>,
    pub correlation_id: Option<RecordId>,
}

fn validate_text(field: &'static str, value: &str, min: usize, max: usize) -> Result<(), AppError> {
    let length = value.trim().chars().count();
    if !(min..=max).contains(&length) || value.chars().any(char::is_control) {
        return Err(AppError::new(
            ErrorKind::InvalidInput,
            format!("{field} is empty, too long, or contains control characters"),
        ));
    }
    Ok(())
}

fn validate_filename(filename: Option<&str>) -> Result<(), AppError> {
    let Some(filename) = filename else {
        return Ok(());
    };
    validate_text("original_filename", filename, 1, 255)?;
    let path = Path::new(filename);
    let mut components = path.components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(AppError::new(
            ErrorKind::InvalidInput,
            "original_filename must not contain a path",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        CasegraphService, CorrectClaimRequest, CreateCaseRequest, IngestBytesRequest,
        ReviewClaimRequest,
    };
    use crate::{
        AppError, ArtifactStore, Clock, ErrorKind, EvidenceRepository, IdGenerator, StoredArtifact,
    };
    use casegraph_domain::{
        ArtifactVersion, AuditEvent, Case, Claim, Contradiction, Correction, HumanReview,
        KnowledgeValue, RecordId, ReviewDecision, TimestampMs,
    };
    use std::sync::{Arc, Mutex};

    struct FixedClock;

    impl Clock for FixedClock {
        fn now(&self) -> Result<TimestampMs, AppError> {
            Ok(TimestampMs::new(1_000).expect("fixture timestamp"))
        }
    }

    struct SequenceIds(Mutex<u64>);

    impl IdGenerator for SequenceIds {
        fn next(&self, kind: &'static str) -> Result<RecordId, AppError> {
            let mut value = self.0.lock().expect("id lock");
            *value += 1;
            RecordId::parse(format!("{kind}_{value}")).map_err(Into::into)
        }
    }

    #[derive(Default)]
    struct MemoryArtifacts;

    impl ArtifactStore for MemoryArtifacts {
        fn put(&self, bytes: &[u8]) -> Result<StoredArtifact, AppError> {
            Ok(StoredArtifact {
                content_sha256: "a".repeat(64),
                content_length: u64::try_from(bytes.len()).expect("fixture size"),
                storage_key: format!("aa/{}", "a".repeat(64)),
                already_existed: false,
            })
        }

        fn read(&self, _storage_key: &str) -> Result<Vec<u8>, AppError> {
            Ok(Vec::new())
        }
    }

    #[derive(Default)]
    struct MemoryRepository(Mutex<Vec<Case>>);

    impl EvidenceRepository for MemoryRepository {
        fn create_case(&self, bundle: &crate::CreateCaseBundle) -> Result<Case, AppError> {
            self.0
                .lock()
                .expect("case lock")
                .push(bundle.case_record.clone());
            Ok(bundle.case_record.clone())
        }

        fn get_case(&self, case_id: &RecordId) -> Result<Option<Case>, AppError> {
            Ok(self
                .0
                .lock()
                .expect("case lock")
                .iter()
                .find(|case| &case.id == case_id)
                .cloned())
        }

        fn ingest(
            &self,
            bundle: &crate::IngestionBundle,
        ) -> Result<crate::IngestionResult, AppError> {
            Ok(crate::IngestionResult {
                artifact: bundle.artifact.clone(),
                artifact_version: bundle.artifact_version.clone(),
                disposition: crate::IngestionDisposition::NewArtifact,
            })
        }

        fn get_artifact_version(
            &self,
            _version_id: &RecordId,
        ) -> Result<Option<ArtifactVersion>, AppError> {
            Ok(None)
        }

        fn list_artifact_versions(
            &self,
            _case_id: &RecordId,
        ) -> Result<Vec<ArtifactVersion>, AppError> {
            Ok(Vec::new())
        }

        fn get_provenance(
            &self,
            _provenance_id: &RecordId,
        ) -> Result<Option<casegraph_domain::ProvenanceRecord>, AppError> {
            Ok(None)
        }

        fn record_claim(
            &self,
            _bundle: &crate::ClaimBundle,
        ) -> Result<crate::ClaimResult, AppError> {
            unreachable!("not used by these tests")
        }

        fn get_claim(&self, _claim_id: &RecordId) -> Result<Option<Claim>, AppError> {
            Ok(None)
        }

        fn list_claims(&self, _case_id: &RecordId) -> Result<Vec<Claim>, AppError> {
            Ok(Vec::new())
        }

        fn list_contradictions(&self, _case_id: &RecordId) -> Result<Vec<Contradiction>, AppError> {
            Ok(Vec::new())
        }

        fn review_claim(&self, _bundle: &crate::ReviewBundle) -> Result<HumanReview, AppError> {
            unreachable!("not used by these tests")
        }

        fn correct_claim(&self, _bundle: &crate::CorrectionBundle) -> Result<Correction, AppError> {
            unreachable!("not used by these tests")
        }

        fn list_audit_events(&self, _case_id: &RecordId) -> Result<Vec<AuditEvent>, AppError> {
            Ok(Vec::new())
        }
    }

    fn service() -> CasegraphService {
        CasegraphService::new(
            Arc::new(MemoryRepository::default()),
            Arc::new(MemoryArtifacts),
            Arc::new(FixedClock),
            Arc::new(SequenceIds(Mutex::new(0))),
            8,
        )
        .expect("service")
    }

    #[test]
    fn create_case_uses_shared_service_and_validates_title() {
        let service = service();
        let case = service
            .create_case(CreateCaseRequest {
                title: "Synthetic Case".to_owned(),
                actor: "test-actor".to_owned(),
                correlation_id: None,
            })
            .expect("create case");
        assert_eq!(case.title, "Synthetic Case");
        assert!(
            service
                .create_case(CreateCaseRequest {
                    title: "".to_owned(),
                    actor: "test-actor".to_owned(),
                    correlation_id: None,
                })
                .is_err()
        );
    }

    #[test]
    fn oversized_artifact_is_rejected_before_storage() {
        let service = service();
        let case = service
            .create_case(CreateCaseRequest {
                title: "Synthetic Case".to_owned(),
                actor: "test-actor".to_owned(),
                correlation_id: None,
            })
            .expect("create case");
        let error = service
            .ingest_bytes(IngestBytesRequest {
                case_id: case.id,
                source_key: "fixture.txt".to_owned(),
                connector: "filesystem".to_owned(),
                locator: "fixture.txt".to_owned(),
                external_record_id: None,
                endpoint: None,
                source_revision: None,
                media_type: "text/plain".to_owned(),
                original_filename: Some("fixture.txt".to_owned()),
                received_at: None,
                bytes: vec![0; 9],
                actor: "test-actor".to_owned(),
                correlation_id: None,
            })
            .expect_err("size limit must be enforced");
        assert_eq!(error.kind(), crate::ErrorKind::TooLarge);
    }

    #[test]
    fn path_like_original_filename_is_rejected() {
        let service = service();
        let case = service
            .create_case(CreateCaseRequest {
                title: "Synthetic Case".to_owned(),
                actor: "test-actor".to_owned(),
                correlation_id: None,
            })
            .expect("create case");
        let request = IngestBytesRequest {
            case_id: case.id,
            source_key: "fixture.txt".to_owned(),
            connector: "filesystem".to_owned(),
            locator: "fixture.txt".to_owned(),
            external_record_id: None,
            endpoint: None,
            source_revision: None,
            media_type: "text/plain".to_owned(),
            original_filename: Some("../fixture.txt".to_owned()),
            received_at: None,
            bytes: b"fixture".to_vec(),
            actor: "test-actor".to_owned(),
            correlation_id: None,
        };
        assert!(service.ingest_bytes(request).is_err());
    }

    #[test]
    fn constructor_and_missing_resource_boundaries_fail_closed() {
        let repository = Arc::new(MemoryRepository::default());
        let artifacts = Arc::new(MemoryArtifacts);
        let clock = Arc::new(FixedClock);
        let ids = Arc::new(SequenceIds(Mutex::new(0)));
        let zero_limit = CasegraphService::new(
            repository.clone(),
            artifacts.clone(),
            clock.clone(),
            ids.clone(),
            0,
        );
        assert!(matches!(
            zero_limit,
            Err(ref error) if error.kind() == ErrorKind::InvalidInput
        ));
        let service =
            CasegraphService::new(repository, artifacts, clock, ids, 8).expect("service fixture");
        let case_id = RecordId::parse("case_missing").expect("fixture id");
        let version_id = RecordId::parse("artifact_version_missing").expect("fixture id");
        let claim_id = RecordId::parse("claim_missing").expect("fixture id");
        let provenance_id = RecordId::parse("provenance_missing").expect("fixture id");

        assert_eq!(
            service.get_case(&case_id).unwrap_err().kind(),
            ErrorKind::NotFound
        );
        assert_eq!(
            service
                .get_artifact_version(&version_id)
                .unwrap_err()
                .kind(),
            ErrorKind::NotFound
        );
        assert_eq!(
            service
                .read_artifact_version(&version_id)
                .unwrap_err()
                .kind(),
            ErrorKind::NotFound
        );
        assert_eq!(
            service.get_provenance(&provenance_id).unwrap_err().kind(),
            ErrorKind::NotFound
        );
        assert!(service.list_artifact_versions(&case_id).unwrap().is_empty());
        assert!(service.list_claims(&case_id).unwrap().is_empty());
        assert!(service.list_contradictions(&case_id).unwrap().is_empty());
        assert!(service.list_audit_events(&case_id).unwrap().is_empty());

        let ingest_error = service
            .ingest_bytes(IngestBytesRequest {
                case_id,
                source_key: "fixture.txt".to_owned(),
                connector: "filesystem".to_owned(),
                locator: "fixture.txt".to_owned(),
                external_record_id: None,
                endpoint: None,
                source_revision: None,
                media_type: "text/plain".to_owned(),
                original_filename: None,
                received_at: None,
                bytes: b"fixture".to_vec(),
                actor: "test".to_owned(),
                correlation_id: None,
            })
            .expect_err("missing case must fail before storage");
        assert_eq!(ingest_error.kind(), ErrorKind::NotFound);

        let review_error = service
            .review_claim(ReviewClaimRequest {
                claim_id: claim_id.clone(),
                decision: ReviewDecision::Verified,
                actor: "test".to_owned(),
                rationale: None,
                correlation_id: None,
            })
            .expect_err("missing claim cannot be reviewed");
        assert_eq!(review_error.kind(), ErrorKind::NotFound);

        let correction_error = service
            .correct_claim(CorrectClaimRequest {
                claim_id,
                corrected_original_representation: "fixture".to_owned(),
                corrected_value: KnowledgeValue::Unknown,
                corrected_temporal: None,
                actor: "test".to_owned(),
                rationale: None,
                correlation_id: None,
            })
            .expect_err("missing claim cannot be corrected");
        assert_eq!(correction_error.kind(), ErrorKind::NotFound);
    }
}
