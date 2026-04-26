# Changelog

All notable changes to **jib** will be documented in this file.

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Tracked under the [v0.2.0 milestone](https://github.com/Real-Fruit-Snacks/jib/milestone/1):

- **`http`**: HTTPS support via `rustls` ([#1](https://github.com/Real-Fruit-Snacks/jib/issues/1))
- **`jq`**: arithmetic ([#2](https://github.com/Real-Fruit-Snacks/jib/issues/2)), comparisons ([#3](https://github.com/Real-Fruit-Snacks/jib/issues/3)), `if/then/elif/else/end` ([#4](https://github.com/Real-Fruit-Snacks/jib/issues/4)), `//` alternative ([#5](https://github.com/Real-Fruit-Snacks/jib/issues/5)), string built-ins ([#6](https://github.com/Real-Fruit-Snacks/jib/issues/6))
- **CI**: parity harness on Linux + macOS ([#7](https://github.com/Real-Fruit-Snacks/jib/issues/7))
- **Stretch**: `chrono`-backed `date` ([#8](https://github.com/Real-Fruit-Snacks/jib/issues/8)); 5 small upstream-only applets ([#9](https://github.com/Real-Fruit-Snacks/jib/issues/9)); `touch -a` via `filetime` ([#10](https://github.com/Real-Fruit-Snacks/jib/issues/10))

Long tail tracked in [#11](https://github.com/Real-Fruit-Snacks/jib/issues/11).

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

[Unreleased]: https://github.com/Real-Fruit-Snacks/jib/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Real-Fruit-Snacks/jib/releases/tag/v0.1.0
