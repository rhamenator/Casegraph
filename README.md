# Casegraph

Casegraph is a domain-neutral foundation for turning messy records into traceable evidence and using
that evidence to explain what happened, what matters, and what must happen next.

> AI interprets ambiguity; deterministic software enforces invariants.

## Status

Foundation implementation is in progress. The repository has a compiling modular workspace,
validated environment configuration, a constrained canonical SQLite schema, checksummed migrations,
and tests for clean/upgrade migrations, configuration safety, migration drift, and immutable
artifact versions. Evidence application services, ingestion, API/CLI operations, and the end-to-end
demonstration are not implemented yet. It is not a production application and contains no
production vertical.

## Architecture

Casegraph is a Rust modular monolith with inward-pointing dependencies: canonical domain types,
application services, infrastructure adapters, an artificial sample domain package, and thin API
and CLI adapters. SQLite is the initial authoritative embedded store; immutable artifact bytes are
kept separately behind a storage port. No AI provider or cloud service is required.

See [architecture](docs/architecture.md), [implementation plan](IMPLEMENTATION_PLAN.md), and the
[dependency register](docs/dependencies.md).

## Developer checks

Install the pinned Rust toolchain, then run:

```console
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

See [developer setup](docs/development.md) for configuration and migration details.

## License

Casegraph is licensed under GPL-3.0-only. See [LICENSE](LICENSE).
