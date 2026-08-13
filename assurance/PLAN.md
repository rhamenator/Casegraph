# Plan for software aspects of assurance

Status: controlled foundation plan, revision 1. This plan is effective for all changes after its
merge. Existing foundation behavior has been retrospectively baselined in `requirements.tsv` and
`traceability.tsv`; gaps discovered during that baseline become controlled problem reports.

## Purpose and scope

Casegraph applies DO-178C lifecycle disciplines to improve confidence in a domain-neutral evidence,
rules, and workflow platform. The software is not being developed for installation on an aircraft.
There is no certification basis, applicant, authority, system safety assessment, aircraft function,
or assigned DAL. Accordingly, this plan establishes repository quality controls but cannot be a
certification PSAC or obtain certification credit.

If Casegraph is proposed for airborne use, work stops at the integration boundary until the
applicant and certification authority approve a project-specific plan based on the purchased
DO-178C/ED-12C text and current errata. That transition must assign the software level from system
safety work, identify applicable supplements, establish independence, assess COTS/open-source and
Rust toolchain acceptability, and perform a gap analysis against every applicable objective.

## Lifecycle and data flow

1. A change starts with a uniquely identified high-level requirement or problem report.
2. Architecture/design analysis identifies affected low-level or derived requirements.
3. An approved change set updates requirements and traces before or with implementation.
4. Source, migrations, tests, user-facing documentation, and assurance data change coherently.
5. Verification uses requirements-based tests plus review and analysis; robustness cases address
   invalid inputs and boundary conditions.
6. An independent reviewer confirms the requirement, implementation, test adequacy, results,
   traceability, configuration impact, and problem-report disposition before a release baseline.
7. A signed Git tag identifies an approved release baseline. The configuration index and
   accomplishment summary record its contents and limitations.

## Lifecycle data and ownership

| Data | Author role | Verification role | Approval role |
|---|---|---|---|
| Plans and standards | Assurance lead | Independent reviewer | Project lead |
| High-level requirements | Product/system role | Verification reviewer | Project lead |
| Low-level/derived requirements and design | Developer | Independent reviewer | Project lead |
| Source and migrations | Developer | Reviewer plus automated checks | Maintainer |
| Test cases, procedures, and results | Verifier | Independent reviewer | Verification lead |
| Configuration index and release baseline | Configuration manager | QA reviewer | Project lead |
| Problem reports and dispositions | Any contributor | Verifier | QA/project lead |

One person may hold several roles during ordinary development, but that is not independent
verification. A release may be called an internal engineering baseline without independence; any
baseline claiming DO-178C verification credit requires a named reviewer who did not author the
item or its verification and who records approval.

## Requirements and derived requirements

`CASEGRAPH_FOUNDATION_SPEC.md` is the originating platform specification. Controlled high-level and
low-level software requirements are in `requirements.tsv`. Each requirement is singular, testable,
unambiguous in project context, and uniquely identified. Low-level requirements name a high-level
parent. A requirement introduced by architecture or implementation rather than directly allocated
from the platform specification is marked `derived=yes`, traced to its parent, and reviewed for
feedback to the product/system process.

No behavior-changing source change may be merged without an affected requirement, design/source
trace, requirements-based test, and impact analysis. Deleting or changing an identifier requires a
recorded change rationale; identifiers are never silently reused.

## DAL-dependent and certification-dependent items

The following are intentionally unclaimed until an airborne system context exists:

- software level/DAL and the resulting objective set;
- independence required for each objective;
- statement, decision, condition, MC/DC, or object-code coverage sufficiency;
- compiler/linker/test/tool qualification credit under DO-330;
- DO-332 object-oriented technology supplement applicability and additional objectives;
- certification authority coordination, reviews, conformity, and approval;
- target-computer timing, resource, hardware/software integration, and executable object-code
  verification;
- airborne safety, partitioning, robustness, deactivated code, parameter data, and field-loadable
  software determinations.

These are transition gates, not waived objectives.
