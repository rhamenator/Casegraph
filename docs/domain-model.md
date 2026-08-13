# Canonical domain model glossary

This glossary distinguishes implemented semantics from later extension points. The initial schema
contains every canonical record named by the foundation specification. Rust services currently
implement the evidence-integrity subset through corrections; rule/workflow services are the next
slice.

| Term | Semantics |
|---|---|
| Source | An external retrieval identity: connector, locator, optional endpoint/record/revision, and retrieval time. |
| Artifact | Stable identity for source material within a case and source key. It does not contain bytes. |
| ArtifactVersion | Immutable exact bytes identified by SHA-256, byte length, version number, media type, and opaque storage key. |
| Observation | Immutable record of what an extractor encountered, preserving original and optional normalized values plus provenance. |
| Claim | An assertion with origin and epistemic state. Its existence does not establish truth. |
| Fact | A value established by explicit verification of a referenced claim. |
| Entity | A typed case subject. External entities require provenance. Service support is deferred. |
| Relationship | A typed link between entities with optional confidence and validity range. Service support is deferred. |
| Event | A case occurrence with separate event/effective/reported/received temporal concepts. Service support is deferred. |
| Evidence | A provenance-backed excerpt/field/attestation or a reproducible rule result. |
| Contradiction | An append-only pairing of incompatible known claims. Both claims remain stored. |
| Rule / RuleVersion | Stable rule identity and immutable deterministic definition version. Service support is next-slice work. |
| RuleEvaluation | Reproducible input/result/explanation/evidence snapshot for an exact rule version. Service support is next-slice work. |
| Obligation | Explainable required state created by an event or rule evaluation. Service support is next-slice work. |
| Deadline | Imprecision-preserving due range/expression/calculation for an obligation. Service support is next-slice work. |
| Case | Domain-neutral evidence and workflow container. |
| Task | Work item that may satisfy an obligation and depend on other tasks. Service support is next-slice work. |
| Action / Outcome | Audited action and its recorded result. Service support is deferred within the workflow slice. |
| HumanReview | Append-only human decision about a claim, contradiction, rule evaluation, or provenance. |
| Correction | Link from an original claim to an appended corrected claim, review, rationale, actor, provenance, and affected derivations. |
| ProvenanceRecord | Immutable source location and extraction/model attribution for a material assertion. |

## Claim states

- `observed`: recorded directly as encountered, before semantic extraction.
- `extracted`: deterministically or probabilistically parsed from a source.
- `inferred`: derived rather than directly stated; it still requires an explainable chain.
- `corroborated`: independently compatible claims exist.
- `disputed`: explicitly challenged without a definitive incompatible assertion.
- `contradicted`: a first-class incompatible known claim exists.
- `superseded`: a later claim or correction replaces this claim for current interpretation; history remains.
- `verified`: a human review established the claim as a fact.
- `rejected`: a human review rejected the claim; it remains historical evidence.
- `unresolved`: available evidence cannot establish the claim.

Initial state is immutable on the claim. Subsequent states are append-only `claim_state_changes`;
consumers select the latest change rather than overwriting the claim.

## Knowledge and confidence

`known(value)`, `unknown`, `not_applicable`, and `not_evaluated` are distinct. Known boolean false
is not unknown. Exact decimal/money values use integer coefficients and decimal scale; binary
floating point is not used for material arithmetic. Confidence is optional and accepted only when a
producer provides a meaningful finite value from 0 through 1.

Temporal knowledge can be an exact date, month, year, before/after bound, range, or unknown original
expression. Ambiguity is never replaced with an invented time of day or exact date.

