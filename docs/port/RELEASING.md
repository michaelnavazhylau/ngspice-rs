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
the first release is a one-off. `0.1.0` was bootstrapped this way, published
by hand from the commit that added this workflow; steps 3–5 below still apply
until they are done.

1. Bump `version` in `Cargo.toml` and run the full local gate.
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

## Prebuilt binaries and cargo-binstall

The same tag also triggers
[`.github/workflows/release.yml`](../../.github/workflows/release.yml). It
builds `spice-rs` in release mode for `x86_64-unknown-linux-musl`,
`aarch64-unknown-linux-musl`, `x86_64-apple-darwin`, `aarch64-apple-darwin`
and `x86_64-pc-windows-msvc`. It then creates the GitHub release for the tag
(with generated notes) and attaches the archives and a `SHA256SUMS` file:

```
ngspice-rs-<target>-v<version>.tar.gz      (.zip for Windows)
└── ngspice-rs-<target>-v<version>/
    ├── spice-rs                            (spice-rs.exe on Windows)
    └── README.md, COPYING, NOTICE, AUTHORS
```

`[package.metadata.binstall]` in `Cargo.toml` points `cargo binstall
ngspice-rs` at these archives; glibc Linux hosts are mapped to the static musl
builds. binstall reads that metadata from the version published on crates.io,
so the names in the workflow and the manifest must change together. The
binaries job is independent of the crates.io publish job and needs no approval.

To rebuild or backfill the binaries for an existing tag, run *Release
binaries* by hand with that tag; existing assets are replaced.

## Cutting a release

1. Bump `version` in `Cargo.toml`, run `cargo update --workspace` so
   `Cargo.lock` records it, and run the local gate, including the
   `NGSPICE_BIN` live-C checks.
2. Merge the bump to `main`.
3. Optionally run the workflow by hand with `dry-run` checked.
4. Tag the merge commit on `main` and push: `git tag -a vX.Y.Z -m "ngspice-rs X.Y.Z"
   && git push origin vX.Y.Z`. This starts both the crates.io publish (approve
   it in the `crates-io` environment) and the binary release.

crates.io versions are permanent. A bad release can be yanked
(`cargo yank --version X.Y.Z`), but the version number cannot be reused.
