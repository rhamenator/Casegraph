# Configuration, diagnostics, and operations

Configuration is read from `CASEGRAPH_*` environment variables. `.env.example` documents safe
defaults; the process does not automatically read `.env` files.

| Variable | Default | Meaning |
|---|---|---|
| `CASEGRAPH_DATA_DIR` | `.casegraph` | Dedicated SQLite/artifact root; filesystem roots are rejected. |
| `CASEGRAPH_BIND_ADDR` | `127.0.0.1:8080` | API bind address. Non-loopback is an explicit insecure choice until auth/TLS exist. |
| `CASEGRAPH_MAX_ARTIFACT_BYTES` | `26214400` | Byte and HTTP-body cap, validated between 1 byte and 1 GiB. |
| `CASEGRAPH_MODEL_POLICY` | `disabled` | `disabled`, `local-only`, or `allow-listed-remote`. No concrete provider ships. |
| `CASEGRAPH_LOG_FORMAT` | `json` | `json` or `pretty`; JSON is the production-oriented default. |

Startup opens artifact storage, configures SQLite, validates every applied migration checksum, and
applies pending migrations before constructing API state. A migration/storage failure prevents
readiness. Disabled optional models never fail readiness.

`/health/live` reports process responsiveness. `/health/ready` reports deterministic composition
readiness. The current readiness response does not continuously re-probe disk capacity or database
writeability; active dependency probing is a known next step.

Operational diagnostics are content-minimized JSON records with correlation ID, stable stage,
outcome, timestamp, optional duration/target, and failure category. Evidentiary provenance and audit
records remain separate. Persisted provenance/audit/rule records carry correlation IDs across
ingestion, extraction, evidence creation, evaluation, and workflow effects. The schema reserves
pipeline-run/failure records, but automatic stage-duration persistence is a documented next step.

Back up the SQLite database together with the artifact directory as one consistency unit. Online
backup/restore tooling, retention, encryption keys, disaster recovery objectives, and object-store
replication are not implemented.
