# Extraction pipeline contracts

## Implemented deterministic flow

```text
immutable artifact version
  -> exact media-type classification
  -> verified byte recovery
  -> raw UTF-8 extraction
  -> structural fields and locations
  -> domain-neutral scalar interpretation
  -> exact normalization
  -> domain validation
  -> transactional provenance/observation/claim/evidence creation
```

`ExtractionPipeline` accepts explicitly registered `DeterministicExtractor` implementations. An
extractor declares stable name/version and supported formats; it returns candidates but cannot
persist data or generate identifiers. The pipeline routes every candidate through the same
`CasegraphService` used by future API and CLI adapters.

The core extractor supports:

- UTF-8 plain text lines in `key: value` form, with paragraph and exact byte span;
- flat JSON objects with scalar values and source-field location; nested values are not converted to
  claims;
- RFC-4180-like CSV rows with quoted fields, escaped quotes, equal-width validation, and row/column
  location.

Normalization recognizes exact ISO dates, booleans, signed integers, fixed-point decimals,
USD values written with `$`, and explicit `null`/`unknown`. Values that do not match a deterministic
form remain text. Original representations always remain in claim and provenance records.

Unsupported media types, invalid UTF-8, malformed JSON/CSV, mismatched CSV width, no observations,
validation rejection, and internal failures are distinct. Ingested evidence is not discarded when
extraction cannot proceed. PDF text/layout, images/OCR, DOCX/XLSX, nested JSON mappings, and other
encodings are responsibly deferred rather than routed through a prompt.

## Optional reasoning

`RawReasoningProvider` is vendor-neutral and identifies provider/model/version/configuration plus
local or remote locality. `ReasoningGateway` checks deployment policy before invocation, accepts
only explicitly redacted context, and validates a strict JSON schema with unknown fields denied.
Semantic validation repeats span ordering and required-key checks.

Disabled, policy-denied, unavailable, malformed-output, and validation failures are recorded through
`ReasoningFailureSink`. Safe records contain identity, category, message, and correlation only—never
prompt context or raw output. Typed interpreted claims are still untrusted candidates and are not
authoritative facts. No concrete model adapter is included, and deterministic health does not
depend on one.

