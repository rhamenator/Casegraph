# Source structural coverage analysis

Status: controlled verification evidence for the current change set. This analysis supplements the
requirements-based traces in `traceability.tsv`; it does not establish certification credit.

## Controlled procedure and result

Environment: Rust 1.95.0 from `rust-toolchain.toml`, `llvm-tools-preview`, Cargo lockfile enforced,
and `cargo-llvm-cov` 0.8.7 installed with `--locked`.

```console
cargo llvm-cov clean --workspace
cargo llvm-cov --workspace --summary-only --locked
```

The initial workspace baseline was 79.14% lines, 75.83% regions, and 66.73% functions. After the
requirements-oriented robustness campaign, the controlled local result is:

| Metric | Covered | Total | Result | CI floor |
|---|---:|---:|---:|---:|
| Lines | 6,876 | 7,363 | 93.39% | 90% |
| Regions | 9,595 | 10,695 | 89.71% | 85% |
| Functions | 546 | 664 | 82.23% | 75% |

CI repeats the workspace run and fails below any floor. Floors intentionally retain margin below
the observed result so platform instrumentation variance cannot silently convert a diagnostic into
a flaky gate; changing a floor requires review of this analysis and CG-HLR-017/CG-LLR-011.

## Verification added

The campaign expanded tests for real HTTP and CLI workflows; evidence persistence and every stored
enum conversion; invalid canonical JSON, identifiers, timestamps, and SQLite constraints; all
epistemic query modes; satisfied, false, missing-anchor, missing-input, superseding, and defensive
disagreement rule outcomes; optional-provider policy, failure sanitization, schema validation, and
semantic validation; deterministic text/JSON/CSV parsing and normalization; exact domain numeric
and temporal boundaries; artifact-store confinement and corruption; configuration choices; and
default repository capability behavior.

Instrumentation exposed two major anomalies. CG-PR-001 corrected stale temporary database reuse in
CLI tests. CG-PR-002 corrected acceptance of malformed signed decimals. The first Linux CI run also
closed minor CG-PR-003 by replacing a Windows-specific root-path test expectation with the host
platform separator. All three have named regression tests and closed dispositions in
`problem-reports.tsv`.

## Residual analysis

The remaining executable lines are concentrated in adapter/platform failures that cannot be
induced portably without corrupting the host environment (system clock before the Unix epoch,
address-space length overflow, filesystem atomic-rename races and permission loss), server shutdown
tails, exhaustive assurance-validator diagnostics, CLI dispatcher arms, and test-double mandatory
methods not invoked by their focused scenarios. Some defensive persistence and rule branches are
infeasible through the production repository invariants; representative cases are exercised using
controlled doubles rather than weakening those invariants.

The suite does not use network services, an AI provider, production data, timing assumptions, or
test ordering. Temporary filesystem/database fixtures are process- and sequence-isolated and are
removed after use. Failure records and assertions use invented content.

## Assurance limitations

Rust stable source-based instrumentation reports line, region, and function metrics here but no
branch metric. Region coverage is not claimed to be DO-178C decision coverage. No MC/DC analysis,
data/control-coupling analysis, executable-object-code coverage, compiler-generated-code analysis,
target-computer execution, deactivated-code classification, or DAL-dependent sufficiency claim has
been made. `cargo-llvm-cov`, LLVM, the Rust compiler, and the test harness are not qualified under
DO-330. A project seeking airborne approval must repeat and extend coverage in its approved target,
tool, DAL, independence, and certification context.
