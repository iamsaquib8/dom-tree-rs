# Security policy

## Supported versions

Security fixes are released for the latest minor version of each package:

| package | supported |
|---|---|
| `domq` (crate) | 0.3.x |
| `domq-cli` (crate / binaries) | 0.3.x |
| `domq` (npm) | 0.4.x |

## What counts as a vulnerability

domq parses untrusted input by design, so these are treated as security
issues:

- a **panic**, abort, or stack overflow on any input (HTML or selector string);
- an **infinite loop**, or time/memory use far out of proportion to the input
  size (denial of service);
- anything that makes the CLI read or write files other than the ones it was
  given.

The crate contains no `unsafe` code (`#![forbid(unsafe_code)]`), so memory
corruption in the library itself is not expected.

**Not in scope:** domq is **not an HTML sanitizer**. It is a forgiving,
non-WHATWG parser, and its tree can differ from what a browser builds. Output
that is dangerous when inserted into a web page is not a vulnerability in this
crate. If you need to clean untrusted HTML, use
[`ammonia`](https://crates.io/crates/ammonia).

## Reporting

Please report privately, not in a public issue:

1. Preferred: GitHub's **[Report a vulnerability](https://github.com/iamsaquib8/domq/security/advisories/new)**
   form (Security tab → Advisories).
2. Or email the maintainer at the address in `Cargo.toml` (`authors`).

Include the input that triggers the problem (attach it as a file if it's large
or binary), the version, and the target (native, wasm, CLI). You'll get an
acknowledgement within a week. Once a fix is released, the issue is disclosed
in a GitHub security advisory and credited to you unless you'd rather not be
named.
