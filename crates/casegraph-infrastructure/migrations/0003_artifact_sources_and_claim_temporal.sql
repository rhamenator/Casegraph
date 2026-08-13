CREATE TABLE artifact_version_sources (
    artifact_version_id TEXT NOT NULL REFERENCES artifact_versions(id),
    source_id TEXT NOT NULL REFERENCES sources(id),
    associated_at_ms INTEGER NOT NULL,
    PRIMARY KEY(artifact_version_id, source_id)
) WITHOUT ROWID, STRICT;

CREATE INDEX idx_artifact_version_sources_source
ON artifact_version_sources(source_id, artifact_version_id);

ALTER TABLE claims ADD COLUMN temporal_json TEXT
    CHECK (temporal_json IS NULL OR json_valid(temporal_json));

