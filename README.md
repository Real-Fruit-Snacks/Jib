<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/Real-Fruit-Snacks/jib/main/docs/assets/logo-dark.svg">
  <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/Real-Fruit-Snacks/jib/main/docs/assets/logo-light.svg">
  <img alt="Jib" src="https://raw.githubusercontent.com/Real-Fruit-Snacks/jib/main/docs/assets/logo-dark.svg" width="100%">
</picture>

> [!IMPORTANT]
> **A BusyBox-style multi-call binary in Rust** — 78 Unix utilities, one ~2 MB executable, native on Linux, Windows, and macOS. Sister project to [`mainsail`](https://github.com/Real-Fruit-Snacks/mainsail) (Python) with a verified-parity test harness.

> *A jib is the triangular sail forward of the mainsail — smaller, faster, more agile, and works alongside the main sail to drive the boat. Felt fitting for a Rust port that's a leaner companion to the Python `mainsail` rather than a successor.*

---

## §1 / Premise

[`mainsail`](https://github.com/Real-Fruit-Snacks/mainsail) is the Python reference implementation — easy to embed, easy to read, easy to extend. The single thing it can't be is **small** and **fast-startup at the same time**: every invocation pays the Python cold-start price.

Jib is the leaner Rust companion. Same applet roster (78 at parity count), same flag conventions, same exit codes, same byte-for-byte stdout — verified by a Python↔Rust diff harness that runs both implementations against every test case in CI. **Native on Windows** without WSL, Cygwin, or git-bash. **Cargo features** gate the applet groups so you can ship a 545 KB binary with the POSIX core, or the full ~2 MB binary with `jq` + `http` + `dig`.

---

## §2 / Specs

| KEY      | VALUE                                                                       |
|----------|-----------------------------------------------------------------------------|
| BINARY   | One **~2 MB stripped executable** · 545 KB `slim` build · static via musl  |
| APPLETS  | **78 POSIX utilities** + `jq` (subset) + `http` (HTTPS) + `dig` (DNS)       |
| BUILDS   | **12 release binaries** — Linux/Windows/macOS · x86_64 + ARM64 · full+slim  |
| FEATURES | `slim` · `extras` · `hashing` · `archives` · `disk` · `network` · `json`    |
| TESTS    | 29 native integration tests · **76+ parity cases** vs Python `mainsail`     |
| STACK    | Rust **1.75+** · Cargo · `rustls` for HTTPS · zero-warning clippy gate     |

Per-applet status against the Python upstream in [`PARITY.md`](PARITY.md). Architecture in §5 below.

---

## §3 / Quickstart

```bash
# From a release — no toolchain required
curl -LO https://github.com/Real-Fruit-Snacks/jib/releases/latest/download/jib-linux-x64
chmod +x jib-linux-x64
./jib-linux-x64 --list

# From source — Rust 1.75+
git clone https://github.com/Real-Fruit-Snacks/jib && cd jib
cargo build --release
./target/release/jib --list

# Multi-call dispatch — symlink any applet name
ln -s jib ls && ./ls -la                   # multi-call: argv[0] basename
ln -s jib cat && echo hi | ./cat           # works for every applet
```

```bash
# Native Windows — no WSL / Cygwin / git-bash
jib dir .                                  :: == ls
jib type file.txt                          :: == cat
jib copy a.txt b.txt                       :: == cp
jib del old.txt                            :: == rm
jib where cargo                            :: == which
```

```bash
# Build subsets via Cargo features
cargo build --release                              # full (78 applets, ~2 MB)
cargo build --release --no-default-features \
                      --features slim              # 34 POSIX coreutils, ~545 KB
cargo build --release --no-default-features \
                      --features "slim,hashing,extras"   # custom mix
```

---

## §4 / Reference

```
APPLET CATEGORIES                                       # 78 total

  FILE OPS      ls cp mv rm mkdir touch find chmod ln stat truncate mktemp dd
  TEXT          cat tac rev grep head tail wc nl sort uniq cut paste tr
                sed awk tee xargs printf echo expand unexpand split cmp
                comm diff join fmt od hexdump
  JSON          jq (filters · iterators · constructors · 20+ builtins · -r/-c/-s)
  NETWORK       http (HTTPS via rustls) · dig (UDP DNS) · nc (TCP)
  HASHING       md5sum · sha1sum · sha256sum · sha512sum
  ARCHIVES      tar (gzip) · gzip · gunzip · zip · unzip
  FILESYSTEM    du · df
  PATHS         basename · dirname · realpath · pwd · which
  SYSTEM        uname · hostname · whoami · date · env · sleep · getopt
  CONTROL       true · false · yes · seq

DISPATCH

  jib <applet> [args]                      # subcommand form
  ln -s jib <applet>                       # multi-call: argv[0] basename
                                           # both dispatch identically
                                           # Windows aliases: dir/type/copy/del/where

CARGO FEATURES                                          # gate the applet groups

  slim       (default)   34 applets        POSIX coreutils + grep/sed/awk/find/tr
  extras     (default)   24 applets        BusyBox parity gap-fillers
  hashing    (default)    4 applets        md5/sha1/sha256/sha512sum
  archives   (default)    5 applets        gzip/gunzip/tar/zip/unzip
  disk       (default)    2 applets        du/df
  network    (default)    3 applets        nc/http/dig (HTTPS via rustls)
  json       (default)    1 applet         jq subset (see PARITY.md)
  full       (default)   78 applets        All groups

RELEASE BINARIES                                        # 12 per release tag

  Linux x86_64 (glibc 2.35+)               jib-linux-x64 · -slim
  Linux x86_64 musl (Alpine)               jib-linux-x64-musl
  Linux ARM64 (glibc)                      jib-linux-arm64 · -slim
  Windows x86_64                           jib-windows-x64.exe · -slim
  Windows ARM64                            jib-windows-arm64.exe · -slim
  macOS ARM64 (Apple Silicon)              jib-macos-arm64 · -slim

NOTABLE FLAG SUPPORT
  find          expression tree · -exec · -prune · -and/-or · parens
                size/time predicates · -delete
  sed           s/// d p q = y/// addresses ranges negation -i in-place BRE+ERE
  awk           BEGIN/END · /regex/ · expressions · ranges · printf · arrays · 12+ builtins
  jq            pipes · comma · constructors · slices · iterators · 20+ builtins
                -r raw · -c compact · -s slurp
  http          GET/POST/PUT/DELETE/HEAD · -H · -d · @file · --json · -f
                HTTPS via rustls + Mozilla CA roots
  tar           create / extract / list · -z gzip · traditional + dashed flags

DEVELOPMENT
  cargo build                              Debug build
  cargo test                               29 native integration tests
  cargo clippy --all-targets -- -D warnings   Zero-warning gate
  cargo build --release                    ~2 MB stripped binary
  python tests/parity/run.py               Python ↔ Rust diff harness
```

---

## §5 / Architecture

```
src/
  main.rs           entry → cli::run → ExitCode
  lib.rs            crate root
  cli.rs            dispatch · argv[0] multi-call + wrapper modes
  registry.rs       Applet table + OnceLock<BTreeMap> lookup
  usage.rs          per-applet --help bodies
  common.rs         shared helpers · err · filemode · uid_gid
  applets/          one file per applet · 78 modules
    ls.rs           → pub const APPLET: Applet { name, help, aliases, main }
    cat.rs
    …
```

**Four-layer flow:** `main.rs` → `cli::run` (dispatch) → `registry` (lookup) → applet `main(&[String]) -> i32`. Adding an applet means a new file under `src/applets/<name>.rs` exposing `pub const APPLET: Applet`, plus an entry under the right feature gate in `applets/mod.rs`'s `ALL` slice. File names that collide with Rust keywords or std modules use a trailing underscore (`env_.rs`, `sleep_.rs`).

The Python parity harness clones `mainsail` into `tests/parity/mainsail-python/`, runs both implementations against a manifest of test cases, and asserts byte-for-byte equality of stdout and exit codes. **76/76 cases match** as of last run across `cat`, `cut`, `sort`, `uniq`, `printf`, `date`, `tr`, `grep`, `sed`, `awk`, and the trivial applets.

---

[License: MIT](LICENSE) · [Parity tracker](PARITY.md) · [Changelog](CHANGELOG.md) · Part of [Real-Fruit-Snacks](https://github.com/Real-Fruit-Snacks) — building offensive security tools, one wave at a time. Sibling: [mainsail](https://github.com/Real-Fruit-Snacks/mainsail) (Python) · [topsail](https://github.com/Real-Fruit-Snacks/topsail) (Go) · [staysail](https://github.com/Real-Fruit-Snacks/Staysail) (Zig) · [moonraker](https://github.com/Real-Fruit-Snacks/Moonraker) (Lua) · [rill](https://github.com/Real-Fruit-Snacks/rill) (NASM).
