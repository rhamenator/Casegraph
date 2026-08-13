# Provenance and evidence semantics

## Invariant

Every externally derived material claim must reference a persisted `ProvenanceRecord`. The Rust
domain validates this before the repository call and the SQLite schema repeats the constraint. The
service creates provenance, observation, claim, evidence, evidence edge, contradiction/corroboration
effects, and audit event in one database transaction.

Document provenance can record artifact version/hash through its reference, page, paragraph, text
span, table/row/column, bounding region, original representation, extraction method, extractor and
version, extraction time, optional meaningful confidence, and human verification state. Structured
provenance supports connector, endpoint, external record, source field, retrieval identity, and
source revision through `Source` and `ProvenanceRecord`.

Model name/provider/version/configuration are mandatory attribution when a model contributes. Model
metadata without a provider is rejected. Model output never becomes authoritative merely because it
passed schema validation.

## Immutable artifacts

Artifact bytes are SHA-256 content addressed. Writes use a new temporary file, flush it, and rename
it into a digest-derived path. Caller filenames never influence storage paths. Existing content is
read and re-hashed before reuse; corruption fails rather than being overwritten. A repeated source
key and hash returns the prior artifact version. A changed hash appends a version. Source bytes can
be recovered by artifact-version ID and are verified again on read.

## Claims, facts, and contradictions

“Source says X” is a claim in `extracted` state with provenance. “X is established” requires a
verified claim and a separate fact. Safe automatic contradiction detection currently applies only
when case, subject key, and predicate match and both normalized values are known but unequal. Equal
known values create a `corroborates` graph edge. Unknown values are not contradictions.

Contradictions preserve both claims and add bidirectional graph edges. Resolution is not yet exposed
as an application service. Corrections append a human claim and link it with `supersedes`; they do
not edit or delete the original observation, claim, provenance, evidence, or contradiction.

## Audit versus provenance

Provenance explains where an assertion came from. Audit events explain state-changing operations,
actors, time, reason, and correlation. Audit result snapshots contain identifiers and state changes,
not source document bytes or claim values. Operational diagnostics likewise must never include
routine source contents.

