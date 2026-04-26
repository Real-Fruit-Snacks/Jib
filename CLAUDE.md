# mainsail (Rust) — agent notes

## User preferences (sticky)

- **No co-author signature on commits or PRs.** Do not append
  `Co-Authored-By: Claude …` lines. Do not add "Generated with Claude
  Code" footers in PR bodies. Commit messages and PR descriptions are
  the user's voice only.

## Project shape

- Rust port of [Real-Fruit-Snacks/mainsail](https://github.com/Real-Fruit-Snacks/mainsail)
  (Python BusyBox-style multi-call binary). 73 applets at parity (count).
- Cargo features gate the applet groups: `slim` (34) / `extras` /
  `archives` / `hashing` / `disk` / `network` / `json`. `full` enables
  everything.
- Behavior parity is verified by `python tests/parity/run.py`, which
  diffs stdout/rc against the upstream Python reference cloned to
  `tests/parity/mainsail-python/`. 76/76 cases match as of last run.

## Repository conventions

- Add an applet: new file in `src/applets/<name>.rs` exposing
  `pub const APPLET: Applet`, plus an entry under the right feature
  gate in `src/applets/mod.rs`'s `ALL` slice.
- File names that collide with Rust keywords or std modules use a
  trailing underscore: `env_.rs`, `sleep_.rs`, `hostname_.rs`,
  `gzip_.rs`, `tar_.rs`, `zip_.rs`. Module declarations use the same
  trailing underscore; applet names (the strings exposed to the CLI)
  do not.
- Per-applet `--help` bodies live in `src/usage.rs`.

## Known gaps (tracked in `PARITY.md`)

- `http`: HTTP/1.1 only — HTTPS needs `rustls`.
- `jq`: missing arithmetic, comparisons, `if/then/else`, `//`,
  recursive descent, several string built-ins.
- `awk`: no user-defined functions, no `getline`, regex `FS`, or
  SUBSEP-based multidim arrays.
- `uname -r/-v/-p` returns `"unknown"` on Windows (needs registry/WMI).
- `touch -a`: stable Rust lacks `set_accessed`; atime is best-effort.
- `date %z`: no full TZ DB; offset is always whatever `utc_offset_secs`
  produces (currently 0).
