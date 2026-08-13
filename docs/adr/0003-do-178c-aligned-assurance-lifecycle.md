# ADR 0003: Adopt a tailored DO-178C-aligned assurance lifecycle

## Status

Accepted 2026-08-13.

## Context

Casegraph's evidence and deterministic-decision purpose benefits from stronger lifecycle assurance
than ordinary application CI. The project owner requested DO-178C quality discipline from the
beginning. DO-178C is an airborne software development-assurance standard whose applicable
objectives and independence depend on system safety allocation and software level. Casegraph has no
airborne installation, certification applicant, authority-approved plan, system safety assessment,
or DAL. Calling the repository compliant or certifiable would therefore be unsupported.

## Decision

Adopt DO-178C-aligned practices as the mandatory repository engineering lifecycle:

- controlled HLRs, LLRs, allocation sources, and derived-requirement feedback;
- bidirectional mappings among requirements, design, source, and named requirements-based tests;
- development, verification, configuration-management, quality-assurance, and lifecycle plans;
- coding/design/test/review standards, configuration indices, anomaly control, baselines, and
  accomplishment records;
- independent-review criteria for assurance baselines;
- automated validation of assurance-data structure with no third-party crate;
- immutable CI action revisions and locked build dependencies.

Do not claim compliance, certification, a DAL, structural-coverage sufficiency, or tool qualification.
Treat each as a project-specific transition gate. If airborne use is proposed, acquire the current
standard/errata and establish a certification project with safety allocation, authority agreement,
supplement/tool assessments, independence, target evidence, and an objective-by-objective gap review.

## Consequences

Every behavior change carries requirements, traceability, verification, review, and configuration
work. CI can reject malformed or incomplete links, but semantic review remains necessary because the
validator and development toolchain are unqualified. Solo-maintainer work can form controlled
internal development baselines but cannot claim independent verification. This rigor is accepted as
the cost of trustworthy evolution.
