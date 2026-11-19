//! Lexer: turns LZ80N source text into tokens.
//!
//! Newlines are not significant. `--` starts a line comment, `--[[ ... ]]`
//! a block comment. The body of an `asm ... end` block is captured verbatim
//! as a single `AsmBody` token.

use crate::diag::{Diag, Pos};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    Int(i64),
    /// `..` (string concatenation)
    DotDot,
    /// decimal literal `1.25`, raw 8.8 value
    Fixed(i64),
    Str(String),
    AsmBody(String),
    // keywords
    And, Or, Not, If, Then, Elseif, Else, End, While, Do, For,
    Function, Return, Break, Const, Var, True, False, Asm, Struct, Class, Asset, Interrupt, Far, Include,
    // punctuation / operators
    Plus, Minus, Star, Slash, Percent, Amp, Pipe, Tilde, Shl, Shr,
    EqEq, NotEq, Lt, Le, Gt, Ge, Assign,
    PlusEq, MinusEq, StarEq, AmpEq, PipeEq, SlashEq, PercentEq, ShlEq, ShrEq,
    LParen, RParen, LBracket, RBracket, LBrace, RBrace, Comma, Dot, Arrow,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub pos: Pos,
}

pub fn lex(src: &str) -> Result<Vec<Token>, Diag> {
    Lexer { s: src.as_bytes(), i: 0, line: 1, col: 1 }.run()
}

struct Lexer<'a> {
    s: &'a [u8],
    i: usize,
    line: u32,
    col: u32,
}

impl<'a> Lexer<'a> {
    fn peek(&self, o: usize) -> u8 {
        *self.s.get(self.i + o).unwrap_or(&0)
    }

    fn bump(&mut self) -> u8 {
        let c = self.peek(0);
        self.i += 1;
        if c == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        c
    }

    fn pos(&self) -> Pos {
        Pos { line: self.line, col: self.col, file: 0 }
    }

    fn run(mut self) -> Result<Vec<Token>, Diag> {
        let mut out = Vec::new();
        loop {
            self.skip_ws_and_comments()?;
            let pos = self.pos();
            if self.i >= self.s.len() {
                out.push(Token { tok: Tok::Eof, pos });
                return Ok(out);
            }
            let tok = self.next_tok(pos)?;
            let is_asm = tok == Tok::Asm;
            out.push(Token { tok, pos });
            if is_asm {
                let pos = self.pos();
                let body = self.asm_body(pos)?;
                out.push(Token { tok: Tok::AsmBody(body), pos });
                out.push(Token { tok: Tok::End, pos: self.pos() });
            }
        }
    }

    fn skip_ws_and_comments(&mut self) -> Result<(), Diag> {
        loop {
            let c = self.peek(0);
            if c == b' ' || c == b'\t' || c == b'\r' || c == b'\n' {
                self.bump();
            } else if c == b'-' && self.peek(1) == b'-' {
                let pos = self.pos();
                self.bump();
                self.bump();
                if self.peek(0) == b'[' && self.peek(1) == b'[' {
                    loop {
                        if self.i >= self.s.len() {
                            return Err(Diag::new(pos, "unterminated block comment"));
                        }
                        if self.peek(0) == b']' && self.peek(1) == b']' {
                            self.bump();
                            self.bump();
                            break;
                        }
                        self.bump();
                    }
                } else {
                    while self.i < self.s.len() && self.peek(0) != b'\n' {
                        self.bump();
                    }
                }
            } else {
                return Ok(());
            }
        }
    }

    /// Capture raw lines until a line consisting only of `end`.
    fn asm_body(&mut self, pos: Pos) -> Result<String, Diag> {
        // rest of the `asm` line is ignored if blank
        while self.i < self.s.len() && self.peek(0) != b'\n' {
            let c = self.bump();
            if !c.is_ascii_whitespace() {
                return Err(Diag::new(pos, "`asm` must be followed by a newline"));
            }
        }
        let mut body = String::new();
        loop {
            if self.i >= self.s.len() {
                return Err(Diag::new(pos, "unterminated `asm` block (missing `end`)"));
            }
            self.bump(); // the '\n'
            let start = self.i;
            while self.i < self.s.len() && self.peek(0) != b'\n' {
                self.bump();
            }
            let line = std::str::from_utf8(&self.s[start..self.i]).unwrap_or("");
            let code = line.split(';').next().unwrap_or("").trim();
            if code == "end" {
                return Ok(body);
            }
            body.push_str(line.trim_end());
            body.push('\n');
        }
    }

    fn next_tok(&mut self, pos: Pos) -> Result<Tok, Diag> {
        let c = self.peek(0);
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = self.i;
            while self.peek(0).is_ascii_alphanumeric() || self.peek(0) == b'_' {
                self.bump();
            }
            let w = std::str::from_utf8(&self.s[start..self.i]).unwrap();
            return Ok(match w {
                "and" => Tok::And,
                "or" => Tok::Or,
                "not" => Tok::Not,
                "if" => Tok::If,
                "then" => Tok::Then,
                "elseif" => Tok::Elseif,
                "else" => Tok::Else,
                "end" => Tok::End,
                "while" => Tok::While,
                "do" => Tok::Do,
                "for" => Tok::For,
                "function" => Tok::Function,
                "return" => Tok::Return,
                "break" => Tok::Break,
                "const" => Tok::Const,
                "var" => Tok::Var,
                "true" => Tok::True,
                "false" => Tok::False,
                "asm" => Tok::Asm,
                "struct" => Tok::Struct,
                "class" => Tok::Class,
                "asset" => Tok::Asset,
                "far" => Tok::Far,
                "include" => Tok::Include,
                "interrupt" => Tok::Interrupt,
                _ => Tok::Ident(w.to_string()),
            });
        }
        if c.is_ascii_digit() || (c == b'$' && self.peek(1).is_ascii_hexdigit()) {
            return self.number(pos);
        }
        if c == b'"' {
            return self.string(pos);
        }
        // character literal: 'a' (an integer constant), escapes \n \t \\ \'
        if c == b'\'' {
            self.bump();
            let mut ch = self.bump();
            if ch == b'\\' {
                ch = match self.bump() {
                    b'n' => b'\n',
                    b't' => b'\t',
                    b'0' => 0,
                    other => other,
                };
            }
            if self.peek(0) != b'\'' {
                return Err(Diag::new(pos, "character literal must be one character in single quotes"));
            }
            self.bump();
            return Ok(Tok::Int(ch as i64));
        }
        self.bump();
        let n = self.peek(0);
        let two = |l: &mut Self, t: Tok| {
            l.bump();
            t
        };
        Ok(match c {
            b'+' if n == b'=' => two(self, Tok::PlusEq),
            b'-' if n == b'=' => two(self, Tok::MinusEq),
            b'-' if n == b'>' => two(self, Tok::Arrow),
            b'*' if n == b'=' => two(self, Tok::StarEq),
            b'/' if n == b'=' => two(self, Tok::SlashEq),
            b'%' if n == b'=' => two(self, Tok::PercentEq),
            b'<' if n == b'<' && self.peek(1) == b'=' => {
                self.bump();
                two(self, Tok::ShlEq)
            }
            b'>' if n == b'>' && self.peek(1) == b'=' => {
                self.bump();
                two(self, Tok::ShrEq)
            }
            b'&' if n == b'=' => two(self, Tok::AmpEq),
            b'|' if n == b'=' => two(self, Tok::PipeEq),
            b'=' if n == b'=' => two(self, Tok::EqEq),
            b'~' if n == b'=' => two(self, Tok::NotEq),
            b'!' if n == b'=' => two(self, Tok::NotEq),
            b'<' if n == b'=' => two(self, Tok::Le),
            b'<' if n == b'<' => two(self, Tok::Shl),
            b'>' if n == b'=' => two(self, Tok::Ge),
            b'>' if n == b'>' => two(self, Tok::Shr),
            b'+' => Tok::Plus,
            b'-' => Tok::Minus,
            b'*' => Tok::Star,
            b'/' => Tok::Slash,
            b'%' => Tok::Percent,
            b'&' => Tok::Amp,
            b'|' => Tok::Pipe,
            b'~' => Tok::Tilde,
            b'<' => Tok::Lt,
            b'>' => Tok::Gt,
            b'=' => Tok::Assign,
            b'(' => Tok::LParen,
            b')' => Tok::RParen,
            b'[' => Tok::LBracket,
            b'{' => Tok::LBrace,
            b'}' => Tok::RBrace,
            b']' => Tok::RBracket,
            b',' => Tok::Comma,
            b'.' if n == b'.' => two(self, Tok::DotDot),
            b'.' => Tok::Dot,
            _ => return Err(Diag::new(pos, format!("unexpected character `{}`", c as char))),
        })
    }

    fn number(&mut self, pos: Pos) -> Result<Tok, Diag> {
        let (radix, skip) = if self.peek(0) == b'$' {
            (16, 1)
        } else if self.peek(0) == b'0' && (self.peek(1) | 0x20) == b'x' {
            (16, 2)
        } else if self.peek(0) == b'0' && (self.peek(1) | 0x20) == b'b' {
            (2, 2)
        } else {
            (10, 0)
        };
        for _ in 0..skip {
            self.bump();
        }
        let start = self.i;
        while self.peek(0).is_ascii_alphanumeric() || self.peek(0) == b'_' {
            self.bump();
        }
        let text: String = std::str::from_utf8(&self.s[start..self.i])
            .unwrap()
            .chars()
            .filter(|&c| c != '_')
            .collect();
        if radix == 10 && self.peek(0) == b'.' && self.peek(1).is_ascii_digit() {
            self.bump();
            let fs = self.i;
            while self.peek(0).is_ascii_digit() {
                self.bump();
            }
            let frac = std::str::from_utf8(&self.s[fs..self.i]).unwrap();
            let v: f64 = format!("{text}.{frac}")
                .parse()
                .map_err(|_| Diag::new(pos, format!("invalid number literal `{text}.{frac}`")))?;
            let raw = (v * 256.0).round() as i64;
            if raw > 32767 {
                return Err(Diag::new(pos, format!("`{text}.{frac}` does not fit in `fixed` (-128 .. 127.99)")));
            }
            return Ok(Tok::Fixed(raw));
        }
        match i64::from_str_radix(&text, radix) {
            Ok(v) if v <= 0xFFFF_FFFF => Ok(Tok::Int(v)),
            _ => Err(Diag::new(pos, format!("invalid number literal `{text}`"))),
        }
    }

    fn string(&mut self, pos: Pos) -> Result<Tok, Diag> {
        self.bump();
        let mut s = String::new();
        loop {
            match self.bump() {
                0 | b'\n' => return Err(Diag::new(pos, "unterminated string literal")),
                b'"' => return Ok(Tok::Str(s)),
                b'\\' => match self.bump() {
                    b'n' => s.push('\n'),
                    b't' => s.push('\t'),
                    b'\\' => s.push('\\'),
                    b'"' => s.push('"'),
                    b'0' => s.push('\0'),
                    c => {
                        return Err(Diag::new(pos, format!("unknown escape `\\{}`", c as char)))
                    }
                },
                c => s.push(c as char),
            }
        }
    }
}
