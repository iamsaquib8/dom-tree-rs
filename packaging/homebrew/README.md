# Homebrew

`domq` is distributed through a personal tap, so users can run:

```sh
brew install iamsaquib8/tap/domq
```

The formula installs the prebuilt binaries that
[`release.yml`](../../.github/workflows/release.yml) attaches to every GitHub
release — nothing is compiled on the user's machine:

| platform | release asset |
|---|---|
| macOS, Apple Silicon | `domq-v<version>-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `domq-v<version>-x86_64-apple-darwin.tar.gz` |
| Linux, arm64 | `domq-v<version>-aarch64-unknown-linux-musl.tar.gz` |
| Linux, x86_64 | `domq-v<version>-x86_64-unknown-linux-musl.tar.gz` |

The Linux binaries are statically linked (musl), so they run on any distro.

## Files

- [`domq.rb`](domq.rb) — the formula **template**. Its `sha256` values are
  placeholders (all zeros); do not publish it as is.
- [`fill-formula.sh`](fill-formula.sh) — prints the template with a real
  version and the checksums of a directory of release archives:
  `packaging/homebrew/fill-formula.sh 0.3.0 path/to/assets > domq.rb`.

## One-time setup: the tap

A tap is just a GitHub repository named `homebrew-<something>`; `brew` maps
`iamsaquib8/tap` to `github.com/iamsaquib8/homebrew-tap`.

1. Create a public repository **`iamsaquib8/homebrew-tap`** with a `Formula/`
   directory (an empty `README.md` is enough to start).
2. That's it — no tokens or secrets are involved. `brew tap iamsaquib8/tap`
   (or the first `brew install iamsaquib8/tap/domq`) clones it.

## Every release

1. Push a `v*` tag. `release.yml` builds the binaries, publishes the GitHub
   release, and then its **homebrew** job runs `fill-formula.sh` against the
   freshly built archives.
2. Open that workflow run: the job summary shows the filled-in formula, and the
   same file is attached to the run as the **`homebrew-formula`** artifact.
3. Copy it to `Formula/domq.rb` in `iamsaquib8/homebrew-tap` and commit:

   ```sh
   cd homebrew-tap
   cp ~/Downloads/domq.rb Formula/domq.rb   # from the artifact
   git commit -am "domq 0.3.0" && git push
   ```

4. Check it:

   ```sh
   brew update
   brew install iamsaquib8/tap/domq   # or: brew upgrade domq
   brew test domq
   brew audit --strict --online iamsaquib8/tap/domq
   ```

Pushing the formula to the tap automatically (a bot commit from `release.yml`)
would need a token with write access to `homebrew-tap` stored as a secret here;
the copy step is kept manual so this repository holds no such token.
