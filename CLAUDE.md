# jib — agent notes

## User preferences (sticky)

- **No co-author signature on commits or PRs.** Do not append
  `Co-Authored-By: Claude …` lines. Do not add "Generated with Claude
  Code" footers in PR bodies. Commit messages and PR descriptions are
  the user's voice only.
- **Naming:** the binary, crate, and repo are all `jib`. The Python
  reference is `Real-Fruit-Snacks/mainsail`. Don't conflate them.

## Project shape

- `jib` is the Rust sister project of [Real-Fruit-Snacks/mainsail](https://github.com/Real-Fruit-Snacks/mainsail)
  (Python BusyBox-style multi-call binary). 78 applets at parity (count).
- Cargo features gate the applet groups: `slim` (34) / `extras` (24) /
  `archives` / `hashing` / `disk` / `network` / `json`. `full` enables
  everything.
- Behavior parity is verified by `python tests/parity/run.py`, which
  diffs stdout/rc against the upstream Python reference cloned to
  `tests/parity/mainsail-python/`. 116/116 cases match as of last run.

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

- `jq`: missing recursive descent (`..`), `to_entries`/`from_entries`/
  `with_entries`, math built-ins (`floor`/`ceil`/`sqrt`),
  `any`/`all`/`isempty`, user-defined functions.
- `awk`: no user-defined functions, no `getline`, regex `FS`, or
  SUBSEP-based multidim arrays.
- `uname -r/-v/-p` returns `"unknown"` on Windows (needs registry/WMI).
- `date %z`: no full TZ DB; offset is always whatever `utc_offset_secs`
  produces (currently 0).
- `id`/`groups`: best-effort without libc; IDs are zeros and the group
  is derived from the user name.
