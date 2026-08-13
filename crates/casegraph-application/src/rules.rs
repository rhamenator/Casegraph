//! Versioned deterministic rules, workflow causality, and grounded querying.

use crate::{
    AppError, Clock, ErrorKind, EvaluationBundle, EvidenceRepository, IdGenerator,
    OperationContext, RegisterRuleBundle,
};
use casegraph_domain::{
    AnswerMode, Date, Deadline, GroundedAnswer, GroundedClaim, KnowledgeValue, MaterialValue,
    Obligation, ObligationStatus, RecordId, Rule, RuleDefinition, RuleEvaluation, RuleInput,
    RuleResult, RuleVersion, TaskStatus, TemporalPrecision, WorkflowMaterialization, WorkflowTask,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::sync::Arc;

/// Shared deterministic rules/workflow/query service.
#[derive(Clone)]
pub struct RuleWorkflowService {
    repository: Arc<dyn EvidenceRepository>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
}

impl RuleWorkflowService {
    pub fn new(
        repository: Arc<dyn EvidenceRepository>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            repository,
            clock,
            ids,
        }
    }

    /// Register one immutable definition version under a stable rule identity.
    pub fn register_rule(&self, request: RegisterRuleRequest) -> Result<RuleVersion, AppError> {
        request.definition.validate()?;
        let now = self.clock.now()?;
        let definition_json = canonical_json(&request.definition)?;
        let rule = Rule {
            id: self.ids.next("rule")?,
            package_id: request.package_id,
            stable_key: request.stable_key,
            title: request.title,
            created_at: now,
        };
        let version = RuleVersion {
            id: self.ids.next("rule_version")?,
            rule_id: rule.id.clone(),
            version: request.version,
            definition: request.definition,
            definition_sha256: sha256(definition_json.as_bytes()),
            effective_from: request.effective_from,
            effective_until: request.effective_until,
            created_at: now,
        };
        let context = OperationContext {
            audit_id: self.ids.next("audit")?,
            actor: request.actor,
            correlation_id: request
                .correlation_id
                .map_or_else(|| self.ids.next("correlation"), Ok)?,
            occurred_at: now,
            reason: None,
        };
        self.repository.register_rule(&RegisterRuleBundle {
            rule,
            version,
            context,
        })
    }

    /// Evaluate verified stored facts against an exact rule version and materialize workflow.
    pub fn evaluate(
        &self,
        request: EvaluateRuleRequest,
    ) -> Result<WorkflowMaterialization, AppError> {
        let version = self
            .repository
            .get_rule_version(&request.rule_version_id)?
            .ok_or_else(|| AppError::new(ErrorKind::NotFound, "rule version was not found"))?;
        let grounded = self.repository.list_grounded_claims(&request.case_id)?;
        let mut inputs = Vec::new();
        let mut missing = Vec::new();
        let mut failed = Vec::new();
        let mut disagreement = Vec::new();

        for condition in &version.definition.all {
            let candidates = grounded
                .iter()
                .filter(|item| {
                    item.current_state == casegraph_domain::ClaimState::Verified
                        && item.claim.subject_key == condition.subject_key
                        && item.claim.predicate == condition.predicate
                })
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                missing.push(condition.predicate.clone());
                continue;
            }
            let matching = candidates
                .iter()
                .filter(|item| item.claim.normalized_value == condition.expected)
                .count();
            if matching > 0 && matching < candidates.len() {
                disagreement.push(condition.predicate.clone());
            } else if matching == 0 {
                failed.push(condition.predicate.clone());
            }
            inputs.extend(candidates.into_iter().map(rule_input));
        }

        let anchor_candidates = grounded
            .iter()
            .filter(|item| {
                item.current_state == casegraph_domain::ClaimState::Verified
                    && item.claim.predicate == version.definition.effect.deadline_anchor_predicate
            })
            .collect::<Vec<_>>();
        let anchor_dates = anchor_candidates
            .iter()
            .filter_map(|item| match &item.claim.normalized_value {
                KnowledgeValue::Known(MaterialValue::Date(date)) => Some(*date),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        inputs.extend(anchor_candidates.into_iter().map(rule_input));
        inputs.sort_by(|left, right| {
            (&left.subject_key, &left.predicate, &left.claim_id).cmp(&(
                &right.subject_key,
                &right.predicate,
                &right.claim_id,
            ))
        });
        inputs.dedup_by(|left, right| left.claim_id == right.claim_id);

        let (result, explanation) = if !disagreement.is_empty() || anchor_dates.len() > 1 {
            (
                RuleResult::Indeterminate,
                "Verified facts disagree; human adjudication is required.".to_owned(),
            )
        } else if !missing.is_empty() {
            (
                RuleResult::Indeterminate,
                format!(
                    "Required verified facts are missing: {}.",
                    missing.join(", ")
                ),
            )
        } else if !failed.is_empty() {
            (
                RuleResult::NotSatisfied,
                format!("Rule conditions were not satisfied: {}.", failed.join(", ")),
            )
        } else if anchor_dates.is_empty() {
            (
                RuleResult::Indeterminate,
                format!(
                    "A verified deadline anchor ({}) is missing.",
                    version.definition.effect.deadline_anchor_predicate
                ),
            )
        } else {
            (
                RuleResult::Satisfied,
                "All conditions and the deadline anchor are established by verified evidence."
                    .to_owned(),
            )
        };

        let now = self.clock.now()?;
        let correlation_id = request
            .correlation_id
            .map_or_else(|| self.ids.next("correlation"), Ok)?;
        let inputs_json = canonical_json(&inputs)?;
        let evaluation = RuleEvaluation {
            id: self.ids.next("rule_evaluation")?,
            case_id: request.case_id.clone(),
            rule_version_id: version.id.clone(),
            inputs,
            inputs_sha256: sha256(inputs_json.as_bytes()),
            result,
            explanation,
            evaluated_at: now,
            evaluator_version: "casegraph-equality-v1".to_owned(),
            correlation_id: correlation_id.clone(),
        };
        let (obligation, deadline, task) = if result == RuleResult::Satisfied {
            let anchor = *anchor_dates.iter().next().ok_or_else(|| {
                AppError::new(ErrorKind::Internal, "satisfied rule has no deadline anchor")
            })?;
            let due = anchor.checked_add_days(
                i32::try_from(version.definition.effect.deadline_days_after).map_err(|_| {
                    AppError::new(ErrorKind::InvalidInput, "deadline interval is too large")
                })?,
            )?;
            let obligation = Obligation {
                id: self.ids.next("obligation")?,
                case_id: request.case_id.clone(),
                created_by_event_id: None,
                created_by_rule_evaluation_id: Some(evaluation.id.clone()),
                kind: version.definition.effect.obligation_kind.clone(),
                description: version.definition.effect.obligation_description.clone(),
                status: ObligationStatus::Open,
                created_at: now,
            };
            let deadline = Deadline {
                id: self.ids.next("deadline")?,
                case_id: request.case_id.clone(),
                obligation_id: obligation.id.clone(),
                due_earliest: Some(due),
                due_latest: Some(due),
                original_expression: format!(
                    "{} days after {}",
                    version.definition.effect.deadline_days_after,
                    version.definition.effect.deadline_anchor_predicate
                ),
                temporal_precision: TemporalPrecision::Day,
                calculation_json: canonical_json(&serde_json::json!({
                    "anchor": anchor.to_iso(),
                    "days_after": version.definition.effect.deadline_days_after,
                    "rule_version_id": version.id,
                }))?,
                created_at: now,
            };
            let task = WorkflowTask {
                id: self.ids.next("task")?,
                case_id: request.case_id.clone(),
                obligation_id: Some(obligation.id.clone()),
                title: version.definition.effect.task_title.clone(),
                status: TaskStatus::Ready,
                created_at: now,
                completed_at: None,
            };
            (Some(obligation), Some(deadline), Some(task))
        } else {
            (None, None, None)
        };
        let materialization = WorkflowMaterialization {
            evaluation,
            obligation,
            deadline,
            task,
        };
        let context = OperationContext {
            audit_id: self.ids.next("audit")?,
            actor: request.actor,
            correlation_id,
            occurred_at: now,
            reason: None,
        };
        self.repository.record_evaluation(&EvaluationBundle {
            materialization,
            context,
        })
    }

    /// Answer a predicate-oriented question from stored claims only.
    pub fn query(&self, case_id: &RecordId, question: &str) -> Result<GroundedAnswer, AppError> {
        let normalized_question = question.to_ascii_lowercase();
        if normalized_question.contains("deadline")
            || normalized_question.contains("what must")
            || normalized_question.contains("need to do")
        {
            let workflows = self.repository.list_workflow(case_id)?;
            if let Some(item) = workflows.into_iter().rev().find(|item| item.task.is_some()) {
                let task = item.task.as_ref().expect("filtered task");
                let deadline = item.deadline.as_ref();
                let due = deadline
                    .and_then(|value| value.due_latest)
                    .map_or_else(|| "an unresolved date".to_owned(), Date::to_iso);
                return Ok(GroundedAnswer {
                    mode: AnswerMode::Established,
                    statement: format!(
                        "{} is required by a satisfied rule evaluation and is due {}.",
                        task.title, due
                    ),
                    claim_ids: item
                        .evaluation
                        .inputs
                        .iter()
                        .map(|input| input.claim_id.clone())
                        .collect(),
                    provenance_ids: Vec::new(),
                    evidence_ids: item
                        .evaluation
                        .inputs
                        .iter()
                        .flat_map(|input| input.evidence_ids.clone())
                        .collect(),
                    rule_evaluation_ids: vec![item.evaluation.id],
                });
            }
            return Ok(unknown_answer());
        }

        let claims = self.repository.list_grounded_claims(case_id)?;
        let mut matched = claims
            .into_iter()
            .filter(|item| normalized_question.contains(&item.claim.predicate.to_ascii_lowercase()))
            .collect::<Vec<_>>();
        if matched.is_empty() {
            return Ok(unknown_answer());
        }
        matched.sort_by(|left, right| left.claim.id.cmp(&right.claim.id));
        let values = matched
            .iter()
            .filter_map(|item| match &item.claim.normalized_value {
                KnowledgeValue::Known(value) => Some(canonical_json(value).ok()),
                _ => None,
            })
            .flatten()
            .collect::<BTreeSet<_>>();
        let verified = matched
            .iter()
            .all(|item| item.current_state == casegraph_domain::ClaimState::Verified);
        let mode = if values.len() > 1 {
            AnswerMode::Conflicting
        } else if verified {
            AnswerMode::Established
        } else if matched.iter().any(|item| {
            matches!(
                item.current_state,
                casegraph_domain::ClaimState::Inferred | casegraph_domain::ClaimState::Corroborated
            )
        }) {
            AnswerMode::Suggested
        } else {
            AnswerMode::Claimed
        };
        let statement = match mode {
            AnswerMode::Established => "The evidence establishes a value for this predicate.",
            AnswerMode::Claimed => "One or more sources claim a value for this predicate.",
            AnswerMode::Suggested => "The available evidence suggests a value for this predicate.",
            AnswerMode::Conflicting => "Sources disagree about this predicate.",
            AnswerMode::Unknown => {
                "The system does not have sufficient evidence to determine this."
            }
        }
        .to_owned();
        Ok(GroundedAnswer {
            mode,
            statement,
            claim_ids: matched.iter().map(|item| item.claim.id.clone()).collect(),
            provenance_ids: matched
                .iter()
                .filter_map(|item| item.provenance_id.clone())
                .collect(),
            evidence_ids: matched
                .iter()
                .flat_map(|item| item.evidence_ids.clone())
                .collect(),
            rule_evaluation_ids: Vec::new(),
        })
    }

    /// Inspect rule evaluations and workflow materializations for a case.
    pub fn list_workflow(
        &self,
        case_id: &RecordId,
    ) -> Result<Vec<WorkflowMaterialization>, AppError> {
        self.repository.list_workflow(case_id)
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RegisterRuleRequest {
    pub package_id: String,
    pub stable_key: String,
    pub title: String,
    pub version: u32,
    pub definition: RuleDefinition,
    pub effective_from: Option<Date>,
    pub effective_until: Option<Date>,
    pub actor: String,
    pub correlation_id: Option<RecordId>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EvaluateRuleRequest {
    pub case_id: RecordId,
    pub rule_version_id: RecordId,
    pub actor: String,
    pub correlation_id: Option<RecordId>,
}

fn rule_input(item: &GroundedClaim) -> RuleInput {
    RuleInput {
        claim_id: item.claim.id.clone(),
        evidence_ids: item.evidence_ids.clone(),
        subject_key: item.claim.subject_key.clone(),
        predicate: item.claim.predicate.clone(),
        value: item.claim.normalized_value.clone(),
    }
}

fn unknown_answer() -> GroundedAnswer {
    GroundedAnswer {
        mode: AnswerMode::Unknown,
        statement: "The system does not have sufficient evidence to determine this.".to_owned(),
        claim_ids: Vec::new(),
        provenance_ids: Vec::new(),
        evidence_ids: Vec::new(),
        rule_evaluation_ids: Vec::new(),
    }
}

fn canonical_json<T: serde::Serialize>(value: &T) -> Result<String, AppError> {
    serde_json::to_string(value).map_err(|_| {
        AppError::new(
            ErrorKind::Internal,
            "could not serialize deterministic rule state",
        )
    })
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ClaimBundle, ClaimResult, CorrectionBundle, CreateCaseBundle, IngestionBundle,
        IngestionResult, ReviewBundle,
    };
    use casegraph_domain::{
        ArtifactVersion, AssertionOrigin, AuditEvent, Case, Claim, ClaimState, Contradiction,
        Correction, HumanReview, RuleCondition, TimestampMs, WorkflowEffect,
    };
    use std::sync::Mutex;

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
    struct RuleRepository {
        version: Mutex<Option<RuleVersion>>,
        claims: Mutex<Vec<GroundedClaim>>,
        workflows: Mutex<Vec<WorkflowMaterialization>>,
    }

    impl EvidenceRepository for RuleRepository {
        fn create_case(&self, _bundle: &CreateCaseBundle) -> Result<Case, AppError> {
            unreachable!("unused repository operation")
        }

        fn get_case(&self, _case_id: &RecordId) -> Result<Option<Case>, AppError> {
            unreachable!("unused repository operation")
        }

        fn ingest(&self, _bundle: &IngestionBundle) -> Result<IngestionResult, AppError> {
            unreachable!("unused repository operation")
        }

        fn get_artifact_version(
            &self,
            _version_id: &RecordId,
        ) -> Result<Option<ArtifactVersion>, AppError> {
            unreachable!("unused repository operation")
        }

        fn list_artifact_versions(
            &self,
            _case_id: &RecordId,
        ) -> Result<Vec<ArtifactVersion>, AppError> {
            unreachable!("unused repository operation")
        }

        fn get_provenance(
            &self,
            _provenance_id: &RecordId,
        ) -> Result<Option<casegraph_domain::ProvenanceRecord>, AppError> {
            unreachable!("unused repository operation")
        }

        fn record_claim(&self, _bundle: &ClaimBundle) -> Result<ClaimResult, AppError> {
            unreachable!("unused repository operation")
        }

        fn get_claim(&self, _claim_id: &RecordId) -> Result<Option<Claim>, AppError> {
            unreachable!("unused repository operation")
        }

        fn list_claims(&self, _case_id: &RecordId) -> Result<Vec<Claim>, AppError> {
            unreachable!("unused repository operation")
        }

        fn list_contradictions(&self, _case_id: &RecordId) -> Result<Vec<Contradiction>, AppError> {
            unreachable!("unused repository operation")
        }

        fn review_claim(&self, _bundle: &ReviewBundle) -> Result<HumanReview, AppError> {
            unreachable!("unused repository operation")
        }

        fn correct_claim(&self, _bundle: &CorrectionBundle) -> Result<Correction, AppError> {
            unreachable!("unused repository operation")
        }

        fn list_audit_events(&self, _case_id: &RecordId) -> Result<Vec<AuditEvent>, AppError> {
            unreachable!("unused repository operation")
        }

        fn register_rule(&self, bundle: &RegisterRuleBundle) -> Result<RuleVersion, AppError> {
            *self.version.lock().expect("version lock") = Some(bundle.version.clone());
            Ok(bundle.version.clone())
        }

        fn get_rule_version(
            &self,
            _rule_version_id: &RecordId,
        ) -> Result<Option<RuleVersion>, AppError> {
            Ok(self.version.lock().expect("version lock").clone())
        }

        fn list_grounded_claims(
            &self,
            _case_id: &RecordId,
        ) -> Result<Vec<GroundedClaim>, AppError> {
            Ok(self.claims.lock().expect("claims lock").clone())
        }

        fn record_evaluation(
            &self,
            bundle: &EvaluationBundle,
        ) -> Result<WorkflowMaterialization, AppError> {
            let value = bundle.materialization.clone();
            self.workflows
                .lock()
                .expect("workflow lock")
                .push(value.clone());
            Ok(value)
        }

        fn list_workflow(
            &self,
            _case_id: &RecordId,
        ) -> Result<Vec<WorkflowMaterialization>, AppError> {
            Ok(self.workflows.lock().expect("workflow lock").clone())
        }
    }

    fn id(value: &str) -> RecordId {
        RecordId::parse(value).expect("fixture id")
    }

    fn definition() -> RuleDefinition {
        RuleDefinition {
            all: vec![RuleCondition {
                subject_key: "subject:one".to_owned(),
                predicate: "eligible".to_owned(),
                expected: KnowledgeValue::Known(MaterialValue::Boolean(true)),
            }],
            effect: WorkflowEffect {
                obligation_kind: "respond".to_owned(),
                obligation_description: "Respond to the synthetic event".to_owned(),
                deadline_anchor_predicate: "received_date".to_owned(),
                deadline_days_after: 10,
                task_title: "Prepare response".to_owned(),
            },
        }
    }

    fn version() -> RuleVersion {
        RuleVersion {
            id: id("rule_version_1"),
            rule_id: id("rule_1"),
            version: 1,
            definition: definition(),
            definition_sha256: "a".repeat(64),
            effective_from: None,
            effective_until: None,
            created_at: TimestampMs::new(1_000).expect("fixture timestamp"),
        }
    }

    fn grounded(
        claim_id: &str,
        predicate: &str,
        value: MaterialValue,
        state: ClaimState,
    ) -> GroundedClaim {
        GroundedClaim {
            claim: Claim {
                id: id(claim_id),
                case_id: id("case_1"),
                subject_id: None,
                subject_key: "subject:one".to_owned(),
                predicate: predicate.to_owned(),
                original_value: "fixture".to_owned(),
                normalized_value: KnowledgeValue::Known(value),
                origin: AssertionOrigin::System,
                initial_state: state,
                primary_provenance_id: None,
                interpretation_confidence: None,
                temporal: None,
                created_at: TimestampMs::new(1_000).expect("fixture timestamp"),
            },
            current_state: state,
            provenance_id: Some(id("provenance_1")),
            evidence_ids: vec![id("evidence_1")],
        }
    }

    fn service(repository: Arc<RuleRepository>) -> RuleWorkflowService {
        RuleWorkflowService::new(
            repository,
            Arc::new(FixedClock),
            Arc::new(SequenceIds(Mutex::new(0))),
        )
    }

    #[test]
    fn defensive_evaluation_refuses_disagreeing_verified_facts() {
        let repository = Arc::new(RuleRepository {
            version: Mutex::new(Some(version())),
            claims: Mutex::new(vec![
                grounded(
                    "claim_true",
                    "eligible",
                    MaterialValue::Boolean(true),
                    ClaimState::Verified,
                ),
                grounded(
                    "claim_false",
                    "eligible",
                    MaterialValue::Boolean(false),
                    ClaimState::Verified,
                ),
                grounded(
                    "claim_date",
                    "received_date",
                    MaterialValue::Date(Date::new(2026, 8, 12).expect("fixture date")),
                    ClaimState::Verified,
                ),
            ]),
            workflows: Mutex::new(Vec::new()),
        });
        let result = service(repository)
            .evaluate(EvaluateRuleRequest {
                case_id: id("case_1"),
                rule_version_id: id("rule_version_1"),
                actor: "test".to_owned(),
                correlation_id: Some(id("correlation_1")),
            })
            .expect("record defensive evaluation");
        assert_eq!(result.evaluation.result, RuleResult::Indeterminate);
        assert!(result.evaluation.explanation.contains("disagree"));
        assert!(result.task.is_none());
    }

    #[test]
    fn satisfied_evaluation_materializes_work_and_grounded_deadline_answer() {
        let repository = Arc::new(RuleRepository {
            version: Mutex::new(Some(version())),
            claims: Mutex::new(vec![
                grounded(
                    "claim_true",
                    "eligible",
                    MaterialValue::Boolean(true),
                    ClaimState::Verified,
                ),
                grounded(
                    "claim_date",
                    "received_date",
                    MaterialValue::Date(Date::new(2026, 8, 12).expect("fixture date")),
                    ClaimState::Verified,
                ),
            ]),
            workflows: Mutex::new(Vec::new()),
        });
        let rules = service(repository.clone());
        let result = rules
            .evaluate(EvaluateRuleRequest {
                case_id: id("case_1"),
                rule_version_id: id("rule_version_1"),
                actor: "test".to_owned(),
                correlation_id: None,
            })
            .expect("satisfied evaluation");
        assert_eq!(result.evaluation.result, RuleResult::Satisfied);
        assert_eq!(
            result
                .deadline
                .as_ref()
                .and_then(|deadline| deadline.due_latest)
                .map(Date::to_iso)
                .as_deref(),
            Some("2026-08-22")
        );
        assert!(result.obligation.is_some());
        assert!(result.task.is_some());

        let answer = rules
            .query(&id("case_1"), "What must I do by the deadline?")
            .unwrap();
        assert_eq!(answer.mode, AnswerMode::Established);
        assert!(answer.statement.contains("2026-08-22"));
        assert_eq!(
            answer.rule_evaluation_ids,
            vec![result.evaluation.id.clone()]
        );
        assert_eq!(
            rules.list_workflow(&id("case_1")).unwrap(),
            vec![result.clone()]
        );

        let mut unresolved = result;
        unresolved.deadline = None;
        *repository.workflows.lock().expect("workflow lock") = vec![unresolved];
        assert!(
            rules
                .query(&id("case_1"), "What is the deadline?")
                .unwrap()
                .statement
                .contains("unresolved date")
        );
    }

    #[test]
    fn disagreeing_deadline_anchors_and_oversized_intervals_fail_closed() {
        let repository = Arc::new(RuleRepository {
            version: Mutex::new(Some(version())),
            claims: Mutex::new(vec![
                grounded(
                    "claim_true",
                    "eligible",
                    MaterialValue::Boolean(true),
                    ClaimState::Verified,
                ),
                grounded(
                    "claim_date_1",
                    "received_date",
                    MaterialValue::Date(Date::new(2026, 8, 12).expect("fixture date")),
                    ClaimState::Verified,
                ),
                grounded(
                    "claim_date_2",
                    "received_date",
                    MaterialValue::Date(Date::new(2026, 8, 13).expect("fixture date")),
                    ClaimState::Verified,
                ),
            ]),
            workflows: Mutex::new(Vec::new()),
        });
        let rules = service(repository.clone());
        let request = EvaluateRuleRequest {
            case_id: id("case_1"),
            rule_version_id: id("rule_version_1"),
            actor: "test".to_owned(),
            correlation_id: None,
        };
        let disagreement = rules.evaluate(request.clone()).unwrap();
        assert_eq!(disagreement.evaluation.result, RuleResult::Indeterminate);
        assert!(disagreement.evaluation.explanation.contains("disagree"));

        let mut oversized = version();
        oversized.definition.effect.deadline_days_after = u32::MAX;
        *repository.version.lock().expect("version lock") = Some(oversized);
        repository
            .claims
            .lock()
            .expect("claims lock")
            .retain(|claim| claim.claim.id.as_str() != "claim_date_2");
        assert_eq!(
            rules.evaluate(request).unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
    }

    #[test]
    fn grounded_query_modes_are_derived_only_from_stored_claims() {
        let repository = Arc::new(RuleRepository::default());
        let rules = service(repository.clone());
        let case_id = id("case_1");

        let cases = [
            (ClaimState::Extracted, AnswerMode::Claimed),
            (ClaimState::Inferred, AnswerMode::Suggested),
            (ClaimState::Corroborated, AnswerMode::Suggested),
            (ClaimState::Verified, AnswerMode::Established),
        ];
        for (state, expected) in cases {
            *repository.claims.lock().expect("claims lock") = vec![grounded(
                "claim_1",
                "status",
                MaterialValue::Text("ready".to_owned()),
                state,
            )];
            assert_eq!(
                rules.query(&case_id, "What is the status?").unwrap().mode,
                expected
            );
        }

        *repository.claims.lock().expect("claims lock") = vec![
            grounded(
                "claim_1",
                "status",
                MaterialValue::Text("ready".to_owned()),
                ClaimState::Verified,
            ),
            grounded(
                "claim_2",
                "status",
                MaterialValue::Text("blocked".to_owned()),
                ClaimState::Verified,
            ),
        ];
        let conflicting = rules.query(&case_id, "What is the status?").unwrap();
        assert_eq!(conflicting.mode, AnswerMode::Conflicting);
        assert_eq!(conflicting.claim_ids.len(), 2);

        assert_eq!(
            rules.query(&case_id, "What is the color?").unwrap().mode,
            AnswerMode::Unknown
        );
    }

    #[test]
    fn rule_registration_validates_before_repository_mutation() {
        let repository = Arc::new(RuleRepository::default());
        let rules = service(repository.clone());
        let request = RegisterRuleRequest {
            package_id: "fixture".to_owned(),
            stable_key: "fixture.rule".to_owned(),
            title: "Fixture rule".to_owned(),
            version: 1,
            definition: definition(),
            effective_from: None,
            effective_until: None,
            actor: "test".to_owned(),
            correlation_id: Some(id("correlation_1")),
        };
        let registered = rules.register_rule(request.clone()).unwrap();
        assert_eq!(registered.version, 1);
        assert_eq!(registered.definition_sha256.len(), 64);

        let mut invalid = request;
        invalid.definition.all.clear();
        assert_eq!(
            rules.register_rule(invalid).unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
        assert!(repository.version.lock().expect("version lock").is_some());
    }
}
