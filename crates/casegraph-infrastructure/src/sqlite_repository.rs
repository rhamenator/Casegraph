//! Transactional SQLite implementation of the evidence repository port.

use crate::migrations;
use casegraph_application::{
    AppError, ClaimBundle, ClaimResult, CorrectionBundle, CreateCaseBundle, ErrorKind,
    EvaluationBundle, EvidenceRepository, IngestionBundle, IngestionDisposition, IngestionResult,
    OperationContext, RegisterRuleBundle, ReviewBundle,
};
use casegraph_domain::{
    Artifact, ArtifactVersion, AssertionOrigin, AuditEvent, Case, CaseStatus, Claim, ClaimState,
    Confidence, Contradiction, ContradictionStatus, Correction, Date, Deadline, DetectionMethod,
    Evidence, EvidenceType, Fact, GroundedClaim, HumanReview, KnowledgeValue, Obligation,
    ObligationStatus, ProvenanceRecord, RecordId, ReviewDecision, RuleEvaluation, RuleResult,
    RuleVersion, Source, TaskStatus, TemporalPrecision, TemporalValue, TimestampMs,
    VerificationState, WorkflowMaterialization, WorkflowTask,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

/// One-process transactional repository backed by embedded SQLite.
pub struct SqliteEvidenceRepository {
    connection: Mutex<Connection>,
}

impl SqliteEvidenceRepository {
    /// Open, configure, and migrate a durable database file.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AppError> {
        let connection = migrations::open_database(path.as_ref()).map_err(migration_error)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    /// Create a fully migrated in-memory repository for tests.
    pub fn in_memory() -> Result<Self, AppError> {
        let mut connection = Connection::open_in_memory().map_err(database_error)?;
        migrations::configure(&connection).map_err(migration_error)?;
        migrations::migrate(&mut connection).map_err(migration_error)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, AppError> {
        self.connection
            .lock()
            .map_err(|_| AppError::new(ErrorKind::Internal, "database lock was poisoned"))
    }
}

impl EvidenceRepository for SqliteEvidenceRepository {
    fn create_case(&self, bundle: &CreateCaseBundle) -> Result<Case, AppError> {
        bundle.case_record.validate()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(database_error)?;
        transaction
            .execute(
                "INSERT INTO cases(id, title, status, created_at_ms, closed_at_ms, metadata_json) \
                 VALUES (?1, ?2, ?3, ?4, ?5, '{}')",
                params![
                    bundle.case_record.id.as_str(),
                    bundle.case_record.title,
                    case_status(bundle.case_record.status),
                    bundle.case_record.created_at.get(),
                    bundle.case_record.closed_at.map(TimestampMs::get),
                ],
            )
            .map_err(database_error)?;
        insert_audit(
            &transaction,
            &bundle.context,
            Some(&bundle.case_record.id),
            "case.create",
            "case",
            &bundle.case_record.id,
            None,
            &serde_json::json!({ "status": "open" }),
        )?;
        transaction.commit().map_err(database_error)?;
        Ok(bundle.case_record.clone())
    }

    fn get_case(&self, case_id: &RecordId) -> Result<Option<Case>, AppError> {
        let connection = self.connection()?;
        load_case(&connection, case_id)
    }

    fn ingest(&self, bundle: &IngestionBundle) -> Result<IngestionResult, AppError> {
        bundle.artifact_version.validate()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(database_error)?;
        ensure_case_exists(&transaction, &bundle.artifact.case_id)?;
        let source_id = insert_or_get_source(&transaction, &bundle.source)?;

        let existing_artifact = load_artifact_by_source_key(
            &transaction,
            &bundle.artifact.case_id,
            &bundle.artifact.source_key,
        )?;
        let (artifact, artifact_version, disposition) = if let Some(artifact) = existing_artifact {
            if let Some(version) = load_artifact_version_by_hash(
                &transaction,
                &artifact.id,
                &bundle.artifact_version.content_sha256,
            )? {
                associate_source(
                    &transaction,
                    &version.id,
                    &source_id,
                    bundle.context.occurred_at,
                )?;
                (artifact, version, IngestionDisposition::ExactDuplicate)
            } else {
                let next_version = transaction
                    .query_row(
                        "SELECT COALESCE(MAX(version_number), 0) + 1 \
                         FROM artifact_versions WHERE artifact_id = ?1",
                        [artifact.id.as_str()],
                        |row| row.get::<_, u32>(0),
                    )
                    .map_err(database_error)?;
                let mut version = bundle.artifact_version.clone();
                version.artifact_id = artifact.id.clone();
                version.version_number = next_version;
                insert_artifact_version(&transaction, &version)?;
                associate_source(
                    &transaction,
                    &version.id,
                    &source_id,
                    bundle.context.occurred_at,
                )?;
                (artifact, version, IngestionDisposition::NewVersion)
            }
        } else {
            let mut artifact = bundle.artifact.clone();
            artifact.source_id = source_id.clone();
            transaction
                .execute(
                    "INSERT INTO artifacts(id, case_id, source_id, source_key, created_at_ms) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        artifact.id.as_str(),
                        artifact.case_id.as_str(),
                        artifact.source_id.as_str(),
                        artifact.source_key,
                        artifact.created_at.get(),
                    ],
                )
                .map_err(database_error)?;
            let version = bundle.artifact_version.clone();
            insert_artifact_version(&transaction, &version)?;
            associate_source(
                &transaction,
                &version.id,
                &source_id,
                bundle.context.occurred_at,
            )?;
            (artifact, version, IngestionDisposition::NewArtifact)
        };

        let result = IngestionResult {
            artifact,
            artifact_version,
            disposition,
        };
        insert_audit(
            &transaction,
            &bundle.context,
            Some(&result.artifact.case_id),
            match disposition {
                IngestionDisposition::NewArtifact => "artifact.ingest",
                IngestionDisposition::NewVersion => "artifact.version",
                IngestionDisposition::ExactDuplicate => "artifact.duplicate",
            },
            "artifact_version",
            &result.artifact_version.id,
            None,
            &serde_json::json!({
                "artifact_id": result.artifact.id,
                "artifact_version_id": result.artifact_version.id,
                "content_sha256": result.artifact_version.content_sha256,
                "disposition": result.disposition,
            }),
        )?;
        transaction.commit().map_err(database_error)?;
        Ok(result)
    }

    fn get_artifact_version(
        &self,
        version_id: &RecordId,
    ) -> Result<Option<ArtifactVersion>, AppError> {
        let connection = self.connection()?;
        load_artifact_version(&connection, version_id)
    }

    fn get_provenance(
        &self,
        provenance_id: &RecordId,
    ) -> Result<Option<ProvenanceRecord>, AppError> {
        let connection = self.connection()?;
        load_provenance(&connection, provenance_id)
    }

    fn record_claim(&self, bundle: &ClaimBundle) -> Result<ClaimResult, AppError> {
        bundle.provenance.validate()?;
        bundle.claim.validate()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(database_error)?;
        ensure_case_exists(&transaction, &bundle.claim.case_id)?;
        insert_provenance(&transaction, &bundle.provenance)?;
        insert_observation(&transaction, &bundle.observation)?;
        insert_claim(&transaction, &bundle.claim)?;
        insert_evidence(&transaction, &bundle.evidence)?;
        insert_edge(
            &transaction,
            &bundle.evidence_edge_id,
            &bundle.claim.case_id,
            "claim",
            &bundle.claim.id,
            "supported_by",
            "evidence",
            &bundle.evidence.id,
            bundle.context.occurred_at,
        )?;

        let mut prior = transaction
            .prepare(
                "SELECT c.id, c.normalized_value_json \
                 FROM claims c \
                 WHERE c.case_id = ?1 AND c.subject_key = ?2 AND c.predicate = ?3 \
                   AND c.id <> ?4 \
                   AND COALESCE(( \
                     SELECT state FROM claim_state_changes s \
                     WHERE s.claim_id = c.id ORDER BY changed_at_ms DESC, id DESC LIMIT 1 \
                   ), c.initial_state) NOT IN ('rejected', 'superseded') \
                 ORDER BY c.created_at_ms, c.id",
            )
            .map_err(database_error)?;
        let rows = prior
            .query_map(
                params![
                    bundle.claim.case_id.as_str(),
                    bundle.claim.subject_key,
                    bundle.claim.predicate,
                    bundle.claim.id.as_str(),
                ],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .map_err(database_error)?;
        let mut prior_claims = Vec::new();
        for row in rows {
            prior_claims.push(row.map_err(database_error)?);
        }
        drop(prior);

        let new_known = matches!(bundle.claim.normalized_value, KnowledgeValue::Known(_));
        let normalized = to_json(&bundle.claim.normalized_value)?;
        let mut contradictions = Vec::new();
        let mut corroborates = Vec::new();
        for (prior_id_raw, prior_normalized) in prior_claims {
            let prior_id = parse_id(prior_id_raw)?;
            let prior_value = from_json::<KnowledgeValue>(&prior_normalized)?;
            if !new_known || !matches!(prior_value, KnowledgeValue::Known(_)) {
                continue;
            }
            if prior_normalized == normalized {
                let edge_id = deterministic_id(
                    "edge",
                    &[bundle.claim.id.as_str(), "corroborates", prior_id.as_str()],
                )?;
                insert_edge(
                    &transaction,
                    &edge_id,
                    &bundle.claim.case_id,
                    "claim",
                    &bundle.claim.id,
                    "corroborates",
                    "claim",
                    &prior_id,
                    bundle.context.occurred_at,
                )?;
                corroborates.push(prior_id);
                continue;
            }

            let (claim_a, claim_b) = ordered_ids(&bundle.claim.id, &prior_id);
            let contradiction_id =
                deterministic_id("contradiction", &[claim_a.as_str(), claim_b.as_str()])?;
            transaction
                .execute(
                    "INSERT OR IGNORE INTO contradictions(\
                       id, case_id, claim_a_id, claim_b_id, status, detection_method, created_at_ms\
                     ) VALUES (?1, ?2, ?3, ?4, 'unresolved', 'automatic', ?5)",
                    params![
                        contradiction_id.as_str(),
                        bundle.claim.case_id.as_str(),
                        claim_a.as_str(),
                        claim_b.as_str(),
                        bundle.context.occurred_at.get(),
                    ],
                )
                .map_err(database_error)?;
            contradictions.push(
                load_contradiction(&transaction, &contradiction_id)?.ok_or_else(|| {
                    AppError::new(ErrorKind::Internal, "created contradiction was not found")
                })?,
            );
            insert_contradiction_edges(
                &transaction,
                &bundle.claim.case_id,
                &bundle.claim.id,
                &prior_id,
                bundle.context.occurred_at,
            )?;
            for claim_id in [claim_a, claim_b] {
                insert_state_change(
                    &transaction,
                    &deterministic_id(
                        "claim_state",
                        &[contradiction_id.as_str(), claim_id.as_str()],
                    )?,
                    claim_id,
                    ClaimState::Contradicted,
                    "system:contradiction-detector",
                    Some("conflicting normalized values coexist"),
                    bundle.context.occurred_at,
                    &bundle.context.correlation_id,
                )?;
            }
        }

        let result = ClaimResult {
            claim: bundle.claim.clone(),
            contradictions,
            corroborates,
        };
        insert_audit(
            &transaction,
            &bundle.context,
            Some(&bundle.claim.case_id),
            "claim.record",
            "claim",
            &bundle.claim.id,
            None,
            &serde_json::json!({
                "state": "extracted",
                "provenance_id": bundle.provenance.id,
                "evidence_id": bundle.evidence.id,
                "contradiction_ids": result
                    .contradictions
                    .iter()
                    .map(|item| &item.id)
                    .collect::<Vec<_>>(),
                "corroborates": result.corroborates,
            }),
        )?;
        transaction.commit().map_err(database_error)?;
        Ok(result)
    }

    fn get_claim(&self, claim_id: &RecordId) -> Result<Option<Claim>, AppError> {
        let connection = self.connection()?;
        load_claim(&connection, claim_id)
    }

    fn list_claims(&self, case_id: &RecordId) -> Result<Vec<Claim>, AppError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("SELECT id FROM claims WHERE case_id = ?1 ORDER BY created_at_ms, id")
            .map_err(database_error)?;
        let ids = statement
            .query_map([case_id.as_str()], |row| row.get::<_, String>(0))
            .map_err(database_error)?;
        let mut claims = Vec::new();
        for id in ids {
            let id = parse_id(id.map_err(database_error)?)?;
            claims.push(
                load_claim(&connection, &id)?.ok_or_else(|| {
                    AppError::new(ErrorKind::Internal, "listed claim was not found")
                })?,
            );
        }
        Ok(claims)
    }

    fn list_contradictions(&self, case_id: &RecordId) -> Result<Vec<Contradiction>, AppError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("SELECT id FROM contradictions WHERE case_id = ?1 ORDER BY created_at_ms, id")
            .map_err(database_error)?;
        let ids = statement
            .query_map([case_id.as_str()], |row| row.get::<_, String>(0))
            .map_err(database_error)?;
        let mut contradictions = Vec::new();
        for id in ids {
            let id = parse_id(id.map_err(database_error)?)?;
            contradictions.push(load_contradiction(&connection, &id)?.ok_or_else(|| {
                AppError::new(ErrorKind::Internal, "listed contradiction was not found")
            })?);
        }
        Ok(contradictions)
    }

    fn review_claim(&self, bundle: &ReviewBundle) -> Result<HumanReview, AppError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(database_error)?;
        ensure_claim_exists(&transaction, &bundle.review.target_id)?;
        insert_review(&transaction, &bundle.review)?;
        insert_state_change(
            &transaction,
            &bundle.state_change_id,
            &bundle.review.target_id,
            bundle.state,
            &bundle.review.actor,
            bundle.review.rationale.as_deref(),
            bundle.review.reviewed_at,
            &bundle.review.correlation_id,
        )?;
        if let Some(fact) = &bundle.fact {
            insert_fact(&transaction, fact)?;
        }
        insert_audit(
            &transaction,
            &bundle.context,
            Some(&bundle.review.case_id),
            "claim.review",
            "claim",
            &bundle.review.target_id,
            None,
            &bundle.review,
        )?;
        transaction.commit().map_err(database_error)?;
        Ok(bundle.review.clone())
    }

    fn correct_claim(&self, bundle: &CorrectionBundle) -> Result<Correction, AppError> {
        bundle.provenance.validate()?;
        bundle.corrected_claim.validate()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(database_error)?;
        ensure_claim_exists(&transaction, &bundle.correction.original_claim_id)?;
        insert_provenance(&transaction, &bundle.provenance)?;
        insert_claim(&transaction, &bundle.corrected_claim)?;
        insert_review(&transaction, &bundle.review)?;

        let affected =
            affected_rule_evaluations(&transaction, &bundle.correction.original_claim_id)?;
        let mut correction = bundle.correction.clone();
        correction.affected_derivations = affected;
        transaction
            .execute(
                "INSERT INTO corrections(\
                   id, case_id, original_claim_id, corrected_claim_id, review_id, provenance_id,\
                   original_value_json, corrected_value_json, actor, rationale, corrected_at_ms,\
                   affected_derivations_json\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    correction.id.as_str(),
                    correction.case_id.as_str(),
                    correction.original_claim_id.as_str(),
                    correction.corrected_claim_id.as_str(),
                    correction.review_id.as_str(),
                    correction.provenance_id.as_str(),
                    to_json(&correction.original_value)?,
                    to_json(&correction.corrected_value)?,
                    correction.actor,
                    correction.rationale,
                    correction.corrected_at.get(),
                    to_json(&correction.affected_derivations)?,
                ],
            )
            .map_err(database_error)?;
        insert_state_change(
            &transaction,
            &bundle.original_state_change_id,
            &correction.original_claim_id,
            ClaimState::Superseded,
            &correction.actor,
            correction.rationale.as_deref(),
            correction.corrected_at,
            &bundle.context.correlation_id,
        )?;
        insert_state_change(
            &transaction,
            &bundle.corrected_state_change_id,
            &correction.corrected_claim_id,
            ClaimState::Verified,
            &correction.actor,
            correction.rationale.as_deref(),
            correction.corrected_at,
            &bundle.context.correlation_id,
        )?;
        let edge_id = deterministic_id(
            "edge",
            &[
                correction.corrected_claim_id.as_str(),
                "supersedes",
                correction.original_claim_id.as_str(),
            ],
        )?;
        insert_edge(
            &transaction,
            &edge_id,
            &correction.case_id,
            "claim",
            &correction.corrected_claim_id,
            "supersedes",
            "claim",
            &correction.original_claim_id,
            correction.corrected_at,
        )?;
        insert_audit(
            &transaction,
            &bundle.context,
            Some(&correction.case_id),
            "claim.correct",
            "claim",
            &correction.original_claim_id,
            None,
            &serde_json::json!({
                "corrected_claim_id": correction.corrected_claim_id,
                "review_id": correction.review_id,
                "affected_derivations": correction.affected_derivations,
            }),
        )?;
        transaction.commit().map_err(database_error)?;
        Ok(correction)
    }

    fn list_audit_events(&self, case_id: &RecordId) -> Result<Vec<AuditEvent>, AppError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT id, case_id, operation, actor, target_kind, target_id,\
                        previous_state_json, resulting_state_json, reason, occurred_at_ms, correlation_id \
                 FROM audit_events WHERE case_id = ?1 ORDER BY occurred_at_ms, id",
            )
            .map_err(database_error)?;
        let rows = statement
            .query_map([case_id.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, String>(10)?,
                ))
            })
            .map_err(database_error)?;
        let mut events = Vec::new();
        for row in rows {
            let (
                id,
                case_id,
                operation,
                actor,
                target_kind,
                target_id,
                previous_state_json,
                resulting_state_json,
                reason,
                occurred_at,
                correlation_id,
            ) = row.map_err(database_error)?;
            events.push(AuditEvent {
                id: parse_id(id)?,
                case_id: case_id.map(parse_id).transpose()?,
                operation,
                actor,
                target_kind,
                target_id: parse_id(target_id)?,
                previous_state_json,
                resulting_state_json,
                reason,
                occurred_at: timestamp(occurred_at)?,
                correlation_id: parse_id(correlation_id)?,
            });
        }
        Ok(events)
    }

    fn register_rule(&self, bundle: &RegisterRuleBundle) -> Result<RuleVersion, AppError> {
        bundle.version.definition.validate()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(database_error)?;
        transaction
            .execute(
                "INSERT INTO rules(id, package_id, stable_key, title, created_at_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    bundle.rule.id.as_str(),
                    bundle.rule.package_id,
                    bundle.rule.stable_key,
                    bundle.rule.title,
                    bundle.rule.created_at.get(),
                ],
            )
            .map_err(database_error)?;
        transaction
            .execute(
                "INSERT INTO rule_versions( \
                   id, rule_id, version, definition_json, definition_sha256, effective_from, \
                   effective_until, created_at_ms \
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    bundle.version.id.as_str(),
                    bundle.version.rule_id.as_str(),
                    bundle.version.version,
                    to_json(&bundle.version.definition)?,
                    bundle.version.definition_sha256,
                    bundle.version.effective_from.map(Date::to_iso),
                    bundle.version.effective_until.map(Date::to_iso),
                    bundle.version.created_at.get(),
                ],
            )
            .map_err(database_error)?;
        insert_audit(
            &transaction,
            &bundle.context,
            None,
            "rule.register",
            "rule_version",
            &bundle.version.id,
            None,
            &serde_json::json!({
                "rule_id": bundle.rule.id,
                "version": bundle.version.version,
                "definition_sha256": bundle.version.definition_sha256,
            }),
        )?;
        transaction.commit().map_err(database_error)?;
        Ok(bundle.version.clone())
    }

    fn get_rule_version(
        &self,
        rule_version_id: &RecordId,
    ) -> Result<Option<RuleVersion>, AppError> {
        let connection = self.connection()?;
        load_rule_version(&connection, rule_version_id)
    }

    fn list_grounded_claims(&self, case_id: &RecordId) -> Result<Vec<GroundedClaim>, AppError> {
        let claims = self.list_claims(case_id)?;
        let connection = self.connection()?;
        let mut grounded = Vec::with_capacity(claims.len());
        for claim in claims {
            let state = connection
                .query_row(
                    "SELECT state FROM claim_state_changes WHERE claim_id = ?1 \
                     ORDER BY changed_at_ms DESC, id DESC LIMIT 1",
                    [claim.id.as_str()],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(database_error)?
                .map_or(Ok(claim.initial_state), |value| parse_claim_state(&value))?;
            let mut statement = connection
                .prepare(
                    "SELECT to_id FROM evidence_edges \
                     WHERE from_kind = 'claim' AND from_id = ?1 \
                       AND relationship_type = 'supported_by' AND to_kind = 'evidence' \
                     ORDER BY to_id",
                )
                .map_err(database_error)?;
            let rows = statement
                .query_map([claim.id.as_str()], |row| row.get::<_, String>(0))
                .map_err(database_error)?;
            let mut evidence_ids = Vec::new();
            for row in rows {
                evidence_ids.push(parse_id(row.map_err(database_error)?)?);
            }
            grounded.push(GroundedClaim {
                provenance_id: claim.primary_provenance_id.clone(),
                claim,
                current_state: state,
                evidence_ids,
            });
        }
        Ok(grounded)
    }

    fn record_evaluation(
        &self,
        bundle: &EvaluationBundle,
    ) -> Result<WorkflowMaterialization, AppError> {
        let item = &bundle.materialization;
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(database_error)?;
        let existing_id = transaction
            .query_row(
                "SELECT id FROM rule_evaluations \
                 WHERE case_id = ?1 AND rule_version_id = ?2 AND inputs_sha256 = ?3",
                params![
                    item.evaluation.case_id.as_str(),
                    item.evaluation.rule_version_id.as_str(),
                    item.evaluation.inputs_sha256,
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(database_error)?;
        if let Some(existing_id) = existing_id {
            return load_workflow(&transaction, &parse_id(existing_id)?);
        }
        transaction
            .execute(
                "INSERT INTO rule_evaluations( \
                   id, case_id, rule_version_id, inputs_json, inputs_sha256, result, result_json, \
                   explanation, evaluated_at_ms, evaluator_version, correlation_id \
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, '{}', ?7, ?8, ?9, ?10)",
                params![
                    item.evaluation.id.as_str(),
                    item.evaluation.case_id.as_str(),
                    item.evaluation.rule_version_id.as_str(),
                    to_json(&item.evaluation.inputs)?,
                    item.evaluation.inputs_sha256,
                    rule_result(item.evaluation.result),
                    item.evaluation.explanation,
                    item.evaluation.evaluated_at.get(),
                    item.evaluation.evaluator_version,
                    item.evaluation.correlation_id.as_str(),
                ],
            )
            .map_err(database_error)?;
        let evidence_ids = item
            .evaluation
            .inputs
            .iter()
            .flat_map(|input| input.evidence_ids.iter())
            .collect::<BTreeSet<_>>();
        for evidence_id in evidence_ids {
            transaction
                .execute(
                    "INSERT INTO rule_evaluation_evidence(rule_evaluation_id, evidence_id) \
                     VALUES (?1, ?2)",
                    params![item.evaluation.id.as_str(), evidence_id.as_str()],
                )
                .map_err(database_error)?;
        }
        if let Some(obligation) = &item.obligation {
            transaction
                .execute(
                    "INSERT INTO obligations( \
                       id, case_id, created_by_event_id, created_by_rule_evaluation_id, kind, \
                       description, status, created_at_ms \
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        obligation.id.as_str(),
                        obligation.case_id.as_str(),
                        obligation
                            .created_by_event_id
                            .as_ref()
                            .map(RecordId::as_str),
                        obligation
                            .created_by_rule_evaluation_id
                            .as_ref()
                            .map(RecordId::as_str),
                        obligation.kind,
                        obligation.description,
                        obligation_status(obligation.status),
                        obligation.created_at.get(),
                    ],
                )
                .map_err(database_error)?;
        }
        if let Some(deadline) = &item.deadline {
            transaction
                .execute(
                    "INSERT INTO deadlines( \
                       id, case_id, obligation_id, due_earliest, due_latest, original_expression, \
                       temporal_precision, calculation_json, created_at_ms \
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        deadline.id.as_str(),
                        deadline.case_id.as_str(),
                        deadline.obligation_id.as_str(),
                        deadline.due_earliest.map(Date::to_iso),
                        deadline.due_latest.map(Date::to_iso),
                        deadline.original_expression,
                        temporal_precision(deadline.temporal_precision),
                        deadline.calculation_json,
                        deadline.created_at.get(),
                    ],
                )
                .map_err(database_error)?;
        }
        if let Some(task) = &item.task {
            transaction
                .execute(
                    "INSERT INTO workflow_tasks( \
                       id, case_id, obligation_id, title, status, created_at_ms, completed_at_ms \
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        task.id.as_str(),
                        task.case_id.as_str(),
                        task.obligation_id.as_ref().map(RecordId::as_str),
                        task.title,
                        task_status(task.status),
                        task.created_at.get(),
                        task.completed_at.map(TimestampMs::get),
                    ],
                )
                .map_err(database_error)?;
        }
        insert_audit(
            &transaction,
            &bundle.context,
            Some(&item.evaluation.case_id),
            "rule.evaluate",
            "rule_evaluation",
            &item.evaluation.id,
            None,
            &serde_json::json!({
                "rule_version_id": item.evaluation.rule_version_id,
                "inputs_sha256": item.evaluation.inputs_sha256,
                "result": item.evaluation.result,
                "obligation_id": item.obligation.as_ref().map(|value| &value.id),
                "deadline_id": item.deadline.as_ref().map(|value| &value.id),
                "task_id": item.task.as_ref().map(|value| &value.id),
            }),
        )?;
        transaction.commit().map_err(database_error)?;
        Ok(item.clone())
    }

    fn list_workflow(&self, case_id: &RecordId) -> Result<Vec<WorkflowMaterialization>, AppError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT id FROM rule_evaluations WHERE case_id = ?1 ORDER BY evaluated_at_ms, id",
            )
            .map_err(database_error)?;
        let rows = statement
            .query_map([case_id.as_str()], |row| row.get::<_, String>(0))
            .map_err(database_error)?;
        let mut items = Vec::new();
        for row in rows {
            let evaluation_id = parse_id(row.map_err(database_error)?)?;
            items.push(load_workflow(&connection, &evaluation_id)?);
        }
        Ok(items)
    }
}

fn insert_or_get_source(
    transaction: &Transaction<'_>,
    source: &Source,
) -> Result<RecordId, AppError> {
    let inserted = transaction
        .execute(
            "INSERT OR IGNORE INTO sources(\
               id, case_id, connector, locator, external_record_id, endpoint, source_revision,\
               retrieved_at_ms, metadata_json\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, '{}')",
            params![
                source.id.as_str(),
                source.case_id.as_str(),
                source.connector,
                source.locator,
                source.external_record_id,
                source.endpoint,
                source.source_revision,
                source.retrieved_at.get(),
            ],
        )
        .map_err(database_error)?;
    if inserted == 1 {
        return Ok(source.id.clone());
    }
    let id = transaction
        .query_row(
            "SELECT id FROM sources \
             WHERE case_id = ?1 AND connector = ?2 AND locator = ?3 AND source_revision IS ?4",
            params![
                source.case_id.as_str(),
                source.connector,
                source.locator,
                source.source_revision,
            ],
            |row| row.get::<_, String>(0),
        )
        .map_err(database_error)?;
    parse_id(id)
}

fn insert_artifact_version(
    transaction: &Transaction<'_>,
    version: &ArtifactVersion,
) -> Result<(), AppError> {
    version.validate()?;
    transaction
        .execute(
            "INSERT INTO artifact_versions(\
               id, artifact_id, version_number, content_sha256, content_length, media_type,\
               storage_key, ingested_at_ms, received_at_ms, original_filename, metadata_json\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, '{}')",
            params![
                version.id.as_str(),
                version.artifact_id.as_str(),
                version.version_number,
                version.content_sha256,
                i64::try_from(version.content_length).map_err(|_| AppError::new(
                    ErrorKind::TooLarge,
                    "artifact length exceeds database range"
                ))?,
                version.media_type,
                version.storage_key,
                version.ingested_at.get(),
                version.received_at.map(TimestampMs::get),
                version.original_filename,
            ],
        )
        .map_err(database_error)?;
    Ok(())
}

fn associate_source(
    transaction: &Transaction<'_>,
    version_id: &RecordId,
    source_id: &RecordId,
    at: TimestampMs,
) -> Result<(), AppError> {
    transaction
        .execute(
            "INSERT OR IGNORE INTO artifact_version_sources(artifact_version_id, source_id, associated_at_ms) \
             VALUES (?1, ?2, ?3)",
            params![version_id.as_str(), source_id.as_str(), at.get()],
        )
        .map_err(database_error)?;
    Ok(())
}

fn insert_provenance(
    transaction: &Transaction<'_>,
    provenance: &ProvenanceRecord,
) -> Result<(), AppError> {
    provenance.validate()?;
    transaction
        .execute(
            "INSERT INTO provenance_records(\
               id, artifact_version_id, connector, endpoint, external_record_id, source_field,\
               page_number, paragraph_number, text_span_start, text_span_end, table_number,\
               row_number, column_number, bounding_region_json, extraction_method, extractor_name,\
               extractor_version, model_provider, model_name, model_version,\
               model_configuration_json, extracted_at_ms, confidence, verification_state,\
               original_representation, correlation_id\
             ) VALUES (\
               ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,\
               ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26\
             )",
            params![
                provenance.id.as_str(),
                provenance
                    .artifact_version_id
                    .as_ref()
                    .map(RecordId::as_str),
                provenance.connector,
                provenance.endpoint,
                provenance.external_record_id,
                provenance.source_field,
                provenance.page_number,
                provenance.paragraph_number,
                provenance
                    .text_span_start
                    .map(i64::try_from)
                    .transpose()
                    .map_err(|_| {
                        AppError::new(
                            ErrorKind::InvalidInput,
                            "text span start exceeds database range",
                        )
                    })?,
                provenance
                    .text_span_end
                    .map(i64::try_from)
                    .transpose()
                    .map_err(|_| {
                        AppError::new(
                            ErrorKind::InvalidInput,
                            "text span end exceeds database range",
                        )
                    })?,
                provenance.table_number,
                provenance.row_number,
                provenance.column_number,
                provenance.bounding_region_json,
                provenance.extraction_method,
                provenance.extractor_name,
                provenance.extractor_version,
                provenance.model_provider,
                provenance.model_name,
                provenance.model_version,
                provenance.model_configuration_json,
                provenance.extracted_at.get(),
                provenance.confidence.map(Confidence::get),
                verification_state(provenance.verification_state),
                provenance.original_representation,
                provenance.correlation_id.as_str(),
            ],
        )
        .map_err(database_error)?;
    Ok(())
}

fn insert_observation(
    transaction: &Transaction<'_>,
    observation: &casegraph_domain::Observation,
) -> Result<(), AppError> {
    transaction
        .execute(
            "INSERT INTO observations(\
               id, case_id, subject_id, predicate, original_value, normalized_value_json,\
               provenance_id, extraction_confidence, observed_at_ms\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                observation.id.as_str(),
                observation.case_id.as_str(),
                observation.subject_id.as_ref().map(RecordId::as_str),
                observation.predicate,
                observation.original_value,
                observation
                    .normalized_value
                    .as_ref()
                    .map(to_json)
                    .transpose()?,
                observation.provenance_id.as_str(),
                observation.extraction_confidence.map(Confidence::get),
                observation.observed_at.get(),
            ],
        )
        .map_err(database_error)?;
    Ok(())
}

fn insert_claim(transaction: &Transaction<'_>, claim: &Claim) -> Result<(), AppError> {
    claim.validate()?;
    transaction
        .execute(
            "INSERT INTO claims(\
               id, case_id, subject_id, subject_key, predicate, original_value, normalized_value_json,\
               origin, initial_state, primary_provenance_id, interpretation_confidence,\
               created_at_ms, temporal_json\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                claim.id.as_str(),
                claim.case_id.as_str(),
                claim.subject_id.as_ref().map(RecordId::as_str),
                claim.subject_key,
                claim.predicate,
                claim.original_value,
                to_json(&claim.normalized_value)?,
                assertion_origin(claim.origin),
                claim_state(claim.initial_state),
                claim.primary_provenance_id.as_ref().map(RecordId::as_str),
                claim.interpretation_confidence.map(Confidence::get),
                claim.created_at.get(),
                claim.temporal.as_ref().map(to_json).transpose()?,
            ],
        )
        .map_err(database_error)?;
    Ok(())
}

fn insert_evidence(transaction: &Transaction<'_>, evidence: &Evidence) -> Result<(), AppError> {
    transaction
        .execute(
            "INSERT INTO evidence(\
               id, case_id, evidence_type, provenance_id, description, created_at_ms\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                evidence.id.as_str(),
                evidence.case_id.as_str(),
                evidence_type(evidence.evidence_type),
                evidence.provenance_id.as_ref().map(RecordId::as_str),
                evidence.description,
                evidence.created_at.get(),
            ],
        )
        .map_err(database_error)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn insert_edge(
    transaction: &Transaction<'_>,
    edge_id: &RecordId,
    case_id: &RecordId,
    from_kind: &str,
    from_id: &RecordId,
    relationship: &str,
    to_kind: &str,
    to_id: &RecordId,
    created_at: TimestampMs,
) -> Result<(), AppError> {
    transaction
        .execute(
            "INSERT OR IGNORE INTO evidence_edges(\
               id, case_id, from_kind, from_id, relationship_type, to_kind, to_id, created_at_ms\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                edge_id.as_str(),
                case_id.as_str(),
                from_kind,
                from_id.as_str(),
                relationship,
                to_kind,
                to_id.as_str(),
                created_at.get(),
            ],
        )
        .map_err(database_error)?;
    Ok(())
}

fn insert_contradiction_edges(
    transaction: &Transaction<'_>,
    case_id: &RecordId,
    left: &RecordId,
    right: &RecordId,
    at: TimestampMs,
) -> Result<(), AppError> {
    for (from, to) in [(left, right), (right, left)] {
        let edge_id = deterministic_id("edge", &[from.as_str(), "contradicts", to.as_str()])?;
        insert_edge(
            transaction,
            &edge_id,
            case_id,
            "claim",
            from,
            "contradicts",
            "claim",
            to,
            at,
        )?;
    }
    Ok(())
}

fn insert_review(transaction: &Transaction<'_>, review: &HumanReview) -> Result<(), AppError> {
    transaction
        .execute(
            "INSERT INTO human_reviews(\
               id, case_id, target_kind, target_id, decision, actor, rationale, reviewed_at_ms,\
               correlation_id\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                review.id.as_str(),
                review.case_id.as_str(),
                review.target_kind,
                review.target_id.as_str(),
                review_decision(review.decision),
                review.actor,
                review.rationale,
                review.reviewed_at.get(),
                review.correlation_id.as_str(),
            ],
        )
        .map_err(database_error)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn insert_state_change(
    transaction: &Transaction<'_>,
    id: &RecordId,
    claim_id: &RecordId,
    state: ClaimState,
    actor: &str,
    reason: Option<&str>,
    changed_at: TimestampMs,
    correlation_id: &RecordId,
) -> Result<(), AppError> {
    transaction
        .execute(
            "INSERT OR IGNORE INTO claim_state_changes(\
               id, claim_id, state, actor, reason, changed_at_ms, correlation_id\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id.as_str(),
                claim_id.as_str(),
                claim_state(state),
                actor,
                reason,
                changed_at.get(),
                correlation_id.as_str(),
            ],
        )
        .map_err(database_error)?;
    Ok(())
}

fn insert_fact(transaction: &Transaction<'_>, fact: &Fact) -> Result<(), AppError> {
    transaction
        .execute(
            "INSERT INTO facts(\
               id, case_id, claim_id, established_value_json, established_at_ms, established_by,\
               verification_state\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'verified')",
            params![
                fact.id.as_str(),
                fact.case_id.as_str(),
                fact.claim_id.as_str(),
                to_json(&fact.established_value)?,
                fact.established_at.get(),
                fact.established_by,
            ],
        )
        .map_err(database_error)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn insert_audit<T: Serialize>(
    transaction: &Transaction<'_>,
    context: &OperationContext,
    case_id: Option<&RecordId>,
    operation: &str,
    target_kind: &str,
    target_id: &RecordId,
    previous_state_json: Option<&str>,
    resulting_state: &T,
) -> Result<(), AppError> {
    transaction
        .execute(
            "INSERT INTO audit_events(\
               id, case_id, operation, actor, target_kind, target_id, previous_state_json,\
               resulting_state_json, reason, occurred_at_ms, correlation_id\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                context.audit_id.as_str(),
                case_id.map(RecordId::as_str),
                operation,
                context.actor,
                target_kind,
                target_id.as_str(),
                previous_state_json,
                to_json(resulting_state)?,
                context.reason,
                context.occurred_at.get(),
                context.correlation_id.as_str(),
            ],
        )
        .map_err(database_error)?;
    Ok(())
}

fn load_case(connection: &Connection, case_id: &RecordId) -> Result<Option<Case>, AppError> {
    let raw = connection
        .query_row(
            "SELECT id, title, status, created_at_ms, closed_at_ms FROM cases WHERE id = ?1",
            [case_id.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            },
        )
        .optional()
        .map_err(database_error)?;
    raw.map(|(id, title, status, created_at, closed_at)| {
        Ok(Case {
            id: parse_id(id)?,
            title,
            status: parse_case_status(&status)?,
            created_at: timestamp(created_at)?,
            closed_at: closed_at.map(timestamp).transpose()?,
        })
    })
    .transpose()
}

fn load_artifact_by_source_key(
    connection: &Connection,
    case_id: &RecordId,
    source_key: &str,
) -> Result<Option<Artifact>, AppError> {
    let raw = connection
        .query_row(
            "SELECT id, case_id, source_id, source_key, created_at_ms \
             FROM artifacts WHERE case_id = ?1 AND source_key = ?2",
            params![case_id.as_str(), source_key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .optional()
        .map_err(database_error)?;
    raw.map(|(id, case_id, source_id, source_key, created_at)| {
        Ok(Artifact {
            id: parse_id(id)?,
            case_id: parse_id(case_id)?,
            source_id: parse_id(source_id)?,
            source_key,
            created_at: timestamp(created_at)?,
        })
    })
    .transpose()
}

fn load_artifact_version(
    connection: &Connection,
    version_id: &RecordId,
) -> Result<Option<ArtifactVersion>, AppError> {
    load_artifact_version_where(connection, "v.id = ?1", version_id.as_str())
}

fn load_artifact_version_by_hash(
    connection: &Connection,
    artifact_id: &RecordId,
    hash: &str,
) -> Result<Option<ArtifactVersion>, AppError> {
    let raw = connection
        .query_row(
            "SELECT id, artifact_id, version_number, content_sha256, content_length, media_type,\
                    storage_key, ingested_at_ms, received_at_ms, original_filename \
             FROM artifact_versions WHERE artifact_id = ?1 AND content_sha256 = ?2",
            params![artifact_id.as_str(), hash],
            artifact_version_row,
        )
        .optional()
        .map_err(database_error)?;
    raw.map(artifact_version_from_raw).transpose()
}

fn load_artifact_version_where(
    connection: &Connection,
    predicate: &str,
    value: &str,
) -> Result<Option<ArtifactVersion>, AppError> {
    let sql = format!(
        "SELECT v.id, v.artifact_id, v.version_number, v.content_sha256, v.content_length,\
                v.media_type, v.storage_key, v.ingested_at_ms, v.received_at_ms, v.original_filename \
         FROM artifact_versions v WHERE {predicate}"
    );
    let raw = connection
        .query_row(&sql, [value], artifact_version_row)
        .optional()
        .map_err(database_error)?;
    raw.map(artifact_version_from_raw).transpose()
}

type ArtifactVersionRaw = (
    String,
    String,
    u32,
    String,
    i64,
    String,
    String,
    i64,
    Option<i64>,
    Option<String>,
);

fn artifact_version_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArtifactVersionRaw> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
    ))
}

fn artifact_version_from_raw(raw: ArtifactVersionRaw) -> Result<ArtifactVersion, AppError> {
    let (id, artifact_id, version, hash, length, media_type, key, ingested, received, filename) =
        raw;
    let record = ArtifactVersion {
        id: parse_id(id)?,
        artifact_id: parse_id(artifact_id)?,
        version_number: version,
        content_sha256: hash,
        content_length: u64::try_from(length).map_err(|_| {
            AppError::new(
                ErrorKind::Storage,
                "database contains a negative byte length",
            )
        })?,
        media_type,
        storage_key: key,
        ingested_at: timestamp(ingested)?,
        received_at: received.map(timestamp).transpose()?,
        original_filename: filename,
    };
    record.validate()?;
    Ok(record)
}

#[derive(Debug)]
struct ProvenanceRaw {
    id: String,
    artifact_version_id: Option<String>,
    connector: Option<String>,
    endpoint: Option<String>,
    external_record_id: Option<String>,
    source_field: Option<String>,
    page_number: Option<u32>,
    paragraph_number: Option<u32>,
    text_span_start: Option<i64>,
    text_span_end: Option<i64>,
    table_number: Option<u32>,
    row_number: Option<u32>,
    column_number: Option<u32>,
    bounding_region_json: Option<String>,
    extraction_method: String,
    extractor_name: String,
    extractor_version: String,
    model_provider: Option<String>,
    model_name: Option<String>,
    model_version: Option<String>,
    model_configuration_json: Option<String>,
    extracted_at: i64,
    confidence: Option<f64>,
    verification_state: String,
    original_representation: Option<String>,
    correlation_id: String,
}

fn load_provenance(
    connection: &Connection,
    provenance_id: &RecordId,
) -> Result<Option<ProvenanceRecord>, AppError> {
    let raw = connection
        .query_row(
            "SELECT id, artifact_version_id, connector, endpoint, external_record_id, source_field,\
                    page_number, paragraph_number, text_span_start, text_span_end, table_number,\
                    row_number, column_number, bounding_region_json, extraction_method, extractor_name,\
                    extractor_version, model_provider, model_name, model_version,\
                    model_configuration_json, extracted_at_ms, confidence, verification_state,\
                    original_representation, correlation_id \
             FROM provenance_records WHERE id = ?1",
            [provenance_id.as_str()],
            |row| {
                Ok(ProvenanceRaw {
                    id: row.get(0)?,
                    artifact_version_id: row.get(1)?,
                    connector: row.get(2)?,
                    endpoint: row.get(3)?,
                    external_record_id: row.get(4)?,
                    source_field: row.get(5)?,
                    page_number: row.get(6)?,
                    paragraph_number: row.get(7)?,
                    text_span_start: row.get(8)?,
                    text_span_end: row.get(9)?,
                    table_number: row.get(10)?,
                    row_number: row.get(11)?,
                    column_number: row.get(12)?,
                    bounding_region_json: row.get(13)?,
                    extraction_method: row.get(14)?,
                    extractor_name: row.get(15)?,
                    extractor_version: row.get(16)?,
                    model_provider: row.get(17)?,
                    model_name: row.get(18)?,
                    model_version: row.get(19)?,
                    model_configuration_json: row.get(20)?,
                    extracted_at: row.get(21)?,
                    confidence: row.get(22)?,
                    verification_state: row.get(23)?,
                    original_representation: row.get(24)?,
                    correlation_id: row.get(25)?,
                })
            },
        )
        .optional()
        .map_err(database_error)?;
    raw.map(|raw| {
        let record = ProvenanceRecord {
            id: parse_id(raw.id)?,
            artifact_version_id: raw.artifact_version_id.map(parse_id).transpose()?,
            connector: raw.connector,
            endpoint: raw.endpoint,
            external_record_id: raw.external_record_id,
            source_field: raw.source_field,
            page_number: raw.page_number,
            paragraph_number: raw.paragraph_number,
            text_span_start: raw
                .text_span_start
                .map(u64::try_from)
                .transpose()
                .map_err(|_| storage_enum("text span start"))?,
            text_span_end: raw
                .text_span_end
                .map(u64::try_from)
                .transpose()
                .map_err(|_| storage_enum("text span end"))?,
            table_number: raw.table_number,
            row_number: raw.row_number,
            column_number: raw.column_number,
            bounding_region_json: raw.bounding_region_json,
            extraction_method: raw.extraction_method,
            extractor_name: raw.extractor_name,
            extractor_version: raw.extractor_version,
            model_provider: raw.model_provider,
            model_name: raw.model_name,
            model_version: raw.model_version,
            model_configuration_json: raw.model_configuration_json,
            extracted_at: timestamp(raw.extracted_at)?,
            confidence: raw.confidence.map(Confidence::new).transpose()?,
            verification_state: parse_verification_state(&raw.verification_state)?,
            original_representation: raw.original_representation,
            correlation_id: parse_id(raw.correlation_id)?,
        };
        record.validate()?;
        Ok(record)
    })
    .transpose()
}

fn load_claim(connection: &Connection, claim_id: &RecordId) -> Result<Option<Claim>, AppError> {
    let raw = connection
        .query_row(
            "SELECT id, case_id, subject_id, subject_key, predicate, original_value,\
                    normalized_value_json, origin, initial_state, primary_provenance_id,\
                    interpretation_confidence, temporal_json, created_at_ms \
             FROM claims WHERE id = ?1",
            [claim_id.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<f64>>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, i64>(12)?,
                ))
            },
        )
        .optional()
        .map_err(database_error)?;
    raw.map(
        |(
            id,
            case_id,
            subject_id,
            subject_key,
            predicate,
            original_value,
            normalized,
            origin,
            state,
            provenance,
            confidence,
            temporal,
            created_at,
        )| {
            let claim = Claim {
                id: parse_id(id)?,
                case_id: parse_id(case_id)?,
                subject_id: subject_id.map(parse_id).transpose()?,
                subject_key,
                predicate,
                original_value,
                normalized_value: from_json(&normalized)?,
                origin: parse_assertion_origin(&origin)?,
                initial_state: parse_claim_state(&state)?,
                primary_provenance_id: provenance.map(parse_id).transpose()?,
                interpretation_confidence: confidence.map(Confidence::new).transpose()?,
                temporal: temporal
                    .as_deref()
                    .map(from_json::<TemporalValue>)
                    .transpose()?,
                created_at: timestamp(created_at)?,
            };
            claim.validate()?;
            Ok(claim)
        },
    )
    .transpose()
}

fn load_contradiction(
    connection: &Connection,
    contradiction_id: &RecordId,
) -> Result<Option<Contradiction>, AppError> {
    let raw = connection
        .query_row(
            "SELECT id, case_id, claim_a_id, claim_b_id, status, detection_method, rationale,\
                    resolution_claim_id, adjudicated_by, created_at_ms, resolved_at_ms \
             FROM contradictions WHERE id = ?1",
            [contradiction_id.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, Option<i64>>(10)?,
                ))
            },
        )
        .optional()
        .map_err(database_error)?;
    raw.map(
        |(id, case_id, a, b, status, method, rationale, resolution, actor, created, resolved)| {
            Ok(Contradiction {
                id: parse_id(id)?,
                case_id: parse_id(case_id)?,
                claim_a_id: parse_id(a)?,
                claim_b_id: parse_id(b)?,
                status: parse_contradiction_status(&status)?,
                detection_method: parse_detection_method(&method)?,
                rationale,
                resolution_claim_id: resolution.map(parse_id).transpose()?,
                adjudicated_by: actor,
                created_at: timestamp(created)?,
                resolved_at: resolved.map(timestamp).transpose()?,
            })
        },
    )
    .transpose()
}

fn load_rule_version(
    connection: &Connection,
    version_id: &RecordId,
) -> Result<Option<RuleVersion>, AppError> {
    let raw = connection
        .query_row(
            "SELECT id, rule_id, version, definition_json, definition_sha256, effective_from, \
                    effective_until, created_at_ms \
             FROM rule_versions WHERE id = ?1",
            [version_id.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            },
        )
        .optional()
        .map_err(database_error)?;
    raw.map(
        |(id, rule_id, version, definition, hash, effective_from, effective_until, created_at)| {
            let definition = from_json(&definition)?;
            let item = RuleVersion {
                id: parse_id(id)?,
                rule_id: parse_id(rule_id)?,
                version,
                definition,
                definition_sha256: hash,
                effective_from: effective_from.as_deref().map(Date::parse_iso).transpose()?,
                effective_until: effective_until
                    .as_deref()
                    .map(Date::parse_iso)
                    .transpose()?,
                created_at: timestamp(created_at)?,
            };
            item.definition.validate()?;
            Ok(item)
        },
    )
    .transpose()
}

fn load_workflow(
    connection: &Connection,
    evaluation_id: &RecordId,
) -> Result<WorkflowMaterialization, AppError> {
    let raw = connection
        .query_row(
            "SELECT id, case_id, rule_version_id, inputs_json, inputs_sha256, result, explanation, \
                    evaluated_at_ms, evaluator_version, correlation_id \
             FROM rule_evaluations WHERE id = ?1",
            [evaluation_id.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                ))
            },
        )
        .map_err(database_error)?;
    let evaluation = RuleEvaluation {
        id: parse_id(raw.0)?,
        case_id: parse_id(raw.1)?,
        rule_version_id: parse_id(raw.2)?,
        inputs: from_json(&raw.3)?,
        inputs_sha256: raw.4,
        result: parse_rule_result(&raw.5)?,
        explanation: raw.6,
        evaluated_at: timestamp(raw.7)?,
        evaluator_version: raw.8,
        correlation_id: parse_id(raw.9)?,
    };
    let obligation = load_obligation_for_evaluation(connection, evaluation_id)?;
    let deadline = obligation
        .as_ref()
        .map(|item| load_deadline_for_obligation(connection, &item.id))
        .transpose()?
        .flatten();
    let task = obligation
        .as_ref()
        .map(|item| load_task_for_obligation(connection, &item.id))
        .transpose()?
        .flatten();
    Ok(WorkflowMaterialization {
        evaluation,
        obligation,
        deadline,
        task,
    })
}

fn load_obligation_for_evaluation(
    connection: &Connection,
    evaluation_id: &RecordId,
) -> Result<Option<Obligation>, AppError> {
    let raw = connection
        .query_row(
            "SELECT id, case_id, created_by_event_id, created_by_rule_evaluation_id, kind, \
                    description, status, created_at_ms \
             FROM obligations WHERE created_by_rule_evaluation_id = ?1",
            [evaluation_id.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            },
        )
        .optional()
        .map_err(database_error)?;
    raw.map(|raw| {
        Ok(Obligation {
            id: parse_id(raw.0)?,
            case_id: parse_id(raw.1)?,
            created_by_event_id: raw.2.map(parse_id).transpose()?,
            created_by_rule_evaluation_id: raw.3.map(parse_id).transpose()?,
            kind: raw.4,
            description: raw.5,
            status: parse_obligation_status(&raw.6)?,
            created_at: timestamp(raw.7)?,
        })
    })
    .transpose()
}

fn load_deadline_for_obligation(
    connection: &Connection,
    obligation_id: &RecordId,
) -> Result<Option<Deadline>, AppError> {
    let raw = connection
        .query_row(
            "SELECT id, case_id, obligation_id, due_earliest, due_latest, original_expression, \
                    temporal_precision, calculation_json, created_at_ms \
             FROM deadlines WHERE obligation_id = ?1",
            [obligation_id.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, i64>(8)?,
                ))
            },
        )
        .optional()
        .map_err(database_error)?;
    raw.map(|raw| {
        Ok(Deadline {
            id: parse_id(raw.0)?,
            case_id: parse_id(raw.1)?,
            obligation_id: parse_id(raw.2)?,
            due_earliest: raw.3.as_deref().map(Date::parse_iso).transpose()?,
            due_latest: raw.4.as_deref().map(Date::parse_iso).transpose()?,
            original_expression: raw.5,
            temporal_precision: parse_temporal_precision(&raw.6)?,
            calculation_json: raw.7,
            created_at: timestamp(raw.8)?,
        })
    })
    .transpose()
}

fn load_task_for_obligation(
    connection: &Connection,
    obligation_id: &RecordId,
) -> Result<Option<WorkflowTask>, AppError> {
    let raw = connection
        .query_row(
            "SELECT id, case_id, obligation_id, title, status, created_at_ms, completed_at_ms \
             FROM workflow_tasks WHERE obligation_id = ?1",
            [obligation_id.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                ))
            },
        )
        .optional()
        .map_err(database_error)?;
    raw.map(|raw| {
        Ok(WorkflowTask {
            id: parse_id(raw.0)?,
            case_id: parse_id(raw.1)?,
            obligation_id: raw.2.map(parse_id).transpose()?,
            title: raw.3,
            status: parse_task_status(&raw.4)?,
            created_at: timestamp(raw.5)?,
            completed_at: raw.6.map(timestamp).transpose()?,
        })
    })
    .transpose()
}

fn affected_rule_evaluations(
    transaction: &Transaction<'_>,
    claim_id: &RecordId,
) -> Result<Vec<RecordId>, AppError> {
    let mut statement = transaction
        .prepare(
            "SELECT DISTINCT ree.rule_evaluation_id \
             FROM evidence_edges edge \
             JOIN rule_evaluation_evidence ree ON ree.evidence_id = edge.to_id \
             WHERE edge.from_kind = 'claim' AND edge.from_id = ?1 \
               AND edge.relationship_type = 'supported_by' AND edge.to_kind = 'evidence' \
             ORDER BY ree.rule_evaluation_id",
        )
        .map_err(database_error)?;
    let rows = statement
        .query_map([claim_id.as_str()], |row| row.get::<_, String>(0))
        .map_err(database_error)?;
    let mut affected = Vec::new();
    for row in rows {
        affected.push(parse_id(row.map_err(database_error)?)?);
    }
    Ok(affected)
}

fn ensure_case_exists(connection: &Connection, case_id: &RecordId) -> Result<(), AppError> {
    let exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM cases WHERE id = ?1)",
            [case_id.as_str()],
            |row| row.get::<_, bool>(0),
        )
        .map_err(database_error)?;
    if exists {
        Ok(())
    } else {
        Err(AppError::new(ErrorKind::NotFound, "case was not found"))
    }
}

fn ensure_claim_exists(connection: &Connection, claim_id: &RecordId) -> Result<(), AppError> {
    let exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM claims WHERE id = ?1)",
            [claim_id.as_str()],
            |row| row.get::<_, bool>(0),
        )
        .map_err(database_error)?;
    if exists {
        Ok(())
    } else {
        Err(AppError::new(ErrorKind::NotFound, "claim was not found"))
    }
}

fn deterministic_id(kind: &str, parts: &[&str]) -> Result<RecordId, AppError> {
    let mut hasher = Sha256::new();
    hasher.update(kind.as_bytes());
    for part in parts {
        hasher.update([0]);
        hasher.update(part.as_bytes());
    }
    let digest = hasher.finalize();
    let suffix = digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    RecordId::parse(format!("{kind}_{suffix}")).map_err(Into::into)
}

fn ordered_ids<'a>(left: &'a RecordId, right: &'a RecordId) -> (&'a RecordId, &'a RecordId) {
    if left < right {
        (left, right)
    } else {
        (right, left)
    }
}

fn to_json<T: Serialize>(value: &T) -> Result<String, AppError> {
    serde_json::to_string(value).map_err(|_| {
        AppError::new(
            ErrorKind::Internal,
            "could not serialize validated application state",
        )
    })
}

fn from_json<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, AppError> {
    serde_json::from_str(value).map_err(|_| {
        AppError::new(
            ErrorKind::Storage,
            "database contains malformed canonical JSON",
        )
    })
}

fn parse_id(value: String) -> Result<RecordId, AppError> {
    RecordId::parse(value).map_err(|_| {
        AppError::new(
            ErrorKind::Storage,
            "database contains an invalid canonical identifier",
        )
    })
}

fn timestamp(value: i64) -> Result<TimestampMs, AppError> {
    TimestampMs::new(value)
        .map_err(|_| AppError::new(ErrorKind::Storage, "database contains an invalid timestamp"))
}

fn migration_error(_: migrations::MigrationError) -> AppError {
    AppError::new(ErrorKind::Storage, "database migration failed")
}

fn database_error(error: rusqlite::Error) -> AppError {
    let kind = if error
        .sqlite_error_code()
        .is_some_and(|code| code == rusqlite::ErrorCode::ConstraintViolation)
    {
        ErrorKind::Conflict
    } else {
        ErrorKind::Storage
    };
    let code = error.sqlite_error().map_or_else(
        || "unknown".to_owned(),
        |details| details.extended_code.to_string(),
    );
    AppError::new(kind, format!("database operation failed (SQLite {code})"))
}

fn case_status(value: CaseStatus) -> &'static str {
    match value {
        CaseStatus::Open => "open",
        CaseStatus::Suspended => "suspended",
        CaseStatus::Closed => "closed",
    }
}

fn parse_case_status(value: &str) -> Result<CaseStatus, AppError> {
    match value {
        "open" => Ok(CaseStatus::Open),
        "suspended" => Ok(CaseStatus::Suspended),
        "closed" => Ok(CaseStatus::Closed),
        _ => Err(storage_enum("case status")),
    }
}

fn assertion_origin(value: AssertionOrigin) -> &'static str {
    match value {
        AssertionOrigin::External => "external",
        AssertionOrigin::Human => "human",
        AssertionOrigin::Rule => "rule",
        AssertionOrigin::System => "system",
    }
}

fn parse_assertion_origin(value: &str) -> Result<AssertionOrigin, AppError> {
    match value {
        "external" => Ok(AssertionOrigin::External),
        "human" => Ok(AssertionOrigin::Human),
        "rule" => Ok(AssertionOrigin::Rule),
        "system" => Ok(AssertionOrigin::System),
        _ => Err(storage_enum("assertion origin")),
    }
}

fn claim_state(value: ClaimState) -> &'static str {
    match value {
        ClaimState::Observed => "observed",
        ClaimState::Extracted => "extracted",
        ClaimState::Inferred => "inferred",
        ClaimState::Corroborated => "corroborated",
        ClaimState::Disputed => "disputed",
        ClaimState::Contradicted => "contradicted",
        ClaimState::Superseded => "superseded",
        ClaimState::Verified => "verified",
        ClaimState::Rejected => "rejected",
        ClaimState::Unresolved => "unresolved",
    }
}

fn parse_claim_state(value: &str) -> Result<ClaimState, AppError> {
    match value {
        "observed" => Ok(ClaimState::Observed),
        "extracted" => Ok(ClaimState::Extracted),
        "inferred" => Ok(ClaimState::Inferred),
        "corroborated" => Ok(ClaimState::Corroborated),
        "disputed" => Ok(ClaimState::Disputed),
        "contradicted" => Ok(ClaimState::Contradicted),
        "superseded" => Ok(ClaimState::Superseded),
        "verified" => Ok(ClaimState::Verified),
        "rejected" => Ok(ClaimState::Rejected),
        "unresolved" => Ok(ClaimState::Unresolved),
        _ => Err(storage_enum("claim state")),
    }
}

fn verification_state(value: VerificationState) -> &'static str {
    match value {
        VerificationState::NotReviewed => "not_reviewed",
        VerificationState::Verified => "verified",
        VerificationState::Rejected => "rejected",
        VerificationState::Corrected => "corrected",
    }
}

fn parse_verification_state(value: &str) -> Result<VerificationState, AppError> {
    match value {
        "not_reviewed" => Ok(VerificationState::NotReviewed),
        "verified" => Ok(VerificationState::Verified),
        "rejected" => Ok(VerificationState::Rejected),
        "corrected" => Ok(VerificationState::Corrected),
        _ => Err(storage_enum("verification state")),
    }
}

fn evidence_type(value: EvidenceType) -> &'static str {
    match value {
        EvidenceType::ArtifactExcerpt => "artifact_excerpt",
        EvidenceType::StructuredField => "structured_field",
        EvidenceType::HumanAttestation => "human_attestation",
        EvidenceType::RuleResult => "rule_result",
    }
}

fn review_decision(value: ReviewDecision) -> &'static str {
    match value {
        ReviewDecision::Verified => "verified",
        ReviewDecision::Rejected => "rejected",
        ReviewDecision::Corrected => "corrected",
        ReviewDecision::NeedsMoreEvidence => "needs_more_evidence",
    }
}

fn parse_contradiction_status(value: &str) -> Result<ContradictionStatus, AppError> {
    match value {
        "unresolved" => Ok(ContradictionStatus::Unresolved),
        "resolved" => Ok(ContradictionStatus::Resolved),
        "superseded" => Ok(ContradictionStatus::Superseded),
        _ => Err(storage_enum("contradiction status")),
    }
}

fn parse_detection_method(value: &str) -> Result<DetectionMethod, AppError> {
    match value {
        "automatic" => Ok(DetectionMethod::Automatic),
        "human" => Ok(DetectionMethod::Human),
        _ => Err(storage_enum("detection method")),
    }
}

fn rule_result(value: RuleResult) -> &'static str {
    match value {
        RuleResult::Satisfied => "satisfied",
        RuleResult::NotSatisfied => "not_satisfied",
        RuleResult::Indeterminate => "indeterminate",
    }
}

fn parse_rule_result(value: &str) -> Result<RuleResult, AppError> {
    match value {
        "satisfied" => Ok(RuleResult::Satisfied),
        "not_satisfied" => Ok(RuleResult::NotSatisfied),
        "indeterminate" => Ok(RuleResult::Indeterminate),
        _ => Err(storage_enum("rule result")),
    }
}

fn obligation_status(value: ObligationStatus) -> &'static str {
    match value {
        ObligationStatus::Open => "open",
        ObligationStatus::Satisfied => "satisfied",
        ObligationStatus::Waived => "waived",
        ObligationStatus::Expired => "expired",
        ObligationStatus::Cancelled => "cancelled",
    }
}

fn parse_obligation_status(value: &str) -> Result<ObligationStatus, AppError> {
    match value {
        "open" => Ok(ObligationStatus::Open),
        "satisfied" => Ok(ObligationStatus::Satisfied),
        "waived" => Ok(ObligationStatus::Waived),
        "expired" => Ok(ObligationStatus::Expired),
        "cancelled" => Ok(ObligationStatus::Cancelled),
        _ => Err(storage_enum("obligation status")),
    }
}

fn temporal_precision(value: TemporalPrecision) -> &'static str {
    match value {
        TemporalPrecision::Instant => "instant",
        TemporalPrecision::Day => "day",
        TemporalPrecision::Month => "month",
        TemporalPrecision::Year => "year",
        TemporalPrecision::Before => "before",
        TemporalPrecision::After => "after",
        TemporalPrecision::Range => "range",
        TemporalPrecision::Unknown => "unknown",
    }
}

fn parse_temporal_precision(value: &str) -> Result<TemporalPrecision, AppError> {
    match value {
        "instant" => Ok(TemporalPrecision::Instant),
        "day" => Ok(TemporalPrecision::Day),
        "month" => Ok(TemporalPrecision::Month),
        "year" => Ok(TemporalPrecision::Year),
        "before" => Ok(TemporalPrecision::Before),
        "after" => Ok(TemporalPrecision::After),
        "range" => Ok(TemporalPrecision::Range),
        "unknown" => Ok(TemporalPrecision::Unknown),
        _ => Err(storage_enum("temporal precision")),
    }
}

fn task_status(value: TaskStatus) -> &'static str {
    match value {
        TaskStatus::Pending => "pending",
        TaskStatus::Ready => "ready",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Done => "done",
        TaskStatus::Cancelled => "cancelled",
    }
}

fn parse_task_status(value: &str) -> Result<TaskStatus, AppError> {
    match value {
        "pending" => Ok(TaskStatus::Pending),
        "ready" => Ok(TaskStatus::Ready),
        "in_progress" => Ok(TaskStatus::InProgress),
        "blocked" => Ok(TaskStatus::Blocked),
        "done" => Ok(TaskStatus::Done),
        "cancelled" => Ok(TaskStatus::Cancelled),
        _ => Err(storage_enum("task status")),
    }
}

fn storage_enum(name: &'static str) -> AppError {
    AppError::new(
        ErrorKind::Storage,
        format!("database contains an invalid {name}"),
    )
}
