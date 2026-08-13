# Software development plan

## Lifecycle model

Casegraph uses incremental vertical slices within a controlled requirements-driven lifecycle. A
slice is not complete until requirements, architecture/design, source, tests, traces, documentation,
problem reports, and configuration impact agree. Deterministic software owns invariants; optional AI
is an untrusted interpretation input and cannot approve or verify lifecycle data.

The development sequence for each change is:

1. authorize and review HLR/problem intent;
2. analyze architecture, interfaces, derived LLRs, and failure behavior;
3. define requirements-based test cases and expected results;
4. implement the smallest coherent source/migration change;
5. run verification and resolve failures without weakening tests;
6. review source, tests, traces, documentation, and configuration impact;
7. merge through controlled change and include only approved commits in a baseline.

## Development environment and methods

The controlled host-independent source environment is stable Rust 1.95, edition 2024, Cargo's
locked dependency graph, rustfmt, Clippy, Git, and GitHub Actions on Ubuntu. Bundled SQLite is
compiled through the locked Rust dependency graph. Developer hosts may differ, but a release records
host and target triples and repeats required checks in the controlled CI environment.

Architecture uses inward dependency direction, typed boundaries, transactional application
services, immutable evidence history, fixed-point material arithmetic, bounded external input, and
explicit uncertainty. Detailed design is maintained close to the implementation in domain types,
ports, migration constraints, module documentation, ADRs, and the controlled design documents named
by each trace record.

## Integration and transition criteria

Integration order is domain, application, infrastructure, delivery adapters, CLI/demo, then the
complete workspace. A unit advances when it formats, compiles warning free, passes its mapped tests,
and has reviewed traceability. The integrated baseline advances only when all required checks pass,
migration upgrade behavior is verified, problem reports are dispositioned, and SQA/configuration
reviews are recorded.

No runtime code generator or model-generated artifact is trusted as source of truth. Generated or
AI-assisted source is checked in, reviewed, traced, and verified exactly like human-authored source.
The generated executable is not yet verified for a target airborne computer; that remains a
certification transition gate.
