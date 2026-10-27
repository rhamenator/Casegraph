# Casegraph .NET

A domain-neutral .NET 10 subproject of Casegraph. This is an incremental implementation, not feature parity with the Rust foundation. The original Rust workspace is unchanged.

Provides tenant-scoped entities, typed relationships, immutable encrypted revisions, source artifacts, observations, optimistic concurrency and content-free audit events over EF Core SQLite/PostgreSQL. Recruitment-specific rules belong to the ATS. Document readers belong to the independent DocumentExtraction package.

Run `dotnet test Casegraph.slnx`. PostgreSQL 18 has an opt-in live integration test (CASEGRAPH_TEST_POSTGRES); local synthetic live acceptance passed. Windows execution remains unverified locally. See ../../recruitment-ats/docs for the consumer's architecture and acceptance ledger (sibling repository on disk).

License: GPL-3.0-only, inherited from the parent repository. No external distribution or alternate license is assumed.

The optional schema validator declares field types/sensitivity and retains unknown data. Callers must enforce schema diagnostics and field access policy; declaring sensitivity alone does not enforce field authorization. Immutable workflow definitions and revisioned instances provide an explicit human-review transition gate; full Rust rule/claim parity is outside this initial slice. GraphStore is the tenant/security boundary; direct DbContext/SQL access is privileged infrastructure access. Bounded listing rejects results above 500 instead of silently truncating them; use ListPageAsync for larger sets.

Version 0.2.0 adds GraphOperationService: schema-versioned encrypted operation requests, stable tenant-scoped IDs, idempotent replay, optimistic claims, expiring fenced leases, retry backoff/caps and fixed failure classifications. Callers must transact their own database effects with CompleteAsync. External side effects are not made exactly-once. No extraction or recruitment dependency was added. See the ATS consumer's docs/REPROCESSING.md for an end-to-end application and acceptance evidence.

Version 0.3.0 adds provider-specific checked migrations (GraphMigrator and tools/Casegraph.Database), explicit legacy adoption, database-created revision snapshots, immutable evidence/history/audit guards and PostgreSQL runtime-role SQL generation. Normal initialization must not use EnsureCreated or EF Database.Migrate. Applied scripts are immutable; add ordered migrations rather than editing them. See the ATS consumer's docs/DATABASE-AND-INTAKE.md for deployment steps, acceptance and owner/runtime trust boundaries.

Version 0.4.0 adds bounded ScanReadyAsync discovery for queued work, due retries and expired leases. It returns IDs/revisions plus a continuation offset; callers must still claim before execution. Filters run after decryption within the requested tenant page; this is not an indexed queue or scheduler. No schema change or consumer dependency was added. See the ATS consumer docs/WORKER.md for the opt-in hosted adapter, acceptance and remaining deployment limits.

Version 0.5.0 adds AnalyzeDefinition for cycle-safe unreachable/terminal-state discovery and stricter workflow label validation. Domain-neutral workflows still permit unreachable states and optional review flags; consumers own their policy. Start/move check definition kind/shape. The ATS consumer adds draft editing and pins applications to existing immutable workflow.definition entities; see its docs/PIPELINES.md. No database migration, Rust change or extraction/recruitment dependency was added.

Version 0.6.0 adds EntityVersion and GetVersionAsync for exact immutable revision reads with tenant filtering and authenticated decryption. Missing versions fail rather than falling back to current; only the selected revision is loaded. No schema change or domain-specific assessment dependency was introduced. The ATS consumer uses version references for immutable source-backed staff scorecards; see its docs/SCORECARDS.md.

Version 0.7.0 adds GraphPage<T>, ReadPageAsync (bounded lookahead/explicit continuation) and AnyAsync (early-exit encrypted predicate traversal in batches of ten). Existing list APIs retain their contracts. Offsets do not supply snapshot semantics, and predicate search is linear rather than indexed. No database migration, consumer dependency or extraction change was added. See the ATS consumer docs/PAGINATION.md for UI/API usage, functional acceptance and limits.

Version 0.8.0 adds RecordVersionExportsAsync: a bounded distinct exact-version export audit batch with tenant validation and atomic inserts. No content is copied into audit. Outer-transaction callers must commit before releasing output; delivery acknowledgment is outside the journal. Existing audit actions/schema and extraction independence remain unchanged. The ATS consumer's scoped JSON adapter and limits are in docs/EVIDENCE-EXPORTS.md.

Version 0.9.0 adds trusted GraphContext Read/Write/Export capabilities and GraphAccessDeniedException enforcement across GraphStore reads, mutations and audited export commands. Existing source calls default to All. The host must derive context from trusted server identity; raw GraphDb/SQL remains privileged and field-level policies are not implemented. No schema/extraction dependency changed. See the ATS consumer docs/STAFF-ACCESS.md for named local roles and acceptance.
