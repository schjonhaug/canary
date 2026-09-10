# Canary postcard feature patch

Source: crates.io `postcard` 1.1.3, MIT OR Apache-2.0. Both licenses and
upstream Rust source/tests are retained unchanged.

The only upstream manifest change is `default = ["heapless-cas"]` to
`default = []` in Cargo.toml. Cargo.toml.orig is retained as the pristine upstream
manifest for comparison. The registry crate's own
Cargo.lock is intentionally omitted; Canary's backend lockfile is authoritative.

phonenumber 0.3.10 enables postcard defaults in both its runtime and build
dependencies. This pulls heapless 0.7's atomic-polyfill, which is unmaintained
(RUSTSEC-2023-0089). Canary cannot disable another dependency's additive Cargo
features. phonenumber uses only `postcard::to_io` (with explicit `use-std`) and
`postcard::from_bytes`; neither needs the embedded heapless APIs.

This patch removes that unused feature and its dependency chain without changing
serialization code or phone-number metadata. Explicit heapless features remain
available upstream but are not enabled by Canary. Remove this patch when the
phone-number dependency disables unnecessary postcard defaults upstream, or a
maintained postcard release removes the unmaintained transitive dependency.

When refreshing, compare upstream source and licenses, reapply only the default
feature change, run backend checks including phone parsing, and run the full
dependency scan. Do not add an advisory suppression for this finding.
