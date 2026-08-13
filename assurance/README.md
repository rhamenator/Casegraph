# Casegraph assurance data

Casegraph uses a **DO-178C-aligned, requirements-driven lifecycle** as its engineering quality
framework. This directory contains controlled planning, requirements, traceability, verification,
configuration-management, quality-assurance, and problem-reporting data.

This is a deliberate application of the standard's engineering disciplines, not a certification
claim. Casegraph is not currently airborne software, has no certification applicant or authority,
has no aircraft/system safety assessment, and has no assigned Design Assurance Level (DAL). Until
those inputs exist, DAL-dependent objectives, independence, structural coverage, certification
liaison, and final approval data cannot be selected or credited. The repository must say
“DO-178C-aligned” or “tailored from DO-178C,” never “DO-178C compliant,” “certified,” or
“certifiable.”

The FAA recognizes DO-178C/ED-12C through active Advisory Circular 20-115D as an acceptable means
for airborne software development assurance. The actual standard is copyrighted and must be
obtained from RTCA or EUROCAE for a certification project. This repository does not reproduce its
objective tables.

Authoritative public references:

- [FAA AC 20-115D](https://www.faa.gov/airports/resources/advisory_circulars/index.cfm/go/document.information/documentNumber/20-115D)
- [FAA aircraft certification software guidance](https://www.faa.gov/aircraft/air_cert/design_approvals/air_software)
- [RTCA DO-178 software standards](https://www.rtca.org/do-178/)
- [RTCA Forum for Aeronautical Software and errata](https://www.rtca.org/sc-240/forum-for-aeronautical-software/)

## Controlled data

- `PLAN.md`: lifecycle scope, transition criteria, roles, and derived-requirement handling.
- `DEVELOPMENT_PLAN.md`: lifecycle model, environment, methods, integration, and transitions.
- `VERIFICATION_PLAN.md`: reviews, requirements-based tests, coverage policy, evidence, and
  independence.
- `CONFIGURATION_MANAGEMENT_PLAN.md`: identification, baselines, change control, status accounting,
  archive, and release rules.
- `QUALITY_ASSURANCE_PLAN.md`: conformance surveillance, records, escalation, and release authority.
- `STANDARDS.md`: requirements, design, Rust, SQL, test, and review standards.
- `requirements.tsv`: uniquely identified high- and low-level requirements.
- `traceability.tsv`: requirement-to-design/source/test mappings.
- `COVERAGE_ANALYSIS.md`: reproducible source structural-coverage result, residual analysis, and
  limits on assurance credit.
- `configuration-index.tsv`: controlled configuration-item classes and paths.
- `problem-reports.tsv`: anomalies and their disposition evidence.
- `REVIEW_RECORD_TEMPLATE.md`: independent-review and verification record structure.
- `SOFTWARE_CONFIGURATION_INDEX.md`: reproducible baseline and release contents.
- `ACCOMPLISHMENT_SUMMARY.md`: current assurance status and open certification gaps.

Run `cargo run -p casegraph-assurance --locked` before review. The dependency-free validator checks
the form and internal consistency of these records, verifies repository-contained references and
test symbols, and fails closed on missing traces. It is a convenience tool only: no DO-330 tool
qualification credit is claimed, so reviewers remain responsible for semantic correctness.
