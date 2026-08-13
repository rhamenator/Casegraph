# Developer setup

## Prerequisites

- Rust 1.95.0 with Cargo, Clippy, and rustfmt. `rust-toolchain.toml` installs the pinned toolchain.
- A C/C++ build toolchain capable of compiling bundled SQLite. No running database, model provider,
  container runtime, or cloud account is required.

## Build and verify

```console
cargo build --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

The repository disables rustc incremental compilation because some Windows/network volumes cannot
atomically finalize its state. Cargo's normal target caching remains active.

## Configuration

Copy `.env.example` values into your process environment; Casegraph does not parse `.env` files and
does not require secrets. Defaults bind only to loopback, cap artifacts at 25 MiB, write beneath
`.casegraph`, emit structured JSON diagnostics, and disable model providers. Startup rejects invalid
addresses, root data directories, unbounded sizes, and unknown policy values.

## Migrations

SQLite is embedded through a pinned bundled build. Migration SQL lives under
`crates/casegraph-infrastructure/migrations`. Applied migrations record their SHA-256 checksum;
changing an applied file makes startup fail rather than silently accepting drift.

The migration tests cover a clean database, idempotent reapplication, the supported version 1 to 2
upgrade, checksum drift rejection, and database-enforced artifact-version immutability:

```console
cargo test -p casegraph-infrastructure migrations --locked
```

Never edit an applied migration. Add the next numbered migration and an upgrade-path test.

