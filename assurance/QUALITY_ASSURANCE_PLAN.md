# Software quality assurance plan

## Quality assurance activities

Software quality assurance (SQA) checks that lifecycle activities and data conform to the controlled
plans and standards. SQA is distinct from development and does not infer product correctness solely
from passing tests.

For each change, SQA confirms:

- an approved requirement or problem report authorizes the work;
- requirements are reviewable and derived requirements receive upstream feedback;
- design, source, tests, documentation, and traces agree;
- required reviews and independence are recorded;
- configuration items, migrations, dependencies, and tools remain controlled;
- all required checks pass on the exact commit;
- anomalies are recorded rather than hidden by weakened tests, warnings, or documentation;
- release claims match the evidence and retain explicit limitations.

SQA may stop a baseline for missing or contradictory lifecycle data, unreviewed changes, failing
checks, unresolved major problems, unapproved deviations, or an unreproducible configuration.

## Records and deviations

Review approvals, CI runs, assurance validator output, test results, audits, problem reports,
configuration indices, and accomplishment summaries are quality records. Deviations from a plan or
standard require a problem report with rationale, impact, compensating verification, approver, and
closure or explicit release acceptance. “Existing code,” schedule pressure, or a green test suite is
not sufficient disposition.

## Independence and organizational limits

Current repository work may be authored and operated by one maintainer. Such work can establish
controlled internal engineering baselines but cannot claim independent verification. The first
release seeking airborne or formal DO-178C credit must appoint competent, organizationally
independent verification and SQA roles to the extent required by its assigned software level and
authority-approved plan.

## Audits

An internal baseline audit checks every configuration-index item, bidirectional traces, open problem
reports, exact dependencies, tool versions, CI evidence, release hashes, and documentation claims.
A certification project additionally schedules authority-facing planning, development, verification,
configuration, and final accomplishment reviews according to its approved certification approach.
