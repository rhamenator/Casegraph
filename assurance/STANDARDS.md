# Software lifecycle standards

## Requirements standard

Requirements use `CG-HLR-NNN` or `CG-LLR-NNN`, one identifier per behavior. They use “shall,” state
observable behavior and conditions, avoid implementation detail at HLR level, avoid ambiguous
qualifiers, and are feasible, testable, consistent, and traceable. LLRs identify one HLR parent.
Derived LLRs are marked and reviewed for upstream safety/product impact. Changes preserve identifier
history; identifiers are never reassigned to different intent.

## Design standard

Design preserves the modular-monolith dependency direction documented in `docs/architecture.md`.
Domain and application code do not depend on adapters. Interfaces specify validation, error,
transaction, determinism, provenance, concurrency, resource, and failure behavior. Material data
flows identify their evidence and audit consequences. Decisions that alter boundaries, persistence,
dependencies, trust, or determinism require an ADR and requirements impact analysis.

## Rust coding standard

- Stable Rust 1.95 and edition 2024 are controlled by `rust-toolchain.toml`.
- `unsafe` is forbidden throughout the workspace; warnings, `todo!`, and debug macros are denied.
- Material money and decimals use fixed-point types; case facts do not use floating point.
- Domain identifiers and untrusted strings are bounded and validated at entry boundaries.
- Panics and unchecked indexing are prohibited in reachable production paths for untrusted input.
  `unwrap`/`expect` are acceptable in tests or for compile-time/static invariants with clear text.
- Errors are typed or mapped deliberately and must not expose source content or secrets.
- Deterministic logic cannot depend on hash iteration order, ambient locale, wall clock, random
  ordering, model output, or platform-specific formatting.
- Every public item communicates invariants through types, validation, or concise documentation.
- New external crates require the dependency ADR process and assurance impact review.

## SQL and persistence standard

SQL is parameterized. Foreign keys and constraints enforce critical invariants at the storage
boundary. State-changing use cases are transactional. Evidentiary history is append only. Applied
migrations are never edited; numbered forward migrations include clean, upgrade, rollback/recovery
analysis, and invariant tests. SQLite-specific design remains behind repository ports.

## Verification and review standard

Tests follow `VERIFICATION_PLAN.md`. Code review checks the applicable requirement, architecture,
data/control flow, nominal and robustness behavior, resource bounds, error handling, provenance,
security, determinism, concurrency, migration compatibility, dead/deactivated code, traceability,
and documentation accuracy. Generated or AI-assisted changes have the same author responsibility
and independent-review burden as hand-written changes; AI output is never verification evidence.
