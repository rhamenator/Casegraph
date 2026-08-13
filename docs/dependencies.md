# Dependency policy and register

Casegraph keeps a deliberately small direct dependency set. Versions are exact in the workspace
manifest and the full transitive graph is locked in `Cargo.lock`. Default features are disabled
where doing so materially reduces the graph. New direct dependencies require an ADR that records
why existing code or the standard library is insufficient, license compatibility, maintenance
signals, security considerations, and removal cost.

| Crate | Version | License | Purpose and rationale |
|---|---:|---|---|
| `serde` | 1.0.229 | MIT OR Apache-2.0 | Widely used, schema-derived serialization avoids bespoke unsafe parsers. |
| `serde_json` | 1.0.151 | MIT OR Apache-2.0 | Strict JSON interchange for API and reproducibility snapshots. |
| `sha2` | 0.11.0 | MIT OR Apache-2.0 | RustCrypto SHA-256 implementation; avoids hand-written cryptography. |
| `rusqlite` | 0.40.2 | MIT; SQLite is public domain | Transactional embedded persistence and tested migrations without a database service. Only the bundled SQLite feature is enabled. |
| `axum` | 0.8.9 | MIT | Maintained HTTP routing/body validation instead of a custom HTTP parser. Only HTTP/1, JSON, and Tokio integration are enabled. |
| `tokio` | 1.53.1 | MIT | Runtime required by Axum. Only macros, networking, multithreaded runtime, and signal handling are enabled. |

The CLI, identifier generation, temporal primitives, rule evaluation, configuration, diagnostics,
evaluation harness, and assurance-data validator intentionally use the standard library and local
crates.

Review sources: each crate's crates.io metadata, upstream repository, published license, selected
feature graph (`cargo info --verbose`), and the resolved `cargo tree`/`Cargo.lock`. CI runs locked.

At the initial lock on 2026-08-12, Cargo resolves six direct and 65 total third-party packages,
including platform-specific and build-time packages. All resolved packages declare combinations of
MIT, Apache-2.0, BSD-3-Clause, Unicode-3.0, LLVM exception, or Unlicense terms compatible with this
GPL-3.0-only project. That total is a tracked cost: dependency reductions are preferred and any
increase must be justified.

GitHub Actions are pinned to immutable commit SHAs in CI. Dependabot may propose updates, but an
update requires configuration/tool impact analysis, review, and a green exact-revision run. Pinned
versions provide configuration identity; they do not constitute DO-330 qualification.
