# ADR 0002: Minimal vetted dependency budget

- Status: accepted
- Date: 2026-08-12

## Context

The user prefers not to depend on outside software but permits a very small number of carefully
vetted Rust crates. Hand-writing cryptography, JSON, SQLite bindings, or an HTTP parser would create
greater correctness and security risk than using established libraries.

## Decision

Allow six exact-version direct crates: Serde, serde_json, SHA-2, rusqlite, Axum, and Tokio. Disable
default features where practical, check in `Cargo.lock`, forbid unsafe code in workspace crates,
record licenses/rationale/feature choices in `docs/dependencies.md`, and require an ADR for a new
direct dependency. CLI parsing, IDs, dates, fixed-point values, CSV, diagnostics, evaluation, rules,
and extension registry remain local code.

## Consequences

The initial lock resolves 65 third-party packages because the HTTP runtime, procedural macros, and
bundled SQLite have transitive/build/platform dependencies. This is larger than the direct count but
is explicit, locked, license-reviewed, and monitored by CI/Dependabot. Removing Axum/Tokio would
materially shrink it but would require maintaining an internet-facing protocol implementation;
removing rusqlite would sacrifice transactional constraints/migrations or require a database
service. Dependency growth is treated as a design cost, not a convenience default.

