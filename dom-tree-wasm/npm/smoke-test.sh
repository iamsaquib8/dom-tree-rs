#!/usr/bin/env sh
# Pack pkg/ exactly as `npm publish` would, install the tarball into a scratch
# project, and run tests/node.mjs against it by package name.
#
#   wasm-pack build --target web --release && node npm/finish.mjs && sh npm/smoke-test.sh
set -eu
crate=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

(cd "$crate/pkg" && npm pack --silent --pack-destination "$work" >/dev/null)
cd "$work"
npm init -y >/dev/null
npm install --silent --no-audit --no-fund ./dom-tree-wasm-*.tgz
cp "$crate/tests/node.mjs" .
node node.mjs
