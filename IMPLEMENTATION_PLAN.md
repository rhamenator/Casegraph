# Casegraph Foundation Implementation Plan

This plan is maintained alongside the implementation. A slice is complete only when its code,
tests, checks, and documentation agree. The authoritative requirements remain
`CASEGRAPH_FOUNDATION_SPEC.md`.

## Constraints and decisions

- Build a Rust modular monolith. Rust 1.95 and Cargo are available locally; no C++ compiler is on
  `PATH`.
- Use embedded SQLite as the authoritative persistence layer in the foundation. It remains behind
  repository ports so a future PostgreSQL adapter does not alter domain or application code.
- Keep a small, exact-version dependency budget. Every direct crate requires a written rationale,
  license check, reduced feature set, lockfile, and CI coverage. Keep domain and application
  services independent of HTTP, CLI, persistence details, and optional AI providers.
- Store artifact bytes behind an `ArtifactStore` boundary; store immutable identity, version,
  digest, and metadata in the embedded store.
- Use explicit typed records for stable concepts and canonical JSON for extensible values and
  reproducibility snapshots.
- Treat API and CLI as adapters over the same application services.
- Ship no production vertical and no polished UI. The artificial `sample-administrative-case`
  package exists only to verify extension and end-to-end behavior.

## Slices

### 1. Workspace and persistence skeleton — complete

Capability: reproducible local development and schema evolution.

- Initialize the Cargo workspace and Git repository.
- Add configuration validation, structured diagnostics, and a safe example environment.
- Add the initial SQLite schema migration with constrained identifiers, explicit temporal fields,
  provenance, evidence graph, contradictions, corrections/reviews, versioned rules, workflows,
  audit events, and pipeline failures.
- Test clean migration, checksum/immutability behavior, and supported upgrade behavior.
- Add CI gates for formatting, Clippy, tests, and migrations.

### 2. Evidence integrity core — complete

Invariants: source bytes are immutable; every externally derived material claim has recoverable
provenance; duplicate ingestion is idempotent; conflicting claims coexist; corrections preserve
history.

- Implement filesystem artifact storage with path confinement, SHA-256 hashing, atomic writes,
  duplicate detection, stable artifact identity, and append-only versions.
- Implement cases, observations, claims, evidence links, provenance locations, contradictions,
  human reviews, corrections, and audit events.
- Add unit, persistence, property/invariant, malformed-input, and path-traversal tests.

### 3. Deterministic pipeline and reasoning boundary — complete

Capability: deterministic text/JSON/CSV extraction, normalization, validation, and evidence
creation without an AI provider.

- Implement staged pipeline contracts with versioned extractors/normalizers.
- Preserve original representations and source locations while creating normalized values.
- Add a provider-neutral reasoning interface, disabled provider, provider policy, schema-validated
  output, and safe failure recording.
- Measure extraction, normalization, provenance completeness, contradiction detection, and
  unsupported-claim rate on synthetic fixtures.

### 4. Rules, workflow, grounding, and sample extension — complete

Invariants: rule evaluations are reproducible; workflow work preserves its causal explanation;
case queries never invent case-specific facts; core behavior is independent of the sample package.

- Implement a deliberately small, versioned deterministic predicate/action rule abstraction.
- Materialize obligation, deadline, and task records from an evaluation, retaining evidence links.
- Implement deterministic grounded query intents and five epistemic answer modes: established,
  claimed, suggested, conflicting, and unknown.
- Add the artificial sample domain package through a registry boundary and prove core startup/tests
  with it disabled.

### 5. API, CLI, diagnostics, and end-to-end demonstration — complete

Capability: shared application behavior through `/api/v1` and `casegraph`, with inspectable
failures and health.

- Expose implemented artifacts, cases, claims, evidence, contradictions, rules, tasks, reviews,
  corrections, and grounded queries through the versioned API.
- Provide useful `init`, `case create`, `ingest`, list, query, verify, correct, demo, and test CLI
  commands over the same services.
- Add correlation-aware structured diagnostics plus liveness and dependency-aware readiness.
- Add API and CLI contract tests and a runnable synthetic end-to-end scenario.

### 6. Verification and handoff — complete

- Complete README, setup, glossary, provenance/evidence semantics, pipeline contracts, extension
  guide, API/CLI guide, testing/evaluation guide, security/privacy/threat assumptions, operations,
  limitations, out-of-scope list, and ADRs.
- Run formatting, Clippy with warnings denied, the full test suite, clean/upgrade migration tests,
  evaluation thresholds, dependency audit where available, and the documented demo.
- Reconcile every required deliverable and acceptance criterion against evidence from checks.

### 7. DO-178C-aligned lifecycle controls — in progress

Capability: requirements-driven development and reviewable assurance evidence from the beginning of
the maintained product lifecycle, without an unsupported certification claim.

- Establish tailored assurance, verification, configuration-management, quality-assurance, and
  lifecycle standards with explicit DAL/certification transition gates.
- Baseline identified HLRs/LLRs and bidirectional design/source/test traces for implemented behavior.
- Control configuration items and problem reports, add contribution/review records, and pin CI tools.
- Add a dependency-free assurance-data validator and enforce it in CI.

## Explicitly deferred unless a foundational requirement forces them

- OCR, PDF layout extraction, DOCX/XLSX, non-filesystem connectors, remote model integrations,
  authentication/enterprise identity, multi-tenancy, a general rule DSL, a BPM designer, a vector
  or graph database, microservices, autonomous actions, and a polished review UI.
