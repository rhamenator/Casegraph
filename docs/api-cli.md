# API and CLI usage

API and CLI adapters call the same `CasegraphService`, `ExtractionPipeline`, and
`RuleWorkflowService`. They do not duplicate evidence, rule, or workflow decisions.

## CLI

All commands emit JSON on standard output; errors go to standard error and return nonzero.

```text
casegraph init
casegraph case create <title>
casegraph ingest <path> --case <case-id>
casegraph artifacts list --case <case-id>
casegraph claims list --case <case-id>
casegraph contradictions list --case <case-id>
casegraph query <case-id> <question>
casegraph verify <claim-id>
casegraph correct <claim-id> <corrected-text>
casegraph demo
casegraph serve
casegraph test
```

`ingest` accepts one regular local file, calculates/stores exact bytes, then deterministically
extracts supported text/JSON/CSV. Unsupported formats remain safely ingested and report extraction
as unsupported. The generic `correct` command treats its argument as text; typed corrections are
available through application/API JSON contracts.

`casegraph test` is a dependency/configuration smoke check and tells the developer to run the real
automated suite; it does not claim to replace `cargo test --workspace --locked`.

## HTTP API

Run `casegraph serve`. Defaults are `127.0.0.1:8080` and the configured maximum artifact body size.
Implemented routes are under `/api/v1`; `/health/live`, `/health/ready`, and `/openapi.json` are
unversioned operational/contract routes.

Raw artifact upload uses:

```http
POST /api/v1/cases/{case_id}/artifacts
Content-Type: text/plain
x-casegraph-source-key: client/stable/source-key
x-casegraph-filename: notice.txt

<exact bytes>
```

The body limit is applied before handler buffering. Header values are bounded and reject control
characters; filenames undergo application path validation. Upload and deterministic extraction are
separate API operations so an ingested unsupported artifact remains inspectable.

Other implemented routes cover cases, artifact versions, deterministic extraction, claims,
provenance, contradictions, verification, correction, rules, evaluation, tasks/workflow, and
grounded queries. Consult the served checked-in OpenAPI 3.1 document for exact paths. JSON request
types reject unknown fields where the adapter owns the schema.

API errors have `{ "error": { "code", "message" } }`. Storage/internal failures never expose
SQLite details or source contents. There is no authentication yet; see the security guide.

