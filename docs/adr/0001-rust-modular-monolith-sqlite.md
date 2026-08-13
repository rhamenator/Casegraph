# ADR 0001: Rust modular monolith with embedded SQLite

- Status: accepted
- Date: 2026-08-12

## Context

The repository began with only the foundation specification. The environment provides Rust 1.95
and Cargo. A C++ compiler is not available on `PATH`. The user requires very few, carefully vetted
outside dependencies. Casegraph needs strong domain boundaries, deterministic behavior, safe input
handling, a versioned HTTP API, a CLI, durable persistence, and tested schema evolution.

## Decision

Build a Rust workspace as a modular monolith. Domain and application crates define behavior and
ports; infrastructure adapters implement SQLite and filesystem artifact storage; Axum and a local
CLI parser remain thin delivery adapters. Versioned SQL migrations are source controlled. Artifact
bytes use a confined content-addressed filesystem store behind an interface. Direct dependencies
are pinned and justified in `docs/dependencies.md`.

## Consequences

- Locked builds require no cloud, database service, or model provider at runtime.
- SQLite deliberately diverges from the specification's PostgreSQL preference to meet the local,
  low-dependency constraint. Repository ports preserve a future PostgreSQL adapter path.
- The core does not depend on HTTP, CLI, storage adapters, a sample domain, or any AI vendor.
- The store targets one service instance with SQLite's transactional concurrency. Horizontal
  writers, PostgreSQL-style operational tooling, and large-scale ad-hoc analytics are deferred.
- Axum is used with a reduced HTTP/JSON feature set instead of maintaining a custom HTTP parser.
