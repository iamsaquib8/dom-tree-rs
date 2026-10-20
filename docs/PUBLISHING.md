# Publishing & release runbook

The library crate, the CLI crate and its prebuilt binaries, the WASM/npm package,
and docs.rs. Run from a clean `main` with green CI. Keep all versions in lock-step
(`Cargo.toml`, `cli/Cargo.toml`, `dom-tree-wasm/Cargo.toml`).

## 0. Pre-flight

```sh
cargo test --workspace --all-features
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo fmt --all -- --check
cargo +1.85.0 check --no-default-features --features json,serde,query,markdown,std  # MSRV
cargo doc --no-deps --all-features                                      # docs build
cargo publish --dry-run                                                 # packaging
```

Bump the version in `Cargo.toml` and move the `CHANGELOG.md` "unreleased"
section under the new version. Commit, then tag:

```sh
git tag v0.3.0 && git push origin main --tags
```

## 1. crates.io

```sh
cargo login                  # paste a token from https://crates.io/me
cargo publish                # the library first…
cargo publish -p domtree-cli # …then the CLI, which depends on it
```

Pushing the `v*` tag runs `.github/workflows/release.yml`, which attaches
prebuilt `domtree` binaries and `SHA256SUMS` to the GitHub release (and
`cargo binstall domtree-cli` picks them up).

docs.rs builds automatically from the published crate (config is already in
`Cargo.toml` under `[package.metadata.docs.rs]`, building all features).

## 2. npm (WASM bindings)

The npm version comes from `dom-tree-wasm/Cargo.toml`; bump it there. The
package is one `--target web` build plus a small Node entry point
(`npm/node.js`) and an `"exports"` map that `npm/finish.mjs` adds. Don't build
with `--target nodejs` or `bundler`: the one package serves every runtime.

```sh
cd dom-tree-wasm
wasm-pack build --target web --release
node npm/finish.mjs                             # Node entry point + "exports" in pkg/package.json
wasm-pack test --node                           # binding tests
sh npm/smoke-test.sh                            # npm-pack pkg/, install it, run tests/node.mjs
gzip -9 -c pkg/dom_tree_wasm_bg.wasm | wc -c    # expect ~38 KB (CI gate: 40960 bytes)

cd pkg
npm pack --dry-run                              # expect 7 files, incl. node.js and the .wasm
npm login
npm publish --access public
```

The npm package name is `dom-tree-wasm` (from the crate name). The existing
0.1.0 on npm is an older, different API; 0.3.0 replaces it, and the package
README has an "Upgrading from 0.1 or 0.2" section.

The playground (<https://iamsaquib8.github.io/dom-tree-rs/>) redeploys by
itself on pushes to `main` that touch `src/`, `dom-tree-wasm/` or `playground/`
(`.github/workflows/pages.yml`). It needs a one-time setting: Settings → Pages
→ Build and deployment → Source: **GitHub Actions**.

## 3. Post-publish

- Confirm <https://docs.rs/dom-tree-rs> built and <https://crates.io/crates/dom-tree-rs>
  shows the new version.
- Confirm the README badges render (CI / crates.io / docs.rs).
- Then run the launch checklist (see the launch drafts handed over separately).
