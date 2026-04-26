//! `awk` — pattern-scanning and processing language.
//!
//! ## Scope of this port
//!
//! Implements a practical POSIX-awk subset covering the most common
//! one-liners and short scripts. The full Python upstream is ~1800 LOC;
//! this port aims at the 80% of usage in ~1000 LOC and explicitly defers
//! the long tail.
//!
//! **Supported:**
//! - `BEGIN`/`END` blocks; pattern-action pairs (regex `/.../`, expression,
//!   range `p1, p2`).
//! - `print`/`printf` with `%d %i %o %x %X %f %e %E %g %G %s %c`.
//! - `$0`, `$1`, ..., `$NF`, `$(expr)`, with `NR`, `NF`, `FS`, `OFS`,
//!   `ORS`, `RS`, `FILENAME`.
//! - Arithmetic, comparisons, `~` / `!~`, `&& || !`, ternary, assignments
//!   (`= += -= *= /= %= ^=`), pre/post `++` `--`, string concatenation
//!   via juxtaposition.
//! - `if`/`else`, `while`, `do`/`while`, `for(;;)`, `for (k in a)`,
//!   `break`, `continue`, `next`, `exit`, `delete`, `k in a`.
//! - Associative arrays (no SUBSEP).
//! - Built-ins: `length`, `substr`, `index`, `split`, `sub`, `gsub`,
//!   `match`, `toupper`, `tolower`, `sprintf`, `int`, `sqrt`, `rand`,
//!   `srand`, `system`.
//!
//! **Not yet supported (parity gaps tracked in PARITY.md):**
//! - User-defined functions (`function name(args) { … }`).
//! - `getline`.
//! - Regex `FS` (literal-character `FS` only — bare `FS=" "` keeps the
//!   special whitespace-splitting POSIX rule).
//! - True multidimensional arrays via `SUBSEP`.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};

use regex::Regex;

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "awk",
    help: "pattern-scanning and processing language",
    aliases: &[],
    main,
};

// ────────────────────────────────────────────────────────────────────
// Lexer
// ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Number(f64),
    String(String),
    Regex(String),
    Ident(String),
    Keyword(String),
    Op(String),
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Semi,
    Comma,
    Newline,
    Eof,
}

fn is_kw(s: &str) -> bool {
    matches!(
        s,
        "BEGIN"
            | "END"
            | "if"
            | "else"
            | "while"
            | "for"
            | "do"
            | "in"
            | "print"
            | "printf"
            | "next"
            | "exit"
            | "break"
            | "continue"
            | "delete"
            | "function"
            | "return"
            | "getline"
    )
}

struct Lexer<'a> {
    src: &'a [u8],
    i: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Lexer {
            src: src.as_bytes(),
            i: 0,
        }
    }

    fn run(&mut self) -> Result<Vec<Tok>, String> {
        let mut out: Vec<Tok> = Vec::new();
        let mut prev: Option<&Tok> = None;
        while self.i < self.src.len() {
            let c = self.src[self.i];
            // comment
            if c == b'#' {
                while self.i < self.src.len() && self.src[self.i] != b'\n' {
                    self.i += 1;
                }
                continue;
            }
            // backslash newline → line continuation
            if c == b'\\' && self.i + 1 < self.src.len() && self.src[self.i + 1] == b'\n' {
                self.i += 2;
                continue;
            }
            // whitespace (no newline)
            if c == b' ' || c == b'\t' {
                self.i += 1;
                continue;
            }
            if c == b'\n' {
                out.push(Tok::Newline);
                self.i += 1;
                prev = out.last();
                continue;
            }
            // number
            if c.is_ascii_digit()
                || (c == b'.'
                    && self.i + 1 < self.src.len()
                    && self.src[self.i + 1].is_ascii_digit())
            {
                let n = self.number()?;
                out.push(Tok::Number(n));
                prev = out.last();
                continue;
            }
            // string
            if c == b'"' {
                let s = self.string()?;
                out.push(Tok::String(s));
                prev = out.last();
                continue;
            }
            // identifier / keyword
            if c.is_ascii_alphabetic() || c == b'_' {
                let s = self.ident();
                if is_kw(&s) {
                    out.push(Tok::Keyword(s));
                } else {
                    out.push(Tok::Ident(s));
                }
                prev = out.last();
                continue;
            }
            // regex literal: `/` only when prev token is an operator / `(` / start.
            if c == b'/' && Self::regex_allowed(prev) {
                let s = self.regex_literal()?;
                out.push(Tok::Regex(s));
                prev = out.last();
                continue;
            }
            // punctuation
            match c {
                b'{' => {
                    out.push(Tok::LBrace);
                    self.i += 1;
                }
                b'}' => {
                    out.push(Tok::RBrace);
                    self.i += 1;
                }
                b'(' => {
                    out.push(Tok::LParen);
                    self.i += 1;
                }
                b')' => {
                    out.push(Tok::RParen);
                    self.i += 1;
                }
                b'[' => {
                    out.push(Tok::LBracket);
                    self.i += 1;
                }
                b']' => {
                    out.push(Tok::RBracket);
                    self.i += 1;
                }
                b';' => {
                    out.push(Tok::Semi);
                    self.i += 1;
                }
                b',' => {
                    out.push(Tok::Comma);
                    self.i += 1;
                }
                _ => {
                    let two = if self.i + 1 < self.src.len() {
                        std::str::from_utf8(&self.src[self.i..self.i + 2])
                            .ok()
                            .map(|s| s.to_string())
                    } else {
                        None
                    };
                    let three = if self.i + 2 < self.src.len() {
                        std::str::from_utf8(&self.src[self.i..self.i + 3])
                            .ok()
                            .map(|s| s.to_string())
                    } else {
                        None
                    };
                    let multi = ["==", "!=", "<=", ">=", "&&", "||", "++", "--",
                        "+=", "-=", "*=", "/=", "%=", "^=", "**", "!~"];
                    if let Some(s) = three.as_deref() {
                        if s == "**=" {
                            out.push(Tok::Op("^=".to_string()));
                            self.i += 3;
                            prev = out.last();
                            continue;
                        }
                    }
                    if let Some(s) = two.as_deref() {
                        if multi.contains(&s) {
                            let mapped = if s == "**" { "^" } else { s };
                            out.push(Tok::Op(mapped.to_string()));
                            self.i += 2;
                            prev = out.last();
                            continue;
                        }
                    }
                    let s = (c as char).to_string();
                    if "+-*/%^=<>!~?:$".contains(&s) {
                        out.push(Tok::Op(s));
                        self.i += 1;
                        prev = out.last();
                        continue;
                    }
                    return Err(format!("unexpected character: {:?}", c as char));
                }
            }
            prev = out.last();
        }
        out.push(Tok::Eof);
        Ok(out)
    }

    fn regex_allowed(prev: Option<&Tok>) -> bool {
        match prev {
            None => true,
            Some(Tok::Number(_)) | Some(Tok::String(_)) | Some(Tok::Ident(_)) => false,
            Some(Tok::RParen) | Some(Tok::RBracket) => false,
            Some(Tok::Op(o)) if o == "++" || o == "--" => false,
            _ => true,
        }
    }

    fn number(&mut self) -> Result<f64, String> {
        let start = self.i;
        while self.i < self.src.len()
            && (self.src[self.i].is_ascii_digit() || self.src[self.i] == b'.')
        {
            self.i += 1;
        }
        // exponent
        if self.i < self.src.len() && (self.src[self.i] == b'e' || self.src[self.i] == b'E') {
            self.i += 1;
            if self.i < self.src.len() && (self.src[self.i] == b'+' || self.src[self.i] == b'-') {
                self.i += 1;
            }
            while self.i < self.src.len() && self.src[self.i].is_ascii_digit() {
                self.i += 1;
            }
        }
        std::str::from_utf8(&self.src[start..self.i])
            .map_err(|e| e.to_string())?
            .parse::<f64>()
            .map_err(|e| e.to_string())
    }

    fn string(&mut self) -> Result<String, String> {
        self.i += 1; // skip "
        let mut out = String::new();
        while self.i < self.src.len() && self.src[self.i] != b'"' {
            let c = self.src[self.i];
            if c == b'\\' && self.i + 1 < self.src.len() {
                let nx = self.src[self.i + 1];
                let esc = match nx {
                    b'n' => '\n',
                    b't' => '\t',
                    b'r' => '\r',
                    b'\\' => '\\',
                    b'"' => '"',
                    b'/' => '/',
                    b'0' => '\0',
                    b'a' => '\x07',
                    b'b' => '\x08',
                    b'f' => '\x0c',
                    b'v' => '\x0b',
                    other => other as char,
                };
                out.push(esc);
                self.i += 2;
                continue;
            }
            out.push(c as char);
            self.i += 1;
        }
        if self.i >= self.src.len() {
            return Err("unterminated string".to_string());
        }
        self.i += 1; // skip "
        Ok(out)
    }

    fn regex_literal(&mut self) -> Result<String, String> {
        self.i += 1; // skip /
        let mut out = String::new();
        while self.i < self.src.len() && self.src[self.i] != b'/' {
            if self.src[self.i] == b'\\' && self.i + 1 < self.src.len() {
                out.push(self.src[self.i] as char);
                out.push(self.src[self.i + 1] as char);
                self.i += 2;
                continue;
            }
            out.push(self.src[self.i] as char);
            self.i += 1;
        }
        if self.i >= self.src.len() {
            return Err("unterminated regex".to_string());
        }
        self.i += 1; // skip /
        Ok(out)
    }

    fn ident(&mut self) -> String {
        let start = self.i;
        while self.i < self.src.len()
            && (self.src[self.i].is_ascii_alphanumeric() || self.src[self.i] == b'_')
        {
            self.i += 1;
        }
        std::str::from_utf8(&self.src[start..self.i])
            .unwrap_or("")
            .to_string()
    }
}

// ────────────────────────────────────────────────────────────────────
// AST
// ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Expr {
    Num(f64),
    Str(String),
    Regex(String),
    Var(String),
    Field(Box<Expr>),
    /// Array subscript: `a[i,j,...]` (multi-key joined with SUBSEP).
    Index(String, Vec<Expr>),
    Bin(String, Box<Expr>, Box<Expr>),
    Unary(String, Box<Expr>),
    Concat(Box<Expr>, Box<Expr>),
    /// `name op= rhs` for non-array vars.
    AssignVar(String, String, Box<Expr>),
    AssignIndex(String, Vec<Expr>, String, Box<Expr>),
    AssignField(Box<Expr>, String, Box<Expr>),
    Pre(String, Box<Expr>),
    Post(String, Box<Expr>),
    Match(bool, Box<Expr>, String), // (negate, lhs, regex)
    Tern(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    InArray(String, Vec<Expr>),
    Group(Box<Expr>),
}

#[derive(Debug, Clone)]
enum Stmt {
    Expr(Expr),
    Print(Vec<Expr>),
    Printf(Vec<Expr>),
    If(Expr, Vec<Stmt>, Option<Vec<Stmt>>),
    While(Expr, Vec<Stmt>),
    DoWhile(Vec<Stmt>, Expr),
    For(Option<Box<Stmt>>, Option<Expr>, Option<Box<Stmt>>, Vec<Stmt>),
    ForIn(String, String, Vec<Stmt>),
    Next,
    Exit(Option<Expr>),
    Break,
    Continue,
    Delete(String, Vec<Expr>),
    Block(Vec<Stmt>),
}

#[derive(Debug, Clone)]
enum PatternKind {
    Always,
    Begin,
    End,
    Regex(String),
    Expr(Expr),
    Range(Box<PatternKind>, Box<PatternKind>),
}

#[derive(Debug, Clone)]
struct Rule {
    pattern: PatternKind,
    action: Vec<Stmt>,
    /// Used for range patterns to track in-range state.
    range_active: bool,
}

// ────────────────────────────────────────────────────────────────────
// Parser
// ────────────────────────────────────────────────────────────────────

struct Parser {
    toks: Vec<Tok>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.i]
    }
    fn bump(&mut self) -> Tok {
        let t = self.toks[self.i].clone();
        self.i += 1;
        t
    }
    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Tok::Newline | Tok::Semi) {
            self.i += 1;
        }
    }
    fn expect(&mut self, t: &Tok) -> Result<(), String> {
        if std::mem::discriminant(self.peek()) != std::mem::discriminant(t) {
            return Err(format!("expected {t:?}, got {:?}", self.peek()));
        }
        if let (Tok::Op(a), Tok::Op(b)) = (self.peek(), t) {
            if a != b {
                return Err(format!("expected {b}, got {a}"));
            }
        }
        self.i += 1;
        Ok(())
    }

    fn parse_program(&mut self) -> Result<Vec<Rule>, String> {
        let mut rules: Vec<Rule> = Vec::new();
        self.skip_newlines();
        while !matches!(self.peek(), Tok::Eof) {
            let pattern = self.parse_pattern()?;
            self.skip_ws_inline();
            let action: Vec<Stmt> = if matches!(self.peek(), Tok::LBrace) {
                self.bump();
                let body = self.parse_block_body()?;
                self.expect(&Tok::RBrace)?;
                body
            } else {
                // Pattern with no action defaults to `{ print }`.
                vec![Stmt::Print(vec![Expr::Field(Box::new(Expr::Num(0.0)))])]
            };
            rules.push(Rule {
                pattern,
                action,
                range_active: false,
            });
            self.skip_newlines();
        }
        Ok(rules)
    }

    fn skip_ws_inline(&mut self) {
        while matches!(self.peek(), Tok::Newline) {
            self.i += 1;
        }
    }

    fn parse_pattern(&mut self) -> Result<PatternKind, String> {
        if let Tok::Keyword(k) = self.peek().clone() {
            if k == "BEGIN" {
                self.bump();
                return Ok(PatternKind::Begin);
            }
            if k == "END" {
                self.bump();
                return Ok(PatternKind::End);
            }
        }
        if matches!(self.peek(), Tok::LBrace) {
            return Ok(PatternKind::Always);
        }
        let first = self.parse_pattern_atom()?;
        if matches!(self.peek(), Tok::Comma) {
            self.bump();
            let second = self.parse_pattern_atom()?;
            return Ok(PatternKind::Range(Box::new(first), Box::new(second)));
        }
        Ok(first)
    }

    fn parse_pattern_atom(&mut self) -> Result<PatternKind, String> {
        if let Tok::Regex(r) = self.peek().clone() {
            self.bump();
            return Ok(PatternKind::Regex(r));
        }
        let e = self.parse_expr()?;
        Ok(PatternKind::Expr(e))
    }

    fn parse_block_body(&mut self) -> Result<Vec<Stmt>, String> {
        let mut out: Vec<Stmt> = Vec::new();
        self.skip_newlines();
        while !matches!(self.peek(), Tok::RBrace | Tok::Eof) {
            let s = self.parse_stmt()?;
            out.push(s);
            // statements may end on newline, semi, or be followed by `}`.
            self.skip_newlines();
        }
        Ok(out)
    }

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        if matches!(self.peek(), Tok::LBrace) {
            self.bump();
            let body = self.parse_block_body()?;
            self.expect(&Tok::RBrace)?;
            return Ok(Stmt::Block(body));
        }
        if let Tok::Keyword(k) = self.peek().clone() {
            match k.as_str() {
                "if" => {
                    self.bump();
                    self.expect(&Tok::LParen)?;
                    let cond = self.parse_expr()?;
                    self.expect(&Tok::RParen)?;
                    self.skip_newlines();
                    let then = self.parse_stmt_or_block()?;
                    self.skip_newlines();
                    let els = if let Tok::Keyword(k) = self.peek() {
                        if k == "else" {
                            self.bump();
                            self.skip_newlines();
                            Some(self.parse_stmt_or_block()?)
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    return Ok(Stmt::If(cond, then, els));
                }
                "while" => {
                    self.bump();
                    self.expect(&Tok::LParen)?;
                    let cond = self.parse_expr()?;
                    self.expect(&Tok::RParen)?;
                    self.skip_newlines();
                    let body = self.parse_stmt_or_block()?;
                    return Ok(Stmt::While(cond, body));
                }
                "do" => {
                    self.bump();
                    self.skip_newlines();
                    let body = self.parse_stmt_or_block()?;
                    self.skip_newlines();
                    if let Tok::Keyword(k) = self.peek().clone() {
                        if k == "while" {
                            self.bump();
                            self.expect(&Tok::LParen)?;
                            let cond = self.parse_expr()?;
                            self.expect(&Tok::RParen)?;
                            return Ok(Stmt::DoWhile(body, cond));
                        }
                    }
                    return Err("expected 'while' after do-block".into());
                }
                "for" => {
                    self.bump();
                    self.expect(&Tok::LParen)?;
                    // Try `for (k in a)` form: ident, "in", ident.
                    let saved = self.i;
                    if let Tok::Ident(k) = self.peek().clone() {
                        let after = self.i + 1;
                        if let Some(Tok::Keyword(kw)) = self.toks.get(after) {
                            if kw == "in" {
                                if let Some(Tok::Ident(arr)) = self.toks.get(after + 1) {
                                    let arr = arr.clone();
                                    self.i = after + 2;
                                    self.expect(&Tok::RParen)?;
                                    self.skip_newlines();
                                    let body = self.parse_stmt_or_block()?;
                                    return Ok(Stmt::ForIn(k, arr, body));
                                }
                            }
                        }
                    }
                    self.i = saved;
                    let init = if matches!(self.peek(), Tok::Semi) {
                        None
                    } else {
                        Some(Box::new(Stmt::Expr(self.parse_expr()?)))
                    };
                    self.expect(&Tok::Semi)?;
                    let cond = if matches!(self.peek(), Tok::Semi) {
                        None
                    } else {
                        Some(self.parse_expr()?)
                    };
                    self.expect(&Tok::Semi)?;
                    let post = if matches!(self.peek(), Tok::RParen) {
                        None
                    } else {
                        Some(Box::new(Stmt::Expr(self.parse_expr()?)))
                    };
                    self.expect(&Tok::RParen)?;
                    self.skip_newlines();
                    let body = self.parse_stmt_or_block()?;
                    return Ok(Stmt::For(init, cond, post, body));
                }
                "next" => {
                    self.bump();
                    return Ok(Stmt::Next);
                }
                "exit" => {
                    self.bump();
                    let e = if matches!(self.peek(), Tok::Newline | Tok::Semi | Tok::RBrace | Tok::Eof) {
                        None
                    } else {
                        Some(self.parse_expr()?)
                    };
                    return Ok(Stmt::Exit(e));
                }
                "break" => {
                    self.bump();
                    return Ok(Stmt::Break);
                }
                "continue" => {
                    self.bump();
                    return Ok(Stmt::Continue);
                }
                "print" => {
                    self.bump();
                    let mut args: Vec<Expr> = Vec::new();
                    if !matches!(self.peek(), Tok::Newline | Tok::Semi | Tok::RBrace | Tok::Eof) {
                        args.push(self.parse_expr_no_inq()?);
                        while matches!(self.peek(), Tok::Comma) {
                            self.bump();
                            args.push(self.parse_expr_no_inq()?);
                        }
                    }
                    return Ok(Stmt::Print(args));
                }
                "printf" => {
                    self.bump();
                    let mut args: Vec<Expr> = Vec::new();
                    args.push(self.parse_expr_no_inq()?);
                    while matches!(self.peek(), Tok::Comma) {
                        self.bump();
                        args.push(self.parse_expr_no_inq()?);
                    }
                    return Ok(Stmt::Printf(args));
                }
                "delete" => {
                    self.bump();
                    if let Tok::Ident(name) = self.peek().clone() {
                        self.bump();
                        if matches!(self.peek(), Tok::LBracket) {
                            self.bump();
                            let mut idx: Vec<Expr> = vec![self.parse_expr()?];
                            while matches!(self.peek(), Tok::Comma) {
                                self.bump();
                                idx.push(self.parse_expr()?);
                            }
                            self.expect(&Tok::RBracket)?;
                            return Ok(Stmt::Delete(name, idx));
                        }
                        return Ok(Stmt::Delete(name, vec![]));
                    }
                    return Err("delete: expected identifier".into());
                }
                _ => {}
            }
        }
        let e = self.parse_expr()?;
        Ok(Stmt::Expr(e))
    }

    fn parse_stmt_or_block(&mut self) -> Result<Vec<Stmt>, String> {
        if matches!(self.peek(), Tok::LBrace) {
            self.bump();
            let body = self.parse_block_body()?;
            self.expect(&Tok::RBrace)?;
            return Ok(body);
        }
        Ok(vec![self.parse_stmt()?])
    }

    /// Like parse_expr but stops at `>` (which we don't support in print
    /// contexts as redirection). Currently same as parse_expr — placeholder
    /// for future redirection support.
    fn parse_expr_no_inq(&mut self) -> Result<Expr, String> {
        self.parse_expr()
    }

    /// Pratt-ish expression parser. Precedence is approximately:
    ///   ternary -> || -> && -> in -> match (~ !~) -> rel -> concat ->
    ///   addsub -> muldivmod -> exp -> unary -> postfix -> atom
    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_assign()
    }

    fn parse_assign(&mut self) -> Result<Expr, String> {
        let left = self.parse_tern()?;
        if let Tok::Op(o) = self.peek().clone() {
            if matches!(o.as_str(), "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "^=") {
                self.bump();
                let rhs = self.parse_assign()?;
                return Ok(match left {
                    Expr::Var(name) => Expr::AssignVar(name, o, Box::new(rhs)),
                    Expr::Index(name, idx) => Expr::AssignIndex(name, idx, o, Box::new(rhs)),
                    Expr::Field(idx) => Expr::AssignField(idx, o, Box::new(rhs)),
                    _ => return Err("invalid assignment target".to_string()),
                });
            }
        }
        Ok(left)
    }

    fn parse_tern(&mut self) -> Result<Expr, String> {
        let c = self.parse_or()?;
        if let Tok::Op(o) = self.peek() {
            if o == "?" {
                self.bump();
                let t = self.parse_assign()?;
                if let Tok::Op(o2) = self.peek().clone() {
                    if o2 == ":" {
                        self.bump();
                        let f = self.parse_assign()?;
                        return Ok(Expr::Tern(Box::new(c), Box::new(t), Box::new(f)));
                    }
                }
                return Err("ternary: expected ':'".into());
            }
        }
        Ok(c)
    }

    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut l = self.parse_and()?;
        while let Tok::Op(o) = self.peek() {
            if o == "||" {
                self.bump();
                let r = self.parse_and()?;
                l = Expr::Bin("||".to_string(), Box::new(l), Box::new(r));
                continue;
            }
            break;
        }
        Ok(l)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut l = self.parse_in()?;
        while let Tok::Op(o) = self.peek() {
            if o == "&&" {
                self.bump();
                let r = self.parse_in()?;
                l = Expr::Bin("&&".to_string(), Box::new(l), Box::new(r));
                continue;
            }
            break;
        }
        Ok(l)
    }

    fn parse_in(&mut self) -> Result<Expr, String> {
        let l = self.parse_match()?;
        if let Tok::Keyword(k) = self.peek().clone() {
            if k == "in" {
                self.bump();
                if let Tok::Ident(name) = self.peek().clone() {
                    self.bump();
                    let key = match l {
                        Expr::Group(g) => vec![*g],
                        other => vec![other],
                    };
                    return Ok(Expr::InArray(name, key));
                }
                return Err("in: expected array identifier".into());
            }
        }
        Ok(l)
    }

    fn parse_match(&mut self) -> Result<Expr, String> {
        let mut l = self.parse_rel()?;
        while let Tok::Op(o) = self.peek().clone() {
            if o == "~" || o == "!~" {
                self.bump();
                let neg = o == "!~";
                let rhs = self.parse_rel()?;
                let pat = match rhs {
                    Expr::Regex(s) => s,
                    Expr::Str(s) => s,
                    other => {
                        // dynamic regex from any other expr: stringify at eval time
                        return Ok(Expr::Bin(
                            if neg { "!~~".to_string() } else { "~~".to_string() },
                            Box::new(l),
                            Box::new(other),
                        ));
                    }
                };
                l = Expr::Match(neg, Box::new(l), pat);
                continue;
            }
            break;
        }
        Ok(l)
    }

    fn parse_rel(&mut self) -> Result<Expr, String> {
        let l = self.parse_concat()?;
        if let Tok::Op(o) = self.peek().clone() {
            if matches!(o.as_str(), "<" | ">" | "<=" | ">=" | "==" | "!=") {
                self.bump();
                let r = self.parse_concat()?;
                return Ok(Expr::Bin(o, Box::new(l), Box::new(r)));
            }
        }
        Ok(l)
    }

    fn parse_concat(&mut self) -> Result<Expr, String> {
        let mut l = self.parse_addsub()?;
        loop {
            if Self::starts_expr(self.peek()) {
                let r = self.parse_addsub()?;
                l = Expr::Concat(Box::new(l), Box::new(r));
            } else {
                break;
            }
        }
        Ok(l)
    }

    fn starts_expr(t: &Tok) -> bool {
        matches!(
            t,
            Tok::Number(_) | Tok::String(_) | Tok::Ident(_) | Tok::LParen | Tok::Op(_)
        ) && !matches!(
            t,
            Tok::Op(o) if matches!(o.as_str(),
                "+" | "-" | "*" | "/" | "%" | "^" | "==" | "!=" | "<" | ">" |
                "<=" | ">=" | "&&" | "||" | "=" | "+=" | "-=" | "*=" | "/=" |
                "%=" | "^=" | "?" | ":" | "~" | "!~" | "++" | "--")
        )
    }

    fn parse_addsub(&mut self) -> Result<Expr, String> {
        let mut l = self.parse_muldiv()?;
        while let Tok::Op(o) = self.peek().clone() {
            if o == "+" || o == "-" {
                self.bump();
                let r = self.parse_muldiv()?;
                l = Expr::Bin(o, Box::new(l), Box::new(r));
                continue;
            }
            break;
        }
        Ok(l)
    }

    fn parse_muldiv(&mut self) -> Result<Expr, String> {
        let mut l = self.parse_exp()?;
        while let Tok::Op(o) = self.peek().clone() {
            if o == "*" || o == "/" || o == "%" {
                self.bump();
                let r = self.parse_exp()?;
                l = Expr::Bin(o, Box::new(l), Box::new(r));
                continue;
            }
            break;
        }
        Ok(l)
    }

    fn parse_exp(&mut self) -> Result<Expr, String> {
        let l = self.parse_unary()?;
        if let Tok::Op(o) = self.peek().clone() {
            if o == "^" {
                self.bump();
                let r = self.parse_exp()?;
                return Ok(Expr::Bin("^".to_string(), Box::new(l), Box::new(r)));
            }
        }
        Ok(l)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        if let Tok::Op(o) = self.peek().clone() {
            if o == "!" || o == "-" || o == "+" {
                self.bump();
                let e = self.parse_unary()?;
                return Ok(Expr::Unary(o, Box::new(e)));
            }
            if o == "++" || o == "--" {
                self.bump();
                let e = self.parse_unary()?;
                return Ok(Expr::Pre(o, Box::new(e)));
            }
            if o == "$" {
                self.bump();
                let e = self.parse_unary()?;
                return Ok(Expr::Field(Box::new(e)));
            }
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut e = self.parse_atom()?;
        if let Tok::Op(o) = self.peek().clone() {
            if o == "++" || o == "--" {
                self.bump();
                e = Expr::Post(o, Box::new(e));
            }
        }
        Ok(e)
    }

    fn parse_atom(&mut self) -> Result<Expr, String> {
        match self.peek().clone() {
            Tok::Number(n) => {
                self.bump();
                Ok(Expr::Num(n))
            }
            Tok::String(s) => {
                self.bump();
                Ok(Expr::Str(s))
            }
            Tok::Regex(r) => {
                self.bump();
                Ok(Expr::Regex(r))
            }
            Tok::LParen => {
                self.bump();
                let e = self.parse_expr()?;
                self.expect(&Tok::RParen)?;
                Ok(Expr::Group(Box::new(e)))
            }
            Tok::Ident(name) => {
                self.bump();
                if matches!(self.peek(), Tok::LBracket) {
                    self.bump();
                    let mut idx: Vec<Expr> = vec![self.parse_expr()?];
                    while matches!(self.peek(), Tok::Comma) {
                        self.bump();
                        idx.push(self.parse_expr()?);
                    }
                    self.expect(&Tok::RBracket)?;
                    return Ok(Expr::Index(name, idx));
                }
                if matches!(self.peek(), Tok::LParen) {
                    self.bump();
                    let mut args: Vec<Expr> = Vec::new();
                    if !matches!(self.peek(), Tok::RParen) {
                        args.push(self.parse_expr()?);
                        while matches!(self.peek(), Tok::Comma) {
                            self.bump();
                            args.push(self.parse_expr()?);
                        }
                    }
                    self.expect(&Tok::RParen)?;
                    return Ok(Expr::Call(name, args));
                }
                Ok(Expr::Var(name))
            }
            other => Err(format!("unexpected token: {other:?}")),
        }
    }
}

// ────────────────────────────────────────────────────────────────────
// Values
// ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Val {
    Str(String),
    Num(f64),
}

impl Val {
    fn to_num(&self) -> f64 {
        match self {
            Val::Num(n) => *n,
            Val::Str(s) => {
                let t = s.trim_start();
                let mut end = 0usize;
                let bytes = t.as_bytes();
                if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
                    end += 1;
                }
                let mut saw_digit = false;
                let mut saw_dot = false;
                while end < bytes.len() {
                    match bytes[end] {
                        b'0'..=b'9' => {
                            saw_digit = true;
                            end += 1;
                        }
                        b'.' if !saw_dot => {
                            saw_dot = true;
                            end += 1;
                        }
                        b'e' | b'E' => {
                            end += 1;
                            if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
                                end += 1;
                            }
                        }
                        _ => break,
                    }
                }
                if !saw_digit {
                    return 0.0;
                }
                t[..end].parse().unwrap_or(0.0)
            }
        }
    }
    fn to_string_(&self) -> String {
        match self {
            Val::Str(s) => s.clone(),
            Val::Num(n) => {
                if n.is_nan() {
                    return "nan".to_string();
                }
                if n.is_infinite() {
                    return if *n > 0.0 { "inf".to_string() } else { "-inf".to_string() };
                }
                if *n == n.trunc() && n.abs() < 1e16 {
                    return (*n as i64).to_string();
                }
                format!("{n:.6}").trim_end_matches('0').trim_end_matches('.').to_string()
            }
        }
    }
    fn truthy(&self) -> bool {
        match self {
            Val::Num(n) => *n != 0.0,
            Val::Str(s) => !s.is_empty(),
        }
    }
}

// ────────────────────────────────────────────────────────────────────
// Interpreter
// ────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct Interp {
    vars: HashMap<String, Val>,
    arrays: HashMap<String, HashMap<String, Val>>,
    fields: Vec<String>,
    nr: u64,
    nf: u64,
    fs: String,
    filename: String,
    rng_state: u64,
}

enum Flow {
    Normal,
    Next,
    Break,
    Continue,
    Exit(i32),
}

impl Interp {
    fn new() -> Self {
        let mut s = Interp {
            fs: " ".to_string(),
            rng_state: 0xdeadbeef,
            ..Default::default()
        };
        s.vars.insert("FS".to_string(), Val::Str(" ".to_string()));
        s.vars.insert("OFS".to_string(), Val::Str(" ".to_string()));
        s.vars.insert("ORS".to_string(), Val::Str("\n".to_string()));
        s.vars.insert("RS".to_string(), Val::Str("\n".to_string()));
        s.vars.insert("NR".to_string(), Val::Num(0.0));
        s.vars.insert("NF".to_string(), Val::Num(0.0));
        s.vars.insert("FILENAME".to_string(), Val::Str(String::new()));
        s
    }

    fn set_record(&mut self, line: &str) {
        self.fs = self.vars.get("FS").map(|v| v.to_string_()).unwrap_or_else(|| " ".to_string());
        self.fields.clear();
        self.fields.push(line.to_string());
        let split: Vec<String> = if self.fs == " " {
            line.split_whitespace().map(String::from).collect()
        } else if self.fs.chars().count() == 1 {
            line.split(self.fs.chars().next().unwrap()).map(String::from).collect()
        } else {
            // Treat as regex.
            match Regex::new(&self.fs) {
                Ok(rx) => rx.split(line).map(String::from).collect(),
                Err(_) => vec![line.to_string()],
            }
        };
        for s in &split {
            self.fields.push(s.clone());
        }
        self.nf = split.len() as u64;
        self.vars.insert("NF".to_string(), Val::Num(self.nf as f64));
    }

    fn rebuild_field0(&mut self) {
        let ofs = self.vars.get("OFS").map(|v| v.to_string_()).unwrap_or_else(|| " ".to_string());
        let parts: Vec<String> = self.fields[1..].to_vec();
        if !self.fields.is_empty() {
            self.fields[0] = parts.join(&ofs);
        }
    }

    fn get_field(&self, n: usize) -> String {
        if n == 0 {
            return self.fields.first().cloned().unwrap_or_default();
        }
        self.fields.get(n).cloned().unwrap_or_default()
    }

    fn set_field(&mut self, n: usize, v: String) {
        if n == 0 {
            self.fields.clear();
            self.fields.push(v.clone());
            // re-split using current FS
            self.set_record(&v);
            return;
        }
        while self.fields.len() <= n {
            self.fields.push(String::new());
        }
        self.fields[n] = v;
        self.nf = (self.fields.len() - 1) as u64;
        self.vars.insert("NF".to_string(), Val::Num(self.nf as f64));
        self.rebuild_field0();
    }

    fn eval(&mut self, e: &Expr) -> Result<Val, String> {
        match e {
            Expr::Num(n) => Ok(Val::Num(*n)),
            Expr::Str(s) => Ok(Val::Str(s.clone())),
            Expr::Regex(r) => Ok(Val::Num(
                if Regex::new(r)
                    .map(|rx| rx.is_match(&self.get_field(0)))
                    .unwrap_or(false)
                {
                    1.0
                } else {
                    0.0
                },
            )),
            Expr::Group(e) => self.eval(e),
            Expr::Var(name) => Ok(self
                .vars
                .get(name)
                .cloned()
                .unwrap_or(Val::Str(String::new()))),
            Expr::Field(idx) => {
                let n = self.eval(idx)?.to_num() as i64;
                Ok(Val::Str(self.get_field(n.max(0) as usize)))
            }
            Expr::Index(name, idx) => {
                let key = self.compose_key(idx)?;
                let map = self.arrays.entry(name.clone()).or_default();
                Ok(map.get(&key).cloned().unwrap_or(Val::Str(String::new())))
            }
            Expr::Bin(op, l, r) => {
                let lv = self.eval(l)?;
                let rv = self.eval(r)?;
                self.bin(op, lv, rv)
            }
            Expr::Unary(op, e) => {
                let v = self.eval(e)?;
                Ok(match op.as_str() {
                    "-" => Val::Num(-v.to_num()),
                    "+" => Val::Num(v.to_num()),
                    "!" => Val::Num(if v.truthy() { 0.0 } else { 1.0 }),
                    _ => v,
                })
            }
            Expr::Concat(l, r) => {
                let lv = self.eval(l)?.to_string_();
                let rv = self.eval(r)?.to_string_();
                Ok(Val::Str(format!("{lv}{rv}")))
            }
            Expr::AssignVar(name, op, rhs) => {
                let new_v = self.assign_op(op, name.clone(), rhs)?;
                self.vars.insert(name.clone(), new_v.clone());
                Ok(new_v)
            }
            Expr::AssignIndex(name, idx, op, rhs) => {
                let key = self.compose_key(idx)?;
                let cur = self
                    .arrays
                    .get(name)
                    .and_then(|m| m.get(&key))
                    .cloned()
                    .unwrap_or(Val::Str(String::new()));
                let rv = self.eval(rhs)?;
                let new_v = self.combine(op, cur, rv);
                let map = self.arrays.entry(name.clone()).or_default();
                map.insert(key, new_v.clone());
                Ok(new_v)
            }
            Expr::AssignField(idx, op, rhs) => {
                let n = self.eval(idx)?.to_num() as i64;
                let n = n.max(0) as usize;
                let cur = Val::Str(self.get_field(n));
                let rv = self.eval(rhs)?;
                let new_v = self.combine(op, cur, rv);
                self.set_field(n, new_v.to_string_());
                Ok(new_v)
            }
            Expr::Pre(op, e) => self.pre_post(op, e, true),
            Expr::Post(op, e) => self.pre_post(op, e, false),
            Expr::Match(neg, lhs, pat) => {
                let s = self.eval(lhs)?.to_string_();
                let m = Regex::new(pat).map(|rx| rx.is_match(&s)).unwrap_or(false);
                Ok(Val::Num(if m != *neg { 1.0 } else { 0.0 }))
            }
            Expr::Tern(c, t, f) => {
                if self.eval(c)?.truthy() {
                    self.eval(t)
                } else {
                    self.eval(f)
                }
            }
            Expr::Call(name, args) => self.call_builtin(name, args),
            Expr::InArray(name, idx) => {
                let key = self.compose_key(idx)?;
                let present = self
                    .arrays
                    .get(name)
                    .map(|m| m.contains_key(&key))
                    .unwrap_or(false);
                Ok(Val::Num(if present { 1.0 } else { 0.0 }))
            }
        }
    }

    fn compose_key(&mut self, idx: &[Expr]) -> Result<String, String> {
        let mut parts: Vec<String> = Vec::with_capacity(idx.len());
        for e in idx {
            parts.push(self.eval(e)?.to_string_());
        }
        Ok(parts.join("\u{1c}")) // SUBSEP
    }

    fn assign_op(&mut self, op: &str, name: String, rhs: &Expr) -> Result<Val, String> {
        let cur = self.vars.get(&name).cloned().unwrap_or(Val::Str(String::new()));
        let rv = self.eval(rhs)?;
        Ok(self.combine(op, cur, rv))
    }

    fn combine(&self, op: &str, cur: Val, rv: Val) -> Val {
        match op {
            "=" => rv,
            "+=" => Val::Num(cur.to_num() + rv.to_num()),
            "-=" => Val::Num(cur.to_num() - rv.to_num()),
            "*=" => Val::Num(cur.to_num() * rv.to_num()),
            "/=" => Val::Num(cur.to_num() / rv.to_num()),
            "%=" => Val::Num(cur.to_num() % rv.to_num()),
            "^=" => Val::Num(cur.to_num().powf(rv.to_num())),
            _ => rv,
        }
    }

    fn pre_post(&mut self, op: &str, e: &Expr, pre: bool) -> Result<Val, String> {
        let cur = self.eval(e)?;
        let n = cur.to_num();
        let delta = if op == "++" { 1.0 } else { -1.0 };
        let new_v = Val::Num(n + delta);
        // Persist to source if it's a var/index/field.
        match e {
            Expr::Var(name) => {
                self.vars.insert(name.clone(), new_v.clone());
            }
            Expr::Field(idx) => {
                let i = self.eval(idx)?.to_num() as i64;
                self.set_field(i.max(0) as usize, new_v.to_string_());
            }
            Expr::Index(name, idx) => {
                let key = self.compose_key(idx)?;
                let map = self.arrays.entry(name.clone()).or_default();
                map.insert(key, new_v.clone());
            }
            _ => {}
        }
        Ok(if pre { new_v } else { Val::Num(n) })
    }

    fn bin(&mut self, op: &str, l: Val, r: Val) -> Result<Val, String> {
        Ok(match op {
            "+" => Val::Num(l.to_num() + r.to_num()),
            "-" => Val::Num(l.to_num() - r.to_num()),
            "*" => Val::Num(l.to_num() * r.to_num()),
            "/" => {
                let d = r.to_num();
                if d == 0.0 {
                    return Err("division by zero".into());
                }
                Val::Num(l.to_num() / d)
            }
            "%" => Val::Num(l.to_num() % r.to_num()),
            "^" => Val::Num(l.to_num().powf(r.to_num())),
            "&&" => Val::Num(if l.truthy() && r.truthy() { 1.0 } else { 0.0 }),
            "||" => Val::Num(if l.truthy() || r.truthy() { 1.0 } else { 0.0 }),
            "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                // Numeric comparison if both look numeric.
                let both_num = matches!(l, Val::Num(_)) && matches!(r, Val::Num(_));
                let ord: std::cmp::Ordering = if both_num {
                    l.to_num().partial_cmp(&r.to_num()).unwrap_or(std::cmp::Ordering::Equal)
                } else {
                    l.to_string_().cmp(&r.to_string_())
                };
                let ok = match op {
                    "==" => ord == std::cmp::Ordering::Equal,
                    "!=" => ord != std::cmp::Ordering::Equal,
                    "<" => ord == std::cmp::Ordering::Less,
                    ">" => ord == std::cmp::Ordering::Greater,
                    "<=" => ord != std::cmp::Ordering::Greater,
                    ">=" => ord != std::cmp::Ordering::Less,
                    _ => false,
                };
                Val::Num(if ok { 1.0 } else { 0.0 })
            }
            "~~" | "!~~" => {
                // Dynamic regex from any expression; pattern is in r.
                let pat = r.to_string_();
                let s = l.to_string_();
                let m = Regex::new(&pat).map(|rx| rx.is_match(&s)).unwrap_or(false);
                let neg = op == "!~~";
                Val::Num(if m != neg { 1.0 } else { 0.0 })
            }
            _ => return Err(format!("unknown operator: {op}")),
        })
    }

    fn call_builtin(&mut self, name: &str, args: &[Expr]) -> Result<Val, String> {
        let mut vals: Vec<Val> = Vec::new();
        for a in args {
            vals.push(self.eval(a)?);
        }
        match name {
            "length" => {
                let s = if vals.is_empty() {
                    self.get_field(0)
                } else {
                    vals[0].to_string_()
                };
                Ok(Val::Num(s.chars().count() as f64))
            }
            "substr" => {
                let s = vals.first().map(|v| v.to_string_()).unwrap_or_default();
                let chars: Vec<char> = s.chars().collect();
                let start = vals.get(1).map(|v| v.to_num() as i64).unwrap_or(1);
                let max_len = chars.len() as i64;
                let len = vals.get(2).map(|v| v.to_num() as i64).unwrap_or(max_len - start + 1);
                let lo = (start - 1).max(0).min(max_len) as usize;
                let hi = ((start - 1 + len).max(0).min(max_len)) as usize;
                Ok(Val::Str(chars[lo..hi].iter().collect()))
            }
            "index" => {
                let s = vals.first().map(|v| v.to_string_()).unwrap_or_default();
                let t = vals.get(1).map(|v| v.to_string_()).unwrap_or_default();
                Ok(Val::Num(match s.find(&t) {
                    Some(i) => (s[..i].chars().count() + 1) as f64,
                    None => 0.0,
                }))
            }
            "split" => {
                let s = vals.first().map(|v| v.to_string_()).unwrap_or_default();
                let arr_name = if let Some(Expr::Var(n)) = args.get(1) {
                    n.clone()
                } else {
                    return Err("split: second arg must be an array name".into());
                };
                let sep = vals.get(2).map(|v| v.to_string_()).unwrap_or_else(|| self.fs.clone());
                let parts: Vec<String> = if sep == " " {
                    s.split_whitespace().map(String::from).collect()
                } else if sep.chars().count() == 1 {
                    s.split(sep.chars().next().unwrap()).map(String::from).collect()
                } else {
                    Regex::new(&sep)
                        .map(|rx| rx.split(&s).map(String::from).collect::<Vec<_>>())
                        .unwrap_or_else(|_| vec![s.clone()])
                };
                let map = self.arrays.entry(arr_name).or_default();
                map.clear();
                for (i, p) in parts.iter().enumerate() {
                    map.insert((i + 1).to_string(), Val::Str(p.clone()));
                }
                Ok(Val::Num(parts.len() as f64))
            }
            "sprintf" => {
                let fmt = vals.first().map(|v| v.to_string_()).unwrap_or_default();
                Ok(Val::Str(awk_sprintf(&fmt, &vals[1..])))
            }
            "sub" | "gsub" => {
                let pat = vals.first().map(|v| v.to_string_()).unwrap_or_default();
                let repl = vals.get(1).map(|v| v.to_string_()).unwrap_or_default();
                // Default target: $0
                let target_idx = args.get(2);
                let original = if let Some(t) = target_idx {
                    self.eval(t)?.to_string_()
                } else {
                    self.get_field(0)
                };
                let rx = match Regex::new(&pat) {
                    Ok(r) => r,
                    Err(e) => return Err(format!("bad regex: {e}")),
                };
                let global = name == "gsub";
                let mut count = 0u64;
                let new = if global {
                    rx.replace_all(&original, |c: &regex::Captures| {
                        count += 1;
                        sub_repl(&repl, c)
                    })
                    .into_owned()
                } else if let Some(c) = rx.captures(&original) {
                    count = 1;
                    let m = c.get(0).unwrap();
                    let mut s = String::new();
                    s.push_str(&original[..m.start()]);
                    s.push_str(&sub_repl(&repl, &c));
                    s.push_str(&original[m.end()..]);
                    s
                } else {
                    original.clone()
                };
                if let Some(t) = target_idx {
                    match t {
                        Expr::Var(n) => {
                            self.vars.insert(n.clone(), Val::Str(new));
                        }
                        Expr::Field(idx) => {
                            let i = self.eval(idx)?.to_num() as i64;
                            self.set_field(i.max(0) as usize, new);
                        }
                        Expr::Index(n, idx) => {
                            let key = self.compose_key(idx)?;
                            let map = self.arrays.entry(n.clone()).or_default();
                            map.insert(key, Val::Str(new));
                        }
                        _ => {}
                    }
                } else {
                    self.set_field(0, new);
                }
                Ok(Val::Num(count as f64))
            }
            "match" => {
                let s = vals.first().map(|v| v.to_string_()).unwrap_or_default();
                let pat = vals.get(1).map(|v| v.to_string_()).unwrap_or_default();
                let rx = Regex::new(&pat).map_err(|e| format!("bad regex: {e}"))?;
                if let Some(m) = rx.find(&s) {
                    let rstart = s[..m.start()].chars().count() + 1;
                    let rlength = s[m.start()..m.end()].chars().count();
                    self.vars.insert("RSTART".to_string(), Val::Num(rstart as f64));
                    self.vars.insert("RLENGTH".to_string(), Val::Num(rlength as f64));
                    Ok(Val::Num(rstart as f64))
                } else {
                    self.vars.insert("RSTART".to_string(), Val::Num(0.0));
                    self.vars.insert("RLENGTH".to_string(), Val::Num(-1.0));
                    Ok(Val::Num(0.0))
                }
            }
            "toupper" => Ok(Val::Str(
                vals.first().map(|v| v.to_string_()).unwrap_or_default().to_uppercase(),
            )),
            "tolower" => Ok(Val::Str(
                vals.first().map(|v| v.to_string_()).unwrap_or_default().to_lowercase(),
            )),
            "int" => Ok(Val::Num(vals.first().map(|v| v.to_num()).unwrap_or(0.0).trunc())),
            "sqrt" => Ok(Val::Num(vals.first().map(|v| v.to_num()).unwrap_or(0.0).sqrt())),
            "log" => Ok(Val::Num(vals.first().map(|v| v.to_num()).unwrap_or(0.0).ln())),
            "exp" => Ok(Val::Num(vals.first().map(|v| v.to_num()).unwrap_or(0.0).exp())),
            "sin" => Ok(Val::Num(vals.first().map(|v| v.to_num()).unwrap_or(0.0).sin())),
            "cos" => Ok(Val::Num(vals.first().map(|v| v.to_num()).unwrap_or(0.0).cos())),
            "atan2" => Ok(Val::Num(
                vals.first().map(|v| v.to_num()).unwrap_or(0.0).atan2(
                    vals.get(1).map(|v| v.to_num()).unwrap_or(0.0),
                ),
            )),
            "rand" => {
                self.rng_state = self.rng_state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                let v = (self.rng_state >> 33) as f64 / (1u64 << 31) as f64;
                Ok(Val::Num(v))
            }
            "srand" => {
                let s = vals.first().map(|v| v.to_num() as u64).unwrap_or(0);
                let prev = self.rng_state;
                self.rng_state = if s == 0 { 0xdeadbeef } else { s };
                Ok(Val::Num(prev as f64))
            }
            "system" => {
                let cmd = vals.first().map(|v| v.to_string_()).unwrap_or_default();
                let status = if cfg!(windows) {
                    std::process::Command::new("cmd").args(["/C", &cmd]).status()
                } else {
                    std::process::Command::new("sh").args(["-c", &cmd]).status()
                };
                Ok(Val::Num(status.ok().and_then(|s| s.code()).unwrap_or(1) as f64))
            }
            other => Err(format!("unknown function: {other}")),
        }
    }

    fn run_stmt(&mut self, s: &Stmt) -> Result<Flow, String> {
        match s {
            Stmt::Expr(e) => {
                self.eval(e)?;
                Ok(Flow::Normal)
            }
            Stmt::Print(args) => {
                let ofs = self.vars.get("OFS").map(|v| v.to_string_()).unwrap_or_else(|| " ".to_string());
                let ors = self.vars.get("ORS").map(|v| v.to_string_()).unwrap_or_else(|| "\n".to_string());
                let stdout = std::io::stdout();
                let mut out = stdout.lock();
                if args.is_empty() {
                    let _ = out.write_all(self.get_field(0).as_bytes());
                } else {
                    let strs: Vec<String> = args
                        .iter()
                        .map(|a| self.eval(a).map(|v| v.to_string_()).unwrap_or_default())
                        .collect();
                    let _ = out.write_all(strs.join(&ofs).as_bytes());
                }
                let _ = out.write_all(ors.as_bytes());
                Ok(Flow::Normal)
            }
            Stmt::Printf(args) => {
                let fmt = self.eval(&args[0])?.to_string_();
                let mut rest: Vec<Val> = Vec::new();
                for a in &args[1..] {
                    rest.push(self.eval(a)?);
                }
                let stdout = std::io::stdout();
                let mut out = stdout.lock();
                let _ = out.write_all(awk_sprintf(&fmt, &rest).as_bytes());
                Ok(Flow::Normal)
            }
            Stmt::If(c, t, e) => {
                if self.eval(c)?.truthy() {
                    self.run_block(t)
                } else if let Some(els) = e {
                    self.run_block(els)
                } else {
                    Ok(Flow::Normal)
                }
            }
            Stmt::While(c, body) => {
                while self.eval(c)?.truthy() {
                    match self.run_block(body)? {
                        Flow::Break => break,
                        Flow::Continue | Flow::Normal => {}
                        f => return Ok(f),
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::DoWhile(body, c) => {
                loop {
                    match self.run_block(body)? {
                        Flow::Break => break,
                        Flow::Continue | Flow::Normal => {}
                        f => return Ok(f),
                    }
                    if !self.eval(c)?.truthy() {
                        break;
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::For(init, cond, post, body) => {
                if let Some(s) = init {
                    self.run_stmt(s)?;
                }
                loop {
                    if let Some(c) = cond {
                        if !self.eval(c)?.truthy() {
                            break;
                        }
                    }
                    match self.run_block(body)? {
                        Flow::Break => break,
                        Flow::Continue | Flow::Normal => {}
                        f => return Ok(f),
                    }
                    if let Some(p) = post {
                        self.run_stmt(p)?;
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::ForIn(k, arr, body) => {
                let keys: Vec<String> = self
                    .arrays
                    .get(arr)
                    .map(|m| m.keys().cloned().collect())
                    .unwrap_or_default();
                for key in keys {
                    self.vars.insert(k.clone(), Val::Str(key));
                    match self.run_block(body)? {
                        Flow::Break => break,
                        Flow::Continue | Flow::Normal => {}
                        f => return Ok(f),
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::Next => Ok(Flow::Next),
            Stmt::Exit(e) => {
                let code = match e {
                    Some(e) => self.eval(e)?.to_num() as i32,
                    None => 0,
                };
                Ok(Flow::Exit(code))
            }
            Stmt::Break => Ok(Flow::Break),
            Stmt::Continue => Ok(Flow::Continue),
            Stmt::Delete(name, idx) => {
                if idx.is_empty() {
                    self.arrays.remove(name);
                } else {
                    let key = self.compose_key(idx)?;
                    if let Some(m) = self.arrays.get_mut(name) {
                        m.remove(&key);
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::Block(stmts) => self.run_block(stmts),
        }
    }

    fn run_block(&mut self, stmts: &[Stmt]) -> Result<Flow, String> {
        for s in stmts {
            match self.run_stmt(s)? {
                Flow::Normal => {}
                f => return Ok(f),
            }
        }
        Ok(Flow::Normal)
    }
}

// awk-style printf — wires to Rust's format with conversions we support.
fn awk_sprintf(fmt: &str, vals: &[Val]) -> String {
    let bytes = fmt.as_bytes();
    let mut out = String::with_capacity(fmt.len());
    let mut arg_idx = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            // Parse flags/width/precision/conv.
            let start = i;
            i += 1;
            while i < bytes.len() && b"-+ #0".contains(&bytes[i]) {
                i += 1;
            }
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'.' {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if i >= bytes.len() {
                out.push_str(&fmt[start..]);
                return out;
            }
            let conv = bytes[i] as char;
            let spec = &fmt[start..=i];
            i += 1;
            if conv == '%' {
                out.push('%');
                continue;
            }
            let val = vals.get(arg_idx).cloned().unwrap_or(Val::Str(String::new()));
            arg_idx += 1;
            // Hand off to a small printf-like.
            out.push_str(&awk_one(spec, conv, &val));
            continue;
        }
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            let nx = bytes[i + 1];
            let mapped: Option<char> = match nx {
                b'n' => Some('\n'),
                b't' => Some('\t'),
                b'r' => Some('\r'),
                b'\\' => Some('\\'),
                b'/' => Some('/'),
                _ => None,
            };
            if let Some(c) = mapped {
                out.push(c);
                i += 2;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn awk_one(spec: &str, conv: char, val: &Val) -> String {
    // Reuse parse machinery: extract width/precision via simple scan.
    let inner = &spec[1..spec.len() - 1];
    let mut chars = inner.chars().peekable();
    let mut left = false;
    let mut zero = false;
    let mut plus = false;
    let mut space = false;
    while let Some(&c) = chars.peek() {
        match c {
            '-' => {
                left = true;
                chars.next();
            }
            '0' => {
                zero = true;
                chars.next();
            }
            '+' => {
                plus = true;
                chars.next();
            }
            ' ' => {
                space = true;
                chars.next();
            }
            '#' => {
                chars.next();
            }
            _ => break,
        }
    }
    let mut width = 0usize;
    while let Some(&c) = chars.peek() {
        if let Some(d) = c.to_digit(10) {
            width = width * 10 + d as usize;
            chars.next();
        } else {
            break;
        }
    }
    let mut precision: Option<usize> = None;
    if chars.peek() == Some(&'.') {
        chars.next();
        let mut p = 0usize;
        while let Some(&c) = chars.peek() {
            if let Some(d) = c.to_digit(10) {
                p = p * 10 + d as usize;
                chars.next();
            } else {
                break;
            }
        }
        precision = Some(p);
    }
    let body = match conv {
        'd' | 'i' => format!("{}", val.to_num() as i64),
        'o' => format!("{:o}", val.to_num() as i64 as u64),
        'x' => format!("{:x}", val.to_num() as i64 as u64),
        'X' => format!("{:X}", val.to_num() as i64 as u64),
        'f' => match precision {
            Some(p) => format!("{:.*}", p, val.to_num()),
            None => format!("{:.6}", val.to_num()),
        },
        'e' => match precision {
            Some(p) => format!("{:.*e}", p, val.to_num()),
            None => format!("{:.6e}", val.to_num()),
        },
        'E' => match precision {
            Some(p) => format!("{:.*E}", p, val.to_num()),
            None => format!("{:.6E}", val.to_num()),
        },
        'g' | 'G' => {
            let s = match precision {
                Some(p) if p > 0 => format!("{:.*}", p, val.to_num()),
                _ => format!("{}", val.to_num()),
            };
            if conv == 'G' { s.to_uppercase() } else { s }
        }
        's' => match precision {
            Some(p) => val.to_string_().chars().take(p).collect(),
            None => val.to_string_(),
        },
        'c' => val.to_string_().chars().next().map(|c| c.to_string()).unwrap_or_default(),
        _ => val.to_string_(),
    };
    let mut signed = body;
    if matches!(conv, 'd' | 'i' | 'e' | 'E' | 'f' | 'g' | 'G') {
        if plus && !signed.starts_with('-') && !signed.starts_with('+') {
            signed.insert(0, '+');
        } else if space && !signed.starts_with('-') && !signed.starts_with('+') {
            signed.insert(0, ' ');
        }
    }
    if width > signed.len() {
        let pad = width - signed.len();
        if left {
            return format!("{signed}{:width$}", "", width = pad);
        }
        if zero && precision.is_none() && matches!(conv, 'd' | 'i' | 'o' | 'x' | 'X' | 'f' | 'g' | 'e') {
            let (sign, rest) = if signed.starts_with('-') || signed.starts_with('+') {
                (&signed[..1], &signed[1..])
            } else {
                ("", signed.as_str())
            };
            return format!("{sign}{:0>width$}", rest, width = width - sign.len());
        }
        return format!("{:width$}{signed}", "", width = pad);
    }
    signed
}

fn sub_repl(repl: &str, caps: &regex::Captures) -> String {
    let bytes = repl.as_bytes();
    let mut out = String::with_capacity(repl.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            let nx = bytes[i + 1];
            if nx == b'&' {
                out.push('&');
                i += 2;
                continue;
            }
            if nx == b'\\' {
                out.push('\\');
                i += 2;
                continue;
            }
            out.push(nx as char);
            i += 2;
            continue;
        }
        if bytes[i] == b'&' {
            out.push_str(caps.get(0).map(|m| m.as_str()).unwrap_or(""));
            i += 1;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

// ────────────────────────────────────────────────────────────────────
// Driver
// ────────────────────────────────────────────────────────────────────

fn pattern_active(pat: &mut PatternKind, interp: &mut Interp) -> bool {
    match pat {
        PatternKind::Always => true,
        PatternKind::Begin | PatternKind::End => false,
        PatternKind::Regex(r) => Regex::new(r)
            .map(|rx| rx.is_match(&interp.get_field(0)))
            .unwrap_or(false),
        PatternKind::Expr(e) => interp.eval(e).map(|v| v.truthy()).unwrap_or(false),
        PatternKind::Range(_, _) => unreachable!("handled by run_rules"),
    }
}

fn run_rules(rules: &mut [Rule], interp: &mut Interp) -> Result<Flow, String> {
    for rule in rules.iter_mut() {
        let active = match &rule.pattern {
            PatternKind::Begin | PatternKind::End => false,
            PatternKind::Range(_, _) => {
                // Special-case: borrow the boxes once.
                let (a, b) = if let PatternKind::Range(a, b) = &rule.pattern {
                    (a.clone(), b.clone())
                } else {
                    unreachable!()
                };
                let was = rule.range_active;
                let start_match = pattern_active(&mut a.as_ref().clone(), interp);
                if !was && start_match {
                    rule.range_active = true;
                }
                let result = rule.range_active;
                if rule.range_active {
                    let end_match = pattern_active(&mut b.as_ref().clone(), interp);
                    if end_match {
                        rule.range_active = false;
                    }
                }
                result
            }
            _ => pattern_active(&mut rule.pattern.clone(), interp),
        };
        if active {
            match interp.run_block(&rule.action)? {
                Flow::Next => return Ok(Flow::Next),
                Flow::Exit(c) => return Ok(Flow::Exit(c)),
                _ => {}
            }
        }
    }
    Ok(Flow::Normal)
}

fn run_phase(rules: &mut [Rule], interp: &mut Interp, phase: &str) -> Result<Flow, String> {
    for rule in rules.iter_mut() {
        let go = match (&rule.pattern, phase) {
            (PatternKind::Begin, "BEGIN") => true,
            (PatternKind::End, "END") => true,
            _ => false,
        };
        if go {
            match interp.run_block(&rule.action)? {
                Flow::Exit(c) => return Ok(Flow::Exit(c)),
                _ => {}
            }
        }
    }
    Ok(Flow::Normal)
}

fn parse_var_assign(s: &str) -> Option<(String, String)> {
    let bytes = s.as_bytes();
    if bytes.is_empty() || !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_') {
        return None;
    }
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'=' {
            let name = std::str::from_utf8(&bytes[..i]).ok()?.to_string();
            let val = std::str::from_utf8(&bytes[i + 1..]).ok()?.to_string();
            return Some((name, val));
        }
        if !(b.is_ascii_alphanumeric() || b == b'_') {
            return None;
        }
    }
    None
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut program_text: Option<String> = None;
    let mut field_sep: Option<String> = None;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        if a == "-F" && i + 1 < args.len() {
            field_sep = Some(args[i + 1].clone());
            i += 2;
            continue;
        }
        if let Some(rest) = a.strip_prefix("-F") {
            field_sep = Some(rest.to_string());
            i += 1;
            continue;
        }
        if a == "-f" && i + 1 < args.len() {
            match std::fs::read_to_string(&args[i + 1]) {
                Ok(s) => {
                    program_text = Some(s);
                    i += 2;
                    continue;
                }
                Err(e) => {
                    err_path("awk", &args[i + 1], &e);
                    return 1;
                }
            }
        }
        if a == "-v" && i + 1 < args.len() {
            // Variable assignment, applied below.
            i += 2;
            continue;
        }
        if a.starts_with('-') && a != "-" && a.len() > 1 {
            err("awk", &format!("invalid option: {a}"));
            return 2;
        }
        break;
    }

    if program_text.is_none() {
        if i >= args.len() {
            err("awk", "missing program");
            return 2;
        }
        program_text = Some(args[i].clone());
        i += 1;
    }

    let program = program_text.unwrap();
    let mut lexer = Lexer::new(&program);
    let toks = match lexer.run() {
        Ok(t) => t,
        Err(e) => {
            err("awk", &format!("syntax error: {e}"));
            return 2;
        }
    };
    let mut parser = Parser { toks, i: 0 };
    let mut rules = match parser.parse_program() {
        Ok(r) => r,
        Err(e) => {
            err("awk", &format!("syntax error: {e}"));
            return 2;
        }
    };

    let mut interp = Interp::new();
    if let Some(fs) = field_sep {
        interp.vars.insert("FS".to_string(), Val::Str(fs));
    }
    // -v assignments
    let mut j = 1;
    while j < argv.len() {
        if argv[j] == "-v" && j + 1 < argv.len() {
            if let Some((k, v)) = parse_var_assign(&argv[j + 1]) {
                interp.vars.insert(k, Val::Str(v));
            }
            j += 2;
            continue;
        }
        j += 1;
    }

    // Files: anything left in args[i..]. Var-assignments mixed with files
    // (the awk way) get applied as we see them.
    let files: Vec<String> = args[i..].to_vec();

    if let Flow::Exit(c) = run_phase(&mut rules, &mut interp, "BEGIN").unwrap_or(Flow::Normal) {
        let _ = run_phase(&mut rules, &mut interp, "END");
        return c;
    }

    // If the program has only BEGIN/END rules, don't read stdin — that
    // would hang on an interactive terminal. POSIX awk does the same.
    let has_main_rule = rules.iter().any(|r| !matches!(r.pattern, PatternKind::Begin | PatternKind::End));

    let mut sources: Vec<(String, Box<dyn BufRead>)> = Vec::new();
    if !has_main_rule {
        // skip reading
    } else if files.is_empty() {
        sources.push(("-".to_string(), Box::new(BufReader::new(io::stdin().lock()))));
    } else {
        for f in &files {
            if let Some((k, v)) = parse_var_assign(f) {
                interp.vars.insert(k, Val::Str(v));
                continue;
            }
            if f == "-" {
                sources.push((f.clone(), Box::new(BufReader::new(io::stdin().lock()))));
                continue;
            }
            match File::open(f) {
                Ok(fh) => sources.push((f.clone(), Box::new(BufReader::new(fh)))),
                Err(e) => {
                    err_path("awk", f, &e);
                    return 1;
                }
            }
        }
    }

    let mut exit_code: Option<i32> = None;
    'outer: for (name, mut src) in sources {
        interp.vars.insert("FILENAME".to_string(), Val::Str(name.clone()));
        interp.filename = name;
        let mut buf = String::new();
        if src.read_to_string(&mut buf).is_err() {
            continue;
        }
        // `split` on a string ending in `\n` produces a trailing empty
        // record we don't want to process — strip the final newline first.
        let trimmed = buf.strip_suffix('\n').unwrap_or(&buf);
        if trimmed.is_empty() && buf.is_empty() {
            continue;
        }
        for raw in trimmed.split('\n') {
            // Strip trailing CR so Windows-encoded text doesn't pollute $0.
            let raw: &str = raw.strip_suffix('\r').unwrap_or(raw);
            interp.nr += 1;
            interp.vars.insert("NR".to_string(), Val::Num(interp.nr as f64));
            interp.set_record(raw);
            match run_rules(&mut rules, &mut interp) {
                Ok(Flow::Exit(c)) => {
                    exit_code = Some(c);
                    break 'outer;
                }
                Ok(_) => {}
                Err(e) => {
                    err("awk", &e);
                    exit_code = Some(2);
                    break 'outer;
                }
            }
        }
    }

    if let Flow::Exit(c) = run_phase(&mut rules, &mut interp, "END").unwrap_or(Flow::Normal) {
        return c;
    }
    exit_code.unwrap_or(0)
}
