# Changelog

All notable changes to **jib** will be documented in this file.

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-04-26

Closes the entire [v0.2.0 milestone](https://github.com/Real-Fruit-Snacks/jib/milestone/1) (10/10 issues). Most of the documented `🟡` parity gaps from v0.1.0 are fixed, the applet count grows from 73 → 78, and the `extras` group nearly doubles.

### Added

**5 new applets** (all under the `extras` feature):

- `base64` — RFC 4648 encode/decode with `-d`/`-w COLS`/`-i`. Hand-rolled, no third-party dep.
- `fold` — wrap to width with `-w`/`-s`, plus `-N` POSIX shorthand.
- `column` — table mode (`-t`) with column auto-sizing and `-s SEP` for non-whitespace separators.
- `id` — `-u`/`-g`/`-G`/`-n` with combined-flag short blocks (`-un`, `-Gn`). Best-effort without libc.
- `groups` — equivalent to `id -Gn`.

**`jq` language** ([#2](https://github.com/Real-Fruit-Snacks/jib/issues/2), [#3](https://github.com/Real-Fruit-Snacks/jib/issues/3), [#4](https://github.com/Real-Fruit-Snacks/jib/issues/4), [#5](https://github.com/Real-Fruit-Snacks/jib/issues/5), [#6](https://github.com/Real-Fruit-Snacks/jib/issues/6)):

- Arithmetic operators `+ - * / %` with full type-aware coercion (number/number, string concat, array concat, object right-merge, array minus, string division → split, null as additive identity, div/mod by zero errors)
- Comparison operators `== != < <= > >=` with jq's canonical type ordering (`null < false < true < number < string < array < object`)
- Alternative operator `//` — keep non-null/false LHS values, fall through to RHS otherwise
- Conditionals `if/then/elif/else/end` with input-passthrough on no-`else`
- 8 string built-ins: `split`, `join`, `startswith`, `endswith`, `ltrimstr`, `rtrimstr`, `ascii_downcase`, `ascii_upcase`

### Changed

- **`http`** ([#1](https://github.com/Real-Fruit-Snacks/jib/issues/1)) — HTTPS support via `rustls` 0.23 + the `webpki-roots` Mozilla bundle. The HTTPS caveat in PARITY/README/docs is gone.
- **`date`** ([#8](https://github.com/Real-Fruit-Snacks/jib/issues/8)) — strftime, calendar math, and TZ resolution are now backed by `chrono` (with the `clock` feature for `iana-time-zone`). `+%z` now reflects the parsed input offset (was always `+0000`); `-d` accepts RFC 3339 with offsets, the space-instead-of-T variant, zone-naive forms, and the GNU `@<unix>` epoch extension.
- **`touch`** ([#10](https://github.com/Real-Fruit-Snacks/jib/issues/10)) — atime is now actually settable via the `filetime` crate (was a best-effort no-op pre-v0.2.0). `-a`/`-r`/`-d` all do the right thing on Linux/macOS/Windows.
- **CI** ([#7](https://github.com/Real-Fruit-Snacks/jib/issues/7)) — parity harness now runs on both `ubuntu-latest` and `macos-latest`; previously Ubuntu-only.

### Quality gates

- Parity harness: 76 → **126 cases** (100% match across Linux + macOS)
- Native integration tests: 29 → **31** (added `touch -a` and `touch -r` coverage)
- All `clippy --all-targets -- -D warnings` clean on both `full` and `slim` feature sets
- Release matrix: 11 native binaries (Linux x64 glibc + musl, Linux ARM64, Windows x64, Windows ARM64, macOS ARM64; full + slim where applicable)

### Known gaps remaining

Documented in [`PARITY.md`](PARITY.md). The remaining `🟡` markers are now the long-tail items:

- `jq`: recursive descent (`..`), `to_entries`/`from_entries`/`with_entries`, math built-ins (`floor`/`ceil`/`sqrt`), user-defined functions
- `awk`: user-defined functions, `getline`, regex `FS`, SUBSEP-based multidim arrays
- `uname -r/-v/-p` returns `"unknown"` on Windows (needs registry/WMI)
- `id`/`groups`: best-effort without libc — IDs are zeros, group derived from user name

## [0.1.0] - 2026-04-26

Initial release. A BusyBox-style multi-call binary in Rust — 73 Unix utilities in one ~2 MB executable, parity-tested against [Real-Fruit-Snacks/mainsail](https://github.com/Real-Fruit-Snacks/mainsail) (Python).

### Added

**File operations** (13)
- `ls`, `cp`, `mv`, `rm`, `mkdir`, `touch`, `find`, `chmod`, `ln`, `stat`, `truncate`, `mktemp`, `dd`

**Text processing** (29)
- `cat`, `tac`, `rev`, `grep`, `head`, `tail`, `wc`, `nl`, `sort`, `uniq`, `cut`, `paste`, `tr`, `sed`, `awk`, `tee`, `xargs`, `printf`, `echo`, `expand`, `unexpand`, `split`, `cmp`, `comm`, `diff`, `join`, `fmt`, `od`, `hexdump`

**JSON, network, hashing, archives** (13)
- `jq` (subset; ~20 built-ins, no arithmetic/comparisons/if-then-else yet)
- `http` (HTTP/1.1 only — HTTPS pending), `dig` (UDP, hand-rolled wire format), `nc` (TCP only)
- `md5sum`, `sha1sum`, `sha256sum`, `sha512sum`
- `tar`, `gzip`, `gunzip`, `zip`, `unzip`

**Filesystem, system, control** (18)
- `du`, `df`, `basename`, `dirname`, `realpath`, `pwd`, `which`
- `uname`, `hostname`, `whoami`, `date`, `env`, `sleep`, `getopt`
- `true`, `false`, `yes`, `seq`

### Architecture

- `argv[0]`-stem multi-call dispatch with `jib <applet>` wrapper mode
- BTreeMap-backed registry, lazily initialized via `OnceLock`
- Cargo features gate applet groups (`slim`/`extras`/`hashing`/`archives`/`disk`/`network`/`json`/`full`)
- One file per applet under `src/applets/`

### Quality gates

- 29 native integration tests
- Python↔Rust diff harness with 76 cases (100% match)
- CI on Linux/macOS/Windows; release matrix builds 11 native binaries
- `clippy --all-targets -- -D warnings` clean (with project-level allows for stylistic categories)
- `cargo fmt --check` clean

### Release artifacts (Linux/Windows/macOS, x64 + ARM64)

Both **full** (73 applets) and **slim** (34 POSIX coreutils — drops engines, archives, hashing, disk, network, JSON, extras) variants. Smallest binary: macOS ARM64 slim at 1.40 MB.

### Known gaps

Documented in [`PARITY.md`](PARITY.md). The largest are:
- `jq` lacks arithmetic, comparisons, `if/then/else`, `//`, several built-ins
- `http` is HTTP-only (no HTTPS yet)
- `awk` lacks user-defined functions, `getline`, regex `FS`, SUBSEP arrays
- `uname -r/-v/-p` returns `"unknown"` on Windows
- `touch -a` is best-effort; stable Rust lacks `set_accessed`
- `date %z` always emits `+0000`

[Unreleased]: https://github.com/Real-Fruit-Snacks/jib/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/Real-Fruit-Snacks/jib/releases/tag/v0.2.0
[0.1.0]: https://github.com/Real-Fruit-Snacks/jib/releases/tag/v0.1.0
