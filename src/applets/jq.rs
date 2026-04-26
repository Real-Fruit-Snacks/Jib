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
                    let hex = std::str::from_utf8(&s[*i + 2..*i + 6])
                        .map_err(|e| e.to_string())?;
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

fn write_json(out: &mut String, v: &J, compact: bool, sort_keys: bool, indent: usize, _raw_top: bool) {
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
                    write_json(out, &J::Str((*k).clone()), compact, sort_keys, indent, false);
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
                    write_json(out, &J::Str((*k).clone()), compact, sort_keys, indent + 1, false);
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
        let mut parts = vec![self.parse_term()?];
        loop {
            self.skip_ws();
            if self.i < self.s.len() && self.s[self.i] == b',' {
                self.i += 1;
                parts.push(self.parse_term()?);
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
                let name = std::str::from_utf8(&self.s[start..self.i]).unwrap_or("").to_string();
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
        while self.i < self.s.len() && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'_') {
            self.i += 1;
        }
        if self.i == start {
            return Err(format!("unexpected '{}'", c as char));
        }
        let name = std::str::from_utf8(&self.s[start..self.i]).unwrap_or("").to_string();
        match name.as_str() {
            "null" => return Ok(F::NullLit),
            "true" => return Ok(F::BoolLit(true)),
            "false" => return Ok(F::BoolLit(false)),
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
                    let name = std::str::from_utf8(&self.s[start..self.i]).unwrap_or("").to_string();
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
    fn parse_pipe_no_comma(&mut self) -> Result<F, String> {
        let mut l = self.parse_term()?;
        loop {
            self.skip_ws();
            if self.i < self.s.len() && self.s[self.i] == b'|' {
                self.i += 1;
                let r = self.parse_term()?;
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
                std::str::from_utf8(&self.s[start..self.i]).unwrap_or("").to_string()
            };
            self.skip_ws();
            let value = if self.i < self.s.len() && self.s[self.i] == b':' {
                self.i += 1;
                // Object-value parsing: pipe is fine, but commas separate
                // pairs, so we don't fall into parse_comma here.
                self.parse_pipe_no_comma()?
            } else {
                F::Pipe(Box::new(F::Identity), Box::new(F::Field(key.clone(), false)))
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
            x.len() == y.len() && x.iter().zip(y.iter()).all(|((k1, v1), (k2, v2))| k1 == k2 && jeq(v1, v2))
        }
        _ => false,
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
                Ok(vec![J::Str(chars[start as usize..end as usize].iter().collect())])
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
                        (J::Num(a), J::Num(b)) => a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal),
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
