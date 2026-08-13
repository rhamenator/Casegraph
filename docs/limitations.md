# Limitations and out-of-scope items

## Responsibly deferred foundation gaps

- PostgreSQL adapter and PostgreSQL migration path. SQLite was chosen for the user's low-dependency,
  no-service constraint; repository ports isolate this choice.
- Authentication, authorization enforcement, multi-tenancy, key management, and remote deployment
  hardening.
- Persistent provider/pipeline failure adapters beyond the current schema, safe sink contract, and
  tests; automatic retry scheduling is not implemented.
- Automatic correction-triggered rule reevaluation. Corrections calculate affected rule evaluation
  IDs, but scheduling/replacement policy is not implemented.
- Contradiction adjudication/resolution service; current contradictions are inspectable and history
  preserving.
- Entity/relationship/event/action/outcome application services. Canonical Rust types and database
  schema exist; current end-to-end slice centers evidence-to-task causality.
- PDF text/layout extraction, OCR/images, DOCX/XLSX, nested JSON mappings, non-UTF-8 text, directory
  recursion, email/cloud/API connectors, malware scanning, and sandboxed parsers.
- General rules DSL, uncertain/business-calendar deadline computation, event-driven obligations,
  task dependency/status mutation, BPM features, and automated external actions.
- Broad natural-language query understanding, semantic search, vector/graph databases, remote or
  local model adapters, and redaction implementations.
- Minimal human review web UI. CLI/API inspection was prioritized; no polished consumer UI exists.
- Production packaging, installers, signed binaries, benchmarks/load tests, recovery drills, and
  security certification.

## Explicit non-goals preserved

There is no benefits, medical, insurance, financial, legal, compliance, or government vertical; no
advice; no automated filing/payment/submission; no foundation-model training; no autonomous agent;
no microservices/Kubernetes/message broker/graph/vector database; and no claim of perfect OCR,
enterprise identity, or production compliance.

