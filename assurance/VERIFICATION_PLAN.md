# Software verification plan

## Strategy

Verification is requirements based. Tests are derived from controlled requirements, not merely from
the implementation. Each verified requirement maps to design, source, named tests, and method in
`traceability.tsv`. Reviews address correctness, consistency, completeness, verifiability,
conformance to standards, trace accuracy, robustness, unintended function, and downstream impact.

The normal verification procedure is:

```console
cargo run -p casegraph-assurance --locked
cargo fmt --all --check
cargo build --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p casegraph-infrastructure migrations --locked
cargo test -p casegraph-infrastructure --test evaluation --locked
cargo llvm-cov --workspace --locked --summary-only --fail-under-lines 90 --fail-under-regions 85 --fail-under-functions 75
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

GitHub Actions executes the assurance-data check, formatting, warning-denied static analysis,
workspace tests, and migration tests on every push to `main` and pull request. Local release
verification adds build, evaluation, documentation, dependency-license review, and vulnerability
audit. Results are identified by immutable commit and CI-run identifiers; release evidence is
linked from the accomplishment summary.

## Test case and procedure rules

- A test name states the condition and expected outcome and is stable enough for traceability.
- Tests establish preconditions, execute through the lowest practical public boundary, and assert
  outputs plus material state changes.
- Nominal, boundary, malformed, missing, conflicting, duplicate, and failure behavior are covered
  where applicable.
- Expected results are explicit; snapshots cannot be accepted merely because output changed.
- Tests are deterministic, isolated, and do not depend on a model provider, wall-clock timing,
  external network, or execution order unless the requirement explicitly demands it.
- Fixtures are invented, version controlled, small, and reviewed with the test.
- A test may cover multiple requirements, but its trace record must make that relationship visible.
- A failed required check blocks the baseline. Skips, ignored tests, warning suppression, or reduced
  assertions require an approved problem report and impact analysis.

## Reviews and independence

Every behavior-changing pull request requires review by someone other than the author before it can
serve as a release assurance baseline. The reviewer records requirement IDs, trace completeness,
test adequacy, results, standards conformance, problem reports, and configuration impact using the
pull-request checklist. High-risk or DAL-dependent work requires the independence assigned by the
project-specific certification plan; ordinary repository approval does not substitute for it.

## Coverage and object code

Requirement coverage is enforced structurally by `casegraph-assurance` and semantically by review.
Source structural coverage is collected with pinned `cargo-llvm-cov` 0.8.7 and the Rust 1.95 LLVM
tools. CI requires workspace totals of at least 90% lines, 85% regions, and 75% functions. The
controlled result, baseline comparison, and residual analysis are in `COVERAGE_ANALYSIS.md`.
These floors are regression controls, not a claim that a DO-178C structural-coverage objective has
been met. Stable source-based coverage does not report branch coverage here, and the current run
does not establish decision or MC/DC coverage, data/control coupling, compiler-generated-code
disposition, executable object-code-to-source trace, target-hardware execution, or robustness
sufficiency. Those objectives require an assigned DAL and approved environment. Uncovered code is
reviewed as a gap; it is not silently classified as unreachable or deactivated.

## Tool assessment

Rust 1.95, Cargo, rustc, Clippy, rustfmt, the Rust test harness, SQLite, `cargo-llvm-cov` 0.8.7,
GitHub Actions, and `casegraph-assurance` are controlled by exact version or immutable
configuration where practical. None is qualified under DO-330. Automated outputs receive no
certification credit that would remove or reduce a required verification activity. Tool anomalies
are problem reports; review or an independently implemented check must detect errors where
qualification would otherwise be needed.
