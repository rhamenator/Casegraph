# Foundation deliverables and acceptance evidence

This checklist reflects the current repository, not the long-term vision. “Boundary” means the
schema/type/port exists but the application service is responsibly deferred.

## Required deliverables

1. **Documented modular architecture:** implemented in `docs/architecture.md` and ADRs.
2. **Canonical domain model:** Rust types and SQLite schema cover every named canonical concept;
   entity/relationship/event/action/outcome services remain boundaries.
3. **Schema and migrations:** four checksummed, source-controlled SQLite migrations with clean,
   idempotent, drift, immutability, and version-1-to-latest tests.
4. **Immutable ingestion:** content-addressed SHA-256 filesystem store, exact duplicate reuse,
   append-only versions, atomic writes, integrity re-check, and recoverable bytes.
5. **Pluggable extraction/normalization:** staged interface plus deterministic UTF-8 text, flat JSON,
   and CSV; original values/locations preserved.
6. **Provenance/evidence/contradictions/corrections/review:** implemented transactionally with
   append-only history and real-adapter integration tests.
7. **Provider-neutral reasoning:** optional local/remote interface, disabled default, locality policy,
   strict schema validation, and safe failure sink. No concrete provider ships.
8. **Versioned deterministic rules:** equality conjunctions, definition/input hashes, exact version,
   idempotent reproducible evaluations, explanations, and evidence links.
9. **Workflow model:** case/obligation/deadline/task creation is implemented; dependency/action/
   outcome schema and types exist, but mutation services are deferred.
10. **Sample package:** artificial package contributes one synthetic rule through a core-owned
    registry; empty core registry is tested.
11. **Grounded querying:** deterministic established/claimed/suggested/conflicting/unknown renderer
    with stored IDs only and no model/world-knowledge fallback.
12. **Versioned API and CLI:** shared services, `/api/v1`, checked-in/served OpenAPI, raw ingestion,
    extraction, cases, artifacts, claims, provenance, contradictions, reviews/corrections, rules,
    tasks/workflow, queries, useful CLI, smoke test, and real HTTP/process tests.
13. **Test/evaluation:** unit, malformed-input, security, migration, persistence, integration, API,
    CLI, package-isolation, provider-isolation, regression fixture, and doc tests; separate evaluation
    metrics are documented.
14. **Synthetic end-to-end demo:** `casegraph demo` covers two immutable artifact versions through a
    grounded workflow answer using invented data and no model provider.
15. **Secure configuration/diagnostics/threat assumptions:** validated environment configuration,
    loopback/size/model defaults, content-minimized correlated diagnostic type, health endpoints, and
    `docs/security.md`/`docs/operations.md`.
16. **Developer/architecture documentation:** README, setup, glossary, provenance, pipeline,
    extension, rule/query, API/CLI, testing/evaluation, security, operations, dependency policy,
    limitations, plan, ADRs, and this checklist.
17. **CI:** formatting, warning-denied Clippy, locked workspace tests, and migration checks on push/PR;
    Dependabot covers Cargo and GitHub Actions.

## Acceptance evidence

- Setup and checks are documented and use the pinned Rust toolchain/lockfile.
- Integration tests recover exact source bytes and prove duplicate/new-version identity/hash rules.
- Pipeline and integration tests recover original representation plus artifact/field/span/row/column
  provenance for every externally derived sample claim.
- Conflicting claims coexist and yield one contradiction; equal values corroborate.
- Correction tests preserve original/corrected claims and identify dependent rule evaluations.
- Rule tests preserve exact version/inputs/hash/result/explanation/evidence and return the same
  materialization for identical inputs/version.
- A satisfied evaluation creates an evidence-linked obligation, exact deadline calculation, and task;
  missing facts create an indeterminate evaluation and no task.
- Query tests exercise established, conflicting, and unknown results; the renderer implements claimed
  and suggested modes from stored state without fact invention.
- Core/package/provider tests run with no model and with an empty domain registry.
- Malformed model output is rejected and recorded by a content-free failure sink.
- API and CLI compose the same services; tests use a real loopback HTTP connection and a real CLI
  process.
- Documentation explicitly lists deferred gaps and does not claim production readiness.

