//! Expressions of the built-in assembler (the subset of sjasmplus syntax
//! the code generator and inline `asm` blocks use).
//!
//! Numbers: `123`, `$FF`, `#FF`, `0xFF`, `0FFh`, `%1010`, `0b1010`, `'c'`.
//! `$` alone is the address of the current line. Operators, loosest first:
//! `||` `&&` `|` `^` `&` `== != <>` `< <= > >=` `<< >>` `+ -` `* / %`,
//! unary `- + ~ !`, and `high x` / `low x` (also written `high(x)`).

/// Symbol lookup: `None` = not (yet) defined.
pub trait Scope {
    fn symbol(&self, name: &str) -> Option<i64>;
    /// Address of the current line (`$`).
    fn here(&self) -> i64;
    /// Full name of a local label (`.loop` -> `func.loop`).
    fn qualify(&self, name: &str) -> String;
}

/// Result of an evaluation. `Undefined` carries the first missing name; in
/// the early passes the caller treats it as 0 and tries again later.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Known(i64),
    Undefined(String),
}

pub fn eval(src: &str, scope: &dyn Scope) -> Result<Value, String> {
    let mut p = Parser { s: src.as_bytes(), i: 0, scope, missing: None };
    p.ws();
    if p.i >= p.s.len() {
        return Err("missing expression".into());
    }
    let v = p.expr(0)?;
    p.ws();
    if p.i < p.s.len() {
        return Err(format!("unexpected `{}` in expression `{}`", &src[p.i..], src.trim()));
    }
    Ok(match p.missing {
        Some(m) => Value::Undefined(m),
        None => Value::Known(v),
    })
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    scope: &'a dyn Scope,
    missing: Option<String>,
}

pub fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b'.' || c == b'@'
}

pub fn is_ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'.' || c == b'?' || c == b'!' || c == b'@'
}

// binary operators: (text, precedence)
const BINOPS: &[(&str, u8)] = &[
    ("||", 1),
    ("&&", 2),
    ("<<", 8),
    (">>", 8),
    ("<=", 7),
    (">=", 7),
    ("==", 6),
    ("!=", 6),
    ("<>", 6),
    ("|", 3),
    ("^", 4),
    ("&", 5),
    ("<", 7),
    (">", 7),
    ("=", 6),
    ("+", 9),
    ("-", 9),
    ("*", 10),
    ("/", 10),
    ("%", 10),
];

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && (self.s[self.i] == b' ' || self.s[self.i] == b'\t') {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn expr(&mut self, min_prec: u8) -> Result<i64, String> {
        let mut lhs = self.unary()?;
        loop {
            self.ws();
            let rest = &self.s[self.i..];
            let Some(&(op, prec)) = BINOPS.iter().find(|(t, _)| rest.starts_with(t.as_bytes())) else {
                break;
            };
            if prec < min_prec {
                break;
            }
            self.i += op.len();
            let rhs = self.expr(prec + 1)?;
            lhs = match op {
                "||" => ((lhs != 0) || (rhs != 0)) as i64,
                "&&" => ((lhs != 0) && (rhs != 0)) as i64,
                "|" => lhs | rhs,
                "^" => lhs ^ rhs,
                "&" => lhs & rhs,
                "==" | "=" => (lhs == rhs) as i64,
                "!=" | "<>" => (lhs != rhs) as i64,
                "<" => (lhs < rhs) as i64,
                "<=" => (lhs <= rhs) as i64,
                ">" => (lhs > rhs) as i64,
                ">=" => (lhs >= rhs) as i64,
                "<<" => lhs.wrapping_shl(rhs as u32),
                ">>" => lhs.wrapping_shr(rhs as u32),
                "+" => lhs.wrapping_add(rhs),
                "-" => lhs.wrapping_sub(rhs),
                "*" => lhs.wrapping_mul(rhs),
                "/" | "%" => {
                    if rhs == 0 {
                        if self.missing.is_some() {
                            0 // operand not known yet
                        } else {
                            return Err("division by zero".into());
                        }
                    } else if op == "/" {
                        lhs / rhs
                    } else {
                        lhs % rhs
                    }
                }
                _ => unreachable!(),
            };
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<i64, String> {
        self.ws();
        match self.peek() {
            Some(b'-') => {
                self.i += 1;
                Ok(self.unary()?.wrapping_neg())
            }
            Some(b'+') => {
                self.i += 1;
                self.unary()
            }
            Some(b'~') => {
                self.i += 1;
                Ok(!self.unary()?)
            }
            Some(b'!') => {
                self.i += 1;
                Ok((self.unary()? == 0) as i64)
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> Result<i64, String> {
        self.ws();
        let Some(c) = self.peek() else {
            return Err("missing operand".into());
        };
        match c {
            b'(' => {
                self.i += 1;
                let v = self.expr(0)?;
                self.ws();
                if self.peek() != Some(b')') {
                    return Err("missing `)`".into());
                }
                self.i += 1;
                Ok(v)
            }
            b'\'' | b'"' => self.char_lit(c),
            b'$' => {
                self.i += 1;
                if self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                    self.radix(16)
                } else if self.peek() == Some(b'$') {
                    self.i += 1; // `$$`: current page (not used here)
                    Ok(0)
                } else {
                    Ok(self.scope.here())
                }
            }
            b'#' => {
                self.i += 1;
                self.radix(16)
            }
            b'%' => {
                self.i += 1;
                self.radix(2)
            }
            b'0'..=b'9' => self.number(),
            c if is_ident_start(c) => {
                let start = self.i;
                while self.peek().is_some_and(is_ident_char) {
                    self.i += 1;
                }
                let name = std::str::from_utf8(&self.s[start..self.i]).unwrap();
                let lname = name.to_ascii_lowercase();
                // `high x` / `low(x)`: unary operators (binding like unary minus)
                if lname == "high" || lname == "low" {
                    self.ws();
                    if self.peek().is_some_and(|c| c == b'(' || c == b'$' || c == b'#' || c == b'%' || c == b'\'' || c.is_ascii_digit() || is_ident_start(c)) {
                        let v = self.unary()?;
                        return Ok(if lname == "high" { (v >> 8) & 0xFF } else { v & 0xFF });
                    }
                }
                let full = self.scope.qualify(name);
                match self.scope.symbol(&full) {
                    Some(v) => Ok(v),
                    None => {
                        if self.missing.is_none() {
                            self.missing = Some(full);
                        }
                        Ok(0)
                    }
                }
            }
            _ => Err(format!("unexpected `{}`", c as char)),
        }
    }

    fn char_lit(&mut self, q: u8) -> Result<i64, String> {
        self.i += 1;
        let mut v: i64 = 0;
        let mut n = 0;
        while let Some(c) = self.peek() {
            self.i += 1;
            if c == q {
                if n == 0 {
                    return Err("empty character constant".into());
                }
                return Ok(v);
            }
            let c = if c == b'\\' && q == b'"' {
                let e = self.peek().ok_or("bad escape")?;
                self.i += 1;
                unescape(e)
            } else {
                c
            };
            v = (v << 8) | c as i64;
            n += 1;
        }
        Err("unterminated character constant".into())
    }

    fn radix(&mut self, r: u32) -> Result<i64, String> {
        let start = self.i;
        while self.peek().is_some_and(|c| (c as char).is_digit(r) || c == b'_') {
            self.i += 1;
        }
        if start == self.i {
            return Err("missing digits".into());
        }
        let t: String = std::str::from_utf8(&self.s[start..self.i]).unwrap().replace('_', "");
        i64::from_str_radix(&t, r).map_err(|e| e.to_string())
    }

    fn number(&mut self) -> Result<i64, String> {
        let rest = &self.s[self.i..];
        if rest.len() > 2 && rest[0] == b'0' && (rest[1] | 0x20) == b'x' {
            self.i += 2;
            return self.radix(16);
        }
        if rest.len() > 2 && rest[0] == b'0' && (rest[1] | 0x20) == b'b' && (rest[2] == b'0' || rest[2] == b'1') {
            self.i += 2;
            return self.radix(2);
        }
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_hexdigit() || c == b'_') {
            self.i += 1;
        }
        let t: String = std::str::from_utf8(&self.s[start..self.i]).unwrap().replace('_', "");
        if self.peek().is_some_and(|c| c | 0x20 == b'h') {
            self.i += 1;
            return i64::from_str_radix(&t, 16).map_err(|e| e.to_string());
        }
        // a trailing `b`/`d` would be part of the hex digits: binary / decimal suffix
        let lt = t.to_ascii_lowercase();
        if let Some(b) = lt.strip_suffix('b').filter(|b| !b.is_empty() && b.bytes().all(|c| c == b'0' || c == b'1')) {
            return i64::from_str_radix(b, 2).map_err(|e| e.to_string());
        }
        if let Some(d) = lt.strip_suffix('d').filter(|d| !d.is_empty() && d.bytes().all(|c| c.is_ascii_digit())) {
            return d.parse().map_err(|e: std::num::ParseIntError| e.to_string());
        }
        t.parse().map_err(|_| format!("bad number `{t}`"))
    }
}

pub fn unescape(e: u8) -> u8 {
    match e {
        b'n' => 10,
        b'r' => 13,
        b't' => 9,
        b'0' => 0,
        b'a' => 7,
        b'b' => 8,
        b'e' => 27,
        b'f' => 12,
        b'v' => 11,
        other => other, // \\ \" \'
    }
}
