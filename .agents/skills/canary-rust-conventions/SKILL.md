---
name: canary-rust-conventions
description: Conventions for writing or changing Rust in Canary's backend (errors, layering, async, types, tests, lints, docs, commits). Use for any backend/ Rust change.
---

# Canary Rust Conventions

Apply these to every Rust change under `backend/`. When code you touch does not follow them yet, bring the touched part in line, but do not widen the change beyond its purpose. Behaviour changes and refactors belong in separate commits.

## Gates

- `.agent-loop/checks.sh backend` must pass. It runs `cargo fmt --check`, `cargo clippy --all-targets --features system-tests -- -D warnings` and `cargo test`.
- `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` must pass.
- Lint policy lives in `[lints]` in `backend/Cargo.toml`. `clippy::pedantic` is on. Do not add crate-wide allows to get past a lint. For a local exception, use `#[expect(lint, reason = "...")]`, never a bare `#[allow]`.

## Layering

- `main.rs` is a thin binary. It parses config, builds `AppState`, spawns the background tasks from the library and serves the router. It uses `canary::...` and never declares modules itself.
- Handlers (`src/handlers/`) only extract input, call a service or repository, and map the result to a response. Business rules live in library modules.
- Database access goes through `MetadataDb` in `src/metadata/`. Never run rusqlite or r2d2 calls directly on an async task.
- All configuration is read once into `AppConfig` (`src/config/`). Do not call `std::env::var` outside config loading. Long-lived services (email, auth, HTTP clients) are built once and shared through `AppState`; do not build them per request.

## Errors

- Library code returns typed errors defined with `thiserror`, one enum per domain (electrum, wallet, db, provider and so on). Keep the source chain with `#[from]` or `#[source]`.
- Never classify errors by matching on message strings. Add a variant instead.
- `anyhow` is only for the binary/bootstrap edge. When you use it, add context with `.context()` / `.with_context()`, not `map_err(|e| anyhow!("...: {e}"))`.
- Do not use `Result<_, String>` or `Result<_, &str>`.
- HTTP handlers return `Result<T, ApiError>`, where `ApiError: IntoResponse` lives in `src/handlers/error.rs`. Internal errors are logged with detail and returned to the client as a generic message. Never put `e.to_string()` into a 5xx body.
- No `unwrap()` in production code. Use `expect("invariant: ...")` only for true invariants, such as static regexes, and state the invariant.

## Async

- Blocking work goes in `tokio::task::spawn_blocking`. That includes SQLite (BDK `persist`, `Connection::open`), password hashing (argon2) and other CPU-heavy work.
- Never hold a lock across `.await` on network I/O. Prefer immutable `Arc<T>` shared state over `Arc<Mutex<T>>` when it is only mutated at startup.
- Use `tracing` for all logging, with structured fields (`tracing::info!(wallet = %id, "synced")`). No `println!`/`eprintln!` outside tests and CLI output. No emoji in logs.

## Types

- Closed sets of values are enums, not strings: statuses, provider kinds, modes. Implement `Display`/`FromStr` or use serde `rename_all` for the DB and API representation.
- Use `PathBuf` for paths, `Option` instead of sentinel values, and newtypes where mixing IDs would be a bug.
- Prefer `std::sync::LazyLock`/`OnceLock` over `once_cell`/`lazy_static`.
- Use `async_trait` only for traits used as `dyn` objects (for example `NotificationProvider`); say so in a comment on the trait.

## Naming and style

- Follow the Rust API Guidelines: no `get_` prefix on getters, `as_`/`to_`/`into_` conversions, and `new` / `with_*` constructors.
- Use inline format args (`"{e}"`, not `"{}", e`).
- Default to the smallest visibility that works: `pub(crate)` unless the item is used by `tests/` or `system_tests/`.
- Keep functions short enough to read in one screen. Extract helpers instead of nesting deeper than about 4 levels. Keep a module under about 800 lines; when it grows past that, split it into a directory module by responsibility.

## Docs

- Every module starts with a `//!` doc saying what it owns.
- `pub` items get a `///` doc saying what they do. Add `# Errors` when the failure modes are not obvious.
- Comments explain *why*, not *what*. Do not leave references to previous implementations ("replaces the old ...").

## Tests

- Unit tests live inline in `#[cfg(test)] mod tests` next to the code they test.
- HTTP and database integration tests go in `backend/tests/<area>_tests.rs`. Shared helpers go in `backend/tests/common/mod.rs`; do not copy helpers between files.
- Docker/regtest scenarios go in `backend/system_tests/`. Each is a `[[test]]` target with `required-features = ["system-tests"]`, so plain `cargo test` does not build them. They run through `backend/run-system-tests.sh`.
- Test-only constructors and helpers are `#[cfg(test)]` or behind the `test-support` feature, never shipped as plain `pub`.
- Assert on behaviour, and do not `println!` in tests.

## Commits and PRs

- One logical change per commit. Commit subjects are imperative, sentence case and have no trailing period ("Move notification dispatch out of main").
- Update branches by rebasing on `origin/master`, not by merging master into them.
- PR descriptions say what changed and why, and how it was verified.
