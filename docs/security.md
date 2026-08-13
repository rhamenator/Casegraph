# Security, privacy, and threat assumptions

## Current trust boundary

Source documents, filenames, HTTP bodies/headers, JSON/CSV/text content, configuration, and any
future model output are untrusted. Casegraph currently assumes one trusted local operator and one
service process. Authentication, tenant isolation, enterprise identity, and per-case authorization
are not implemented. Actor strings are audit labels, not authenticated principals.

The API binds to loopback by default. Do not expose it to an untrusted network. A future remote
deployment requires authenticated identity, authorization checks at application-service entry
points, TLS through a vetted reverse proxy, request rate limiting, CSRF/browser policy where
applicable, and encrypted storage/key management.

## Implemented controls

- External strings and canonical IDs are bounded and validated; JSON requests use typed schemas.
- API bodies have a configured upper limit; CLI rejects non-regular files and enforces the same
  application byte limit.
- Caller filenames never determine artifact paths. SHA-256-derived keys are validated, resolved
  beneath a dedicated root, written atomically, and re-hashed on duplicate/read.
- SQLite queries are parameterized. Foreign keys, strict tables, check constraints, immutable
  triggers, full synchronization, and migration checksums enforce critical boundaries.
- Rust workspace code forbids unsafe code. Fixed-point material arithmetic avoids floating-point
  monetary errors.
- Model invocation defaults disabled. Provider locality policy is checked before invocation; strict
  output schemas reject unknown/malformed values; failure records exclude prompts/raw output.
- Audit snapshots minimize content. Diagnostics have no arbitrary source-content field and carry
  correlation, stage, outcome, duration, and IDs only.
- Secrets use environment configuration and `.env` is ignored; the checked-in example has no
  secrets. No cloud credentials are required.
- Dependency versions are exact and locked; features and licenses are documented; CI uses locked
  builds, actions are commit-pinned, and Dependabot proposes reviewable updates.

## Threats and residual risks

- Local OS users with filesystem access can read the unencrypted SQLite database/artifacts. Use
  encrypted volumes and restrictive OS ACLs for sensitive deployments; application-layer
  encryption/key rotation is deferred.
- SHA-256 content addressing exposes equality of identical documents within one store. Per-tenant
  stores or encrypted object keys may be needed later.
- SQLite and the filesystem store are not a multi-node coordination design. Network filesystem
  semantics and concurrent processes are unsupported.
- Deterministic parsers bound body/file size but do not yet impose field/row-count limits independent
  of artifact size. The artifact cap bounds the immediate memory risk; finer quotas remain next work.
- The API has no authentication, rate limiting, TLS, or browser-origin policy. Loopback binding is a
  safety boundary, not production authorization.
- No malware scanning, PDF parser sandbox, OCR, archive extraction, or active-content handling is
  present. Unsupported bytes are stored but never executed.
- Audit records are append-only at the database boundary, but a database administrator can still
  alter files. Signed/tamper-evident audit chains and external retention are deferred.
- Dependency audit tooling is not bundled; CI dependency updates and lock review do not replace
  vulnerability monitoring.

Casegraph makes no claim of security certification, regulatory compliance, or suitability for
legally/medically/financially consequential autonomous action.
