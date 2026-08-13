# Casegraph Foundation Specification

## Mission

Create the initial production-quality foundation for a new open, extensible software platform whose purpose is:

> **Turn messy real-world records into structured, traceable evidence and use that evidence to determine what happened, what matters, what must happen next, and why.**

The project name is **Casegraph**. Keep naming sufficiently isolated that the project can be renamed later without major architectural disruption.

This development cycle is **not** intended to build a complete vertical application such as a government-benefits navigator, prior-authorization system, financial-forensics application, insurance-appeal system, or regulatory-compliance product. Build the reusable, domain-neutral substrate upon which those applications can later be constructed.

The platform must be useful without an LLM and must never depend upon an LLM as its database, source of truth, workflow engine, arithmetic engine, or rules engine.

The governing architectural principles are:

> **AI interprets ambiguity; deterministic software enforces invariants.**

> **No material assertion without provenance.**

> **Uncertainty, disagreement, and contradiction are data, not errors to hide.**

---

# 1. Development Method

Begin by inspecting the repository and development environment. If the repository is empty, initialize an appropriate project structure. If work already exists, understand it before changing it.

Before substantial implementation:

1. inspect the repository;
2. inspect existing documentation;
3. inspect issues and TODOs if available;
4. inspect existing tests and CI;
5. identify architectural constraints;
6. write or update an architectural plan;
7. divide the work into small, independently testable development slices.

Do not attempt to implement the entire long-term vision in one enormous pass. Work iteratively.

For every slice:

1. identify the invariant or capability being introduced;
2. implement the smallest coherent version;
3. write thorough automated tests;
4. run the relevant test suite;
5. fix failures;
6. update documentation;
7. commit the completed slice with a meaningful commit message when repository policy and the execution environment permit commits;
8. continue to the next logical slice when safe to do so.

Prefer small, comprehensible commits over giant commits. Do not leave the repository in a knowingly broken state between completed slices. Do not silently weaken tests to make failures disappear.

---

# 2. Architectural Objective

Establish a layered architecture approximately equivalent to:

```text
Vertical Applications
        │
Vertical Domain Packages
        │
Workflow Engine
        │
Rules Engine
        │
Reasoning / Interpretation Layer
        │
Evidence and Provenance Layer
        │
Extraction and Normalization
        │
Artifact Store
        │
Connectors / Ingestion
```

These boundaries do not have to correspond one-to-one with processes, services, packages, or deployable components.

Start with a **modular monolith unless there is a compelling technical reason not to**. Do not prematurely introduce microservices. Architectural boundaries should nevertheless be explicit enough that components could later be separated if scale or operational requirements justify doing so.

---

# 3. Canonical Domain Model

Design and implement a domain-neutral canonical model capable of representing at least:

- Source
- Artifact
- ArtifactVersion
- Observation
- Claim
- Fact
- Entity
- Relationship
- Event
- Evidence
- Contradiction
- Rule
- RuleVersion
- RuleEvaluation
- Obligation
- Deadline
- Case
- Task
- Action
- Outcome
- HumanReview
- Correction
- ProvenanceRecord

Do not assume that every extracted statement is true. A critical distinction must exist between:

```text
source says X
```

and:

```text
X has been established as true
```

Claims should be capable of having states such as:

- observed
- extracted
- inferred
- corroborated
- disputed
- contradicted
- superseded
- verified
- rejected
- unresolved

Choose precise terminology and document its semantics.

---

# 4. Provenance as a Fundamental Invariant

Every material datum derived from an external artifact must be traceable back to its origin.

For documents, provenance should be capable of identifying, where available:

- source artifact;
- artifact version and hash;
- page;
- paragraph;
- text span;
- table;
- row;
- column;
- bounding region;
- extraction method;
- extractor and version;
- model, provider, and version if AI-assisted;
- extraction timestamp;
- confidence;
- subsequent human verification or correction.

For structured sources, provenance should support analogous information such as:

- connector;
- endpoint or system;
- record identifier;
- field;
- retrieval timestamp;
- source version or revision where available.

Never destroy original provenance when information is normalized. For example:

```text
"$1,427.00"
```

may normalize to:

```text
Money(
    amount=1427.00,
    currency="USD"
)
```

but the original representation and its source location must remain recoverable.

---

# 5. Immutable Source Artifacts

Treat ingested source material as immutable evidence.

At ingestion time:

- calculate a cryptographic content hash;
- assign an internal stable identifier;
- preserve relevant metadata;
- detect exact duplicates;
- distinguish artifact identity from artifact version;
- never silently overwrite source evidence.

If a document changes, store another version rather than modifying historical evidence. Design this so that a future audit can reconstruct exactly what information was available to the system at a particular point in time.

---

# 6. Evidence Graph

Implement a logical evidence graph. Do **not** assume this requires a dedicated graph database.

Prefer PostgreSQL as the authoritative persistence layer initially unless repository or environment constraints strongly justify another choice. Use normalized relational structures where practical and JSON/JSONB only where flexibility provides a real advantage.

The evidence graph must support relationships such as:

```text
CLAIM -> SUPPORTED_BY -> EVIDENCE
CLAIM -> CONTRADICTED_BY -> EVIDENCE
CLAIM -> CORROBORATES -> CLAIM
CLAIM -> CONTRADICTS -> CLAIM
CLAIM -> SUPERSEDES -> CLAIM
EVENT -> INVOLVES -> ENTITY
EVENT -> SUPPORTED_BY -> EVIDENCE
OBLIGATION -> CREATED_BY -> EVENT
DEADLINE -> APPLIES_TO -> OBLIGATION
TASK -> SATISFIES -> OBLIGATION
ACTION -> PRODUCES -> OUTCOME
```

Do not hard-code the system exclusively to these relationship types if a clean extensibility mechanism is appropriate.

---

# 7. Contradictions Must Be First-Class

Do not resolve conflicting evidence by silently selecting one value.

For example:

```text
Claim A:
monthly_income = 1427 USD

Claim B:
monthly_income = 1511 USD
```

must be capable of producing:

```text
Contradiction:
    claim_a: ...
    claim_b: ...
    status: unresolved
```

The system should support:

- automatic contradiction detection where safely possible;
- human-created contradictions;
- resolution status;
- resolution rationale;
- supporting evidence;
- supersession;
- human adjudication.

Preserve the losing or superseded claim. History is evidence.

---

# 8. Temporal Semantics

Time will be fundamental to nearly every future vertical. Design temporal representation carefully.

Distinguish concepts such as:

- event time;
- effective date;
- reported date;
- received date;
- created date;
- modified date;
- extraction date;
- verification date;
- due date;
- expiration date.

Do not collapse all temporal information into generic `created_at` and `updated_at` fields.

Support incomplete temporal knowledge where reasonable. Examples such as:

```text
"sometime in June 2026"
"before July 15"
"effective beginning August"
```

must not be converted into falsely precise timestamps.

---

# 9. Confidence and Uncertainty

Confidence must be explicit when information is probabilistic. Avoid a design in which everything is represented as certain simply because it exists in the database.

Support:

- extraction confidence;
- interpretation confidence;
- relationship confidence;
- human verification state.

Do not manufacture numeric confidence scores where the underlying extractor or model does not meaningfully provide them.

Distinguish:

```text
unknown
```

from:

```text
known false
```

and:

```text
not applicable
```

from:

```text
not yet evaluated
```

where relevant.

---

# 10. Human Correction

Humans must be able to correct machine-derived information without destroying history.

A correction should record:

- original value;
- corrected value;
- who or what performed the correction;
- timestamp;
- rationale if supplied;
- provenance;
- downstream information potentially affected.

Corrections should support re-evaluation of dependent rules and conclusions. Never simply mutate an incorrect extracted value and erase the fact that it was previously extracted.

---

# 11. Ingestion Layer

Create an extensible connector interface.

The first implementation only needs filesystem or directory ingestion, but design interfaces capable of later supporting:

- email;
- scanners;
- cloud storage;
- SQL databases;
- REST APIs;
- webhooks;
- legacy applications;
- message systems;
- financial systems;
- government systems.

Initial supported artifacts should include, where reasonably feasible:

- PDF;
- plain text;
- JSON;
- CSV;
- common image formats.

DOCX and XLSX may be included if doing so does not materially distract from the foundational objective. Do not attempt to support every format during this cycle.

---

# 12. Extraction Architecture

Create a pluggable extraction pipeline. The pipeline should distinguish:

```text
ingestion
    ↓
artifact classification
    ↓
raw extraction
    ↓
structural extraction
    ↓
semantic extraction
    ↓
normalization
    ↓
validation
    ↓
evidence creation
```

Do not merge all of these into a single LLM prompt. Prefer deterministic extraction when possible.

Examples:

- PDF text extraction: deterministic library;
- CSV parsing: deterministic parser;
- ISO date parsing: deterministic;
- monetary arithmetic: deterministic;
- cryptographic hashes: deterministic;
- schema validation: deterministic.

Use model-assisted extraction only where interpretation materially benefits from it.

---

# 13. Model Provider Abstraction

The system must not be permanently coupled to OpenAI or any other model vendor.

Define a provider-neutral reasoning interface capable of supporting:

- OpenAI;
- other hosted models;
- local models;
- no model at all.

The platform should remain operational in degraded deterministic mode when no AI provider is configured.

Record model provenance whenever a model contributes to a claim or interpretation. Do not assume model output is authoritative. Validate structured model output against schemas. Reject malformed output rather than pretending it is valid.

---

# 14. Rules Engine

Create the foundation for a deterministic rules engine. Rules should eventually be expressible declaratively. Do not build a gigantic DSL in this first cycle.

Establish the abstraction and implement enough functionality to demonstrate:

```text
facts + rule version
        ↓
rule evaluation
        ↓
result
        ↓
explanation
        ↓
provenance
```

A rule evaluation must be reproducible. Store:

- rule identifier;
- rule version;
- inputs;
- result;
- evaluation timestamp;
- explanation;
- evidence used.

Rules must be versioned because laws, policies, contracts, and business rules change over time. A historical case must be capable of explaining which rule version produced an earlier result.

---

# 15. Workflow Foundation

Implement a minimal workflow abstraction supporting:

- case;
- task;
- status;
- dependency;
- obligation;
- deadline;
- action;
- outcome.

Do not build a giant BPM product. Demonstrate that evidence or rule evaluations can produce obligations and tasks.

Example:

```text
notice_received
    ↓
rule evaluation
    ↓
response_required
    ↓
obligation
    ↓
deadline
    ↓
task
```

The platform must preserve why the task exists. A user should eventually be able to ask:

> Why do I need to do this?

and receive a provenance-backed answer.

---

# 16. Domain Packages

Create a clean extension mechanism for future domain packages.

A package should eventually be able to contribute:

- schemas;
- entity types;
- extractors;
- normalizers;
- rules;
- workflows;
- terminology;
- document templates;
- validators;
- jurisdiction-specific configuration.

Do not implement a full benefits, medical, financial, insurance, legal, or government vertical in this cycle.

Instead, create one deliberately artificial sample domain package for testing the extension architecture. Call it something obviously non-production such as:

```text
sample-administrative-case
```

Use it to prove that domain behavior can be added without contaminating the core with domain-specific assumptions.

---

# 17. Grounded Querying

Implement a minimal query capability capable of answering questions from the evidence graph.

The critical invariant is:

> **Answers about case facts must be grounded in stored evidence.**

The query layer must distinguish among:

```text
The evidence establishes X.

Source A claims X.

The available evidence suggests X.

Sources disagree about X.

The system does not have sufficient evidence to determine X.
```

Do not allow a model to fill missing case facts using world knowledge. World knowledge may eventually explain terminology or context, but it must never masquerade as evidence about a particular case.

---

# 18. Auditability

Design every important state-changing operation with auditability in mind.

Record:

- operation;
- actor;
- timestamp;
- previous state where appropriate;
- resulting state;
- reason;
- correlation or request identifier where appropriate.

Avoid logging sensitive document contents unnecessarily.

Separate operational logging from evidentiary provenance. These are related but not identical concepts.

---

# 19. Security and Privacy

Assume eventually this system may process extremely sensitive information. Establish secure defaults now.

At minimum:

- validate all external input;
- prevent path traversal;
- constrain uploaded artifact handling;
- avoid unsafe deserialization;
- parameterize database queries;
- prevent secrets from entering source control;
- provide environment-based secret configuration;
- sanitize logs;
- define authorization boundaries even if authentication is initially minimal;
- design for encryption at rest and in transit;
- document threat assumptions.

Do not build elaborate enterprise identity infrastructure yet. Do build boundaries that will not make proper authorization impossible later.

Treat source documents and extracted evidence as untrusted input. Treat LLM output as untrusted input.

---

# 20. Privacy-Preserving AI Architecture

Design for the possibility that some future deployments will prohibit sending sensitive data to third-party model providers.

Support architectural paths for:

- local models;
- selective or redacted context;
- provider policies;
- disabling remote AI entirely.

Do not make cloud-model access a hidden dependency.

---

# 21. API

Expose a versioned API suitable for eventual use by:

- web applications;
- desktop applications;
- CLI clients;
- automation;
- MCP servers;
- third-party integrations.

Do not overbuild the API. Initial capabilities should cover core operations such as:

```text
artifacts
cases
entities
claims
evidence
contradictions
rules
tasks
queries
```

Generate an OpenAPI specification if the selected framework supports doing so reliably.

---

# 22. CLI

Provide a useful CLI before investing heavily in GUI work.

The initial CLI should support workflows approximately like:

```text
casegraph init

casegraph case create "Sample Case"

casegraph ingest ./documents --case <case>

casegraph artifacts list --case <case>

casegraph claims list --case <case>

casegraph contradictions list --case <case>

casegraph query <case> "What deadlines appear in these records?"

casegraph verify <claim>

casegraph test
```

Exact syntax may differ if a cleaner interface emerges. The CLI should exercise the same application services as the API rather than duplicating business logic.

---

# 23. Minimal Human Review Interface

Do not spend this cycle building a polished consumer UI.

If practical, provide a minimal web interface for inspecting:

- artifacts;
- extracted claims;
- provenance;
- contradictions;
- verification state;
- cases.

Functionality and inspectability matter more than visual polish. If this would substantially delay the core architecture, defer it and document the decision.

---

# 24. Testing Strategy

Testing is a first-class deliverable.

Establish:

- unit tests;
- integration tests;
- persistence tests;
- API tests;
- CLI tests;
- migration tests;
- property or invariant tests where useful;
- malformed-input tests;
- security-oriented tests.

Create synthetic fixtures. Do not use real personally identifiable or medical information in the repository.

Important invariants should include tests such as:

## Provenance invariant

Every externally derived claim has provenance.

## Immutability invariant

An ingested artifact cannot silently change without producing a new version and hash.

## Contradiction invariant

Conflicting claims can coexist without either being silently destroyed.

## Correction invariant

Human corrections preserve the original extraction.

## Rule reproducibility invariant

The same inputs and rule version produce the same deterministic result.

## Model isolation invariant

Core deterministic functionality operates without a configured AI provider.

## Domain isolation invariant

Removing the sample domain package does not break the core platform.

## Evidence-grounding invariant

Case queries cannot invent case-specific facts absent from evidence.

---

# 25. Evaluation Harness

Create the beginnings of an evaluation framework. Future model-assisted extraction changes must be measurable.

Establish fixtures with known expected:

- entities;
- dates;
- amounts;
- claims;
- relationships;
- contradictions;
- provenance.

Measure at least:

- extraction correctness;
- normalization correctness;
- provenance completeness;
- contradiction detection;
- unsupported-claim rate.

Do not optimize for a single aggregate “AI accuracy” number. Preserve failure examples as regression fixtures.

---

# 26. Database Migrations

Use proper schema migrations from the beginning. Do not rely on deleting and recreating the database during normal development.

Migration history should be source controlled and tested. Design for future schema evolution.

---

# 27. Reproducibility

Where reasonably possible, a historical conclusion should be reproducible from:

```text
artifact version
+
extractor version
+
model/version/configuration if applicable
+
normalizer version
+
rule version
+
human corrections
```

Absolute bit-for-bit reproducibility from nondeterministic models may not always be possible. When it is not, preserve enough information to explain what happened rather than pretending otherwise.

---

# 28. Observability

Introduce structured logging and basic diagnostics.

Make it possible to trace a document through:

```text
ingestion
→ extraction
→ normalization
→ evidence creation
→ rule evaluation
→ workflow action
```

Use correlation identifiers to connect operations across layers. Record durations and outcomes for important pipeline stages without placing sensitive source contents into routine logs.

Expose useful health and readiness information for external dependencies such as the database, artifact storage, and optional model providers. A disabled model provider must not make deterministic platform health fail.

---

# 29. Error Handling and Failure Semantics

Failures must be explicit, inspectable, and recoverable where reasonable.

Distinguish among:

- an artifact that could not be read;
- an artifact format that is unsupported;
- extraction that completed with warnings;
- extraction that produced no observations;
- validation that rejected malformed output;
- an unavailable optional model provider;
- a rules evaluation that lacked required facts;
- an unexpected internal failure.

Do not silently discard a document or partial result. Preserve enough failure context to diagnose and retry safely, while excluding secrets and unnecessary sensitive content.

Retry operations must be idempotent or explicitly guarded against duplicate evidence and duplicate workflow actions.

---

# 30. Configuration and Deployment

Keep local development straightforward and reproducible.

Provide:

- documented environment-based configuration;
- a checked-in example configuration containing no secrets;
- a simple way to start required local dependencies;
- deterministic database migration and test commands;
- container support where it improves repeatability without obscuring development;
- sensible development defaults that do not weaken production security expectations.

Separate configuration for storage, database, model providers, logging, and feature flags. Validate configuration at startup and report actionable errors.

Do not require cloud infrastructure for the foundational local workflow.

---

# 31. Documentation and Architectural Decisions

Documentation is a deliverable, not cleanup work.

Create and maintain:

- a project README with mission, status, quick start, architecture summary, and limitations;
- a developer setup guide;
- a canonical-domain-model glossary;
- provenance and evidence semantics;
- extraction-pipeline contracts;
- domain-package extension guidance;
- API and CLI usage;
- testing and evaluation instructions;
- security, privacy, and threat assumptions;
- explicit out-of-scope items;
- architectural decision records for consequential choices.

Document what is implemented versus aspirational. Do not describe planned capabilities as if they already exist.

Use diagrams where they clarify boundaries, data flow, or lifecycle. Keep terminology consistent across code, schemas, APIs, CLI output, and documentation.

---

# 32. Dependency and Technology Selection

Choose a mature, maintainable technology stack that fits the repository and environment. Prefer boring, well-supported components over novelty.

Before adding a dependency, consider:

- whether the standard library or existing dependency is sufficient;
- project health and maintenance;
- license compatibility;
- security history;
- transitive dependency cost;
- deterministic and offline behavior;
- testability;
- whether it creates avoidable vendor coupling.

Pin dependencies appropriately and enable automated dependency and security checks where practical.

Do not introduce a graph database, message broker, vector database, distributed workflow system, or Kubernetes requirement unless a demonstrated foundational need justifies it.

---

# 33. Initial End-to-End Demonstration

Build one narrow, synthetic, end-to-end scenario using the sample administrative domain package.

The demonstration should prove this flow:

```text
synthetic source artifacts
        ↓
immutable ingestion and hashing
        ↓
deterministic extraction and normalization
        ↓
claims with precise provenance
        ↓
at least one corroboration or contradiction
        ↓
human verification or correction
        ↓
versioned deterministic rule evaluation
        ↓
obligation, deadline, and task
        ↓
grounded query explaining what must happen and why
```

The scenario must use invented people, organizations, identifiers, amounts, and documents. It must not encode assumptions from a real production vertical into the core.

Make the demonstration runnable from documented commands and covered by automated tests.

---

# 34. Required Deliverables

The foundation cycle should leave the repository with, at minimum:

1. a documented modular architecture;
2. an implemented canonical domain model with clear semantics;
3. database schema and tested migrations;
4. immutable artifact ingestion with hashing and duplicate handling;
5. a pluggable extraction and normalization pipeline;
6. first-class provenance, evidence, contradictions, corrections, and human review;
7. a provider-neutral optional reasoning interface;
8. a small versioned deterministic rules capability;
9. a minimal case, obligation, deadline, task, action, and outcome workflow model;
10. a sample domain package proving core isolation;
11. grounded evidence querying;
12. a versioned API and useful CLI for the implemented capabilities;
13. an automated test suite and initial evaluation harness;
14. synthetic fixtures and a repeatable end-to-end demonstration;
15. secure configuration, structured diagnostics, and documented threat assumptions;
16. complete developer and architecture documentation;
17. CI that runs formatting, linting or static analysis, tests, and migration checks appropriate to the selected stack.

If a listed deliverable cannot responsibly be completed in this cycle, implement the supporting boundary, document the gap and rationale, and avoid a misleading placeholder that implies production readiness.

---

# 35. Acceptance Criteria

The foundation is acceptable when all of the following are demonstrably true:

- A developer can set up the project from the documentation.
- The system ingests synthetic artifacts without altering the source bytes and records stable identity, version, and cryptographic hash.
- Every externally derived material claim in the sample scenario links to recoverable provenance.
- Normalized values retain their original representations and locations.
- Conflicting claims coexist and produce an inspectable contradiction rather than silent data loss.
- A human correction preserves history and identifies affected derived state.
- A deterministic rule evaluation records its exact version, inputs, result, explanation, and evidence.
- Evidence or a rule evaluation can create an obligation, deadline, and task with an explainable causal chain.
- Grounded querying distinguishes established, claimed, suggested, conflicting, and unknown information.
- The core sample workflow operates with no model provider configured.
- Malformed or unsupported model output is rejected and recorded safely.
- Removing or disabling the sample domain package does not break the domain-neutral core.
- API and CLI operations use shared application services and agree on behavior.
- Migrations work against a clean database and through supported upgrade paths.
- Automated tests cover the stated invariants and pass in CI.
- Documentation accurately distinguishes implemented functionality from future work.

---

# 36. Explicit Non-Goals for This Cycle

Do not let these items displace the foundation:

- a production benefits, medical, insurance, financial, legal, or compliance vertical;
- a polished consumer application;
- automated filing, submission, payment, or legally consequential action;
- legal, medical, financial, or eligibility advice;
- a comprehensive rules DSL;
- a general-purpose BPM suite;
- perfect OCR for every document type;
- support for every file format and connector;
- training or fine-tuning a foundation model;
- autonomous agents with broad system access;
- a graph database adopted merely because the model is graph-shaped;
- premature microservices or distributed infrastructure;
- enterprise-scale identity and multitenancy before authorization boundaries are understood;
- claims of production compliance or security certification.

Favor a trustworthy, inspectable vertical slice of the substrate over a wide collection of shallow features.

---

# 37. Decision Priorities

When requirements compete, use this priority order:

1. preserve source evidence and provenance;
2. prevent fabricated or silently altered case facts;
3. preserve history, uncertainty, and contradiction;
4. maintain deterministic invariants and reproducibility;
5. protect sensitive information;
6. keep domain concerns out of the core;
7. make behavior testable and inspectable;
8. favor simple, maintainable architecture;
9. improve convenience and presentation.

When uncertain, choose the smaller reversible design, record the decision, and keep the extension point clear.

---

# 38. Completion and Handoff

At the end of each meaningful development slice, report:

- what changed;
- which invariant or capability it establishes;
- tests and checks run, with results;
- documentation updated;
- known limitations or deferred work;
- the next recommended slice.

At the end of the foundation cycle, provide a concise architecture and implementation summary, a deliverables checklist, test and evaluation results, key decisions, unresolved risks, and a prioritized roadmap for the next cycle.

Do not claim completion while required tests are failing, while material assertions lack provenance, or while documentation materially overstates the implementation.

The desired result is not a grand demo held together by prompts. It is a durable, domain-neutral foundation on which real Casegraph vertical applications can be built safely and incrementally.
