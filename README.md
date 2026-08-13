# Casegraph

Casegraph is a domain-neutral foundation for turning messy records into traceable evidence and using
that evidence to explain what happened, what matters, and what must happen next.

> AI interprets ambiguity; deterministic software enforces invariants.

Development follows a controlled, requirements-driven lifecycle tailored from DO-178C: uniquely
identified requirements, bidirectional design/source/test traceability, requirements-based
verification, configuration baselines, problem reporting, independent-review criteria, and SQA
records. This is an engineering-quality framework, not an airborne certification or compliance
claim; no DAL has been assigned. See [assurance data](assurance/README.md).

## Status

The foundation baseline is implemented. Capabilities include validated canonical
evidence types; exact decimal/money and uncertain temporal values; content-addressed immutable
artifact storage; transactional case, ingestion, provenance, observation, claim, evidence,
contradiction, verification, correction, and audit services; and checksummed SQLite migrations.
The staged deterministic pipeline currently extracts simple UTF-8 `key: value` text, flat JSON,
and CSV into provenance-backed claims without a model provider. A provider-neutral optional
reasoning gateway enforces locality policy and strict output validation. A small versioned equality
rules engine consumes verified facts and can atomically create evidence-linked obligations,
deadlines, and tasks. Grounded querying emits explicit epistemic modes and citations. A versioned
HTTP API, useful CLI, structured diagnostic records, offline evaluation fixtures, and a runnable
end-to-end demonstration are implemented. It remains a foundation—not a production vertical,
polished review UI, or compliance-certified deployment.

## Architecture

Casegraph is a Rust modular monolith with inward-pointing dependencies: canonical domain types,
application services, infrastructure adapters, an artificial sample domain package, and thin API
and CLI adapters. SQLite is the initial authoritative embedded store; immutable artifact bytes are
kept separately behind a storage port. No AI provider or cloud service is required.

See [architecture](docs/architecture.md), [implementation plan](IMPLEMENTATION_PLAN.md), and the
[dependency register](docs/dependencies.md). Canonical terminology and evidence semantics are in
[the glossary](docs/domain-model.md) and [provenance guide](docs/provenance.md).
Extraction/provider contracts and current format limits are in [the pipeline guide](docs/extraction-pipeline.md).
Rules, workflow causality, grounded answers, and domain extension are documented in
[rules and workflow](docs/rules-workflow-query.md) and [domain packages](docs/domain-packages.md).

## Quick start

```console
cargo run -p casegraph-cli --locked -- init
cargo run -p casegraph-cli --locked -- demo
```

The demo uses invented records and runs immutable ingestion, two artifact versions, deterministic
extraction/normalization, provenance-backed claims, a contradiction, human verification and
correction, a versioned rule evaluation, obligation/deadline/task creation, and a grounded answer.
It does not configure or invoke a model provider.

For an incremental CLI workflow:

```console
cargo run -p casegraph-cli --locked -- case create "Synthetic Case"
cargo run -p casegraph-cli --locked -- ingest fixtures/evaluation/text/simple_record.txt --case <case-id>
cargo run -p casegraph-cli --locked -- claims list --case <case-id>
```

Run the loopback API with `cargo run -p casegraph-cli --locked -- serve`; its OpenAPI document is at
`/openapi.json`.

## Developer checks

Install the pinned Rust toolchain, then run:

```console
cargo run -p casegraph-assurance --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

See [developer setup](docs/development.md) for configuration and migration details.

## Current limitations

SQLite and the filesystem store target a single service instance. Authentication/authorization is
not implemented; the server binds to loopback by default and must not be exposed to untrusted
networks. PDF/image bytes can be preserved but PDF extraction/OCR is not implemented. Query intent,
rules, workflows, and package loading are deliberately narrow. See [limitations and scope](docs/limitations.md)
and [security assumptions](docs/security.md).

## License

Casegraph is licensed under GPL-3.0-only. See [LICENSE](LICENSE).
