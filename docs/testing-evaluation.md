# Testing and evaluation

## Quality gates

```console
cargo run -p casegraph-assurance --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p casegraph-infrastructure migrations --locked
cargo test -p casegraph-infrastructure --test evaluation --locked
cargo llvm-cov --workspace --locked --summary-only --fail-under-lines 90 --fail-under-regions 85 --fail-under-functions 75
```

Tests include domain units, malformed input, application-service units, filesystem security and
integrity, clean/upgrade/checksum migration tests, real SQLite/filesystem integration, real HTTP/1
loopback API, CLI full demo, package isolation, optional-model policy/schema rejection, and doc
tests. Fixtures contain invented names, records, dates, and amounts only.

Named tests are mapped to controlled requirements in `assurance/traceability.tsv`. The verification
policy, independence criteria, robustness expectations, structural-coverage controls, and
unqualified-tool boundary are controlled in `assurance/VERIFICATION_PLAN.md` and
`assurance/COVERAGE_ANALYSIS.md`.

## Structural coverage

Install the controlled diagnostic tool with
`cargo install cargo-llvm-cov --version 0.8.7 --locked`; the pinned Rust toolchain also needs the
`llvm-tools-preview` component. The command above runs every workspace test under source-based
instrumentation and enforces the same floors as CI. The current controlled run covers 93.39% of
lines, 89.71% of regions, and 82.23% of functions, up from the initial 79.14%, 75.83%, and 66.73%
baseline. Stable instrumentation reports no branch metric, so the result must not be described as
decision or MC/DC coverage. Exact results and residuals are maintained with the assurance data.

## Evaluation harness

`fixtures/evaluation` contains plain text, flat JSON, quoted CSV, and a preserved malformed CSV
regression case. The harness reports separate assertions rather than one aggregate “AI accuracy”:

- extraction correctness: expected field count;
- semantic field correctness: expected predicates;
- normalization correctness: every expected scalar is a known typed value;
- provenance completeness: every field has an appropriate field/span or row/column location;
- unsupported-claim rate: empty/unsupported claims remain zero;
- contradiction detection: real-adapter integration proves unequal known values create exactly one
  inspectable contradiction while equal values corroborate.

Current deterministic fixtures expect 13/13 extracted fields, 13/13 correct predicates, 13/13
normalized values, 13/13 complete locations, and zero unsupported claims. These are regression
expectations for a deliberately simple fixture set, not a claim of general document accuracy.

Future model-assisted extractors must add failure examples, expected entities/dates/amounts/claims,
model identity/configuration, and provider-policy test coverage before comparison.
