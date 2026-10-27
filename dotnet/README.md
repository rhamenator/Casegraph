# Casegraph .NET

A domain-neutral .NET 10 subproject of Casegraph. This is an incremental implementation, not feature parity with the Rust foundation. The original Rust workspace is unchanged.

Provides tenant-scoped entities, typed relationships, immutable encrypted revisions, source artifacts, observations, optimistic concurrency and content-free audit events over EF Core SQLite/PostgreSQL. Recruitment-specific rules belong to the ATS. Document readers belong to the independent DocumentExtraction package.

Run `dotnet test Casegraph.slnx`. PostgreSQL 18 has an opt-in live integration test (CASEGRAPH_TEST_POSTGRES); local synthetic live acceptance passed. Windows execution remains unverified locally. See ../../recruitment-ats/docs for the consumer's architecture and acceptance ledger (sibling repository on disk).

License: GPL-3.0-only, inherited from the parent repository. No external distribution or alternate license is assumed.

The optional schema validator declares field types/sensitivity and retains unknown data. Callers must enforce schema diagnostics and field access policy; declaring sensitivity alone does not enforce field authorization. Immutable workflow definitions and revisioned instances provide an explicit human-review transition gate; full Rust rule/claim parity is outside this initial slice. GraphStore is the tenant/security boundary; direct DbContext/SQL access is privileged infrastructure access. Bounded listing rejects results above 500 instead of silently truncating them; use ListPageAsync for larger sets.

Version 0.2.0 adds GraphOperationService: schema-versioned encrypted operation requests, stable tenant-scoped IDs, idempotent replay, optimistic claims, expiring fenced leases, retry backoff/caps and fixed failure classifications. Callers must transact their own database effects with CompleteAsync. External side effects are not made exactly-once. No extraction or recruitment dependency was added. See the ATS consumer's docs/REPROCESSING.md for an end-to-end application and acceptance evidence.

Version 0.3.0 adds provider-specific checked migrations (GraphMigrator and tools/Casegraph.Database), explicit legacy adoption, database-created revision snapshots, immutable evidence/history/audit guards and PostgreSQL runtime-role SQL generation. Normal initialization must not use EnsureCreated or EF Database.Migrate. Applied scripts are immutable; add ordered migrations rather than editing them. See the ATS consumer's docs/DATABASE-AND-INTAKE.md for deployment steps, acceptance and owner/runtime trust boundaries.

Version 0.4.0 adds bounded ScanReadyAsync discovery for queued work, due retries and expired leases. It returns IDs/revisions plus a continuation offset; callers must still claim before execution. Filters run after decryption within the requested tenant page; this is not an indexed queue or scheduler. No schema change or consumer dependency was added. See the ATS consumer docs/WORKER.md for the opt-in hosted adapter, acceptance and remaining deployment limits.
