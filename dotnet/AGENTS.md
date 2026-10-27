# .NET Casegraph continuation

Keep this subproject domain-neutral: no recruitment or extraction dependencies. Use GraphMigrator for supported initialization; EnsureCreated belongs only in deliberate legacy fixtures. Released/deployed migration SQL is immutable: add an ordered migration and matching checksums/version logic for future changes. Database triggers own revision snapshots; GraphStore owns actor audit and atomic write/savepoint boundaries. Preserve original Rust code and tests. Use only synthetic records; no external publication or production configuration is implied by local implementation/testing.

Keep relevant test projects in every .NET slnx. New behavior and fixes require meaningful tests alongside the code; use focused tests during development and appropriate provider integration checks before pushing completed changes. Keep schema constraints generic and non-coercing; sensitivity declarations alone do not enforce field access.
