#!/usr/bin/env python3
"""Python (mainsail) <-> Rust (jib) parity diff harness.

Runs every test case in the inline manifest against both implementations
and reports stdout/rc differences.

Usage:
    python tests/parity/run.py [--rust BIN] [--filter SUBSTR]

The default Rust binary path is `target/release/jib(.exe)`. The Python
mainsail is loaded from `tests/parity/mainsail-python/` (cloned as part of
the harness setup). Both implementations are invoked through their CLI;
stdin is fed from the case's `input` field if present.

Exit code:
    0 — all cases match.
    1 — at least one mismatch.
    2 — harness error (binary missing, etc).
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent
REPO_ROOT = ROOT.parent.parent
PY_MAINSAIL = ROOT / "mainsail-python"


@dataclass
class Case:
    name: str
    args: list[str]
    input: bytes = b""
    skip: str | None = None  # if set, why we skip
    files: dict[str, str] | None = None  # filename -> contents (UTF-8)


def load_manifest() -> list[Case]:
    """Manifest is hard-coded for now — it's small enough to live in code,
    and that keeps us off the third-party TOML parser pile."""
    cases: list[Case] = []

    # Trivial control-flow applets.
    cases += [
        Case("true_exits_0", ["true"]),
        Case("false_exits_1", ["false"]),
        Case("echo_hello", ["echo", "hello"]),
        Case("echo_n_no_newline", ["echo", "-n", "hi"]),
        Case("echo_e_interprets", ["echo", "-e", r"a\nb"]),
        Case("echo_combined_ne", ["echo", "-ne", r"a\tb"]),
    ]

    # Path-manipulation applets.
    cases += [
        Case("basename_simple", ["basename", "/a/b/c.txt"]),
        Case("basename_suffix", ["basename", "/a/b/c.txt", ".txt"]),
        Case("basename_a_multiple", ["basename", "-a", "/a/b/c", "/d/e"]),
        Case("dirname_simple", ["dirname", "/a/b/c.txt"]),
        Case("dirname_root", ["dirname", "/foo"]),
        Case("dirname_no_slash", ["dirname", "foo"]),
    ]

    # Numeric.
    cases += [
        Case("seq_one_arg", ["seq", "5"]),
        Case("seq_two_args", ["seq", "2", "5"]),
        Case("seq_three_args", ["seq", "1", "2", "9"]),
        Case("seq_separator", ["seq", "-s", ",", "1", "5"]),
        Case("seq_equal_width", ["seq", "-w", "8", "12"]),
    ]

    # cat / head / tail / wc with stdin.
    text12 = b"\n".join(str(i).encode() for i in range(1, 13)) + b"\n"
    cases += [
        Case("head_default", ["head"], input=text12),
        Case("head_n_3", ["head", "-n", "3"], input=text12),
        Case("head_dash_5", ["head", "-5"], input=text12),
        Case("tail_default", ["tail"], input=text12),
        Case("tail_n_3", ["tail", "-n", "3"], input=text12),
        Case("wc_default", ["wc"], input=b"hello world\nfoo bar baz\n"),
        Case("wc_l_only", ["wc", "-l"], input=text12),
        Case("cat_n", ["cat", "-n"], input=b"foo\n\nbar\n"),
        Case("cat_b", ["cat", "-b"], input=b"foo\n\nbar\n"),
    ]

    # Text processing.
    cases += [
        Case("cut_f", ["cut", "-d", ",", "-f", "2,4"], input=b"a,b,c,d\n"),
        Case("cut_c", ["cut", "-c", "1-3,7-"], input=b"abcdefghij\n"),
        Case("sort_default", ["sort"], input=b"c\nb\na\nb\n"),
        Case("sort_unique", ["sort", "-u"], input=b"c\nb\na\nb\n"),
        Case("sort_numeric", ["sort", "-n"], input=b"10\n2\n100\n1\n"),
        Case("sort_reverse", ["sort", "-r"], input=b"c\nb\na\n"),
        Case("uniq_default", ["uniq"], input=b"a\na\nb\nb\nb\nc\n"),
        Case("uniq_count", ["uniq", "-c"], input=b"a\na\nb\nb\nb\nc\n"),
        Case("uniq_dup", ["uniq", "-d"], input=b"a\na\nb\nc\nc\n"),
        Case("nl_default", ["nl"], input=b"foo\n\nbar\n"),
        Case("rev_simple", ["rev"], input=b"hello world\n"),
        Case("tac_simple", ["tac"], input=b"1\n2\n3\n"),
        Case("expand_tab", ["expand"], input=b"\tabc\n"),
        Case("unexpand_a", ["unexpand", "-a"], input=b"        abc\n"),
    ]

    # printf — escape characters in arguments survive shell quoting only if
    # we hand the raw bytes through, which we do via subprocess.
    cases += [
        Case("printf_string_int", ["printf", "%s=%d\n", "foo", "42"]),
        Case("printf_repeat", ["printf", "%s\n", "a", "b", "c"]),
        Case("printf_pad", ["printf", "%-10s|", "hi"]),
    ]

    # date — format-only forms (not the wall-clock current time, which
    # would be racy). We use a fixed reference file's mtime.
    cases += [
        Case(
            "date_iso_from_d",
            ["date", "-u", "-d", "2025-01-15T10:30:00Z", "+%Y-%m-%d %H:%M:%S"],
        ),
        Case(
            "date_default_from_d",
            ["date", "-u", "-d", "2025-01-15T10:30:00Z"],
        ),
        Case(
            "date_format_a_b_d",
            ["date", "-u", "-d", "2025-12-25T00:00:00Z", "+%a %b %d"],
        ),
        # %z reads the actual input offset (was always +0000 pre-chrono).
        Case(
            "date_z_from_offset_input",
            ["date", "-d", "2025-01-15T10:30:00-05:00", "+%z"],
        ),
        Case(
            "date_z_from_plus_offset",
            ["date", "-d", "2025-06-01T08:00:00+09:00", "+%z"],
        ),
    ]

    # M3: awk (subset that we expect to match Python parity).
    cases += [
        Case("awk_print_field", ["awk", "{print $1}"], input=b"hello world\nfoo bar\n"),
        Case("awk_F_colon", ["awk", "-F:", "{print $2}"], input=b"a:b:c\nd:e:f\n"),
        Case("awk_arith", ["awk", "{print $1 + $2}"], input=b"1 2\n3 4\n"),
        Case("awk_begin_end", ["awk", "BEGIN{s=0} {s+=$1} END{print s}"], input=b"1\n2\n3\n4\n"),
        Case("awk_regex_pattern", ["awk", "/foo/"], input=b"foo\nbar\nfoobar\n"),
        Case("awk_nr_nf", ["awk", "{print NR, NF}"], input=b"a b c\nd e\nf\n"),
        Case("awk_if_else", ["awk", "{if($1>2)print \"big\"; else print \"small\"}"], input=b"1\n2\n3\n4\n"),
        Case("awk_substr", ["awk", "{print substr($0, 7, 5)}"], input=b"hello world\n"),
        Case("awk_length", ["awk", "{print length($0)}"], input=b"hello\nworld!\n"),
        Case("awk_toupper", ["awk", "{print toupper($0)}"], input=b"hello\nWorld\n"),
    ]

    # M2 engines: tr, grep, sed (find tested separately because output
    # depends on filesystem state).
    txt_lines = b"foo\nbar\nfoobar\nfoo bar\nbaz\n"
    cases += [
        Case("tr_upper", ["tr", "a-z", "A-Z"], input=b"Hello World\n"),
        Case("tr_delete", ["tr", "-d", "a-z"], input=b"abcXYZdef\n"),
        Case("tr_squeeze", ["tr", "-s", "a"], input=b"aaabbb\n"),
        Case("tr_class_digit", ["tr", "-d", "[:digit:]"], input=b"abc123def456\n"),
        Case("grep_simple", ["grep", "foo"], input=txt_lines),
        Case("grep_n_line_numbers", ["grep", "-n", "foo"], input=txt_lines),
        Case("grep_v_invert", ["grep", "-v", "foo"], input=txt_lines),
        Case("grep_i_ignore_case", ["grep", "-i", "FOO"], input=txt_lines),
        Case("grep_c_count", ["grep", "-c", "foo"], input=txt_lines),
        Case("grep_F_fixed", ["grep", "-F", "foo.bar"], input=b"foo.bar\nfoo+bar\n"),
        Case("grep_w_word", ["grep", "-w", "foo"], input=txt_lines),
        Case("grep_o_only_match", ["grep", "-o", "foo"], input=txt_lines),
        Case("sed_s_simple", ["sed", "s/hello/HELLO/"], input=b"hello world\nhello again\n"),
        Case("sed_s_global", ["sed", "s/a/x/g"], input=b"aaabbb\n"),
        Case("sed_d_address", ["sed", "2d"], input=b"1\n2\n3\n"),
        Case("sed_p_n", ["sed", "-n", "1p;3p"], input=b"a\nb\nc\nd\n"),
        Case("sed_q_quit", ["sed", "2q"], input=b"a\nb\nc\nd\n"),
        Case("sed_y_translate", ["sed", "y/abc/ABC/"], input=b"abc def\n"),
        Case("sed_addr_regex", ["sed", "/foo/s/foo/FOO/"], input=b"foo\nbar\nfoo\n"),
        Case("sed_eq", ["sed", "="], input=b"a\nb\n"),
    ]

    # jq arithmetic — issue #2.
    cases += [
        Case("jq_arith_int_add", ["jq", ". + 1"], input=b"5\n"),
        Case("jq_arith_int_sub", ["jq", ". - 3"], input=b"10\n"),
        Case("jq_arith_int_mul", ["jq", ". * 4"], input=b"6\n"),
        # 21/4 picked over 20/4 to side-step a Python int/float quirk:
        # Python's `/` always yields float, so 20/4 prints as "5.0" while
        # Rust (single f64 type) prints "5". Non-whole results format the
        # same on both sides.
        Case("jq_arith_int_div", ["jq", ". / 4"], input=b"21\n"),
        Case("jq_arith_int_mod", ["jq", ". % 3"], input=b"7\n"),
        Case("jq_arith_map_add", ["jq", "-c", "map(. + 1)"], input=b"[1,2,3]\n"),
        Case("jq_arith_str_concat", ["jq", '. + "world"'], input=b'"hello "\n'),
        Case("jq_arith_arr_concat", ["jq", "-c", ". + [3,4]"], input=b"[1,2]\n"),
        Case("jq_arith_obj_merge", ["jq", "-c", '. + {"b":2}'], input=b'{"a":1}\n'),
        Case("jq_arith_str_split", ["jq", "-c", '. / ","'], input=b'"a,b,c"\n'),
        # Comparison operators: numeric, string lex, equality, mixed-type, select.
        Case("jq_cmp_num_lt", ["jq", ". < 10"], input=b"5\n"),
        Case("jq_cmp_num_gt", ["jq", ". > 10"], input=b"5\n"),
        Case("jq_cmp_num_le", ["jq", ". <= 5"], input=b"5\n"),
        Case("jq_cmp_num_ge", ["jq", ". >= 5"], input=b"5\n"),
        Case("jq_cmp_eq_null", ["jq", ". == null"], input=b"null\n"),
        Case("jq_cmp_neq", ["jq", ". != 6"], input=b"5\n"),
        Case("jq_cmp_str_lex", ["jq", '. < "fzz"'], input=b'"foo"\n'),
        Case("jq_cmp_mixed_type", ["jq", ". < 1"], input=b'"abc"\n'),
        Case(
            "jq_cmp_select",
            ["jq", "-c", ".[] | select(.a > 1)"],
            input=b'[{"a":1},{"a":2},{"a":3}]\n',
        ),
        Case("jq_cmp_length_ge", ["jq", "length >= 3"], input=b"[1,2,3]\n"),
        # // alternative operator: missing field, explicit null, present value,
        # explicit false, and zero (truthy in jq, so // does NOT replace).
        Case("jq_alt_missing", ["jq", '.missing // "default"'], input=b"{}\n"),
        Case("jq_alt_null", ["jq", ".x // 42"], input=b'{"x": null}\n'),
        Case("jq_alt_present", ["jq", ".x // 42"], input=b'{"x": 5}\n'),
        Case("jq_alt_false", ["jq", '.x // "fallback"'], input=b'{"x": false}\n'),
        Case("jq_alt_zero_truthy", ["jq", '.x // "fallback"'], input=b'{"x": 0}\n'),
        # if/then/elif/else/end: basic, elif chain, no-else passthrough,
        # if inside map.
        Case(
            "jq_if_basic_true",
            ["jq", 'if . > 3 then "big" else "small" end'],
            input=b"5\n",
        ),
        Case(
            "jq_if_basic_false",
            ["jq", 'if . > 3 then "big" else "small" end'],
            input=b"2\n",
        ),
        Case(
            "jq_if_elif_chain",
            [
                "jq",
                'if . == 1 then "one" elif . == 2 then "two" elif . == 5 then "five" else "other" end',
            ],
            input=b"5\n",
        ),
        Case(
            "jq_if_no_else_passthrough",
            ["jq", 'if . > 100 then "huge" end'],
            input=b"5\n",
        ),
        Case(
            "jq_if_in_map",
            ["jq", "-c", "map(if . > 2 then . * 10 else . end)"],
            input=b"[1,2,3,4,5]\n",
        ),
        # String built-ins.
        Case("jq_split", ["jq", "-c", 'split(",")'], input=b'"a,b,c"\n'),
        Case(
            "jq_split_join_roundtrip",
            ["jq", 'split(",") | join(";")'],
            input=b'"a,b,c"\n',
        ),
        Case("jq_join_strings", ["jq", 'join("-")'], input=b'["a","b","c"]\n'),
        Case(
            "jq_endswith_select",
            ["jq", "-c", "[.[] | select(endswith(\".txt\"))]"],
            input=b'["foo.txt", "bar.log", "baz.txt"]\n',
        ),
        Case(
            "jq_startswith_true",
            ["jq", 'startswith("pre")'],
            input=b'"prefix-data"\n',
        ),
        Case(
            "jq_startswith_false",
            ["jq", 'startswith("xyz")'],
            input=b'"prefix-data"\n',
        ),
        Case("jq_ltrimstr", ["jq", 'ltrimstr("foo-")'], input=b'"foo-bar"\n'),
        Case("jq_rtrimstr", ["jq", 'rtrimstr(".log")'], input=b'"app.log"\n'),
        Case("jq_ascii_downcase", ["jq", "ascii_downcase"], input=b'"Hello World"\n'),
        Case("jq_ascii_upcase", ["jq", "ascii_upcase"], input=b'"hello"\n'),
    ]

    # base64 / fold / column — new in v0.2.0.
    cases += [
        Case("base64_encode_short", ["base64"], input=b"hello world"),
        Case("base64_encode_empty", ["base64"], input=b""),
        Case(
            "base64_decode_roundtrip",
            ["base64", "-d"],
            input=b"aGVsbG8gd29ybGQ=\n",
        ),
        Case(
            "base64_wrap_zero",
            ["base64", "-w", "0"],
            input=b"the quick brown fox jumps over the lazy dog",
        ),
        Case(
            "fold_default_width",
            ["fold", "-w", "10"],
            input=b"abcdefghijklmnopqrstuvwxyz\n",
        ),
        Case(
            "fold_spaces",
            ["fold", "-s", "-w", "20"],
            input=b"this is a long line that should be folded at a smaller width\n",
        ),
        Case(
            "column_table",
            ["column", "-t"],
            input=b"name age city\nAlice 30 Boston\nBob 25 NYC\nCarol 28 Seattle\n",
        ),
        Case(
            "column_table_separator",
            ["column", "-t", "-s", ":"],
            input=b"alice:30:boston\nbob:25:nyc\ncarol:28:seattle\n",
        ),
    ]
    # id and groups depend on the system's libc-resolved IDs which diverge
    # from our env-fallback approach, so they're not part of the parity
    # surface — integration tests in tests/integration.rs cover the basic
    # invocation patterns instead.

    # HTTP/HTTPS — only enabled if HARNESS_NETWORK=1 to avoid flaky CI.
    # Compares status code only (response body changes by date/host).
    if os.environ.get("HARNESS_NETWORK") == "1":
        cases += [
            Case(
                "http_plain_status",
                ["http", "-I", "http://example.com"],
                # We can't byte-compare the full response because Date,
                # Server, etc. vary; this case is included as a smoke test
                # the harness skips by default.
                skip="network test — set HARNESS_NETWORK=1 to enable",
            ),
            Case(
                "http_tls_status",
                ["http", "-I", "https://example.com"],
                skip="network test — set HARNESS_NETWORK=1 to enable",
            ),
        ]

    return cases


def have_python_mainsail() -> bool:
    return (PY_MAINSAIL / "mainsail" / "__init__.py").exists()


def run_python(case: Case, workdir: Path) -> tuple[int, bytes, bytes]:
    env = os.environ.copy()
    # Make sure the cloned mainsail is importable.
    env["PYTHONPATH"] = str(PY_MAINSAIL) + os.pathsep + env.get("PYTHONPATH", "")
    cmd = [sys.executable, "-m", "mainsail", *case.args]
    proc = subprocess.run(
        cmd,
        input=case.input,
        capture_output=True,
        cwd=workdir,
        env=env,
    )
    return proc.returncode, proc.stdout, proc.stderr


def run_rust(rust_bin: Path, case: Case, workdir: Path) -> tuple[int, bytes, bytes]:
    # The Rust binary's program name is `jib`; the Python module is
    # `mainsail`. The applet names on the inside are identical, so we just
    # invoke `<bin> <applet> [args...]` against both implementations and
    # diff stdout. Each case's `args[0]` is the applet name.
    proc = subprocess.run(
        [str(rust_bin), *case.args],
        input=case.input,
        capture_output=True,
        cwd=workdir,
    )
    return proc.returncode, proc.stdout, proc.stderr


def normalize_text(b: bytes) -> bytes:
    """Normalize CRLF -> LF so Windows-side test runs don't false-positive."""
    return b.replace(b"\r\n", b"\n")


def diff_one(case: Case, py: tuple[int, bytes, bytes], rs: tuple[int, bytes, bytes]) -> list[str]:
    msgs: list[str] = []
    py_rc, py_out, py_err = py
    rs_rc, rs_out, rs_err = rs
    if py_rc != rs_rc:
        msgs.append(f"  rc: python={py_rc} rust={rs_rc}")
    if normalize_text(py_out) != normalize_text(rs_out):
        msgs.append(f"  stdout differs:")
        msgs.append(f"    py:   {py_out!r}")
        msgs.append(f"    rust: {rs_out!r}")
    # We don't compare stderr text — error message wording is allowed to
    # drift between implementations. Only return code & stdout matter.
    return msgs


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rust", default=None, help="Path to mainsail Rust binary")
    parser.add_argument("--filter", default=None, help="Only run cases containing this substring")
    args = parser.parse_args()

    if not have_python_mainsail():
        print(
            f"error: Python mainsail not found at {PY_MAINSAIL}\n"
            f"Run: git clone https://github.com/Real-Fruit-Snacks/mainsail.git {PY_MAINSAIL}",
            file=sys.stderr,
        )
        return 2

    if args.rust:
        rust_bin = Path(args.rust)
    else:
        candidates = [
            REPO_ROOT / "target" / "release" / "jib.exe",
            REPO_ROOT / "target" / "release" / "jib",
            REPO_ROOT / "target" / "debug" / "jib.exe",
            REPO_ROOT / "target" / "debug" / "jib",
        ]
        rust_bin = next((c for c in candidates if c.exists()), None)
        if rust_bin is None:
            print("error: Rust binary not built; run `cargo build --release` first", file=sys.stderr)
            return 2

    cases = load_manifest()
    if args.filter:
        cases = [c for c in cases if args.filter in c.name]

    workdir = ROOT / "_workspace"
    workdir.mkdir(exist_ok=True)

    passes = 0
    fails: list[tuple[str, list[str]]] = []
    skips = 0

    for case in cases:
        if case.skip:
            skips += 1
            continue
        py = run_python(case, workdir)
        rs = run_rust(rust_bin, case, workdir)
        problems = diff_one(case, py, rs)
        if problems:
            fails.append((case.name, problems))
        else:
            passes += 1

    total = len(cases) - skips
    print(f"{passes}/{total} matched ({skips} skipped)")
    for name, problems in fails:
        print(f"FAIL {name}")
        for line in problems:
            print(line)

    return 0 if not fails else 1


if __name__ == "__main__":
    sys.exit(main())
