//! `find` — search for files in a directory hierarchy.
//!
//! Implements the bulk of the find expression language: tests
//! `-name`/`-iname`/`-path`/`-ipath`/`-type`/`-size`/`-mtime`/`-mmin`/
//! `-atime`/`-amin`/`-ctime`/`-cmin`/`-newer`/`-empty`, actions `-print`/
//! `-print0`/`-delete`/`-prune`/`-exec`, operators `-and`/`-or`/`-not`/
//! `!`/`(`/`)`, and globals `-maxdepth`/`-mindepth`.

use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "find",
    help: "search for files in a directory hierarchy",
    aliases: &[],
    main,
};

struct Ctx {
    now_secs: i64,
    pruned: bool,
    rc: i32,
}

trait Node {
    fn eval(&mut self, p: &Path, st: &Metadata, ctx: &mut Ctx) -> bool;
    fn finalize(&mut self) {}
    fn has_action(&self) -> bool {
        false
    }
}

// --- operators --------------------------------------------------------------

struct And(Box<dyn Node>, Box<dyn Node>);
impl Node for And {
    fn eval(&mut self, p: &Path, st: &Metadata, ctx: &mut Ctx) -> bool {
        self.0.eval(p, st, ctx) && self.1.eval(p, st, ctx)
    }
    fn finalize(&mut self) {
        self.0.finalize();
        self.1.finalize();
    }
    fn has_action(&self) -> bool {
        self.0.has_action() || self.1.has_action()
    }
}

struct Or(Box<dyn Node>, Box<dyn Node>);
impl Node for Or {
    fn eval(&mut self, p: &Path, st: &Metadata, ctx: &mut Ctx) -> bool {
        self.0.eval(p, st, ctx) || self.1.eval(p, st, ctx)
    }
    fn finalize(&mut self) {
        self.0.finalize();
        self.1.finalize();
    }
    fn has_action(&self) -> bool {
        self.0.has_action() || self.1.has_action()
    }
}

struct Not(Box<dyn Node>);
impl Node for Not {
    fn eval(&mut self, p: &Path, st: &Metadata, ctx: &mut Ctx) -> bool {
        !self.0.eval(p, st, ctx)
    }
    fn finalize(&mut self) {
        self.0.finalize();
    }
    fn has_action(&self) -> bool {
        self.0.has_action()
    }
}

struct True_;
impl Node for True_ {
    fn eval(&mut self, _p: &Path, _st: &Metadata, _ctx: &mut Ctx) -> bool {
        true
    }
}

// --- predicates -------------------------------------------------------------

/// Tiny fnmatch — supports `*`, `?`, and `[...]` character classes.
fn fnmatch(pat: &str, name: &str, ci: bool) -> bool {
    let (pat_owned, name_owned);
    let (pat, name) = if ci {
        pat_owned = pat.to_lowercase();
        name_owned = name.to_lowercase();
        (pat_owned.as_str(), name_owned.as_str())
    } else {
        (pat, name)
    };
    fn rec(p: &[u8], s: &[u8]) -> bool {
        let mut pi = 0usize;
        let mut si = 0usize;
        let mut star: Option<(usize, usize)> = None;
        while si < s.len() {
            if pi < p.len() {
                match p[pi] {
                    b'?' => {
                        pi += 1;
                        si += 1;
                        continue;
                    }
                    b'*' => {
                        star = Some((pi, si));
                        pi += 1;
                        continue;
                    }
                    b'[' => {
                        // Character class.
                        let mut j = pi + 1;
                        let mut negate = false;
                        if j < p.len() && (p[j] == b'!' || p[j] == b'^') {
                            negate = true;
                            j += 1;
                        }
                        let class_start = j;
                        while j < p.len() && p[j] != b']' {
                            j += 1;
                        }
                        if j >= p.len() {
                            // Unterminated → treat as literal '['.
                        } else {
                            let class = &p[class_start..j];
                            let mut hit = false;
                            let mut k = 0;
                            while k < class.len() {
                                if k + 2 < class.len() && class[k + 1] == b'-' {
                                    if s[si] >= class[k] && s[si] <= class[k + 2] {
                                        hit = true;
                                        break;
                                    }
                                    k += 3;
                                } else {
                                    if s[si] == class[k] {
                                        hit = true;
                                        break;
                                    }
                                    k += 1;
                                }
                            }
                            if hit != negate {
                                pi = j + 1;
                                si += 1;
                                continue;
                            }
                        }
                    }
                    c if c == s[si] => {
                        pi += 1;
                        si += 1;
                        continue;
                    }
                    _ => {}
                }
            }
            if let Some((sp, ss)) = star {
                pi = sp + 1;
                si = ss + 1;
                star = Some((sp, si));
                continue;
            }
            return false;
        }
        while pi < p.len() && p[pi] == b'*' {
            pi += 1;
        }
        pi == p.len()
    }
    rec(pat.as_bytes(), name.as_bytes())
}

struct NameTest {
    pat: String,
    ci: bool,
}
impl Node for NameTest {
    fn eval(&mut self, p: &Path, _st: &Metadata, _ctx: &mut Ctx) -> bool {
        let n = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        fnmatch(&self.pat, n, self.ci)
    }
}

struct PathTest {
    pat: String,
    ci: bool,
}
impl Node for PathTest {
    fn eval(&mut self, p: &Path, _st: &Metadata, _ctx: &mut Ctx) -> bool {
        let s = p.display().to_string();
        fnmatch(&self.pat, &s, self.ci)
    }
}

struct TypeTest {
    t: char,
}
impl Node for TypeTest {
    fn eval(&mut self, _p: &Path, st: &Metadata, _ctx: &mut Ctx) -> bool {
        match self.t {
            'f' => st.is_file() && !st.file_type().is_symlink(),
            'd' => st.is_dir() && !st.file_type().is_symlink(),
            'l' => st.file_type().is_symlink(),
            _ => false,
        }
    }
}

struct SizeTest {
    cmp_: char,
    n: u64,
    unit: u64,
}
impl Node for SizeTest {
    fn eval(&mut self, _p: &Path, st: &Metadata, _ctx: &mut Ctx) -> bool {
        let units = st.len().div_ceil(self.unit);
        match self.cmp_ {
            '+' => units > self.n,
            '-' => units < self.n,
            _ => units == self.n,
        }
    }
}

struct TimeTest {
    which: char,
    in_minutes: bool,
    cmp_: char,
    n: i64,
}
impl Node for TimeTest {
    fn eval(&mut self, _p: &Path, st: &Metadata, ctx: &mut Ctx) -> bool {
        let t = match self.which {
            'm' => st.modified().ok(),
            'a' => st.accessed().ok(),
            'c' => st.created().or_else(|_| st.modified()).ok(),
            _ => None,
        };
        let secs = match t {
            Some(t) => t
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
            None => return false,
        };
        let diff = ctx.now_secs - secs;
        let div = if self.in_minutes { 60 } else { 86_400 };
        let units = diff / div;
        match self.cmp_ {
            '+' => units > self.n,
            '-' => units < self.n,
            _ => units == self.n,
        }
    }
}

struct NewerTest {
    ref_secs: i64,
}
impl Node for NewerTest {
    fn eval(&mut self, _p: &Path, st: &Metadata, _ctx: &mut Ctx) -> bool {
        let m = st
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        m > self.ref_secs
    }
}

struct EmptyTest;
impl Node for EmptyTest {
    fn eval(&mut self, p: &Path, st: &Metadata, _ctx: &mut Ctx) -> bool {
        if st.is_file() {
            return st.len() == 0;
        }
        if st.is_dir() {
            return std::fs::read_dir(p)
                .map(|mut it| it.next().is_none())
                .unwrap_or(false);
        }
        false
    }
}

// --- actions ----------------------------------------------------------------

struct PrintAction {
    null: bool,
}
impl Node for PrintAction {
    fn eval(&mut self, p: &Path, _st: &Metadata, _ctx: &mut Ctx) -> bool {
        let end = if self.null { '\0' } else { '\n' };
        print!("{}{}", p.display(), end);
        true
    }
    fn has_action(&self) -> bool {
        true
    }
}

struct DeleteAction;
impl Node for DeleteAction {
    fn eval(&mut self, p: &Path, st: &Metadata, ctx: &mut Ctx) -> bool {
        let res = if st.is_dir() && !st.file_type().is_symlink() {
            std::fs::remove_dir(p)
        } else {
            std::fs::remove_file(p)
        };
        if let Err(e) = res {
            err_path("find", &p.display().to_string(), &e);
            ctx.rc = 1;
            return false;
        }
        true
    }
    fn has_action(&self) -> bool {
        true
    }
}

struct PruneAction;
impl Node for PruneAction {
    fn eval(&mut self, _p: &Path, _st: &Metadata, ctx: &mut Ctx) -> bool {
        ctx.pruned = true;
        true
    }
}

struct ExecAction {
    cmd: Vec<String>,
    /// `;` immediate, `+` batch.
    mode: char,
    batch: Vec<PathBuf>,
}
impl Node for ExecAction {
    fn eval(&mut self, p: &Path, _st: &Metadata, ctx: &mut Ctx) -> bool {
        if self.mode == ';' {
            let argv: Vec<String> = self
                .cmd
                .iter()
                .map(|tok| {
                    if tok == "{}" {
                        p.display().to_string()
                    } else {
                        tok.clone()
                    }
                })
                .collect();
            match Command::new(&argv[0]).args(&argv[1..]).status() {
                Ok(s) => s.success(),
                Err(e) => {
                    err("find", &format!("exec: {e}"));
                    ctx.rc = 1;
                    false
                }
            }
        } else {
            self.batch.push(p.to_path_buf());
            if self.batch.len() >= 1000 {
                self.flush();
            }
            true
        }
    }
    fn finalize(&mut self) {
        if self.mode == '+' {
            self.flush();
        }
    }
    fn has_action(&self) -> bool {
        true
    }
}
impl ExecAction {
    fn flush(&mut self) {
        if self.batch.is_empty() {
            return;
        }
        let mut argv: Vec<String> = Vec::new();
        let mut placeholder = false;
        for tok in &self.cmd {
            if tok == "{}" {
                argv.extend(self.batch.iter().map(|p| p.display().to_string()));
                placeholder = true;
            } else {
                argv.push(tok.clone());
            }
        }
        if !placeholder {
            argv.extend(self.batch.iter().map(|p| p.display().to_string()));
        }
        if let Err(e) = Command::new(&argv[0]).args(&argv[1..]).status() {
            err("find", &format!("exec: {e}"));
        }
        self.batch.clear();
    }
}

// --- parser -----------------------------------------------------------------

struct Parser {
    toks: Vec<String>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<&str> {
        self.toks.get(self.i).map(String::as_str)
    }
    fn consume(&mut self) -> String {
        let t = self.toks[self.i].clone();
        self.i += 1;
        t
    }
    fn expect(&mut self, t: &str) -> Result<(), String> {
        if self.peek() != Some(t) {
            return Err(format!(
                "expected '{t}', got '{}'",
                self.peek().unwrap_or("<end>")
            ));
        }
        self.consume();
        Ok(())
    }
    fn need_arg(&mut self, flag: &str) -> Result<String, String> {
        if self.peek().is_none() {
            return Err(format!("{flag}: missing argument"));
        }
        Ok(self.consume())
    }
    fn parse_expr(&mut self) -> Result<Box<dyn Node>, String> {
        self.parse_or()
    }
    fn parse_or(&mut self) -> Result<Box<dyn Node>, String> {
        let mut left = self.parse_and()?;
        while matches!(self.peek(), Some("-o" | "-or")) {
            self.consume();
            let right = self.parse_and()?;
            left = Box::new(Or(left, right));
        }
        Ok(left)
    }
    fn parse_and(&mut self) -> Result<Box<dyn Node>, String> {
        let mut left = self.parse_not()?;
        loop {
            match self.peek() {
                Some("-a" | "-and") => {
                    self.consume();
                    let r = self.parse_not()?;
                    left = Box::new(And(left, r));
                }
                None | Some("-o" | "-or" | ")") => break,
                _ => {
                    let r = self.parse_not()?;
                    left = Box::new(And(left, r));
                }
            }
        }
        Ok(left)
    }
    fn parse_not(&mut self) -> Result<Box<dyn Node>, String> {
        if matches!(self.peek(), Some("-not" | "!")) {
            self.consume();
            let inner = self.parse_not()?;
            return Ok(Box::new(Not(inner)));
        }
        self.parse_primary()
    }
    fn parse_primary(&mut self) -> Result<Box<dyn Node>, String> {
        if self.peek() == Some("(") {
            self.consume();
            let inner = self.parse_expr()?;
            self.expect(")")?;
            return Ok(inner);
        }
        let tok = self.consume();
        match tok.as_str() {
            "-name" => Ok(Box::new(NameTest {
                pat: self.need_arg("-name")?,
                ci: false,
            })),
            "-iname" => Ok(Box::new(NameTest {
                pat: self.need_arg("-iname")?,
                ci: true,
            })),
            "-path" => Ok(Box::new(PathTest {
                pat: self.need_arg("-path")?,
                ci: false,
            })),
            "-ipath" => Ok(Box::new(PathTest {
                pat: self.need_arg("-ipath")?,
                ci: true,
            })),
            "-type" => {
                let v = self.need_arg("-type")?;
                if !matches!(v.as_str(), "f" | "d" | "l") {
                    return Err(format!("-type: unsupported type '{v}'"));
                }
                Ok(Box::new(TypeTest {
                    t: v.chars().next().unwrap(),
                }))
            }
            "-size" => self.parse_size(),
            "-mtime" | "-mmin" | "-atime" | "-amin" | "-ctime" | "-cmin" => self.parse_time(&tok),
            "-newer" => {
                let f = self.need_arg("-newer")?;
                let m = std::fs::metadata(&f).map_err(|e| format!("-newer: {f}: {e}"))?;
                let secs = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                Ok(Box::new(NewerTest { ref_secs: secs }))
            }
            "-empty" => Ok(Box::new(EmptyTest)),
            "-true" => Ok(Box::new(True_)),
            "-print" => Ok(Box::new(PrintAction { null: false })),
            "-print0" => Ok(Box::new(PrintAction { null: true })),
            "-delete" => Ok(Box::new(DeleteAction)),
            "-prune" => Ok(Box::new(PruneAction)),
            "-exec" => self.parse_exec(),
            other => Err(format!("unknown predicate: '{other}'")),
        }
    }
    fn parse_size(&mut self) -> Result<Box<dyn Node>, String> {
        let v = self.need_arg("-size")?;
        let (cmp_, body) = if let Some(rest) = v.strip_prefix('+') {
            ('+', rest)
        } else if let Some(rest) = v.strip_prefix('-') {
            ('-', rest)
        } else {
            ('=', v.as_str())
        };
        let mut j = 0;
        while j < body.len() && body.as_bytes()[j].is_ascii_digit() {
            j += 1;
        }
        if j == 0 {
            return Err("-size: invalid value".to_string());
        }
        let n: u64 = body[..j]
            .parse()
            .map_err(|_| "-size: invalid value".to_string())?;
        let suffix = &body[j..];
        let unit: u64 = match suffix {
            "" | "b" => 512,
            "c" => 1,
            "w" => 2,
            "k" => 1024,
            "M" => 1024 * 1024,
            "G" => 1024 * 1024 * 1024,
            other => return Err(format!("-size: unknown unit '{other}'")),
        };
        Ok(Box::new(SizeTest { cmp_, n, unit }))
    }
    fn parse_time(&mut self, tok: &str) -> Result<Box<dyn Node>, String> {
        let v = self.need_arg(tok)?;
        let (cmp_, body) = if let Some(r) = v.strip_prefix('+') {
            ('+', r)
        } else if let Some(r) = v.strip_prefix('-') {
            ('-', r)
        } else {
            ('=', v.as_str())
        };
        let n: i64 = body
            .parse()
            .map_err(|_| format!("{tok}: invalid value '{v}'"))?;
        let which = tok.as_bytes()[1] as char;
        let in_minutes = tok.ends_with("min");
        Ok(Box::new(TimeTest {
            which,
            in_minutes,
            cmp_,
            n,
        }))
    }
    fn parse_exec(&mut self) -> Result<Box<dyn Node>, String> {
        let mut cmd: Vec<String> = Vec::new();
        loop {
            match self.peek() {
                None => return Err("-exec: unterminated (expected ';' or '+')".to_string()),
                Some(";") => {
                    self.consume();
                    return Ok(Box::new(ExecAction {
                        cmd,
                        mode: ';',
                        batch: Vec::new(),
                    }));
                }
                Some("+") => {
                    self.consume();
                    return Ok(Box::new(ExecAction {
                        cmd,
                        mode: '+',
                        batch: Vec::new(),
                    }));
                }
                Some(_) => cmd.push(self.consume()),
            }
        }
    }
}

fn extract_globals(tokens: Vec<String>) -> Result<(Vec<String>, i32, i32), String> {
    let mut mindepth = 0i32;
    let mut maxdepth = -1i32;
    let mut remaining: Vec<String> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i].as_str() {
            "-maxdepth" if i + 1 < tokens.len() => {
                maxdepth = tokens[i + 1]
                    .parse()
                    .map_err(|_| format!("-maxdepth: invalid value '{}'", tokens[i + 1]))?;
                i += 2;
            }
            "-mindepth" if i + 1 < tokens.len() => {
                mindepth = tokens[i + 1]
                    .parse()
                    .map_err(|_| format!("-mindepth: invalid value '{}'", tokens[i + 1]))?;
                i += 2;
            }
            _ => {
                remaining.push(tokens[i].clone());
                i += 1;
            }
        }
    }
    Ok((remaining, mindepth, maxdepth))
}

fn walk(root: &Path, expr: &mut dyn Node, mindepth: i32, maxdepth: i32, ctx: &mut Ctx) {
    fn visit(p: &Path, depth: i32, expr: &mut dyn Node, mn: i32, mx: i32, ctx: &mut Ctx) {
        let st = match std::fs::symlink_metadata(p) {
            Ok(m) => m,
            Err(e) => {
                err_path("find", &p.display().to_string(), &e);
                ctx.rc = 1;
                return;
            }
        };
        ctx.pruned = false;
        if depth >= mn && (mx < 0 || depth <= mx) {
            expr.eval(p, &st, ctx);
        }
        if ctx.pruned {
            return;
        }
        if mx >= 0 && depth >= mx {
            return;
        }
        if !st.is_dir() || st.file_type().is_symlink() {
            return;
        }
        let entries = match std::fs::read_dir(p) {
            Ok(d) => {
                let mut v: Vec<_> = d.flatten().collect();
                v.sort_by_key(|e| e.file_name());
                v
            }
            Err(e) => {
                err_path("find", &p.display().to_string(), &e);
                ctx.rc = 1;
                return;
            }
        };
        for entry in entries {
            visit(&entry.path(), depth + 1, expr, mn, mx, ctx);
        }
    }
    visit(root, 0, expr, mindepth, maxdepth, ctx);
}

fn main(argv: &[String]) -> i32 {
    let args = &argv[1..];
    let mut paths: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        let a = &args[i];
        if a.starts_with('-') || a == "(" || a == ")" || a == "!" {
            break;
        }
        paths.push(a.clone());
        i += 1;
    }
    let expr_tokens: Vec<String> = args[i..].to_vec();
    if paths.is_empty() {
        paths.push(".".to_string());
    }

    let (tokens, mindepth, maxdepth) = match extract_globals(expr_tokens) {
        Ok(x) => x,
        Err(e) => {
            err("find", &e);
            return 2;
        }
    };

    let no_tokens = tokens.is_empty();
    let mut parser = Parser { toks: tokens, i: 0 };
    let expr_root: Box<dyn Node> = if no_tokens {
        Box::new(True_)
    } else {
        match parser.parse_expr() {
            Ok(e) => e,
            Err(msg) => {
                err("find", &msg);
                return 2;
            }
        }
    };
    if parser.i != parser.toks.len() {
        err(
            "find",
            &format!("unexpected token: '{}'", parser.toks[parser.i]),
        );
        return 2;
    }

    let mut expr: Box<dyn Node> = if expr_root.has_action() {
        expr_root
    } else {
        Box::new(And(expr_root, Box::new(PrintAction { null: false })))
    };

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let mut ctx = Ctx {
        now_secs,
        pruned: false,
        rc: 0,
    };
    for p in &paths {
        let root = PathBuf::from(p);
        if !root.exists() && !root.is_symlink() {
            err_path(
                "find",
                p,
                &std::io::Error::new(std::io::ErrorKind::NotFound, "No such file or directory"),
            );
            ctx.rc = 1;
            continue;
        }
        walk(&root, expr.as_mut(), mindepth, maxdepth, &mut ctx);
    }
    expr.finalize();
    ctx.rc
}
