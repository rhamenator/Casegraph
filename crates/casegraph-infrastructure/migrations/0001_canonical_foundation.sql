CREATE TABLE cases (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 300),
    status TEXT NOT NULL CHECK (status IN ('open', 'suspended', 'closed')),
    created_at_ms INTEGER NOT NULL,
    closed_at_ms INTEGER,
    metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata_json)),
    CHECK (closed_at_ms IS NULL OR closed_at_ms >= created_at_ms)
) STRICT;

CREATE TABLE sources (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id),
    connector TEXT NOT NULL CHECK (length(trim(connector)) BETWEEN 1 AND 100),
    locator TEXT NOT NULL CHECK (length(trim(locator)) BETWEEN 1 AND 2048),
    external_record_id TEXT,
    endpoint TEXT,
    source_revision TEXT,
    retrieved_at_ms INTEGER NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata_json)),
    UNIQUE(case_id, connector, locator, source_revision)
) STRICT;

CREATE TABLE artifacts (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id),
    source_id TEXT NOT NULL REFERENCES sources(id),
    source_key TEXT NOT NULL CHECK (length(trim(source_key)) BETWEEN 1 AND 2048),
    created_at_ms INTEGER NOT NULL,
    UNIQUE(case_id, source_key)
) STRICT;

CREATE TABLE artifact_versions (
    id TEXT PRIMARY KEY,
    artifact_id TEXT NOT NULL REFERENCES artifacts(id),
    version_number INTEGER NOT NULL CHECK (version_number > 0),
    content_sha256 TEXT NOT NULL CHECK (
        length(content_sha256) = 64
        AND content_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    content_length INTEGER NOT NULL CHECK (content_length >= 0),
    media_type TEXT NOT NULL CHECK (length(trim(media_type)) BETWEEN 1 AND 255),
    storage_key TEXT NOT NULL CHECK (length(trim(storage_key)) BETWEEN 1 AND 255),
    ingested_at_ms INTEGER NOT NULL,
    received_at_ms INTEGER,
    original_filename TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata_json)),
    UNIQUE(artifact_id, version_number),
    UNIQUE(artifact_id, content_sha256)
) STRICT;

CREATE TRIGGER artifact_versions_no_update
BEFORE UPDATE ON artifact_versions
BEGIN
    SELECT RAISE(ABORT, 'artifact versions are immutable');
END;

CREATE TRIGGER artifact_versions_no_delete
BEFORE DELETE ON artifact_versions
BEGIN
    SELECT RAISE(ABORT, 'artifact versions are immutable');
END;

CREATE TABLE provenance_records (
    id TEXT PRIMARY KEY,
    artifact_version_id TEXT REFERENCES artifact_versions(id),
    connector TEXT,
    endpoint TEXT,
    external_record_id TEXT,
    source_field TEXT,
    page_number INTEGER CHECK (page_number IS NULL OR page_number > 0),
    paragraph_number INTEGER CHECK (paragraph_number IS NULL OR paragraph_number > 0),
    text_span_start INTEGER CHECK (text_span_start IS NULL OR text_span_start >= 0),
    text_span_end INTEGER CHECK (text_span_end IS NULL OR text_span_end >= 0),
    table_number INTEGER CHECK (table_number IS NULL OR table_number > 0),
    row_number INTEGER CHECK (row_number IS NULL OR row_number >= 0),
    column_number INTEGER CHECK (column_number IS NULL OR column_number >= 0),
    bounding_region_json TEXT CHECK (bounding_region_json IS NULL OR json_valid(bounding_region_json)),
    extraction_method TEXT NOT NULL CHECK (length(trim(extraction_method)) BETWEEN 1 AND 100),
    extractor_name TEXT NOT NULL CHECK (length(trim(extractor_name)) BETWEEN 1 AND 200),
    extractor_version TEXT NOT NULL CHECK (length(trim(extractor_version)) BETWEEN 1 AND 100),
    model_provider TEXT,
    model_name TEXT,
    model_version TEXT,
    model_configuration_json TEXT CHECK (
        model_configuration_json IS NULL OR json_valid(model_configuration_json)
    ),
    extracted_at_ms INTEGER NOT NULL,
    confidence REAL CHECK (confidence IS NULL OR confidence BETWEEN 0.0 AND 1.0),
    verification_state TEXT NOT NULL CHECK (
        verification_state IN ('not_reviewed', 'verified', 'rejected', 'corrected')
    ),
    original_representation TEXT,
    correlation_id TEXT NOT NULL,
    CHECK (
        text_span_start IS NULL OR text_span_end IS NULL OR text_span_end >= text_span_start
    ),
    CHECK (
        model_provider IS NOT NULL OR (model_name IS NULL AND model_version IS NULL)
    )
) STRICT;

CREATE TRIGGER provenance_records_no_update
BEFORE UPDATE ON provenance_records
BEGIN
    SELECT RAISE(ABORT, 'provenance records are immutable; append verification or correction');
END;

CREATE TRIGGER provenance_records_no_delete
BEFORE DELETE ON provenance_records
BEGIN
    SELECT RAISE(ABORT, 'provenance records are immutable');
END;

CREATE TABLE entities (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id),
    entity_type TEXT NOT NULL CHECK (length(trim(entity_type)) BETWEEN 1 AND 200),
    display_name TEXT NOT NULL CHECK (length(trim(display_name)) BETWEEN 1 AND 500),
    origin TEXT NOT NULL CHECK (origin IN ('external', 'human', 'rule', 'system')),
    primary_provenance_id TEXT REFERENCES provenance_records(id),
    created_at_ms INTEGER NOT NULL,
    attributes_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(attributes_json)),
    CHECK (origin <> 'external' OR primary_provenance_id IS NOT NULL)
) STRICT;

CREATE TABLE observations (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id),
    subject_id TEXT REFERENCES entities(id),
    predicate TEXT NOT NULL CHECK (length(trim(predicate)) BETWEEN 1 AND 300),
    original_value TEXT NOT NULL,
    normalized_value_json TEXT CHECK (
        normalized_value_json IS NULL OR json_valid(normalized_value_json)
    ),
    provenance_id TEXT NOT NULL REFERENCES provenance_records(id),
    extraction_confidence REAL CHECK (
        extraction_confidence IS NULL OR extraction_confidence BETWEEN 0.0 AND 1.0
    ),
    observed_at_ms INTEGER NOT NULL
) STRICT;

CREATE TRIGGER observations_no_update
BEFORE UPDATE ON observations
BEGIN
    SELECT RAISE(ABORT, 'observations are immutable');
END;

CREATE TRIGGER observations_no_delete
BEFORE DELETE ON observations
BEGIN
    SELECT RAISE(ABORT, 'observations are immutable');
END;

CREATE TABLE claims (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id),
    subject_id TEXT REFERENCES entities(id),
    subject_key TEXT NOT NULL CHECK (length(trim(subject_key)) BETWEEN 1 AND 300),
    predicate TEXT NOT NULL CHECK (length(trim(predicate)) BETWEEN 1 AND 300),
    original_value TEXT NOT NULL,
    normalized_value_json TEXT NOT NULL CHECK (json_valid(normalized_value_json)),
    origin TEXT NOT NULL CHECK (origin IN ('external', 'human', 'rule', 'system')),
    initial_state TEXT NOT NULL CHECK (
        initial_state IN (
            'observed', 'extracted', 'inferred', 'corroborated', 'disputed',
            'contradicted', 'superseded', 'verified', 'rejected', 'unresolved'
        )
    ),
    primary_provenance_id TEXT REFERENCES provenance_records(id),
    interpretation_confidence REAL CHECK (
        interpretation_confidence IS NULL OR interpretation_confidence BETWEEN 0.0 AND 1.0
    ),
    event_earliest TEXT,
    event_latest TEXT,
    temporal_precision TEXT CHECK (
        temporal_precision IS NULL OR temporal_precision IN (
            'instant', 'day', 'month', 'year', 'before', 'after', 'range', 'unknown'
        )
    ),
    created_at_ms INTEGER NOT NULL,
    CHECK (origin <> 'external' OR primary_provenance_id IS NOT NULL)
) STRICT;

CREATE TABLE claim_state_changes (
    id TEXT PRIMARY KEY,
    claim_id TEXT NOT NULL REFERENCES claims(id),
    state TEXT NOT NULL CHECK (
        state IN (
            'observed', 'extracted', 'inferred', 'corroborated', 'disputed',
            'contradicted', 'superseded', 'verified', 'rejected', 'unresolved'
        )
    ),
    actor TEXT NOT NULL,
    reason TEXT,
    changed_at_ms INTEGER NOT NULL,
    correlation_id TEXT NOT NULL
) STRICT;

CREATE TABLE facts (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id),
    claim_id TEXT NOT NULL UNIQUE REFERENCES claims(id),
    established_value_json TEXT NOT NULL CHECK (json_valid(established_value_json)),
    established_at_ms INTEGER NOT NULL,
    established_by TEXT NOT NULL,
    verification_state TEXT NOT NULL CHECK (verification_state IN ('verified', 'rejected'))
) STRICT;

CREATE TABLE relationships (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id),
    from_entity_id TEXT NOT NULL REFERENCES entities(id),
    relationship_type TEXT NOT NULL CHECK (length(trim(relationship_type)) BETWEEN 1 AND 200),
    to_entity_id TEXT NOT NULL REFERENCES entities(id),
    confidence REAL CHECK (confidence IS NULL OR confidence BETWEEN 0.0 AND 1.0),
    origin TEXT NOT NULL CHECK (origin IN ('external', 'human', 'rule', 'system')),
    primary_provenance_id TEXT REFERENCES provenance_records(id),
    valid_earliest TEXT,
    valid_latest TEXT,
    created_at_ms INTEGER NOT NULL,
    CHECK (from_entity_id <> to_entity_id),
    CHECK (origin <> 'external' OR primary_provenance_id IS NOT NULL)
) STRICT;

CREATE TABLE events (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id),
    event_type TEXT NOT NULL CHECK (length(trim(event_type)) BETWEEN 1 AND 200),
    label TEXT NOT NULL CHECK (length(trim(label)) BETWEEN 1 AND 500),
    event_earliest TEXT,
    event_latest TEXT,
    effective_date TEXT,
    reported_date TEXT,
    received_date TEXT,
    temporal_precision TEXT NOT NULL CHECK (
        temporal_precision IN ('instant', 'day', 'month', 'year', 'before', 'after', 'range', 'unknown')
    ),
    origin TEXT NOT NULL CHECK (origin IN ('external', 'human', 'rule', 'system')),
    primary_provenance_id TEXT REFERENCES provenance_records(id),
    created_at_ms INTEGER NOT NULL,
    CHECK (origin <> 'external' OR primary_provenance_id IS NOT NULL)
) STRICT;

CREATE TABLE event_entities (
    event_id TEXT NOT NULL REFERENCES events(id),
    entity_id TEXT NOT NULL REFERENCES entities(id),
    role TEXT NOT NULL CHECK (length(trim(role)) BETWEEN 1 AND 200),
    PRIMARY KEY(event_id, entity_id, role)
) WITHOUT ROWID, STRICT;

CREATE TABLE evidence (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id),
    evidence_type TEXT NOT NULL CHECK (
        evidence_type IN ('artifact_excerpt', 'structured_field', 'human_attestation', 'rule_result')
    ),
    provenance_id TEXT REFERENCES provenance_records(id),
    description TEXT NOT NULL CHECK (length(trim(description)) BETWEEN 1 AND 1000),
    created_at_ms INTEGER NOT NULL,
    CHECK (evidence_type = 'rule_result' OR provenance_id IS NOT NULL)
) STRICT;

CREATE TABLE evidence_edges (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id),
    from_kind TEXT NOT NULL CHECK (
        from_kind IN ('claim', 'event', 'obligation', 'deadline', 'task', 'rule_evaluation')
    ),
    from_id TEXT NOT NULL,
    relationship_type TEXT NOT NULL CHECK (
        relationship_type IN (
            'supported_by', 'contradicted_by', 'corroborates', 'contradicts',
            'supersedes', 'involves', 'created_by', 'applies_to', 'satisfies', 'produces'
        )
    ),
    to_kind TEXT NOT NULL CHECK (
        to_kind IN ('evidence', 'claim', 'entity', 'event', 'obligation', 'outcome')
    ),
    to_id TEXT NOT NULL,
    confidence REAL CHECK (confidence IS NULL OR confidence BETWEEN 0.0 AND 1.0),
    created_at_ms INTEGER NOT NULL,
    UNIQUE(from_kind, from_id, relationship_type, to_kind, to_id)
) STRICT;

CREATE TABLE contradictions (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id),
    claim_a_id TEXT NOT NULL REFERENCES claims(id),
    claim_b_id TEXT NOT NULL REFERENCES claims(id),
    status TEXT NOT NULL CHECK (status IN ('unresolved', 'resolved', 'superseded')),
    detection_method TEXT NOT NULL CHECK (detection_method IN ('automatic', 'human')),
    rationale TEXT,
    resolution_claim_id TEXT REFERENCES claims(id),
    adjudicated_by TEXT,
    created_at_ms INTEGER NOT NULL,
    resolved_at_ms INTEGER,
    CHECK (claim_a_id < claim_b_id),
    CHECK (status <> 'resolved' OR (rationale IS NOT NULL AND adjudicated_by IS NOT NULL)),
    UNIQUE(claim_a_id, claim_b_id)
) STRICT;

CREATE TABLE human_reviews (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id),
    target_kind TEXT NOT NULL CHECK (
        target_kind IN ('claim', 'contradiction', 'rule_evaluation', 'provenance')
    ),
    target_id TEXT NOT NULL,
    decision TEXT NOT NULL CHECK (
        decision IN ('verified', 'rejected', 'corrected', 'needs_more_evidence')
    ),
    actor TEXT NOT NULL CHECK (length(trim(actor)) BETWEEN 1 AND 300),
    rationale TEXT,
    reviewed_at_ms INTEGER NOT NULL,
    correlation_id TEXT NOT NULL
) STRICT;

CREATE TABLE corrections (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id),
    original_claim_id TEXT NOT NULL REFERENCES claims(id),
    corrected_claim_id TEXT NOT NULL UNIQUE REFERENCES claims(id),
    review_id TEXT NOT NULL UNIQUE REFERENCES human_reviews(id),
    provenance_id TEXT NOT NULL REFERENCES provenance_records(id),
    original_value_json TEXT NOT NULL CHECK (json_valid(original_value_json)),
    corrected_value_json TEXT NOT NULL CHECK (json_valid(corrected_value_json)),
    actor TEXT NOT NULL,
    rationale TEXT,
    corrected_at_ms INTEGER NOT NULL,
    affected_derivations_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(affected_derivations_json)),
    CHECK (original_claim_id <> corrected_claim_id)
) STRICT;

CREATE TRIGGER corrections_no_update
BEFORE UPDATE ON corrections
BEGIN
    SELECT RAISE(ABORT, 'corrections preserve history and are immutable');
END;

CREATE TRIGGER corrections_no_delete
BEFORE DELETE ON corrections
BEGIN
    SELECT RAISE(ABORT, 'corrections preserve history and are immutable');
END;

CREATE TABLE rules (
    id TEXT PRIMARY KEY,
    package_id TEXT NOT NULL,
    stable_key TEXT NOT NULL,
    title TEXT NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 500),
    created_at_ms INTEGER NOT NULL,
    UNIQUE(package_id, stable_key)
) STRICT;

CREATE TABLE rule_versions (
    id TEXT PRIMARY KEY,
    rule_id TEXT NOT NULL REFERENCES rules(id),
    version INTEGER NOT NULL CHECK (version > 0),
    definition_json TEXT NOT NULL CHECK (json_valid(definition_json)),
    definition_sha256 TEXT NOT NULL CHECK (length(definition_sha256) = 64),
    effective_from TEXT,
    effective_until TEXT,
    created_at_ms INTEGER NOT NULL,
    UNIQUE(rule_id, version)
) STRICT;

CREATE TABLE rule_evaluations (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id),
    rule_version_id TEXT NOT NULL REFERENCES rule_versions(id),
    inputs_json TEXT NOT NULL CHECK (json_valid(inputs_json)),
    inputs_sha256 TEXT NOT NULL CHECK (length(inputs_sha256) = 64),
    result TEXT NOT NULL CHECK (result IN ('satisfied', 'not_satisfied', 'indeterminate')),
    result_json TEXT NOT NULL CHECK (json_valid(result_json)),
    explanation TEXT NOT NULL,
    evaluated_at_ms INTEGER NOT NULL,
    evaluator_version TEXT NOT NULL,
    correlation_id TEXT NOT NULL,
    UNIQUE(case_id, rule_version_id, inputs_sha256)
) STRICT;

CREATE TABLE rule_evaluation_evidence (
    rule_evaluation_id TEXT NOT NULL REFERENCES rule_evaluations(id),
    evidence_id TEXT NOT NULL REFERENCES evidence(id),
    PRIMARY KEY(rule_evaluation_id, evidence_id)
) WITHOUT ROWID, STRICT;

CREATE TABLE obligations (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id),
    created_by_event_id TEXT REFERENCES events(id),
    created_by_rule_evaluation_id TEXT REFERENCES rule_evaluations(id),
    kind TEXT NOT NULL CHECK (length(trim(kind)) BETWEEN 1 AND 200),
    description TEXT NOT NULL CHECK (length(trim(description)) BETWEEN 1 AND 1000),
    status TEXT NOT NULL CHECK (status IN ('open', 'satisfied', 'waived', 'expired', 'cancelled')),
    created_at_ms INTEGER NOT NULL,
    CHECK (created_by_event_id IS NOT NULL OR created_by_rule_evaluation_id IS NOT NULL)
) STRICT;

CREATE TABLE deadlines (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id),
    obligation_id TEXT NOT NULL REFERENCES obligations(id),
    due_earliest TEXT,
    due_latest TEXT,
    original_expression TEXT NOT NULL,
    temporal_precision TEXT NOT NULL CHECK (
        temporal_precision IN ('instant', 'day', 'month', 'year', 'before', 'after', 'range', 'unknown')
    ),
    calculation_json TEXT NOT NULL CHECK (json_valid(calculation_json)),
    created_at_ms INTEGER NOT NULL,
    CHECK (due_earliest IS NOT NULL OR due_latest IS NOT NULL)
) STRICT;

CREATE TABLE workflow_tasks (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id),
    obligation_id TEXT REFERENCES obligations(id),
    title TEXT NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 500),
    status TEXT NOT NULL CHECK (status IN ('pending', 'ready', 'in_progress', 'blocked', 'done', 'cancelled')),
    created_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER
) STRICT;

CREATE TABLE task_dependencies (
    task_id TEXT NOT NULL REFERENCES workflow_tasks(id),
    depends_on_task_id TEXT NOT NULL REFERENCES workflow_tasks(id),
    PRIMARY KEY(task_id, depends_on_task_id),
    CHECK (task_id <> depends_on_task_id)
) WITHOUT ROWID, STRICT;

CREATE TABLE actions (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id),
    task_id TEXT REFERENCES workflow_tasks(id),
    action_type TEXT NOT NULL CHECK (length(trim(action_type)) BETWEEN 1 AND 200),
    actor TEXT NOT NULL,
    input_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(input_json)),
    performed_at_ms INTEGER NOT NULL,
    correlation_id TEXT NOT NULL
) STRICT;

CREATE TABLE outcomes (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id),
    action_id TEXT NOT NULL REFERENCES actions(id),
    outcome_type TEXT NOT NULL CHECK (length(trim(outcome_type)) BETWEEN 1 AND 200),
    result_json TEXT NOT NULL CHECK (json_valid(result_json)),
    recorded_at_ms INTEGER NOT NULL,
    UNIQUE(action_id)
) STRICT;

CREATE TABLE audit_events (
    id TEXT PRIMARY KEY,
    case_id TEXT REFERENCES cases(id),
    operation TEXT NOT NULL CHECK (length(trim(operation)) BETWEEN 1 AND 200),
    actor TEXT NOT NULL CHECK (length(trim(actor)) BETWEEN 1 AND 300),
    target_kind TEXT NOT NULL,
    target_id TEXT NOT NULL,
    previous_state_json TEXT CHECK (previous_state_json IS NULL OR json_valid(previous_state_json)),
    resulting_state_json TEXT NOT NULL CHECK (json_valid(resulting_state_json)),
    reason TEXT,
    occurred_at_ms INTEGER NOT NULL,
    correlation_id TEXT NOT NULL
) STRICT;

CREATE TRIGGER audit_events_no_update
BEFORE UPDATE ON audit_events
BEGIN
    SELECT RAISE(ABORT, 'audit events are append-only');
END;

CREATE TRIGGER audit_events_no_delete
BEFORE DELETE ON audit_events
BEGIN
    SELECT RAISE(ABORT, 'audit events are append-only');
END;

CREATE TABLE pipeline_runs (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id),
    artifact_version_id TEXT NOT NULL REFERENCES artifact_versions(id),
    stage TEXT NOT NULL CHECK (
        stage IN (
            'ingestion', 'classification', 'raw_extraction', 'structural_extraction',
            'semantic_extraction', 'normalization', 'validation', 'evidence_creation'
        )
    ),
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'completed_with_warnings', 'failed')),
    started_at_ms INTEGER NOT NULL,
    finished_at_ms INTEGER,
    correlation_id TEXT NOT NULL,
    CHECK (finished_at_ms IS NULL OR finished_at_ms >= started_at_ms)
) STRICT;

CREATE TABLE pipeline_failures (
    id TEXT PRIMARY KEY,
    pipeline_run_id TEXT NOT NULL REFERENCES pipeline_runs(id),
    failure_kind TEXT NOT NULL CHECK (
        failure_kind IN (
            'unreadable_artifact', 'unsupported_format', 'extraction_warning',
            'no_observations', 'validation_rejected', 'provider_unavailable',
            'missing_rule_facts', 'internal'
        )
    ),
    safe_message TEXT NOT NULL,
    retryable INTEGER NOT NULL CHECK (retryable IN (0, 1)),
    occurred_at_ms INTEGER NOT NULL,
    details_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(details_json))
) STRICT;

