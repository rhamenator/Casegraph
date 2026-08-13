# Casegraph

Casegraph is a domain-neutral foundation for turning messy records into traceable evidence and using
that evidence to explain what happened, what matters, and what must happen next.

> AI interprets ambiguity; deterministic software enforces invariants.

## Status

Foundation implementation is in progress. Implemented capabilities include validated canonical
evidence types; exact decimal/money and uncertain temporal values; content-addressed immutable
artifact storage; transactional case, ingestion, provenance, observation, claim, evidence,
contradiction, verification, correction, and audit services; and checksummed SQLite migrations.
The staged deterministic pipeline currently extracts simple UTF-8 `key: value` text, flat JSON,
and CSV into provenance-backed claims without a model provider. A provider-neutral optional
reasoning gateway enforces locality policy and strict output validation. Rules/workflow, grounded
query, API/CLI operations, evaluation harness, and the end-to-end demonstration remain incomplete.
It is not a production application and contains no production vertical.

## Architecture

Casegraph is a Rust modular monolith with inward-pointing dependencies: canonical domain types,
application services, infrastructure adapters, an artificial sample domain package, and thin API
and CLI adapters. SQLite is the initial authoritative embedded store; immutable artifact bytes are
kept separately behind a storage port. No AI provider or cloud service is required.

See [architecture](docs/architecture.md), [implementation plan](IMPLEMENTATION_PLAN.md), and the
[dependency register](docs/dependencies.md). Canonical terminology and evidence semantics are in
[the glossary](docs/domain-model.md) and [provenance guide](docs/provenance.md).
Extraction/provider contracts and current format limits are in [the pipeline guide](docs/extraction-pipeline.md).

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
