# Contributing

Thanks for your interest! This is a small, focused crate — contributions that
keep it small and focused are very welcome.

## Two load-bearing principles

1. **Never panic on input.** `parse` must always return a best-effort tree for
   *any* byte sequence. No `unwrap`/`expect`/`panic`/indexing on input-derived
   data — the lib denies those clippy lints. Use `get(..)` and iterators. New
   code should be exercised by the proptest suite and ideally the fuzz harness.
2. **Stay tiny and zero-dependency by default.** The default build has no
   external runtime dependencies. New dependencies must be optional, behind a
   feature, and justified. Prefer hand-rolled, `no_std`-friendly code.

## Scope

This is intentionally a *forgiving* parser, not a WHATWG-compliant one. Proposals
to add the full tree-construction algorithm (adoption agency, foster parenting,
auto `<html>`/`<head>`/`<body>`) are out of scope — that's what `html5ever` is
for. Bug fixes, more entity coverage, more implicit-close cases, performance, and
ergonomics are all in scope.

## Development

```sh
cargo test --all-features        # unit + corpus snapshots + proptest + doctests
cargo clippy --all-features --all-targets -- -D warnings
cargo fmt --all

# #[derive(FromHtml)]: end-to-end tests, then the macro's compile-fail tests
# (after changing a diagnostic: TRYBUILD=overwrite cargo test -p dom-tree-derive)
cargo test --features derive
cargo test -p dom-tree-derive

# CLI (workspace member)
cargo test -p domtree-cli
cargo run -p domtree-cli -- --help

# WASM
cd dom-tree-wasm && wasm-pack build --target web --release && wasm-pack test --node

# Playground (after building the wasm into playground/pkg — see playground/README.md)
python3 -m http.server -d playground 8000

# Benchmarks (see benchmarks/README.md); the memory report; the simd feature
cd benchmarks && cargo bench
cd benchmarks && cargo run --release --example memory
cargo test --features simd

# Fuzzing (nightly)
cargo +nightly fuzz run parse
cargo +nightly fuzz run differential   # our tree vs html5ever on the claimed subset
```

Snapshot tests use [`insta`]: after an intentional output change, run
`INSTA_UPDATE=always cargo test --test corpus` and review the diff before
committing the updated `.snap` files.

[`insta`]: https://insta.rs/

## Before opening a PR

- `cargo fmt`, `cargo clippy` clean, tests pass.
- Add a test for the behaviour you changed (a corpus fixture is great for
  parsing changes).
- Keep the MSRV at 1.85 (edition 2024); the CI `msrv` job enforces it.
- Add an entry to `CHANGELOG.md` for anything user-visible.
