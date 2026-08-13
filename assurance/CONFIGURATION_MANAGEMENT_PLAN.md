# Software configuration management plan

## Identification and control

Git is the configuration-control system. `configuration-index.tsv` identifies controlled item
classes. Source, requirements, designs, plans, standards, tests, fixtures, migrations, toolchain,
lockfile, CI, problem reports, release index, and accomplishment summary are configuration items.
The canonical repository is `https://github.com/rhamenator/Casegraph`; `main` is the integration
branch.

Each change has a commit identity and, for release baselines, a reviewed pull request or equivalent
signed review record. Commit history is not rewritten after baseline publication. Applied database
migrations are immutable and checksum verified; a schema change adds a numbered migration and an
upgrade test. Dependency changes update `Cargo.toml`, `Cargo.lock`, the dependency register, impact
analysis, licenses, and verification evidence together.

## Baselines and releases

A baseline is reproducible only when it identifies:

- immutable Git commit and signed tag;
- requirements and traceability revisions at that commit;
- Rust toolchain and complete Cargo lockfile;
- target platform(s), build profile, commands, and environment assumptions;
- all resolved problem reports and accepted open/deferred reports;
- CI and independent-review evidence;
- generated binaries and cryptographic hashes when distributing executables.

Tags use `assurance-baseline-X.Y.Z` for internal assurance baselines and `vX.Y.Z` for product
releases. A tag without the configuration index, accomplishment summary, green checks, and approval
is not an assurance baseline. Tags do not imply airborne certification.

## Change control and status accounting

Changes originate from controlled requirements or `CG-PR-NNN` problem reports. Pull requests record
affected requirements, safety/security/data impact, derived requirements, tests, tool changes,
migration/data compatibility, and reviewer independence. `problem-reports.tsv` is the status ledger;
closed entries link immutable verification evidence, while deferred entries state impact and release
acceptance.

`casegraph-assurance` validates identifiers, parentage, paths, named tests, status values, and the
configuration index. Git and GitHub retain author, reviewer, commit, check, and timestamp history.
Before release, the configuration manager compares the index to the tree, confirms a clean worktree,
records dependencies and advisories, and archives the tag plus evidence outside the working copy.

## Archive, recovery, and unauthorized change

The GitHub remote is not the sole long-term certification archive. A certification project must
define retention duration, access controls, media, redundant storage, restoration tests, signatures,
and authority access. For current internal baselines, GitHub plus a separately retained bundle of
the signed tag, source archive, CI logs, dependency metadata, and binary hashes is recommended.
Unexpected baseline differences are treated as major problem reports until impact is established.
