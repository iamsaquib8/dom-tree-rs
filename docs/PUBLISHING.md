# Publishing & release runbook

The library crate, the CLI crate and its prebuilt binaries, the WASM/npm package,
and docs.rs. Run from a clean `main` with green CI. Keep all versions in lock-step
(`Cargo.toml`, `cli/Cargo.toml`, `wasm/Cargo.toml`).

## 0. Pre-flight

```sh
cargo test --workspace --all-features
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo fmt --all -- --check
cargo +1.88.0 check --no-default-features --features json,serde,query,markdown,std,derive  # MSRV
cargo doc --no-deps --all-features                                      # docs build
cargo publish --dry-run                                                 # packaging
```

Bump the version in `Cargo.toml` and move the `CHANGELOG.md` "unreleased"
section under the new version. Commit, then tag:

```sh
git tag v0.4.0 && git push origin main --tags
```

## 1. crates.io

```sh
cargo login                  # paste a token from https://crates.io/me
cargo publish -p domq-derive # the derive macro first (the library's `derive` feature needs it)…
cargo publish -p domq        # …then the library…
cargo publish -p domq-cli    # …then the CLI, which depends on it
```

Pushing the `v*` tag runs `.github/workflows/release.yml`, which attaches
prebuilt `domq` binaries and `SHA256SUMS` to the GitHub release (and
`cargo binstall domq-cli` picks them up).

docs.rs builds automatically from the published crate (config is already in
`Cargo.toml` under `[package.metadata.docs.rs]`, building all features).

## 2. npm (WASM bindings)

**Automated:** pushing a `v*` tag runs the `npm` job in
`.github/workflows/release.yml`, which builds, smoke-tests and publishes with
npm *trusted publishing* (OIDC, with provenance) — no token in the repo. One-time
setup: npmjs.com → `domq` → Settings → Trusted publishing → GitHub
Actions, repository `iamsaquib8/domq`, workflow `release.yml`,
environment `npm`. The manual steps below are the fallback.

The npm version comes from `wasm/Cargo.toml`; bump it there. The
package is one `--target web` build plus a small Node entry point
(`npm/node.js`) and an `"exports"` map that `npm/finish.mjs` adds. Don't build
with `--target nodejs` or `bundler`: the one package serves every runtime.

```sh
cd wasm
wasm-pack build --target web --release
node npm/finish.mjs                             # Node entry point + "exports" in pkg/package.json
wasm-pack test --node                           # binding tests
sh npm/smoke-test.sh                            # npm-pack pkg/, install it, run tests/node.mjs
gzip -9 -c pkg/domq_wasm_bg.wasm | wc -c    # expect ~41 KB (CI gate: 46080 bytes)

cd pkg
npm pack --dry-run                              # expect 7 files, incl. node.js and the .wasm
npm login
npm publish --access public
```

The npm package is `domq-wasm`, like the crate. npm refuses plain `domq` as too
similar to an existing package (`dom-q`). Up to 0.3.0 it was published as
`dom-tree-wasm`, which is now deprecated in favour of `domq-wasm`.

The playground (<https://iamsaquib8.github.io/domq/>) redeploys by
itself on pushes to `main` that touch `src/`, `wasm/` or `playground/`
(`.github/workflows/pages.yml`). It needs a one-time setting: Settings → Pages
→ Build and deployment → Source: **GitHub Actions**.

## 3. Post-publish

- Confirm <https://docs.rs/domq> built and <https://crates.io/crates/domq>
  shows the new version.
- Confirm the README badges render (CI / crates.io / docs.rs).
- Then run the launch checklist (see the launch drafts handed over separately).
