# Architecture

## Status

This document records the intended foundation boundaries. Features are marked implemented in the
README only after their tests pass; this document is not a claim that every boundary is complete.

```text
API (Axum)       CLI (local parser)
      \           /
       Application services
        /       |        \
 Domain model  Ports   Sample package registry
        \       |        /
 Embedded SQLite   Artifact store   Optional reasoning provider
```

The deployable is a modular monolith. Dependency direction points inward: delivery and
infrastructure adapters may depend on application/domain contracts, but domain code may not depend
on persistence layout, HTTP, CLI, filesystem layout, or model vendors.

The ingestion data flow is staged and observable:

```text
connector -> immutable ingestion -> classification -> raw extraction
          -> structural extraction -> semantic extraction -> normalization
          -> validation -> provenance/evidence creation
```

Each material externally derived datum must cross the provenance validation boundary before it can
be persisted as a claim. Rule and workflow services consume stored facts/evidence, not raw model
output. Grounded querying renders only stored epistemic states and never asks a model to supply
missing case facts.

## Module boundaries

- `casegraph-assurance`: dependency-free lifecycle-data validator for requirements, traceability,
  configuration items, and problem reports. It is development tooling, not runtime code and not a
  qualified verification tool.
- `casegraph-domain`: canonical types, invariants, epistemic states, temporal uncertainty, and
  repository/provider ports. No adapter dependencies.
- `casegraph-application`: transactions and use cases for ingestion, evidence, corrections, rules,
  workflow, queries, and evaluation.
- `casegraph-infrastructure`: SQLite repositories, migrations, filesystem artifact storage,
  extraction adapters, and diagnostics.
- `casegraph-sample-domain`: artificial extractors/rules/workflow contributions registered through
  core extension contracts.
- `casegraph-api`: bounded `/api/v1` HTTP routes and a maintained OpenAPI document over application
  services.
- `casegraph-cli`: command parsing and human/machine-readable output over application services.

Operational audit events and evidentiary provenance are separate records. Audit events describe who
changed system state and why; provenance explains where a material assertion came from and how it
was produced.
