<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/Real-Fruit-Snacks/jib/main/docs/assets/logo-dark.svg">
  <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/Real-Fruit-Snacks/jib/main/docs/assets/logo-light.svg">
  <img alt="jib" src="https://raw.githubusercontent.com/Real-Fruit-Snacks/jib/main/docs/assets/logo-dark.svg" width="420">
</picture>

![Rust](https://img.shields.io/badge/language-Rust-dea584.svg)
![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20Windows%20%7C%20macOS-lightgrey)
![Arch](https://img.shields.io/badge/arch-x86__64%20%7C%20ARM64-blue)
![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Tests](https://img.shields.io/badge/tests-29%20unit%20%2B%2076%20parity-brightgreen.svg)

A BusyBox-style multi-call binary in Rust — **73 Unix utilities**, one ~2 MB executable, native on Linux, Windows, and macOS. Sister project to [Real-Fruit-Snacks/mainsail](https://github.com/Real-Fruit-Snacks/mainsail) (Python), with a verified-parity test harness.

[Download Latest](https://github.com/Real-Fruit-Snacks/jib/releases/latest)
&nbsp;·&nbsp;
[GitHub Pages](https://real-fruit-snacks.github.io/jib/)
&nbsp;·&nbsp;
[Parity tracker](PARITY.md)

</div>

---

## Quick Start

**From a release** — no toolchain required:

```bash
# Linux (glibc — Ubuntu, Debian, RHEL, …)
curl -LO https://github.com/Real-Fruit-Snacks/jib/releases/latest/download/jib-linux-x64
chmod +x jib-linux-x64
./jib-linux-x64 --version
```

**From source** — Rust 1.75+:

```bash
git clone https://github.com/Real-Fruit-Snacks/jib.git
cd jib
cargo build --release
./target/release/jib --list
```

**Wire up multi-call dispatch** — symlink (or hardlink) the binary to any applet name and call it directly:

```bash
ln -s jib ls && ./ls -la                # multi-call: argv[0] basename
ln -s jib cat && echo hi | ./cat        # works for every applet
```

---

## Pre-built artifacts

Every release tag (`v0.x.x`) ships **12 native binaries** built and verified by GitHub Actions:

| Target                           | Full _(73 applets)_       | Slim _(34 applets — POSIX coreutils)_ |
|----------------------------------|---------------------------|---------------------------------------|
| Linux x86_64 (glibc 2.35+)       | `jib-linux-x64`           | `jib-linux-x64-slim`                  |
| Linux x86_64 **musl** (Alpine)   | `jib-linux-x64-musl`      | _(use full or build slim locally)_    |
| Linux ARM64 (glibc)              | `jib-linux-arm64`         | `jib-linux-arm64-slim`                |
| Windows x86_64                   | `jib-windows-x64.exe`     | `jib-windows-x64-slim.exe`            |
| Windows ARM64                    | `jib-windows-arm64.exe`   | `jib-windows-arm64-slim.exe`          |
| macOS ARM64 (Apple Silicon)      | `jib-macos-arm64`         | `jib-macos-arm64-slim`                |

Drop any binary anywhere on `PATH` and run.

### Build your own subset

Everything is gated behind Cargo features. Pick what you want:

```bash
cargo build --release                                                # full (73 applets, ~2 MB)
cargo build --release --no-default-features --features slim          # 34 POSIX coreutils, ~545 KB
cargo build --release --no-default-features --features "slim,hashing,extras"   # custom
```

Feature groups: `slim` (39 POSIX core minus 5 engines = 34), `extras` (19 BusyBox parity), `hashing` (4), `archives` (5), `disk` (2), `network` (3), `json` (1). `full` enables everything.

---

## Features

### One binary, seventy-three utilities

Every common POSIX tool you'd reach for in a shell pipeline — plus `jq` for JSON, `http` for HTTP, `dig` for DNS, `nc` for TCP, and the BusyBox parity gap-fillers (`dd`, `od`, `hexdump`, `diff`, `join`, `fmt`, …). Dispatch via `jib <applet>` or symlink/hardlink to call the applet directly.

```bash
jib ls -la                           # GNU-style flags
jib cat file.txt | jib grep -C 2 pattern
jib find . -name '*.rs' -size +1k -mtime -7
jib seq 100 | jib sort -rn | jib head -5
```

### Native Windows

No WSL, no Cygwin, no git-bash. `jib.exe` runs on bare Windows and recognises Windows-native command names as aliases.

```cmd
jib dir .                           :: == ls
jib type file.txt                   :: == cat
jib copy a.txt b.txt                :: == cp
jib del old.txt                     :: == rm
jib where cargo                     :: == which
```

### Real applets, not stubs

Each applet implements the common POSIX flags and edge cases.

- `find` — expression tree with `-exec`, `-prune`, `-and`/`-or`, parens, size/time predicates, `-delete`
- `sed` — `s///`, `d`, `p`, `q`, `=`, `y///`, addresses, ranges, negation, `-i` in-place edit, BRE + ERE
- `awk` — BEGIN/END, `/regex/` and expression patterns, range patterns, `print`/`printf`, full control flow, associative arrays, the standard built-ins (`length`, `substr`, `index`, `split`, `sub`, `gsub`, `match`, `toupper`, `tolower`, `sprintf`, `int`)
- `jq` — practical subset: pipes, comma, object & array constructors, slices and iterators, **20+ built-in functions** (`select`, `map`, `keys`, `values`, `length`, `type`, `sort`, `unique`, `add`, `min`, `max`, …), raw output (`-r`), compact (`-c`), slurp (`-s`)
- `http` — `GET`/`POST`/`PUT`/`DELETE`/`HEAD`, custom headers, body literal or `@file`, `--json` shortcut, redirect handling, `-f` for HTTP errors. _HTTP only — HTTPS via `rustls` is on the roadmap._
- `dig` — direct UDP DNS queries: A, AAAA, MX, TXT, CNAME, NS, SOA, PTR; `+short`; reverse lookups via `-x`
- `sort` — `-k` key fields, `-t` custom separator, `-o` output file, numeric/reverse/unique
- `tar` — create/extract/list with optional gzip (`-z`); accepts traditional (`cvfz`) and dashed (`-cvfz`) flag forms

```bash
jib find . -name '*.tmp' -delete
jib sed -i 's/foo/bar/g' *.txt
jib awk -F, '{s+=$3} END{print s/NR}' data.csv
jib jq '.servers[] | select(.region) | .name' inventory.json
jib http -H 'Authorization: Bearer $TOKEN' http://api.example.com/me
jib dig MX gmail.com +short
jib sort -k 3,3n -t , data.csv
jib tar -czf src.tar.gz src/
```

### Pipeline-grade I/O

Binary-safe through `cat`/`tee`/`gzip`. CRLF survives Windows text-mode round-trips. `tail -f` follows files and detects rotation. `xargs` accepts `-0` to handle Windows backslashes.

```bash
jib find . -type f -print0 | jib xargs -0 jib sha256sum
jib tail -f /var/log/app.log
jib gzip -c data.bin | jib gunzip > data.bin.copy
```

### Verified parity with the Python `mainsail`

The repo ships a Python↔Rust diff harness that runs every test case against both implementations and asserts byte-for-byte equality of stdout and exit codes. Currently **76/76 cases match** across `cat`, `cut`, `sort`, `uniq`, `printf`, `date`, `tr`, `grep`, `sed`, `awk`, and the trivial applets. Run:

```bash
git clone https://github.com/Real-Fruit-Snacks/mainsail.git tests/parity/mainsail-python
python tests/parity/run.py
```

---

## Supported applets

| Category    | Applets |
|-------------|---------|
| File ops    | `ls` `cp` `mv` `rm` `mkdir` `touch` `find` `chmod` `ln` `stat` `truncate` `mktemp` `dd` |
| Text        | `cat` `tac` `rev` `grep` `head` `tail` `wc` `nl` `sort` `uniq` `cut` `paste` `tr` `sed` `awk` `tee` `xargs` `printf` `echo` `expand` `unexpand` `split` `cmp` `comm` `diff` `join` `fmt` `od` `hexdump` |
| **JSON**    | **`jq`** _(filters, fields, iteration, constructors, 20+ built-ins; arithmetic + comparison + `if/then/else` are on the roadmap)_ |
| **Network** | **`http`** _(HTTP/1.1 client; HTTPS pending `rustls`)_ • **`dig`** _(UDP DNS, hand-rolled wire format)_ • **`nc`** _(TCP netcat: connect, listen, port-scan)_ |
| Hashing     | `md5sum` `sha1sum` `sha256sum` `sha512sum` |
| Archives    | `tar` `gzip` `gunzip` `zip` `unzip` |
| Filesystem  | `du` `df` |
| Paths       | `basename` `dirname` `realpath` `pwd` `which` |
| System      | `uname` `hostname` `whoami` `date` `env` `sleep` `getopt` |
| Control     | `true` `false` `yes` `seq` |

Run `jib --list` for the full set with one-line descriptions, or `jib <applet> --help` for per-applet usage and flags. See [`PARITY.md`](PARITY.md) for the per-applet status against the Python upstream.

---

## Architecture

```
src/
├── main.rs         # entry → cli::run → ExitCode
├── lib.rs          # crate root
├── cli.rs          # dispatch: argv[0] multi-call + wrapper modes
├── registry.rs     # Applet table + OnceLock<BTreeMap> lookup
├── usage.rs        # per-applet --help bodies
├── common.rs       # shared helpers: err, filemode, uid_gid, …
└── applets/        # one file per applet
    ├── ls.rs       #   pub const APPLET: Applet { name, help, aliases, main }
    ├── cat.rs
    └── …           # 73 modules total
```

**Four-layer flow:**

1. **Entry** — `main.rs` collects `env::args` and calls `cli::run`. The exit code is clamped to a `u8` `ExitCode`.
2. **Dispatch** — `cli.rs` checks `argv[0]`'s lowercased stem against the registry (multi-call mode); falls through to `jib <applet> [args...]` otherwise. Intercepts `--help` only — `-h` is reserved for applet flags like `df -h`.
3. **Registry** — `registry.rs` builds a `BTreeMap<&str, &'static Applet>` once via `OnceLock`, indexing every applet's canonical name and aliases.
4. **Applet** — receives `argv` as `&[String]`, returns an `i32` exit code (`0` success, `1` runtime error, `2` usage error). Reads bytes via `io::stdin().lock()`; writes bytes via `io::stdout().lock()`.

Adding an applet means dropping a file under `src/applets/<name>.rs` exposing `pub const APPLET: Applet`, plus an entry in `src/applets/mod.rs`'s `ALL` slice under the right feature gate.

---

## Cargo features

| Feature   | Default | Applets | Notes |
|-----------|---------|---------|-------|
| `slim`    | yes     | 34      | POSIX coreutils + `grep`/`sed`/`awk`/`find`/`tr` (uses `regex`) |
| `extras`  | yes     | 19      | `dd`/`od`/`hexdump`/`fmt`/`getopt`/`split`/`diff`/`join`/`yes`/`nl`/`tac`/`rev`/`cmp`/`comm`/`expand`/`unexpand`/`paste`/`mktemp`/`truncate` (uses `similar` for diff) |
| `hashing` | yes     | 4       | `md5sum`/`sha1sum`/`sha256sum`/`sha512sum` (`md-5`, `sha1`, `sha2`) |
| `archives`| yes     | 5       | `gzip`/`gunzip`/`tar`/`zip`/`unzip` (`flate2`, `tar`, `zip`) |
| `disk`    | yes     | 2       | `du`/`df` |
| `network` | yes     | 3       | `nc`/`http`/`dig` (HTTPS in `http` is pending) |
| `json`    | yes     | 1       | `jq` subset (see PARITY.md) |
| `full`    | yes     | 73      | All groups |

---

## Development

```bash
cargo build                                  # debug build
cargo test                                   # 29 native integration tests
cargo clippy --all-targets -- -D warnings    # zero-warning gate
cargo build --release                        # ~2 MB stripped binary
```

### Parity testing

```bash
git clone --depth 1 https://github.com/Real-Fruit-Snacks/mainsail.git \
    tests/parity/mainsail-python
python tests/parity/run.py
```

The harness runs both implementations against a manifest of test cases and diffs `stdout` and exit codes. CI runs the same harness on Ubuntu against every push.

---

## Why "jib"?

A jib is the triangular sail forward of the mainsail — smaller, faster, more agile, and works alongside the main sail to drive the boat. Felt fitting for a Rust port that's a leaner companion to the Python `mainsail` rather than a successor.

## License

MIT.
