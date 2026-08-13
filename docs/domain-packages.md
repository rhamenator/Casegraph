# Domain package extension guide

Core owns the `DomainPackage`, `RuleContribution`, and `DomainRegistry` contracts. A package reports
a stable ID/version and contributes versioned rules. The artificial
`sample-administrative-case` package contributes one invented response rule; it contains no real
eligibility, legal, medical, insurance, financial, or government policy.

Core crates do not depend on the sample package. An empty registry is tested, and domain/application
tests compile independently. The infrastructure integration suite includes the package only as a
development dependency to prove the extension path.

Future compatible contributions can add schemas, entity types, deterministic extractors,
normalizers, workflows, terminology, templates, validators, and jurisdiction configuration through
new core-owned contracts. Packages must not bypass provenance validation, write storage directly,
invent confidence, or weaken core authorization/audit boundaries.

The registry is currently process-local and explicit. Package discovery, signed manifests,
compatibility ranges, configuration persistence, and runtime loading are deferred.

