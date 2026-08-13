# Rules, workflow, and grounded query semantics

## Deterministic rules

The first-cycle rule definition is intentionally small: one to 32 equality conditions over verified
`subject_key`/`predicate` facts and one workflow effect. Definitions are canonical JSON with SHA-256
and immutable version numbers. This is an extension point, not a general DSL.

Evaluation reads current verified claims and their evidence links. Inputs are sorted, canonicalized,
and hashed. The stored evaluation contains exact rule version, inputs, input hash, result,
explanation, evaluator version, evidence used, timestamp, and correlation ID.

- All conditions match and one verified deadline anchor exists: `satisfied`.
- A condition is established false: `not_satisfied`.
- A required fact/anchor is missing or verified facts disagree: `indeterminate`.

The same case, rule version, and input hash returns the prior materialization rather than duplicating
workflow effects. Tests prove equal inputs/version produce the same result, hash, evaluation ID, and
task.

## Workflow causality

A satisfied evaluation atomically creates an open obligation, exact-day deadline calculation, and
ready task. The obligation references the evaluation, the deadline references the obligation, and
the task references the obligation. The evaluation references every evidence record used. An
indeterminate or unsatisfied result never creates work.

Only exact day offsets from verified exact dates are implemented. Business calendars, uncertain
deadline ranges, task dependency mutation, actions/outcomes, event-created obligations, and
correction-triggered automatic reevaluation remain deferred behind the existing schema boundaries.

## Grounded querying

The deterministic query adapter recognizes a bounded deadline/workflow intent or matches stored
predicate names in the question. It never invokes a model or supplies values from world knowledge.
Answers carry claim, provenance, evidence, and rule-evaluation identifiers as applicable.

- `established`: matching claims are verified, or a workflow is caused by a satisfied evaluation.
- `claimed`: stored source claims exist but are not established.
- `suggested`: inferred or corroborated evidence exists without establishment.
- `conflicting`: distinct known values exist for the same requested predicate.
- `unknown`: no matching evidence/workflow exists; citation arrays are empty.

Natural-language intent breadth is deliberately limited. A future semantic query adapter must still
render only these stored grounded results and may not fill missing case facts.

