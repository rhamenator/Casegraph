CREATE INDEX idx_artifacts_case ON artifacts(case_id, created_at_ms);
CREATE INDEX idx_artifact_versions_artifact ON artifact_versions(artifact_id, version_number);
CREATE INDEX idx_provenance_artifact_version ON provenance_records(artifact_version_id);
CREATE INDEX idx_claims_case_predicate ON claims(case_id, predicate, subject_key);
CREATE INDEX idx_claim_state_changes_claim ON claim_state_changes(claim_id, changed_at_ms);
CREATE INDEX idx_evidence_edges_from ON evidence_edges(case_id, from_kind, from_id);
CREATE INDEX idx_contradictions_case_status ON contradictions(case_id, status);
CREATE INDEX idx_rule_evaluations_case ON rule_evaluations(case_id, evaluated_at_ms);
CREATE INDEX idx_obligations_case_status ON obligations(case_id, status);
CREATE INDEX idx_deadlines_case ON deadlines(case_id, due_earliest, due_latest);
CREATE INDEX idx_tasks_case_status ON workflow_tasks(case_id, status);
CREATE INDEX idx_audit_correlation ON audit_events(correlation_id, occurred_at_ms);
CREATE INDEX idx_pipeline_correlation ON pipeline_runs(correlation_id, started_at_ms);

CREATE TABLE domain_packages (
    package_id TEXT PRIMARY KEY CHECK (length(trim(package_id)) BETWEEN 1 AND 200),
    version TEXT NOT NULL CHECK (length(trim(version)) BETWEEN 1 AND 100),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    capabilities_json TEXT NOT NULL CHECK (json_valid(capabilities_json)),
    registered_at_ms INTEGER NOT NULL
) STRICT;

