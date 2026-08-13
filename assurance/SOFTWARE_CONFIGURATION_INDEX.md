# Software configuration index

Status: living index for the next internal assurance baseline.

The machine-readable item index is `configuration-index.tsv`. A release baseline supplements that
index with its immutable signed tag, commit, target triples, build profile, Rust version, `Cargo.lock`,
verification run URLs, open/deferred problem reports, binary names and SHA-256 hashes, and archive
location. Until those fields are recorded for a tag, the repository is controlled development data,
not a released assurance baseline.

The foundation source baseline preceding adoption of this process is Git commit `c9254bc`. It was
green in GitHub Actions but did not use the lifecycle controls introduced here and therefore is not
retroactively represented as a DO-178C-aligned release baseline.
