# Releasing ngspice-rs to crates.io

`ngspice-rs` is published as a single crate by
[`.github/workflows/publish.yml`](../../.github/workflows/publish.yml). The
`xtask` workspace member has `publish = false` and is never uploaded.

## What the workflow does

1. **Release gate** (`verify` job, no credentials):
   - checks that the pushed tag equals `v` + `version` in `Cargo.toml`;
   - `cargo fmt --all -- --check`;
   - `cargo clippy --workspace --all-targets --locked -- -D warnings`;
   - `cargo test --workspace --locked`;
   - `cargo xtask golden verify`;
   - `cargo publish --dry-run --locked`, which builds the packaged crate in
     isolation, then lists the packaged files in the log.
2. **Publish** (`publish` job): runs only after the gate passes, in the
   `crates-io` GitHub environment, and uploads with `cargo publish --locked`.

The opt-in `NGSPICE_BIN` live-C checks are not part of the workflow: CI has no
C reference build. Run them locally before tagging (see
[VERIFICATION.md](VERIFICATION.md)).

The package contents are set by `include` in `Cargo.toml`: `src/`, `examples/`,
the manifest and lockfile, `README.md`, `RUST_PORT.md` and the licence and
attribution files (`COPYING`, `NOTICE`, `AUTHORS`). `conformance/`, `tests/`,
`docs/` and `xtask/` stay in the repository only. Check the list with
`cargo package --list`.

## Triggers

| Trigger | Result |
| --- | --- |
| push tag `vX.Y.Z` | gate, then publish |
| Actions → Publish → Run workflow, `dry-run` checked (default) | gate only |
| Actions → Publish → Run workflow, `dry-run` unchecked | gate, then publish the ref's `Cargo.toml` version |

## Authentication: crates.io Trusted Publishing

The publish job exchanges a GitHub OIDC token for a short-lived crates.io token
via `rust-lang/crates-io-auth-action`. No API token is stored in the
repository.

Trusted Publishing can only be configured for a crate that already exists, so
the first release is a one-off:

1. Bump `version` in `Cargo.toml` (it is `0.0.0` until the first release),
   run the full local gate, and merge.
2. From a clean checkout of that commit, publish once by hand:
   `cargo login` with a scoped API token, then `cargo publish --locked`.
   Tag the commit `vX.Y.Z` afterwards; the tag-triggered run will then fail at
   upload because the version already exists, which is expected for this
   release only.
3. On crates.io → `ngspice-rs` → Settings → Trusted Publishing, add a GitHub
   publisher: owner `michaelnavazhylau`, repository `ngspice-rs`, workflow
   `publish.yml`, environment `crates-io`.
4. Optionally revoke the API token used in step 2.
5. In GitHub → Settings → Environments, create `crates-io` (the first workflow
   run also creates it) and add required reviewers if publishes should need
   approval.

## Cutting a release

1. Bump `version` in `Cargo.toml`, run `cargo update --workspace` so
   `Cargo.lock` records it, and run the local gate, including the
   `NGSPICE_BIN` live-C checks.
2. Merge the bump to `main`.
3. Optionally run the workflow by hand with `dry-run` checked.
4. Tag and push: `git tag vX.Y.Z && git push origin vX.Y.Z`.

crates.io versions are permanent. A bad release can be yanked
(`cargo yank --version X.Y.Z`), but the version number cannot be reused.
