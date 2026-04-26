# Parity tracker

Applet-by-applet status of `jib` (this Rust binary) against the
Python implementation
([Real-Fruit-Snacks/mainsail](https://github.com/Real-Fruit-Snacks/mainsail)).

- ✅ ported and parity-tested via `tests/parity/run.py`
- 🟡 ported but with known gaps (see notes)
- ❌ not yet ported

Run `python tests/parity/run.py` to verify byte-for-byte match against the
Python implementation.

## Slim group (`--features slim`, 34 applets)

| Applet     | Status | Notes |
|------------|--------|-------|
| basename   | ✅ | |
| bools (true/false) | ✅ | |
| cat        | ✅ | |
| chmod      | ✅ | symbolic + octal modes; Windows toggles read-only only |
| cp         | ✅ | -r/-p/-a/-v/-i/-n/-u |
| cut        | ✅ | |
| date       | 🟡 | full strftime; no full TZ DB — `+%z` always emits the configured offset |
| dirname    | ✅ | |
| echo       | ✅ | |
| env        | ✅ | |
| find       | ✅ | -name/-type/-size/-mtime/-print/-delete/-exec etc |
| grep       | ✅ | -i/-v/-n/-r/-F/-E/-l/-c/-o/-w/-q/-A/-B/-C |
| head       | ✅ | |
| hostname   | ✅ | -s/-f/-I |
| ln         | ✅ | -s/-r/-f/-T/-v |
| ls         | ✅ | -l/-a/-A/-1/-R/-F/-S/-t/-r |
| mkdir      | ✅ | |
| mv         | ✅ | -f/-i/-n/-u/-v |
| printf     | ✅ | %d/i/o/u/x/X/e/E/f/g/G/c/s/b/% |
| pwd        | ✅ | |
| realpath   | ✅ | -e/-m/-s/-z/--relative-to |
| rm         | ✅ | |
| sed        | ✅ | s/d/p/q/=/y; addresses; ranges; -n/-E/-i |
| seq        | ✅ | |
| sleep      | ✅ | smhd suffixes |
| sort       | ✅ | -r/-n/-u/-f/-b, -k/-t/-o |
| stat       | ✅ | -c/-t/-L; full %a/%A/%n/%s etc |
| tail       | ✅ | -f via polling |
| tee        | ✅ | |
| touch      | ✅ | -t/-r/-d/-a/-m via the `filetime` crate |
| tr         | ✅ | -d/-s/-c/-t with ranges and POSIX classes |
| uname      | 🟡 | -s/-n/-m/-o; -r/-v/-p return "unknown" on stable Rust without libc |
| uniq       | ✅ | -c/-d/-u/-i, -f/-s/-w |
| wc         | ✅ | -l/-w/-c/-m |
| which      | ✅ | -a; PATHEXT on Windows |
| whoami     | ✅ | env-var fallback chain |
| xargs      | ✅ | -n/-L/-I/-d/-0/-r/-t/-a |

## Awk (M3, 1 applet)

| awk        | 🟡 | BEGIN/END, patterns, $field, NR/NF/FS/OFS, all operators, if/else/while/for/for-in, arrays, sub/gsub/match/length/substr/sprintf/split/index/toupper/tolower. **Not implemented**: user-defined functions, getline, regex FS (literal-char only), SUBSEP-based multidim arrays. |

## Hashing (`--features hashing`, 4 applets)

| md5sum, sha1sum, sha256sum, sha512sum | ✅ | -c/-b/--tag/--quiet/--status/--strict |

## Archives (`--features archives`, 5 applets)

| gzip       | ✅ | -d/-c/-k/-f/-1..-9 |
| gunzip     | ✅ | -c/-k/-f |
| tar        | ✅ | -c/-x/-t with -z/-f/-v/-C |
| zip        | ✅ | recursive |
| unzip      | ✅ | -l/-d |

## Disk (`--features disk`, 2 applets)

| du         | ✅ | -s/-h/-a/-c |
| df         | ✅ | -h/-T |

## Network (`--features network`, 3 applets)

| nc         | 🟡 | TCP only (UDP rejected); client / -l listen / -z scan |
| http       | ✅ | HTTP/1.1 client + HTTPS via `rustls` and the `webpki-roots` Mozilla bundle |
| dig        | ✅ | A/AAAA/MX/TXT/CNAME/NS/SOA/PTR/ANY; +short, -t, -x, --timeout, @server |

## JSON (`--features json`, 1 applet)

| jq         | 🟡 | Identity, fields (`.foo`, `.foo.bar`, `.["k"]`), index `.[0]`, slice `.[2:5]`, iterate `.[]`, optional `?`, pipe `\|`, comma `,`, parens, array/object constructors. **Arithmetic** (`+ - * / %`) with type-aware coercion (number/number, string concat, array concat, object merge, array-minus-array, string division → array of parts). **Comparisons** (`== != < <= > >=`) with jq's canonical type ordering (null < false < true < number < string < array < object). **Alternative** (`//`) — keep non-null/false LHS values, fall back to RHS otherwise. **Conditionals** `if/then/elif/else/end` with input-passthrough on no-else. Built-ins: length, keys, values, type, has, select, map, not, empty, tostring, tonumber, add, min, max, first, last, reverse, sort, unique, **split, join, startswith, endswith, ltrimstr, rtrimstr, ascii_downcase, ascii_upcase**. **Not implemented**: recursive descent (`..`), `path()`, `paths()`, `to_entries`, `from_entries`, `with_entries`, `floor`/`ceil`/`sqrt`, `any`/`all`/`isempty`, `ascii`, `explode`/`implode`, user functions. |

## Misc extras (`--features extras`, 19 applets)

| cmp, comm, dd, diff, expand, fmt, getopt, hexdump, join, mktemp, nl, od, paste, rev, split, tac, truncate, unexpand, yes | ✅ | All ported. `diff -u` uses the `similar` crate for Myers diff. |

## Tracker

- **Reference (mainsail v0.2.1):** 73 applets
- **`jib`:** 73 applets (full feature set)
- **`jib --features slim`:** 34 applets, ~545 KB release binary on Windows x64
- **Parity harness cases passing:** 116/116 across the basic / cut / sort / uniq / cat / printf / date / tr / grep / sed / awk / jq surfaces

The known 🟡 gaps are documented above and tracked in the upstream
`CHANGELOG.md` for follow-up versions. The jq surface now covers
arithmetic, comparisons, `//`, conditionals, and the common string
built-ins. Remaining jq follow-ups are recursive descent (`..`),
`to_entries`/`from_entries`/`with_entries`, math built-ins
(`floor`/`ceil`/`sqrt`), and user-defined functions.
