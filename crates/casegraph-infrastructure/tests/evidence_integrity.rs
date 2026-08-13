use casegraph_application::{
    CasegraphService, CorrectClaimRequest, CreateCaseRequest, DomainPackage, EvaluateRuleRequest,
    ExtractArtifactRequest, ExtractionPipeline, IdGenerator, IngestBytesRequest, PipelineStage,
    RecordExternalClaimRequest, RegisterRuleRequest, ReviewClaimRequest, RuleWorkflowService,
};
use casegraph_domain::{
    Confidence, Decimal, KnowledgeValue, MaterialValue, Money, RecordId, ReviewDecision,
    TimestampMs,
};
use casegraph_infrastructure::{
    CoreDeterministicExtractor, FilesystemArtifactStore, SqliteEvidenceRepository, SystemClock,
};
use casegraph_sample_domain::SampleAdministrativeCase;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
struct SequenceIds(Mutex<u64>);

impl IdGenerator for SequenceIds {
    fn next(&self, kind: &'static str) -> Result<RecordId, casegraph_application::AppError> {
        let mut counter = self.0.lock().expect("test identifier lock");
        *counter += 1;
        RecordId::parse(format!("{kind}_{counter:06}")).map_err(Into::into)
    }
}

struct Fixture {
    root: PathBuf,
    service: CasegraphService,
    rules: RuleWorkflowService,
}

impl Fixture {
    fn new() -> Self {
        let sequence = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "casegraph-evidence-integration-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create isolated test root");
        let repository = Arc::new(
            SqliteEvidenceRepository::open(root.join("casegraph.db")).expect("open repository"),
        );
        let artifact_store = Arc::new(
            FilesystemArtifactStore::new(root.join("artifacts")).expect("open artifact store"),
        );
        let ids = Arc::new(SequenceIds::default());
        let clock = Arc::new(SystemClock);
        let service = CasegraphService::new(
            repository.clone(),
            artifact_store,
            clock.clone(),
            ids.clone(),
            1024 * 1024,
        )
        .expect("compose service");
        let rules = RuleWorkflowService::new(repository, clock, ids);
        Self {
            root,
            service,
            rules,
        }
    }

    fn create_case(&self) -> casegraph_domain::Case {
        self.service
            .create_case(CreateCaseRequest {
                title: "Synthetic Administrative Case".to_owned(),
                actor: "integration-test".to_owned(),
                correlation_id: None,
            })
            .expect("create case")
    }

    fn ingest(&self, case_id: &RecordId, bytes: &[u8]) -> casegraph_application::IngestionResult {
        self.service
            .ingest_bytes(IngestBytesRequest {
                case_id: case_id.clone(),
                source_key: "records/notice.txt".to_owned(),
                connector: "filesystem".to_owned(),
                locator: "synthetic/notice.txt".to_owned(),
                external_record_id: None,
                endpoint: None,
                source_revision: None,
                media_type: "text/plain".to_owned(),
                original_filename: Some("notice.txt".to_owned()),
                received_at: Some(TimestampMs::new(1_786_579_200_000).expect("timestamp")),
                bytes: bytes.to_vec(),
                actor: "integration-test".to_owned(),
                correlation_id: None,
            })
            .expect("ingest bytes")
    }

    fn claim(
        &self,
        case_id: &RecordId,
        version_id: &RecordId,
        original: &str,
        cents: i64,
    ) -> casegraph_application::ClaimResult {
        let money = Money::new(Decimal::new(cents, 2).expect("decimal"), "USD").expect("money");
        self.service
            .record_external_claim(RecordExternalClaimRequest {
                case_id: case_id.clone(),
                artifact_version_id: version_id.clone(),
                subject_id: None,
                subject_key: "party:synthetic-alex".to_owned(),
                predicate: "monthly_amount".to_owned(),
                original_value: original.to_owned(),
                normalized_value: KnowledgeValue::Known(MaterialValue::Money(money)),
                temporal: None,
                connector: Some("filesystem".to_owned()),
                endpoint: None,
                external_record_id: None,
                source_field: None,
                page_number: Some(1),
                paragraph_number: Some(2),
                text_span_start: Some(17),
                text_span_end: Some(26),
                table_number: None,
                row_number: None,
                column_number: None,
                bounding_region_json: None,
                extraction_method: "deterministic_text".to_owned(),
                extractor_name: "casegraph.synthetic-kv".to_owned(),
                extractor_version: "1".to_owned(),
                model_provider: None,
                model_name: None,
                model_version: None,
                model_configuration_json: None,
                extraction_confidence: Some(Confidence::new(1.0).expect("confidence")),
                interpretation_confidence: None,
                actor: "system:deterministic-extractor".to_owned(),
                correlation_id: None,
            })
            .expect("record claim")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).ok();
    }
}

fn extract_and_verify(fixture: &Fixture, case_id: &RecordId, bytes: &[u8]) {
    let ingestion = fixture.ingest(case_id, bytes);
    let pipeline = ExtractionPipeline::new(
        fixture.service.clone(),
        vec![Arc::new(CoreDeterministicExtractor)],
    );
    let extraction = pipeline
        .extract(ExtractArtifactRequest {
            case_id: case_id.clone(),
            artifact_version_id: ingestion.artifact_version.id,
            media_type: "text/plain".to_owned(),
            connector: Some("filesystem".to_owned()),
            actor: "system:deterministic-pipeline".to_owned(),
            correlation_id: None,
        })
        .expect("extract rule fixture");
    for claim in extraction.claims {
        fixture
            .service
            .review_claim(ReviewClaimRequest {
                claim_id: claim.id,
                decision: ReviewDecision::Verified,
                actor: "human:synthetic-reviewer".to_owned(),
                rationale: Some("Verified invented rule fixture".to_owned()),
                correlation_id: None,
            })
            .expect("verify rule fixture claim");
    }
}

fn register_sample_rule(fixture: &Fixture) -> casegraph_domain::RuleVersion {
    let package = SampleAdministrativeCase;
    let contribution = package.rules().remove(0);
    fixture
        .rules
        .register_rule(RegisterRuleRequest {
            package_id: package.package_id().to_owned(),
            stable_key: contribution.stable_key.to_owned(),
            title: contribution.title.to_owned(),
            version: contribution.version,
            definition: contribution.definition,
            effective_from: None,
            effective_until: None,
            actor: "system:sample-package".to_owned(),
            correlation_id: None,
        })
        .expect("register sample rule")
}

#[test]
fn immutable_ingestion_distinguishes_duplicate_from_new_version() {
    let fixture = Fixture::new();
    let case_record = fixture.create_case();
    let original_bytes = b"Synthetic record\nAmount: $1,427.00\n";
    let first = fixture.ingest(&case_record.id, original_bytes);
    assert_eq!(
        first.disposition,
        casegraph_application::IngestionDisposition::NewArtifact
    );
    assert_eq!(first.artifact_version.version_number, 1);

    let duplicate = fixture.ingest(&case_record.id, original_bytes);
    assert_eq!(
        duplicate.disposition,
        casegraph_application::IngestionDisposition::ExactDuplicate
    );
    assert_eq!(duplicate.artifact.id, first.artifact.id);
    assert_eq!(duplicate.artifact_version.id, first.artifact_version.id);

    let revised_bytes = b"Synthetic record\nAmount: $1,511.00\n";
    let revised = fixture.ingest(&case_record.id, revised_bytes);
    assert_eq!(
        revised.disposition,
        casegraph_application::IngestionDisposition::NewVersion
    );
    assert_eq!(revised.artifact.id, first.artifact.id);
    assert_eq!(revised.artifact_version.version_number, 2);
    assert_ne!(
        revised.artifact_version.content_sha256,
        first.artifact_version.content_sha256
    );
    assert_eq!(
        fixture
            .service
            .read_artifact_version(&first.artifact_version.id)
            .expect("recover original bytes"),
        original_bytes
    );
    assert_eq!(
        fixture
            .service
            .read_artifact_version(&revised.artifact_version.id)
            .expect("recover revised bytes"),
        revised_bytes
    );
}

#[test]
fn conflicting_claims_keep_provenance_and_correction_history() {
    let fixture = Fixture::new();
    let case_record = fixture.create_case();
    let first_version = fixture.ingest(&case_record.id, b"Synthetic record\nAmount: $1,427.00\n");
    let second_version = fixture.ingest(&case_record.id, b"Synthetic record\nAmount: $1,511.00\n");

    let first_claim = fixture.claim(
        &case_record.id,
        &first_version.artifact_version.id,
        "$1,427.00",
        142_700,
    );
    assert!(first_claim.contradictions.is_empty());
    let first_provenance = fixture
        .service
        .get_provenance(
            first_claim
                .claim
                .primary_provenance_id
                .as_ref()
                .expect("external claim provenance"),
        )
        .expect("recover provenance");
    assert_eq!(
        first_provenance.artifact_version_id,
        Some(first_version.artifact_version.id.clone())
    );
    assert_eq!(
        first_provenance.original_representation.as_deref(),
        Some("$1,427.00")
    );
    assert_eq!(first_provenance.page_number, Some(1));
    assert_eq!(first_provenance.text_span_start, Some(17));

    let conflicting_claim = fixture.claim(
        &case_record.id,
        &second_version.artifact_version.id,
        "$1,511.00",
        151_100,
    );
    assert_eq!(conflicting_claim.contradictions.len(), 1);
    let stored_claims = fixture
        .service
        .list_claims(&case_record.id)
        .expect("list claims");
    assert_eq!(stored_claims.len(), 2, "neither conflicting claim is lost");
    assert_eq!(
        fixture
            .service
            .list_contradictions(&case_record.id)
            .expect("list contradictions")
            .len(),
        1
    );
    let conflict_answer = fixture
        .rules
        .query(&case_record.id, "What is the monthly_amount?")
        .expect("grounded conflict answer");
    assert_eq!(
        conflict_answer.mode,
        casegraph_domain::AnswerMode::Conflicting
    );
    assert_eq!(conflict_answer.claim_ids.len(), 2);
    assert_eq!(conflict_answer.provenance_ids.len(), 2);
    assert_eq!(conflict_answer.evidence_ids.len(), 2);

    let review = fixture
        .service
        .review_claim(ReviewClaimRequest {
            claim_id: conflicting_claim.claim.id.clone(),
            decision: ReviewDecision::Verified,
            actor: "human:synthetic-reviewer".to_owned(),
            rationale: Some("Matched the revised source version".to_owned()),
            correlation_id: None,
        })
        .expect("verify claim");
    assert_eq!(review.decision, ReviewDecision::Verified);

    let corrected_money =
        Money::new(Decimal::new(142_750, 2).expect("decimal"), "USD").expect("money");
    let correction = fixture
        .service
        .correct_claim(CorrectClaimRequest {
            claim_id: first_claim.claim.id.clone(),
            corrected_original_representation: "$1,427.50".to_owned(),
            corrected_value: KnowledgeValue::Known(MaterialValue::Money(corrected_money)),
            corrected_temporal: None,
            actor: "human:synthetic-reviewer".to_owned(),
            rationale: Some("Corrected a deterministic transcription error".to_owned()),
            correlation_id: None,
        })
        .expect("correct claim");
    assert_eq!(correction.original_claim_id, first_claim.claim.id);
    assert_ne!(correction.original_claim_id, correction.corrected_claim_id);

    let claims_after_correction = fixture
        .service
        .list_claims(&case_record.id)
        .expect("list claims");
    assert_eq!(
        claims_after_correction.len(),
        3,
        "original claims and appended correction all remain inspectable"
    );
    let audits = fixture
        .service
        .list_audit_events(&case_record.id)
        .expect("list audits");
    assert!(
        audits
            .iter()
            .any(|event| event.operation == "claim.correct")
    );
    assert!(audits.iter().all(|event| {
        !event.resulting_state_json.contains("Synthetic record")
            && !event.resulting_state_json.contains("source bytes")
    }));
}

#[test]
fn equal_known_claims_corroborate_instead_of_contradicting() {
    let fixture = Fixture::new();
    let case_record = fixture.create_case();
    let version = fixture.ingest(&case_record.id, b"Synthetic record\nAmount: $1,427.00\n");
    let first = fixture.claim(
        &case_record.id,
        &version.artifact_version.id,
        "$1,427.00",
        142_700,
    );
    let second = fixture.claim(
        &case_record.id,
        &version.artifact_version.id,
        "$1,427.00",
        142_700,
    );
    assert!(second.contradictions.is_empty());
    assert_eq!(second.corroborates, vec![first.claim.id]);
}

#[test]
fn deterministic_pipeline_creates_provenance_without_any_model_provider() {
    let fixture = Fixture::new();
    let case_record = fixture.create_case();
    let ingestion = fixture.ingest(
        &case_record.id,
        b"received_date: 2026-08-12\namount: $1,427.00\nactive: false\n",
    );
    let pipeline = ExtractionPipeline::new(
        fixture.service.clone(),
        vec![Arc::new(CoreDeterministicExtractor)],
    );
    let result = pipeline
        .extract(ExtractArtifactRequest {
            case_id: case_record.id.clone(),
            artifact_version_id: ingestion.artifact_version.id.clone(),
            media_type: "text/plain".to_owned(),
            connector: Some("filesystem".to_owned()),
            actor: "system:deterministic-pipeline".to_owned(),
            correlation_id: None,
        })
        .expect("deterministic extraction");
    assert_eq!(result.claims.len(), 3);
    assert_eq!(result.stages.first(), Some(&PipelineStage::Classification));
    assert_eq!(result.stages.last(), Some(&PipelineStage::EvidenceCreation));
    for claim in result.claims {
        let provenance = fixture
            .service
            .get_provenance(
                claim
                    .primary_provenance_id
                    .as_ref()
                    .expect("external claim has provenance"),
            )
            .expect("recover provenance");
        assert_eq!(
            provenance.artifact_version_id,
            Some(ingestion.artifact_version.id.clone())
        );
        assert_eq!(provenance.model_provider, None);
        assert_eq!(provenance.extraction_method, "deterministic");
    }
}

#[test]
fn versioned_rule_materializes_explainable_workflow_and_is_reproducible() {
    let fixture = Fixture::new();
    let case_record = fixture.create_case();
    let ingestion = fixture.ingest(
        &case_record.id,
        b"received_date: 2026-08-12\nresponse_required: true\n",
    );
    let pipeline = ExtractionPipeline::new(
        fixture.service.clone(),
        vec![Arc::new(CoreDeterministicExtractor)],
    );
    let extraction = pipeline
        .extract(ExtractArtifactRequest {
            case_id: case_record.id.clone(),
            artifact_version_id: ingestion.artifact_version.id,
            media_type: "text/plain".to_owned(),
            connector: Some("filesystem".to_owned()),
            actor: "system:deterministic-pipeline".to_owned(),
            correlation_id: None,
        })
        .expect("extract fixture");
    for claim in &extraction.claims {
        fixture
            .service
            .review_claim(ReviewClaimRequest {
                claim_id: claim.id.clone(),
                decision: ReviewDecision::Verified,
                actor: "human:synthetic-reviewer".to_owned(),
                rationale: Some("Verified against invented fixture".to_owned()),
                correlation_id: None,
            })
            .expect("verify fixture claim");
    }

    let package = SampleAdministrativeCase;
    let contribution = package.rules().remove(0);
    let rule_version = fixture
        .rules
        .register_rule(RegisterRuleRequest {
            package_id: package.package_id().to_owned(),
            stable_key: contribution.stable_key.to_owned(),
            title: contribution.title.to_owned(),
            version: contribution.version,
            definition: contribution.definition,
            effective_from: None,
            effective_until: None,
            actor: "system:sample-package".to_owned(),
            correlation_id: None,
        })
        .expect("register sample rule");
    let request = EvaluateRuleRequest {
        case_id: case_record.id.clone(),
        rule_version_id: rule_version.id,
        actor: "system:deterministic-rules".to_owned(),
        correlation_id: None,
    };
    let first = fixture
        .rules
        .evaluate(request.clone())
        .expect("evaluate rule");
    let second = fixture
        .rules
        .evaluate(request)
        .expect("idempotent reevaluation");
    assert_eq!(
        first.evaluation.result,
        casegraph_domain::RuleResult::Satisfied
    );
    assert_eq!(first.evaluation.id, second.evaluation.id);
    assert_eq!(
        first.evaluation.inputs_sha256,
        second.evaluation.inputs_sha256
    );
    assert_eq!(
        first
            .deadline
            .as_ref()
            .and_then(|item| item.due_latest)
            .map(|date| date.to_iso())
            .as_deref(),
        Some("2026-08-22")
    );
    assert_eq!(
        first.task.as_ref().map(|task| task.title.as_str()),
        Some("Prepare synthetic response")
    );

    let answer = fixture
        .rules
        .query(&case_record.id, "What deadlines and what must happen next?")
        .expect("grounded workflow answer");
    assert_eq!(answer.mode, casegraph_domain::AnswerMode::Established);
    assert_eq!(
        answer.rule_evaluation_ids,
        vec![first.evaluation.id.clone()]
    );
    assert!(!answer.evidence_ids.is_empty());

    let response_claim = extraction
        .claims
        .iter()
        .find(|claim| claim.predicate == "response_required")
        .expect("response claim");
    let correction = fixture
        .service
        .correct_claim(CorrectClaimRequest {
            claim_id: response_claim.id.clone(),
            corrected_original_representation: "false".to_owned(),
            corrected_value: KnowledgeValue::Known(MaterialValue::Boolean(false)),
            corrected_temporal: None,
            actor: "human:synthetic-reviewer".to_owned(),
            rationale: Some("Invented post-evaluation correction".to_owned()),
            correlation_id: None,
        })
        .expect("correction identifies derived state");
    assert_eq!(
        correction.affected_derivations,
        vec![first.evaluation.id.clone()],
        "dependent rule evaluation is explicitly identified for reevaluation"
    );

    let unknown = fixture
        .rules
        .query(&case_record.id, "What is the invented_color?")
        .expect("grounded unknown answer");
    assert_eq!(unknown.mode, casegraph_domain::AnswerMode::Unknown);
    assert!(unknown.claim_ids.is_empty());

    let empty_case = fixture
        .service
        .create_case(CreateCaseRequest {
            title: "Synthetic Empty Case".to_owned(),
            actor: "integration-test".to_owned(),
            correlation_id: None,
        })
        .expect("create empty case");
    let indeterminate = fixture
        .rules
        .evaluate(EvaluateRuleRequest {
            case_id: empty_case.id,
            rule_version_id: first.evaluation.rule_version_id,
            actor: "system:deterministic-rules".to_owned(),
            correlation_id: None,
        })
        .expect("missing inputs produce recorded indeterminate evaluation");
    assert_eq!(
        indeterminate.evaluation.result,
        casegraph_domain::RuleResult::Indeterminate
    );
    assert!(indeterminate.task.is_none());
}

#[test]
fn rule_evaluation_fails_closed_for_false_missing_and_superseding_evidence() {
    let fixture = Fixture::new();
    let version = register_sample_rule(&fixture);

    let false_case = fixture.create_case();
    extract_and_verify(
        &fixture,
        &false_case.id,
        b"received_date: 2026-08-12\nresponse_required: false\n",
    );
    let not_satisfied = fixture
        .rules
        .evaluate(EvaluateRuleRequest {
            case_id: false_case.id.clone(),
            rule_version_id: version.id.clone(),
            actor: "system:deterministic-rules".to_owned(),
            correlation_id: None,
        })
        .expect("false condition produces a recorded evaluation");
    assert_eq!(
        not_satisfied.evaluation.result,
        casegraph_domain::RuleResult::NotSatisfied
    );
    assert!(not_satisfied.task.is_none());
    assert!(
        not_satisfied
            .evaluation
            .explanation
            .contains("response_required")
    );

    let missing_anchor_case = fixture.create_case();
    extract_and_verify(
        &fixture,
        &missing_anchor_case.id,
        b"response_required: true\n",
    );
    let missing_anchor = fixture
        .rules
        .evaluate(EvaluateRuleRequest {
            case_id: missing_anchor_case.id,
            rule_version_id: version.id.clone(),
            actor: "system:deterministic-rules".to_owned(),
            correlation_id: None,
        })
        .expect("missing anchor produces a recorded evaluation");
    assert_eq!(
        missing_anchor.evaluation.result,
        casegraph_domain::RuleResult::Indeterminate
    );
    assert!(
        missing_anchor
            .evaluation
            .explanation
            .contains("deadline anchor")
    );

    let disagreeing_condition_case = fixture.create_case();
    extract_and_verify(
        &fixture,
        &disagreeing_condition_case.id,
        b"received_date: 2026-08-12\nresponse_required: true\n",
    );
    extract_and_verify(
        &fixture,
        &disagreeing_condition_case.id,
        b"received_date: 2026-08-12\nresponse_required: false\n",
    );
    let disagreement = fixture
        .rules
        .evaluate(EvaluateRuleRequest {
            case_id: disagreeing_condition_case.id,
            rule_version_id: version.id.clone(),
            actor: "system:deterministic-rules".to_owned(),
            correlation_id: None,
        })
        .expect("disagreement produces a recorded evaluation");
    assert_eq!(
        disagreement.evaluation.result,
        casegraph_domain::RuleResult::NotSatisfied,
        "the repository exposes only the later reviewed value as currently verified"
    );

    let disagreeing_dates_case = fixture.create_case();
    extract_and_verify(
        &fixture,
        &disagreeing_dates_case.id,
        b"received_date: 2026-08-12\nresponse_required: true\n",
    );
    extract_and_verify(
        &fixture,
        &disagreeing_dates_case.id,
        b"received_date: 2026-08-13\nresponse_required: true\n",
    );
    let date_disagreement = fixture
        .rules
        .evaluate(EvaluateRuleRequest {
            case_id: disagreeing_dates_case.id,
            rule_version_id: version.id.clone(),
            actor: "system:deterministic-rules".to_owned(),
            correlation_id: None,
        })
        .expect("date disagreement produces a recorded evaluation");
    assert_eq!(
        date_disagreement.evaluation.result,
        casegraph_domain::RuleResult::Satisfied,
        "the later reviewed date is authoritative while the contradiction remains recorded"
    );
    assert_eq!(
        date_disagreement
            .deadline
            .and_then(|deadline| deadline.due_latest)
            .map(|date| date.to_iso())
            .as_deref(),
        Some("2026-08-23")
    );

    let absent = fixture.rules.evaluate(EvaluateRuleRequest {
        case_id: false_case.id,
        rule_version_id: RecordId::parse("rule_version_absent").expect("fixture id"),
        actor: "system:deterministic-rules".to_owned(),
        correlation_id: None,
    });
    assert_eq!(
        absent.expect_err("unknown version must fail").kind(),
        casegraph_application::ErrorKind::NotFound
    );
}
