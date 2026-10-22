#!/usr/bin/env bash
# Print packaging/homebrew/domq.rb with a real version and real sha256s.
#
#   packaging/homebrew/fill-formula.sh <version> <assets-dir> > domq.rb
#
# <version> is without the leading "v" (e.g. 0.3.0); <assets-dir> holds the
# release archives built by .github/workflows/release.yml
# (domq-v<version>-<target>.tar.gz). Used by the `homebrew` job in
# release.yml; runs on Linux (sha256sum) and macOS (shasum).
set -euo pipefail

if [ $# -ne 2 ]; then
  echo "usage: $0 <version> <assets-dir>" >&2
  exit 2
fi
version="$1"
assets="$2"
template="$(dirname "$0")/domq.rb"
targets="aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-musl x86_64-unknown-linux-musl"

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

formula="$(cat "$template")"
# The template's own header comment describes the template, not the result.
formula="$(printf '%s\n' "$formula" | sed '/^# --- template notes/,/^# --- end template notes ---$/d')"
formula="$(printf '%s\n' "$formula" | sed -E "s/^(  version )\"[^\"]*\"/\1\"${version}\"/")"

for target in $targets; do
  archive="${assets}/domq-v${version}-${target}.tar.gz"
  if [ ! -f "$archive" ]; then
    echo "missing release asset: $archive" >&2
    exit 1
  fi
  sum="$(sha256 "$archive")"
  formula="$(printf '%s\n' "$formula" | sed -E "s/sha256 \"[0-9a-f]{64}\" # PLACEHOLDER: ${target}\$/sha256 \"${sum}\"/")"
done

if printf '%s\n' "$formula" | grep -q 'PLACEHOLDER'; then
  echo "some sha256 placeholders were not filled in" >&2
  exit 1
fi
printf '%s\n' "$formula"
