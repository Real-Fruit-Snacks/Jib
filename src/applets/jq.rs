//! `jq` — JSON query (subset of the full jq language).
//!
//! ## Scope
//! Implements the patterns most commonly used at the command line:
//! - Identity `.`
//! - Field access `.foo`, `.foo.bar`, `."key with space"`, `.[\"foo\"]`
//! - Optional access `.foo?` (suppresses error if missing)
//! - Index `.[0]`, slice `.[2:5]`
//! - Iterate `.[]` (over arrays and objects)
//! - Pipe `|`, comma `,`, parens `( … )`
//! - Constructors: array `[ … ]`, object `{a: …, b: …}`
//! - Built-ins: `length`, `keys`, `values`, `type`, `has(k)`, `select(f)`,
//!   `map(f)`, `not`, `empty`, `tostring`, `tonumber`, `add`, `min`, `max`,
//!   `first`, `last`, `reverse`, `sort`, `unique`.
//! - Numeric/string literals.
//!
//! Flags: `-r` raw output, `-c` compact (no indent), `-S` sort keys,
//! `-s` slurp, `-n` null input, `-e` set exit status to 1 if no output
//! is non-null/false.
//!
//! Hand-rolled JSON parser + filter compiler. Not full jq.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{self, Read};

use crate::common::err;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "jq",
    help: "JSON query (subset of the jq language)",
    aliases: &[],
    main,
};

// ────────────────────────────────────────────────────────────────────
// JSON value
// ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum J {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<J>),
    Obj(BTreeMap<String, J>),
}

fn skip_ws(s: &[u8], i: &mut usize) {
    while *i < s.len() && matches!(s[*i], b' ' | b'\t' | b'\n' | b'\r') {
        *i += 1;
    }
}

fn parse_value(s: &[u8], i: &mut usize) -> Result<J, String> {
    skip_ws(s, i);
    if *i >= s.len() {
        return Err("unexpected end".to_string());
    }
    match s[*i] {
        b'n' => {
            if s.get(*i..*i + 4) == Some(b"null") {
                *i += 4;
                Ok(J::Null)
            } else {
                Err("expected null".to_string())
            }
        }
        b't' => {
            if s.get(*i..*i + 4) == Some(b"true") {
                *i += 4;
                Ok(J::Bool(true))
            } else {
                Err("expected true".to_string())
            }
        }
        b'f' => {
            if s.get(*i..*i + 5) == Some(b"false") {
                *i += 5;
                Ok(J::Bool(false))
            } else {
                Err("expected false".to_string())
            }
        }
        b'"' => Ok(J::Str(parse_string(s, i)?)),
        b'[' => parse_array(s, i),
        b'{' => parse_object(s, i),
        b'-' | b'0'..=b'9' => parse_number(s, i),
        _ => Err(format!("unexpected char: {}", s[*i] as char)),
    }
}

fn parse_string(s: &[u8], i: &mut usize) -> Result<String, String> {
    *i += 1; // "
    let mut out = String::new();
    while *i < s.len() && s[*i] != b'"' {
        if s[*i] == b'\\' && *i + 1 < s.len() {
            let nx = s[*i + 1];
            let esc = match nx {
                b'"' => '"',
                b'\\' => '\\',
                b'/' => '/',
                b'b' => '\x08',
                b'f' => '\x0c',
                b'n' => '\n',
                b'r' => '\r',
                b't' => '\t',
                b'u' => {
                    if *i + 5 >= s.len() {
                        return Err("bad \\u".to_string());
                    }
                    let hex = std::str::from_utf8(&s[*i + 2..*i + 6]).map_err(|e| e.to_string())?;
                    let code = u32::from_str_radix(hex, 16).map_err(|e| e.to_string())?;
                    *i += 6;
                    if let Some(c) = char::from_u32(code) {
                        out.push(c);
                    }
                    continue;
                }
                _ => nx as char,
            };
            out.push(esc);
            *i += 2;
            continue;
        }
        out.push(s[*i] as char);
        *i += 1;
    }
    if *i >= s.len() {
        return Err("unterminated string".to_string());
    }
    *i += 1;
    Ok(out)
}

fn parse_number(s: &[u8], i: &mut usize) -> Result<J, String> {
    let start = *i;
    if s[*i] == b'-' {
        *i += 1;
    }
    while *i < s.len() && s[*i].is_ascii_digit() {
        *i += 1;
    }
    if *i < s.len() && s[*i] == b'.' {
        *i += 1;
        while *i < s.len() && s[*i].is_ascii_digit() {
            *i += 1;
        }
    }
    if *i < s.len() && (s[*i] == b'e' || s[*i] == b'E') {
        *i += 1;
        if *i < s.len() && (s[*i] == b'+' || s[*i] == b'-') {
            *i += 1;
        }
        while *i < s.len() && s[*i].is_ascii_digit() {
            *i += 1;
        }
    }
    let raw = std::str::from_utf8(&s[start..*i]).map_err(|e| e.to_string())?;
    raw.parse().map(J::Num).map_err(|e| e.to_string())
}

fn parse_array(s: &[u8], i: &mut usize) -> Result<J, String> {
    *i += 1; // [
    let mut out = Vec::new();
    skip_ws(s, i);
    if *i < s.len() && s[*i] == b']' {
        *i += 1;
        return Ok(J::Arr(out));
    }
    loop {
        out.push(parse_value(s, i)?);
        skip_ws(s, i);
        if *i >= s.len() {
            return Err("unterminated array".to_string());
        }
        if s[*i] == b',' {
            *i += 1;
            continue;
        }
        if s[*i] == b']' {
            *i += 1;
            return Ok(J::Arr(out));
        }
        return Err(format!("array: unexpected {}", s[*i] as char));
    }
}

fn parse_object(s: &[u8], i: &mut usize) -> Result<J, String> {
    *i += 1; // {
    let mut map = BTreeMap::new();
    skip_ws(s, i);
    if *i < s.len() && s[*i] == b'}' {
        *i += 1;
        return Ok(J::Obj(map));
    }
    loop {
        skip_ws(s, i);
        if *i >= s.len() || s[*i] != b'"' {
            return Err("object: expected key".to_string());
        }
        let k = parse_string(s, i)?;
        skip_ws(s, i);
        if *i >= s.len() || s[*i] != b':' {
            return Err("object: expected ':'".to_string());
        }
        *i += 1;
        let v = parse_value(s, i)?;
        map.insert(k, v);
        skip_ws(s, i);
        if *i >= s.len() {
            return Err("unterminated object".to_string());
        }
        if s[*i] == b',' {
            *i += 1;
            continue;
        }
        if s[*i] == b'}' {
            *i += 1;
            return Ok(J::Obj(map));
        }
        return Err(format!("object: unexpected {}", s[*i] as char));
    }
}

fn json_to_string(v: &J, raw: bool, compact: bool, sort_keys: bool, indent: usize) -> String {
    let mut out = String::new();
    write_json(&mut out, v, compact, sort_keys, indent, raw);
    if raw {
        if let J::Str(s) = v {
            return s.clone();
        }
    }
    out
}

fn write_json(
    out: &mut String,
    v: &J,
    compact: bool,
    sort_keys: bool,
    indent: usize,
    _raw_top: bool,
) {
    match v {
        J::Null => out.push_str("null"),
        J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        J::Num(n) => {
            if *n == n.trunc() && n.is_finite() && n.abs() < 1e16 {
                let _ = write!(out, "{}", *n as i64);
            } else {
                let _ = write!(out, "{n}");
            }
        }
        J::Str(s) => {
            out.push('"');
            for c in s.chars() {
                match c {
                    '"' => out.push_str(r#"\""#),
                    '\\' => out.push_str(r"\\"),
                    '\n' => out.push_str(r"\n"),
                    '\r' => out.push_str(r"\r"),
                    '\t' => out.push_str(r"\t"),
                    c if (c as u32) < 0x20 => {
                        let _ = write!(out, "\\u{:04x}", c as u32);
                    }
                    c => out.push(c),
                }
            }
            out.push('"');
        }
        J::Arr(a) => {
            if compact {
                out.push('[');
                for (i, x) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_json(out, x, compact, sort_keys, indent, false);
                }
                out.push(']');
            } else {
                if a.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push('[');
                let pad = " ".repeat((indent + 1) * 2);
                for (i, x) in a.iter().enumerate() {
                    out.push('\n');
                    out.push_str(&pad);
                    write_json(out, x, compact, sort_keys, indent + 1, false);
                    if i + 1 < a.len() {
                        out.push(',');
                    }
                }
                out.push('\n');
                out.push_str(&" ".repeat(indent * 2));
                out.push(']');
            }
        }
        J::Obj(o) => {
            if compact {
                out.push('{');
                let it: Vec<_> = if sort_keys {
                    let mut v: Vec<_> = o.iter().collect();
                    v.sort_by_key(|x| x.0);
                    v
                } else {
                    o.iter().collect()
                };
                for (i, (k, v)) in it.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_json(
                        out,
                        &J::Str((*k).clone()),
                        compact,
                        sort_keys,
                        indent,
                        false,
                    );
                    out.push(':');
                    write_json(out, v, compact, sort_keys, indent, false);
                }
                out.push('}');
            } else {
                if o.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push('{');
                let pad = " ".repeat((indent + 1) * 2);
                let it: Vec<_> = if sort_keys {
                    let mut v: Vec<_> = o.iter().collect();
                    v.sort_by_key(|x| x.0);
                    v
                } else {
                    o.iter().collect()
                };
                for (i, (k, v)) in it.iter().enumerate() {
                    out.push('\n');
                    out.push_str(&pad);
                    write_json(
                        out,
                        &J::Str((*k).clone()),
                        compact,
                        sort_keys,
                        indent + 1,
                        false,
                    );
                    out.push_str(": ");
                    write_json(out, v, compact, sort_keys, indent + 1, false);
                    if i + 1 < it.len() {
                        out.push(',');
                    }
                }
                out.push('\n');
                out.push_str(&" ".repeat(indent * 2));
                out.push('}');
            }
        }
    }
}

// ────────────────────────────────────────────────────────────────────
// Filter
// ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum F {
    Identity,
    Field(String, bool), // (name, optional?)
    Index(i64),
    Slice(Option<i64>, Option<i64>),
    Iterate(bool),
    Pipe(Box<F>, Box<F>),
    Comma(Vec<F>),
    Group(Box<F>),
    NumLit(f64),
    StrLit(String),
    NullLit,
    BoolLit(bool),
    ArrayCtor(Box<F>),
    ObjCtor(Vec<(String, F)>),
    Call(String, Vec<F>),
    /// Binary arithmetic / comparison — op is one of "+", "-", "*", "/",
    /// "%", "==", "!=", "<", "<=", ">", ">=". Each side produces a stream;
    /// the cross-product of values is folded with the operator (matches
    /// jq's spec).
    Bin(String, Box<F>, Box<F>),
    /// `lhs // rhs` — produce values from `lhs` that are not null/false;
    /// if none remain, evaluate `rhs` instead.
    Alt(Box<F>, Box<F>),
    /// `if cond then then_ else else_? end`. elif chains are parsed into
    /// nested If nodes so this single shape is enough at eval time.
    If(Box<F>, Box<F>, Option<Box<F>>),
}

struct PFilter<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> PFilter<'a> {
    fn skip_ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }
    fn parse(&mut self) -> Result<F, String> {
        self.parse_pipe()
    }
    fn parse_pipe(&mut self) -> Result<F, String> {
        let mut l = self.parse_comma()?;
        loop {
            self.skip_ws();
            if self.i < self.s.len() && self.s[self.i] == b'|' {
                self.i += 1;
                let r = self.parse_comma()?;
                l = F::Pipe(Box::new(l), Box::new(r));
            } else {
                break;
            }
        }
        Ok(l)
    }
    fn parse_comma(&mut self) -> Result<F, String> {
        let mut parts = vec![self.parse_alternative()?];
        loop {
            self.skip_ws();
            if self.i < self.s.len() && self.s[self.i] == b',' {
                self.i += 1;
                parts.push(self.parse_alternative()?);
            } else {
                break;
            }
        }
        if parts.len() == 1 {
            Ok(parts.into_iter().next().unwrap())
        } else {
            Ok(F::Comma(parts))
        }
    }
    /// `//` alternative operator. Left-associative, sits between comma
    /// and compare in jq's precedence chain.
    fn parse_alternative(&mut self) -> Result<F, String> {
        let mut l = self.parse_compare()?;
        loop {
            self.skip_ws();
            if self.i + 1 < self.s.len() && self.s[self.i] == b'/' && self.s[self.i + 1] == b'/' {
                self.i += 2;
                let r = self.parse_compare()?;
                l = F::Alt(Box::new(l), Box::new(r));
            } else {
                break;
            }
        }
        Ok(l)
    }
    /// Comparison operators `==`, `!=`, `<`, `<=`, `>`, `>=`. Sit between
    /// alternative and addsub so arithmetic results feed in.
    /// Left-associative.
    fn parse_compare(&mut self) -> Result<F, String> {
        let mut l = self.parse_addsub()?;
        loop {
            self.skip_ws();
            if self.i >= self.s.len() {
                break;
            }
            let c0 = self.s[self.i];
            let c1 = self.s.get(self.i + 1).copied();
            let op = match (c0, c1) {
                (b'=', Some(b'=')) => {
                    self.i += 2;
                    "=="
                }
                (b'!', Some(b'=')) => {
                    self.i += 2;
                    "!="
                }
                (b'<', Some(b'=')) => {
                    self.i += 2;
                    "<="
                }
                (b'>', Some(b'=')) => {
                    self.i += 2;
                    ">="
                }
                (b'<', _) => {
                    self.i += 1;
                    "<"
                }
                (b'>', _) => {
                    self.i += 1;
                    ">"
                }
                _ => break,
            };
            let r = self.parse_addsub()?;
            l = F::Bin(op.to_string(), Box::new(l), Box::new(r));
        }
        Ok(l)
    }
    /// Left-associative `+` / `-`. Sits between compare and muldiv in jq's
    /// precedence chain.
    fn parse_addsub(&mut self) -> Result<F, String> {
        let mut l = self.parse_muldiv()?;
        loop {
            self.skip_ws();
            if self.i < self.s.len() && (self.s[self.i] == b'+' || self.s[self.i] == b'-') {
                let op = (self.s[self.i] as char).to_string();
                self.i += 1;
                let r = self.parse_muldiv()?;
                l = F::Bin(op, Box::new(l), Box::new(r));
            } else {
                break;
            }
        }
        Ok(l)
    }
    /// Left-associative `*` / `/` / `%`. Highest precedence below unary.
    fn parse_muldiv(&mut self) -> Result<F, String> {
        let mut l = self.parse_term()?;
        loop {
            self.skip_ws();
            if self.i < self.s.len()
                && matches!(self.s[self.i], b'*' | b'/' | b'%')
                // Don't consume `//` — that's the alternative operator,
                // handled in a separate precedence level.
                && !(self.s[self.i] == b'/'
                    && self.i + 1 < self.s.len()
                    && self.s[self.i + 1] == b'/')
            {
                let op = (self.s[self.i] as char).to_string();
                self.i += 1;
                let r = self.parse_term()?;
                l = F::Bin(op, Box::new(l), Box::new(r));
            } else {
                break;
            }
        }
        Ok(l)
    }
    fn parse_term(&mut self) -> Result<F, String> {
        self.skip_ws();
        if self.i >= self.s.len() {
            return Ok(F::Identity);
        }
        let c = self.s[self.i];
        if c == b'(' {
            self.i += 1;
            let inner = self.parse_pipe()?;
            self.skip_ws();
            if self.i >= self.s.len() || self.s[self.i] != b')' {
                return Err("expected ')'".to_string());
            }
            self.i += 1;
            return self.parse_postfix(F::Group(Box::new(inner)));
        }
        if c == b'.' {
            self.i += 1;
            // Direct .name (or .[...]) — handle the first segment here so
            // parse_postfix can chain the rest.
            if self.i < self.s.len()
                && (self.s[self.i].is_ascii_alphabetic() || self.s[self.i] == b'_')
            {
                let start = self.i;
                while self.i < self.s.len()
                    && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'_')
                {
                    self.i += 1;
                }
                let name = std::str::from_utf8(&self.s[start..self.i])
                    .unwrap_or("")
                    .to_string();
                let opt = self.i < self.s.len() && self.s[self.i] == b'?';
                if opt {
                    self.i += 1;
                }
                return self.parse_postfix(F::Field(name, opt));
            }
            if self.i < self.s.len() && self.s[self.i] == b'"' {
                let mut idx = self.i;
                let name = parse_string(self.s, &mut idx)?;
                self.i = idx;
                let opt = self.i < self.s.len() && self.s[self.i] == b'?';
                if opt {
                    self.i += 1;
                }
                return self.parse_postfix(F::Field(name, opt));
            }
            // Bare `.` is identity.
            return self.parse_postfix(F::Identity);
        }
        if c == b'[' {
            return self.parse_array_ctor();
        }
        if c == b'{' {
            return self.parse_object_ctor();
        }
        if c == b'"' {
            let mut idx = self.i;
            let s = parse_string(self.s, &mut idx)?;
            self.i = idx;
            return Ok(F::StrLit(s));
        }
        if c == b'-' || c.is_ascii_digit() {
            let mut idx = self.i;
            match parse_number(self.s, &mut idx)? {
                J::Num(n) => {
                    self.i = idx;
                    return Ok(F::NumLit(n));
                }
                _ => return Err("bad number".to_string()),
            }
        }
        // identifier — could be a built-in call.
        let start = self.i;
        while self.i < self.s.len()
            && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'_')
        {
            self.i += 1;
        }
        if self.i == start {
            return Err(format!("unexpected '{}'", c as char));
        }
        let name = std::str::from_utf8(&self.s[start..self.i])
            .unwrap_or("")
            .to_string();
        match name.as_str() {
            "null" => return Ok(F::NullLit),
            "true" => return Ok(F::BoolLit(true)),
            "false" => return Ok(F::BoolLit(false)),
            "if" => return self.parse_if_after_keyword(),
            // Reserved if/then/elif/else/end keywords — if we hit one of
            // these in term position it means an outer parse_if is waiting
            // for it. Rewind so the outer caller can advance past it.
            "then" | "elif" | "else" | "end" => {
                self.i = start;
                return Ok(F::Identity);
            }
            _ => {}
        }
        // optional arglist
        let mut args: Vec<F> = Vec::new();
        self.skip_ws();
        if self.i < self.s.len() && self.s[self.i] == b'(' {
            self.i += 1;
            args.push(self.parse_pipe()?);
            self.skip_ws();
            while self.i < self.s.len() && self.s[self.i] == b';' {
                self.i += 1;
                args.push(self.parse_pipe()?);
                self.skip_ws();
            }
            if self.i >= self.s.len() || self.s[self.i] != b')' {
                return Err("expected ')'".to_string());
            }
            self.i += 1;
        }
        Ok(F::Call(name, args))
    }
    fn parse_postfix(&mut self, mut node: F) -> Result<F, String> {
        loop {
            self.skip_ws();
            if self.i >= self.s.len() {
                break;
            }
            let c = self.s[self.i];
            if c == b'.' {
                self.i += 1;
                if self.i < self.s.len() && self.s[self.i].is_ascii_alphabetic() {
                    let start = self.i;
                    while self.i < self.s.len()
                        && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'_')
                    {
                        self.i += 1;
                    }
                    let name = std::str::from_utf8(&self.s[start..self.i])
                        .unwrap_or("")
                        .to_string();
                    let opt = self.i < self.s.len() && self.s[self.i] == b'?';
                    if opt {
                        self.i += 1;
                    }
                    node = F::Pipe(Box::new(node), Box::new(F::Field(name, opt)));
                    continue;
                }
                if self.i < self.s.len() && self.s[self.i] == b'"' {
                    let mut idx = self.i;
                    let name = parse_string(self.s, &mut idx)?;
                    self.i = idx;
                    let opt = self.i < self.s.len() && self.s[self.i] == b'?';
                    if opt {
                        self.i += 1;
                    }
                    node = F::Pipe(Box::new(node), Box::new(F::Field(name, opt)));
                    continue;
                }
                // bare `.` with nothing after = identity already; back up
                self.i -= 1;
                break;
            }
            if c == b'[' {
                self.i += 1;
                self.skip_ws();
                if self.i < self.s.len() && self.s[self.i] == b']' {
                    self.i += 1;
                    let opt = self.i < self.s.len() && self.s[self.i] == b'?';
                    if opt {
                        self.i += 1;
                    }
                    node = F::Pipe(Box::new(node), Box::new(F::Iterate(opt)));
                    continue;
                }
                // index, slice, or string-key
                if self.i < self.s.len() && self.s[self.i] == b'"' {
                    let mut idx = self.i;
                    let name = parse_string(self.s, &mut idx)?;
                    self.i = idx;
                    self.skip_ws();
                    if self.i >= self.s.len() || self.s[self.i] != b']' {
                        return Err("expected ']'".to_string());
                    }
                    self.i += 1;
                    let opt = self.i < self.s.len() && self.s[self.i] == b'?';
                    if opt {
                        self.i += 1;
                    }
                    node = F::Pipe(Box::new(node), Box::new(F::Field(name, opt)));
                    continue;
                }
                let mut start: Option<i64> = None;
                let mut end: Option<i64> = None;
                let mut is_slice = false;
                if self.i < self.s.len() && self.s[self.i] != b':' {
                    let mut idx = self.i;
                    match parse_number(self.s, &mut idx)? {
                        J::Num(n) => {
                            start = Some(n as i64);
                            self.i = idx;
                        }
                        _ => return Err("bad number".to_string()),
                    }
                }
                self.skip_ws();
                if self.i < self.s.len() && self.s[self.i] == b':' {
                    is_slice = true;
                    self.i += 1;
                    self.skip_ws();
                    if self.i < self.s.len() && self.s[self.i] != b']' {
                        let mut idx = self.i;
                        match parse_number(self.s, &mut idx)? {
                            J::Num(n) => {
                                end = Some(n as i64);
                                self.i = idx;
                            }
                            _ => return Err("bad number".to_string()),
                        }
                    }
                }
                self.skip_ws();
                if self.i >= self.s.len() || self.s[self.i] != b']' {
                    return Err("expected ']'".to_string());
                }
                self.i += 1;
                let post = if is_slice {
                    F::Slice(start, end)
                } else if let Some(n) = start {
                    F::Index(n)
                } else {
                    F::Iterate(false)
                };
                node = F::Pipe(Box::new(node), Box::new(post));
                continue;
            }
            break;
        }
        Ok(node)
    }
    /// True if, after skipping whitespace, the next token is `kw` followed
    /// by a non-identifier byte (so "thenfoo" doesn't match "then").
    fn peek_keyword(&self, kw: &str) -> bool {
        let kb = kw.as_bytes();
        let mut j = self.i;
        while j < self.s.len() && matches!(self.s[j], b' ' | b'\t' | b'\n' | b'\r') {
            j += 1;
        }
        if j + kb.len() > self.s.len() {
            return false;
        }
        if &self.s[j..j + kb.len()] != kb {
            return false;
        }
        let after = j + kb.len();
        if after < self.s.len() && (self.s[after].is_ascii_alphanumeric() || self.s[after] == b'_')
        {
            return false;
        }
        true
    }
    fn consume_keyword(&mut self, kw: &str) -> Result<(), String> {
        if !self.peek_keyword(kw) {
            return Err(format!("expected '{kw}'"));
        }
        self.skip_ws();
        self.i += kw.len();
        Ok(())
    }
    /// Parse the body of an `if` after the `if` keyword has been consumed:
    /// `COND then THEN (elif COND then THEN)* (else ELSE)? end`.
    /// elif chains are flattened into nested F::If nodes so eval has a
    /// single shape to handle.
    fn parse_if_after_keyword(&mut self) -> Result<F, String> {
        let cond = self.parse_pipe()?;
        self.consume_keyword("then")?;
        let then_ = self.parse_pipe()?;
        let mut elifs: Vec<(F, F)> = Vec::new();
        let mut else_branch: Option<F> = None;
        loop {
            if self.peek_keyword("elif") {
                self.consume_keyword("elif")?;
                let ec = self.parse_pipe()?;
                self.consume_keyword("then")?;
                let et = self.parse_pipe()?;
                elifs.push((ec, et));
            } else if self.peek_keyword("else") {
                self.consume_keyword("else")?;
                else_branch = Some(self.parse_pipe()?);
                break;
            } else {
                break;
            }
        }
        self.consume_keyword("end")?;
        // Right-fold the elif chain into nested else-branches so the AST
        // only ever sees the simple If(cond, then, else?) shape.
        let mut tail = else_branch.map(Box::new);
        while let Some((ec, et)) = elifs.pop() {
            tail = Some(Box::new(F::If(Box::new(ec), Box::new(et), tail)));
        }
        Ok(F::If(Box::new(cond), Box::new(then_), tail))
    }
    fn parse_pipe_no_comma(&mut self) -> Result<F, String> {
        let mut l = self.parse_alternative()?;
        loop {
            self.skip_ws();
            if self.i < self.s.len() && self.s[self.i] == b'|' {
                self.i += 1;
                let r = self.parse_alternative()?;
                l = F::Pipe(Box::new(l), Box::new(r));
            } else {
                break;
            }
        }
        Ok(l)
    }

    fn parse_array_ctor(&mut self) -> Result<F, String> {
        self.i += 1; // [
        self.skip_ws();
        if self.i < self.s.len() && self.s[self.i] == b']' {
            self.i += 1;
            return Ok(F::ArrayCtor(Box::new(F::Comma(vec![]))));
        }
        let inner = self.parse_pipe()?;
        self.skip_ws();
        if self.i >= self.s.len() || self.s[self.i] != b']' {
            return Err("expected ']'".to_string());
        }
        self.i += 1;
        Ok(F::ArrayCtor(Box::new(inner)))
    }
    fn parse_object_ctor(&mut self) -> Result<F, String> {
        self.i += 1; // {
        let mut entries: Vec<(String, F)> = Vec::new();
        self.skip_ws();
        if self.i < self.s.len() && self.s[self.i] == b'}' {
            self.i += 1;
            return Ok(F::ObjCtor(entries));
        }
        loop {
            self.skip_ws();
            let key: String = if self.i < self.s.len() && self.s[self.i] == b'"' {
                let mut idx = self.i;
                let s = parse_string(self.s, &mut idx)?;
                self.i = idx;
                s
            } else {
                let start = self.i;
                while self.i < self.s.len()
                    && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'_')
                {
                    self.i += 1;
                }
                std::str::from_utf8(&self.s[start..self.i])
                    .unwrap_or("")
                    .to_string()
            };
            self.skip_ws();
            let value = if self.i < self.s.len() && self.s[self.i] == b':' {
                self.i += 1;
                // Object-value parsing: pipe is fine, but commas separate
                // pairs, so we don't fall into parse_comma here.
                self.parse_pipe_no_comma()?
            } else {
                F::Pipe(
                    Box::new(F::Identity),
                    Box::new(F::Field(key.clone(), false)),
                )
            };
            entries.push((key, value));
            self.skip_ws();
            if self.i < self.s.len() && self.s[self.i] == b',' {
                self.i += 1;
                continue;
            }
            if self.i < self.s.len() && self.s[self.i] == b'}' {
                self.i += 1;
                return Ok(F::ObjCtor(entries));
            }
            return Err("object ctor: expected , or }".to_string());
        }
    }
}

fn truthy(v: &J) -> bool {
    !matches!(v, J::Null | J::Bool(false))
}

fn jeq(a: &J, b: &J) -> bool {
    match (a, b) {
        (J::Null, J::Null) => true,
        (J::Bool(x), J::Bool(y)) => x == y,
        (J::Num(x), J::Num(y)) => x == y,
        (J::Str(x), J::Str(y)) => x == y,
        (J::Arr(x), J::Arr(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| jeq(a, b))
        }
        (J::Obj(x), J::Obj(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y.iter())
                    .all(|((k1, v1), (k2, v2))| k1 == k2 && jeq(v1, v2))
        }
        _ => false,
    }
}

/// jq's canonical type ordering for cross-type comparison:
/// null < false < true < number < string < array < object.
fn type_rank(v: &J) -> u8 {
    match v {
        J::Null => 0,
        J::Bool(false) => 1,
        J::Bool(true) => 2,
        J::Num(_) => 3,
        J::Str(_) => 4,
        J::Arr(_) => 5,
        J::Obj(_) => 6,
    }
}

/// Total order matching jq's spec. Used by `<`, `<=`, `>`, `>=`, `sort`,
/// and friends. Equal values compare equal; mixed types fall back to
/// `type_rank`. NaN is treated as equal to itself for stability.
fn jcmp(a: &J, b: &J) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let ra = type_rank(a);
    let rb = type_rank(b);
    if ra != rb {
        return ra.cmp(&rb);
    }
    match (a, b) {
        (J::Null, J::Null) => Ordering::Equal,
        (J::Bool(x), J::Bool(y)) => x.cmp(y),
        (J::Num(x), J::Num(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (J::Str(x), J::Str(y)) => x.cmp(y),
        (J::Arr(x), J::Arr(y)) => {
            for (xi, yi) in x.iter().zip(y.iter()) {
                match jcmp(xi, yi) {
                    Ordering::Equal => continue,
                    o => return o,
                }
            }
            x.len().cmp(&y.len())
        }
        (J::Obj(x), J::Obj(y)) => {
            // BTreeMap iterates in sorted-key order — pair up keys then values.
            for ((kx, vx), (ky, vy)) in x.iter().zip(y.iter()) {
                match kx.cmp(ky) {
                    Ordering::Equal => match jcmp(vx, vy) {
                        Ordering::Equal => continue,
                        o => return o,
                    },
                    o => return o,
                }
            }
            x.len().cmp(&y.len())
        }
        _ => Ordering::Equal,
    }
}

fn apply(f: &F, v: &J) -> Result<Vec<J>, String> {
    match f {
        F::Identity => Ok(vec![v.clone()]),
        F::Group(inner) => apply(inner, v),
        F::NumLit(n) => Ok(vec![J::Num(*n)]),
        F::StrLit(s) => Ok(vec![J::Str(s.clone())]),
        F::NullLit => Ok(vec![J::Null]),
        F::BoolLit(b) => Ok(vec![J::Bool(*b)]),
        F::Field(name, opt) => match v {
            J::Obj(o) => Ok(vec![o.get(name).cloned().unwrap_or(J::Null)]),
            J::Null => Ok(vec![J::Null]),
            _ => {
                if *opt {
                    Ok(vec![])
                } else {
                    Err(format!("cannot index {} with .{name}", type_of(v)))
                }
            }
        },
        F::Index(i) => match v {
            J::Arr(a) => {
                let len = a.len() as i64;
                let idx = if *i < 0 { len + *i } else { *i };
                if idx >= 0 && idx < len {
                    Ok(vec![a[idx as usize].clone()])
                } else {
                    Ok(vec![J::Null])
                }
            }
            J::Null => Ok(vec![J::Null]),
            _ => Err(format!("cannot index {} with [{i}]", type_of(v))),
        },
        F::Slice(a, b) => match v {
            J::Arr(arr) => {
                let len = arr.len() as i64;
                let start = a.unwrap_or(0).max(0).min(len);
                let end = b.unwrap_or(len).max(0).min(len);
                Ok(vec![J::Arr(arr[start as usize..end as usize].to_vec())])
            }
            J::Str(s) => {
                let chars: Vec<char> = s.chars().collect();
                let len = chars.len() as i64;
                let start = a.unwrap_or(0).max(0).min(len);
                let end = b.unwrap_or(len).max(0).min(len);
                Ok(vec![J::Str(
                    chars[start as usize..end as usize].iter().collect(),
                )])
            }
            _ => Err(format!("cannot slice {}", type_of(v))),
        },
        F::Iterate(opt) => match v {
            J::Arr(a) => Ok(a.clone()),
            J::Obj(o) => Ok(o.values().cloned().collect()),
            _ => {
                if *opt {
                    Ok(vec![])
                } else {
                    Err(format!("cannot iterate over {}", type_of(v)))
                }
            }
        },
        F::Pipe(l, r) => {
            let left = apply(l, v)?;
            let mut out = Vec::new();
            for x in left {
                out.extend(apply(r, &x)?);
            }
            Ok(out)
        }
        F::Comma(parts) => {
            let mut out = Vec::new();
            for p in parts {
                out.extend(apply(p, v)?);
            }
            Ok(out)
        }
        F::ArrayCtor(inner) => {
            let items = apply(inner, v)?;
            Ok(vec![J::Arr(items)])
        }
        F::ObjCtor(entries) => {
            let mut o = BTreeMap::new();
            for (k, ef) in entries {
                let vs = apply(ef, v)?;
                if let Some(first) = vs.into_iter().next() {
                    o.insert(k.clone(), first);
                }
            }
            Ok(vec![J::Obj(o)])
        }
        F::Call(name, args) => call_builtin(name, args, v),
        F::Bin(op, l, r) => {
            // Cross-product semantics: produce one result per (lv, rv) pair.
            let lvs = apply(l, v)?;
            let rvs = apply(r, v)?;
            let mut out = Vec::with_capacity(lvs.len() * rvs.len().max(1));
            for lv in &lvs {
                for rv in &rvs {
                    out.push(apply_binop(op, lv, rv)?);
                }
            }
            Ok(out)
        }
        F::Alt(l, r) => {
            // jq's // : keep all LHS values that are neither null nor false;
            // if every LHS value is filtered out (or the LHS produces no
            // values at all), evaluate the RHS instead.
            let lvs = apply(l, v).unwrap_or_default();
            let kept: Vec<J> = lvs
                .into_iter()
                .filter(|j| !matches!(j, J::Null | J::Bool(false)))
                .collect();
            if kept.is_empty() {
                apply(r, v)
            } else {
                Ok(kept)
            }
        }
        F::If(cond, then_, else_) => {
            // Each value the cond produces picks a branch independently;
            // both branches see the original input. With no else clause,
            // a falsy cond passes the input through unchanged (jq spec).
            let cvs = apply(cond, v)?;
            let mut out = Vec::new();
            for cv in cvs {
                if truthy(&cv) {
                    out.extend(apply(then_, v)?);
                } else if let Some(e) = else_ {
                    out.extend(apply(e, v)?);
                } else {
                    out.push(v.clone());
                }
            }
            Ok(out)
        }
    }
}

/// Type-aware binary arithmetic and comparison, matching jq's spec:
///
/// - number op number = number arithmetic
/// - string + string = concat
/// - array + array  = concat
/// - object + object = right-merge (keys in `b` overwrite `a`)
/// - null + x or x + null = x (jq treats null as additive identity)
/// - division/modulus by zero is an error
/// - `==` / `!=` use `jeq`; `<` `<=` `>` `>=` use `jcmp` (jq's total order)
///
/// Anything else is an error with a jq-style message.
fn apply_binop(op: &str, a: &J, b: &J) -> Result<J, String> {
    match op {
        "+" => match (a, b) {
            (J::Null, x) | (x, J::Null) => Ok(x.clone()),
            (J::Num(x), J::Num(y)) => Ok(J::Num(x + y)),
            (J::Str(x), J::Str(y)) => Ok(J::Str(format!("{x}{y}"))),
            (J::Arr(x), J::Arr(y)) => {
                let mut z = x.clone();
                z.extend_from_slice(y);
                Ok(J::Arr(z))
            }
            (J::Obj(x), J::Obj(y)) => {
                let mut z = x.clone();
                for (k, v) in y {
                    z.insert(k.clone(), v.clone());
                }
                Ok(J::Obj(z))
            }
            _ => Err(format!(
                "{} ({}) and {} ({}) cannot be added",
                type_of(a),
                preview(a),
                type_of(b),
                preview(b)
            )),
        },
        "-" => match (a, b) {
            (J::Num(x), J::Num(y)) => Ok(J::Num(x - y)),
            (J::Arr(x), J::Arr(y)) => {
                // Array-minus-array: drop every value in b from a (jq semantics).
                Ok(J::Arr(
                    x.iter()
                        .filter(|item| !y.iter().any(|drop| jeq(item, drop)))
                        .cloned()
                        .collect(),
                ))
            }
            _ => Err(format!(
                "{} ({}) and {} ({}) cannot be subtracted",
                type_of(a),
                preview(a),
                type_of(b),
                preview(b)
            )),
        },
        "*" => match (a, b) {
            (J::Num(x), J::Num(y)) => Ok(J::Num(x * y)),
            (J::Null, _) | (_, J::Null) => Ok(J::Null),
            (J::Str(x), J::Str(y)) => {
                // jq's string-times-string is "split a by b then keep
                // non-empty parts" — niche but documented. We stop at
                // number*number for the common case; widen later if needed.
                let _ = (x, y);
                Err("string * string is not implemented yet".to_string())
            }
            _ => Err(format!(
                "{} ({}) and {} ({}) cannot be multiplied",
                type_of(a),
                preview(a),
                type_of(b),
                preview(b)
            )),
        },
        "/" => match (a, b) {
            (J::Num(_), J::Num(y)) if *y == 0.0 => Err(format!(
                "{} ({}) and {} ({}) cannot be divided because the divisor is zero",
                type_of(a),
                preview(a),
                type_of(b),
                preview(b)
            )),
            (J::Num(x), J::Num(y)) => Ok(J::Num(x / y)),
            (J::Str(x), J::Str(y)) => {
                if y.is_empty() {
                    return Err("string / empty string is not allowed".to_string());
                }
                Ok(J::Arr(
                    x.split(y.as_str()).map(|s| J::Str(s.to_string())).collect(),
                ))
            }
            _ => Err(format!(
                "{} ({}) and {} ({}) cannot be divided",
                type_of(a),
                preview(a),
                type_of(b),
                preview(b)
            )),
        },
        "%" => match (a, b) {
            (J::Num(_), J::Num(y)) if *y == 0.0 => {
                Err("number and number cannot be modulo'd because the divisor is zero".to_string())
            }
            (J::Num(x), J::Num(y)) => {
                // jq's % truncates both sides to int, matching C-style %.
                let xi = *x as i64;
                let yi = *y as i64;
                Ok(J::Num((xi % yi) as f64))
            }
            _ => Err(format!(
                "{} ({}) and {} ({}) cannot be modulo'd",
                type_of(a),
                preview(a),
                type_of(b),
                preview(b)
            )),
        },
        "==" => Ok(J::Bool(jeq(a, b))),
        "!=" => Ok(J::Bool(!jeq(a, b))),
        "<" => Ok(J::Bool(jcmp(a, b) == std::cmp::Ordering::Less)),
        "<=" => Ok(J::Bool(jcmp(a, b) != std::cmp::Ordering::Greater)),
        ">" => Ok(J::Bool(jcmp(a, b) == std::cmp::Ordering::Greater)),
        ">=" => Ok(J::Bool(jcmp(a, b) != std::cmp::Ordering::Less)),
        _ => Err(format!("unknown operator: {op}")),
    }
}

/// Compact one-line preview of a value, used in arithmetic-error messages
/// to mirror jq's wording. Long values get truncated.
fn preview(v: &J) -> String {
    let s = json_to_string(v, false, true, false, 0);
    if s.len() > 40 {
        format!("{}...", &s[..40])
    } else {
        s
    }
}

fn type_of(v: &J) -> &'static str {
    match v {
        J::Null => "null",
        J::Bool(_) => "boolean",
        J::Num(_) => "number",
        J::Str(_) => "string",
        J::Arr(_) => "array",
        J::Obj(_) => "object",
    }
}

fn call_builtin(name: &str, args: &[F], v: &J) -> Result<Vec<J>, String> {
    match name {
        "length" => Ok(vec![J::Num(match v {
            J::Null => 0.0,
            J::Bool(_) => 0.0,
            J::Num(_) => 0.0,
            J::Str(s) => s.chars().count() as f64,
            J::Arr(a) => a.len() as f64,
            J::Obj(o) => o.len() as f64,
        })]),
        "type" => Ok(vec![J::Str(type_of(v).to_string())]),
        "keys" => match v {
            J::Obj(o) => Ok(vec![J::Arr(o.keys().cloned().map(J::Str).collect())]),
            _ => Err(format!("keys: expected object, got {}", type_of(v))),
        },
        "values" => match v {
            J::Obj(o) => Ok(vec![J::Arr(o.values().cloned().collect())]),
            J::Arr(a) => Ok(vec![J::Arr(a.clone())]),
            _ => Err(format!("values: expected object/array, got {}", type_of(v))),
        },
        "has" => {
            if args.len() != 1 {
                return Err("has: requires 1 arg".into());
            }
            let key_val = apply(&args[0], v)?.into_iter().next().unwrap_or(J::Null);
            let present = match (v, &key_val) {
                (J::Obj(o), J::Str(k)) => o.contains_key(k),
                (J::Arr(a), J::Num(n)) => {
                    let i = *n as i64;
                    i >= 0 && (i as usize) < a.len()
                }
                _ => false,
            };
            Ok(vec![J::Bool(present)])
        }
        "select" => {
            if args.len() != 1 {
                return Err("select: requires 1 arg".into());
            }
            let pred_results = apply(&args[0], v)?;
            let pass = pred_results.iter().any(truthy);
            Ok(if pass { vec![v.clone()] } else { vec![] })
        }
        "map" => {
            if args.len() != 1 {
                return Err("map: requires 1 arg".into());
            }
            match v {
                J::Arr(a) => {
                    let mut out = Vec::new();
                    for x in a {
                        out.extend(apply(&args[0], x)?);
                    }
                    Ok(vec![J::Arr(out)])
                }
                _ => Err(format!("map: expected array, got {}", type_of(v))),
            }
        }
        "not" => Ok(vec![J::Bool(!truthy(v))]),
        "empty" => Ok(vec![]),
        "tostring" => Ok(vec![J::Str(match v {
            J::Str(s) => s.clone(),
            other => json_to_string(other, false, true, false, 0),
        })]),
        "tonumber" => match v {
            J::Num(_) => Ok(vec![v.clone()]),
            J::Str(s) => s
                .parse::<f64>()
                .map(|n| vec![J::Num(n)])
                .map_err(|e| format!("tonumber: {e}")),
            _ => Err(format!("tonumber: not a number-ish: {}", type_of(v))),
        },
        "add" => match v {
            J::Arr(a) => {
                if a.is_empty() {
                    return Ok(vec![J::Null]);
                }
                let mut acc = a[0].clone();
                for x in &a[1..] {
                    acc = match (&acc, x) {
                        (J::Num(a), J::Num(b)) => J::Num(a + b),
                        (J::Str(a), J::Str(b)) => J::Str(format!("{a}{b}")),
                        (J::Arr(a), J::Arr(b)) => {
                            let mut v = a.clone();
                            v.extend_from_slice(b);
                            J::Arr(v)
                        }
                        _ => return Err("add: incompatible types".into()),
                    };
                }
                Ok(vec![acc])
            }
            _ => Err(format!("add: expected array, got {}", type_of(v))),
        },
        "min" | "max" => match v {
            J::Arr(a) => {
                if a.is_empty() {
                    return Ok(vec![J::Null]);
                }
                let cmp = |x: &J, y: &J| -> std::cmp::Ordering {
                    match (x, y) {
                        (J::Num(a), J::Num(b)) => {
                            a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
                        }
                        (J::Str(a), J::Str(b)) => a.cmp(b),
                        _ => std::cmp::Ordering::Equal,
                    }
                };
                let mut best = a[0].clone();
                for x in &a[1..] {
                    let take = if name == "min" {
                        cmp(x, &best) == std::cmp::Ordering::Less
                    } else {
                        cmp(x, &best) == std::cmp::Ordering::Greater
                    };
                    if take {
                        best = x.clone();
                    }
                }
                Ok(vec![best])
            }
            _ => Err(format!("{name}: expected array")),
        },
        "first" => match v {
            J::Arr(a) => Ok(vec![a.first().cloned().unwrap_or(J::Null)]),
            _ => Err("first: expected array".into()),
        },
        "last" => match v {
            J::Arr(a) => Ok(vec![a.last().cloned().unwrap_or(J::Null)]),
            _ => Err("last: expected array".into()),
        },
        "reverse" => match v {
            J::Arr(a) => Ok(vec![J::Arr(a.iter().rev().cloned().collect())]),
            J::Str(s) => Ok(vec![J::Str(s.chars().rev().collect())]),
            _ => Err("reverse: expected array/string".into()),
        },
        "sort" => match v {
            J::Arr(a) => {
                let mut b = a.clone();
                b.sort_by(|x, y| match (x, y) {
                    (J::Num(a), J::Num(b)) => a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal),
                    (J::Str(a), J::Str(b)) => a.cmp(b),
                    _ => std::cmp::Ordering::Equal,
                });
                Ok(vec![J::Arr(b)])
            }
            _ => Err("sort: expected array".into()),
        },
        "unique" => match v {
            J::Arr(a) => {
                let mut b = a.clone();
                b.sort_by(|x, y| match (x, y) {
                    (J::Num(a), J::Num(b)) => a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal),
                    (J::Str(a), J::Str(b)) => a.cmp(b),
                    _ => std::cmp::Ordering::Equal,
                });
                b.dedup_by(|a, b| jeq(a, b));
                Ok(vec![J::Arr(b)])
            }
            _ => Err("unique: expected array".into()),
        },
        other => Err(format!("unknown function: {other}")),
    }
}

fn main(argv: &[String]) -> i32 {
    let args = &argv[1..];
    let mut raw = false;
    let mut compact = false;
    let mut sort_keys = false;
    let mut slurp = false;
    let mut null_input = false;
    let mut exit_status = false;
    let mut filter_text: Option<String> = None;
    let mut files: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            // The `--` separator ends option parsing; subsequent positionals
            // (filter then files) are picked up by the catch-all branch
            // below, so we just bump and let the loop continue. (We don't
            // `break` because the filter and file arguments still need
            // processing.)
            i += 1;
            continue;
        }
        match a.as_str() {
            "-r" => {
                raw = true;
                i += 1;
            }
            "-c" => {
                compact = true;
                i += 1;
            }
            "-S" => {
                sort_keys = true;
                i += 1;
            }
            "-s" => {
                slurp = true;
                i += 1;
            }
            "-n" => {
                null_input = true;
                i += 1;
            }
            "-e" => {
                exit_status = true;
                i += 1;
            }
            s if s.starts_with('-') && s.len() > 1 && s != "-" => {
                err("jq", &format!("unknown option: {s}"));
                return 2;
            }
            _ => {
                if filter_text.is_none() {
                    filter_text = Some(a);
                } else {
                    files.push(a);
                }
                i += 1;
            }
        }
    }
    let filter_text = match filter_text {
        Some(f) => f,
        None => {
            err("jq", "missing filter");
            return 2;
        }
    };
    let mut p = PFilter {
        s: filter_text.as_bytes(),
        i: 0,
    };
    let filter = match p.parse() {
        Ok(f) => f,
        Err(e) => {
            err("jq", &format!("bad filter: {e}"));
            return 2;
        }
    };

    // Read input.
    let inputs: Vec<J> = if null_input {
        vec![J::Null]
    } else {
        let mut buf = String::new();
        if files.is_empty() {
            let _ = io::stdin().lock().read_to_string(&mut buf);
        } else {
            for f in &files {
                match std::fs::read_to_string(f) {
                    Ok(s) => buf.push_str(&s),
                    Err(e) => {
                        err("jq", &format!("{f}: {e}"));
                        return 1;
                    }
                }
            }
        }
        // Parse multiple top-level values (whitespace-separated, jq-style).
        let mut out = Vec::new();
        let bytes = buf.as_bytes();
        let mut i = 0;
        skip_ws(bytes, &mut i);
        while i < bytes.len() {
            match parse_value(bytes, &mut i) {
                Ok(v) => out.push(v),
                Err(e) => {
                    err("jq", &format!("parse error: {e}"));
                    return 2;
                }
            }
            skip_ws(bytes, &mut i);
        }
        out
    };
    let inputs = if slurp { vec![J::Arr(inputs)] } else { inputs };

    let mut produced_truthy = false;
    for v in inputs {
        match apply(&filter, &v) {
            Ok(results) => {
                for r in results {
                    if truthy(&r) {
                        produced_truthy = true;
                    }
                    let s = if raw {
                        if let J::Str(ref t) = r {
                            t.clone()
                        } else {
                            json_to_string(&r, false, compact, sort_keys, 0)
                        }
                    } else {
                        json_to_string(&r, false, compact, sort_keys, 0)
                    };
                    println!("{s}");
                }
            }
            Err(e) => {
                err("jq", &e);
                return 5;
            }
        }
    }
    if exit_status && !produced_truthy {
        return 1;
    }
    0
}
