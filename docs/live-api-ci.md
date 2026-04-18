# Live API CI

This repository uses two GitHub Actions workflows for CI:

- `.github/workflows/ci.yml`
  Runs the main validation suite on pushes to `main`, pull requests, manual dispatches, and a weekly maintenance schedule.
- `.github/workflows/live-daily.yml`
  Runs the public live Figshare smoke test every day and exposes the authenticated draft-only smoke as a manual opt-in job.

## Main CI

The main workflow covers:

- formatting with `cargo fmt --all --check`
- link validation for the README, this document, and workflow files
- docs with `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features --locked`
- MSRV validation against Rust `1.86`
- clippy with `-D warnings`
- tests on Linux, macOS, and Windows
- a Linux feature matrix for default, `all-features`, and `native-tls`
- an extra Windows `native-tls` smoke test
- `cargo publish --dry-run --locked`
- `cargo semver-checks check-release --release-type patch`
- `cargo audit`
- tarpaulin coverage with a `90%` floor

## Live API Checks

The live workflow has two jobs:

- `live-public`
  Runs the ignored public smoke test from `tests/live_public.rs` every day.
- `live-account`
  Runs the ignored authenticated smoke test from `tests/live_account.rs` only when manually requested and only when `FIGSHARE_TOKEN` is configured as a repository secret.

The authenticated smoke is intentionally draft-only. It is allowed to create temporary draft state, upload a file, verify private download behavior, and then clean up the draft again.

## Required Secret

Authenticated live checks require:

- `FIGSHARE_TOKEN`

Do not paste the token into workflow inputs or commit it into the repository.

## Local Parity

The closest local commands are:

```bash
cargo fmt --all --check
cargo test --all-features --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features --locked
cargo publish --dry-run --locked
```
