# Publishing & release runbook

The crate, the WASM/npm package, and docs.rs. Run from a clean `main` with green
CI.

## 0. Pre-flight

```sh
cargo test --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo fmt --all -- --check
cargo +1.70.0 check --no-default-features --features json,serde,query   # MSRV
cargo doc --no-deps --all-features                                      # docs build
cargo publish --dry-run                                                 # packaging
```

Bump the version in `Cargo.toml` and move the `CHANGELOG.md` "unreleased"
section under the new version. Commit, then tag:

```sh
git tag v0.2.0 && git push origin main --tags
```

## 1. crates.io

```sh
cargo login            # paste a token from https://crates.io/me
cargo publish          # the main library
```

docs.rs builds automatically from the published crate (config is already in
`Cargo.toml` under `[package.metadata.docs.rs]`, building all features).

## 2. npm (WASM bindings)

```sh
cd dom-tree-wasm
wasm-pack build --target web --release      # also: --target bundler / nodejs as needed
# sanity-check the gzipped size
gzip -9 -c pkg/dom_tree_wasm_bg.wasm | wc -c   # expect ~19-20 KB

cd pkg
npm login
npm publish --access public
```

The npm package name is `dom-tree-wasm` (from the crate name; edit
`pkg/package.json` first if you want a different name/scope).

## 3. Post-publish

- Confirm <https://docs.rs/dom-tree-rs> built and <https://crates.io/crates/dom-tree-rs>
  shows the new version.
- Confirm the README badges render (CI / crates.io / docs.rs).
- Then run the launch checklist (see the launch drafts handed over separately).
