//! Recursive-descent parser producing the AST.

use crate::ast::*;
use crate::diag::{Diag, Pos};
use crate::lexer::{Tok, Token};

pub fn parse(toks: Vec<Token>) -> Result<Program, Diag> {
    let mut p = Parser { t: toks, i: 0 };
    let mut prog = Program::default();
    while !p.at(&Tok::Eof) {
        if p.at(&Tok::Function) {
            prog.items.push(Item::Func(p.func()?));
        } else if p.at(&Tok::Interrupt) {
            let pos = p.pos();
            p.next();
            let line = match p.ident()?.as_str() {
                "raster" => {
                    p.expect(&Tok::LParen, "`(`")?;
                    let e = p.expr()?;
                    p.expect(&Tok::RParen, "`)`")?;
                    Some(e)
                }
                "frame" => None,
                _ => return Err(Diag::new(pos, "expected `interrupt raster(line)` or `interrupt frame`")),
            };
            let body = p.block(&[Tok::End])?;
            p.expect(&Tok::End, "`end`")?;
            prog.items.push(Item::Interrupt(line, body, pos));
        } else if p.at(&Tok::Struct) || p.at(&Tok::Class) {
            prog.items.push(Item::Struct(p.struct_decl()?));
        } else {
            prog.items.push(Item::Stmt(p.stmt()?));
        }
    }
    Ok(prog)
}

struct Parser {
    t: Vec<Token>,
    i: usize,
}

fn tok_name(t: &Tok) -> String {
    match t {
        Tok::Ident(s) => format!("identifier `{s}`"),
        Tok::Int(v) => format!("number `{v}`"),
        Tok::Str(_) => "string".into(),
        Tok::Eof => "end of file".into(),
        other => format!("`{}`", format!("{other:?}").to_lowercase()),
    }
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.t[self.i].tok
    }
    fn peek_at(&self, o: usize) -> &Tok {
        &self.t[(self.i + o).min(self.t.len() - 1)].tok
    }
    fn pos(&self) -> Pos {
        self.t[self.i].pos
    }
    fn at(&self, t: &Tok) -> bool {
        self.peek() == t
    }
    fn next(&mut self) -> Tok {
        let t = self.t[self.i].tok.clone();
        if self.i < self.t.len() - 1 {
            self.i += 1;
        }
        t
    }
    fn eat(&mut self, t: &Tok) -> bool {
        if self.at(t) {
            self.next();
            true
        } else {
            false
        }
    }
    fn expect(&mut self, t: &Tok, what: &str) -> Result<(), Diag> {
        if self.eat(t) {
            Ok(())
        } else {
            Err(self.err(&format!("expected {what}, found {}", tok_name(self.peek()))))
        }
    }
    fn err(&self, msg: &str) -> Diag {
        Diag::new(self.pos(), msg)
    }
    fn ident(&mut self) -> Result<String, Diag> {
        match self.peek().clone() {
            Tok::Ident(s) => {
                self.next();
                Ok(s)
            }
            t => Err(self.err(&format!("expected identifier, found {}", tok_name(&t)))),
        }
    }
    /// Primitive type (return types).
    fn ty(&mut self) -> Result<Ty, Diag> {
        let pos = self.pos();
        let n = self.ident()?;
        Ty::from_name(&n).ok_or_else(|| Diag::new(pos, format!("expected a primitive type, found `{n}`")))
    }
    /// Primitive or named (struct/class) type.
    fn type_ref(&mut self) -> Result<TypeRef, Diag> {
        let n = self.ident()?;
        Ok(match Ty::from_name(&n) {
            Some(t) => TypeRef::Prim(t),
            None => TypeRef::Named(n),
        })
    }
    /// `[size]` -- any constant expression, checked by the code generator.
    /// `{ expr, expr, ... }` (a trailing comma is allowed)
    fn list(&mut self) -> Result<Expr, Diag> {
        let pos = self.pos();
        self.expect(&Tok::LBrace, "`{`")?;
        let mut items = Vec::new();
        while !self.at(&Tok::RBrace) {
            // nested lists for multi-dimensional arrays: {{1, 2}, {3, 4}}
            items.push(if self.at(&Tok::LBrace) { self.list()? } else { self.expr()? });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(&Tok::RBrace, "`}`")?;
        if items.is_empty() {
            return Err(Diag::new(pos, "an initializer list needs at least one value"));
        }
        Ok(Expr { kind: ExprKind::List(items), pos })
    }

    fn array_suffix(&mut self) -> Result<Option<Expr>, Diag> {
        if !self.eat(&Tok::LBracket) {
            return Ok(None);
        }
        let e = if self.at(&Tok::RBracket) {
            // `name[] = {...}`: the size comes from the initializer
            Expr { kind: ExprKind::List(vec![]), pos: self.pos() }
        } else {
            self.expr()?
        };
        self.expect(&Tok::RBracket, "`]`")?;
        if !self.at(&Tok::LBracket) {
            return Ok(Some(e));
        }
        // more dimensions: `[a][b]...` is kept as a list of sizes
        let pos = e.pos;
        let mut dims = vec![e];
        while self.eat(&Tok::LBracket) {
            if self.at(&Tok::RBracket) {
                return Err(Diag::new(self.pos(), "only the first size can be `[]`"));
            }
            dims.push(self.expr()?);
            self.expect(&Tok::RBracket, "`]`")?;
        }
        Ok(Some(Expr { kind: ExprKind::List(dims), pos }))
    }

    fn struct_decl(&mut self) -> Result<StructDecl, Diag> {
        let pos = self.pos();
        self.next(); // struct / class
        let name = self.ident()?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        loop {
            match self.peek() {
                Tok::End => {
                    self.next();
                    break;
                }
                Tok::Function => methods.push(self.func()?),
                Tok::Ident(_) => {
                    let fpos = self.pos();
                    let ty = self.type_ref()?;
                    let fname = self.ident()?;
                    let array = self.array_suffix()?;
                    let init = if self.eat(&Tok::Assign) { Some(self.expr()?) } else { None };
                    fields.push(FieldDecl { name: fname, ty, array, init, pos: fpos });
                }
                t => {
                    return Err(self.err(&format!("expected a field, `function` or `end`, found {}", tok_name(t))))
                }
            }
        }
        Ok(StructDecl { name, fields, methods, pos })
    }

    fn func(&mut self) -> Result<Func, Diag> {
        let pos = self.pos();
        self.expect(&Tok::Function, "`function`")?;
        let name = self.ident()?;
        self.expect(&Tok::LParen, "`(`")?;
        let mut params = Vec::new();
        if !self.at(&Tok::RParen) {
            loop {
                let ty = self.type_ref()?;
                let pname = self.ident()?;
                params.push((pname, ty));
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(&Tok::RParen, "`)`")?;
        let mut ret_str = false;
        let ret = if self.eat(&Tok::Arrow) {
            if matches!(self.peek(), Tok::Ident(n) if n == "string") {
                self.next();
                ret_str = true;
                None
            } else {
                Some(self.ty()?)
            }
        } else {
            None
        };
        let body = self.block(&[Tok::End])?;
        self.expect(&Tok::End, "`end`")?;
        Ok(Func { name, params, ret, ret_str, body, pos })
    }

    /// Parse statements until one of `terms` (not consumed).
    fn block(&mut self, terms: &[Tok]) -> Result<Vec<Stmt>, Diag> {
        let mut v = Vec::new();
        while !terms.contains(self.peek()) {
            if self.at(&Tok::Eof) {
                return Err(self.err("unexpected end of file (missing `end`?)"));
            }
            if self.at(&Tok::Function) || self.at(&Tok::Interrupt) {
                return Err(self.err("functions and interrupts can only be declared at top level"));
            }
            v.push(self.stmt()?);
        }
        Ok(v)
    }

    fn stmt(&mut self) -> Result<Stmt, Diag> {
        let pos = self.pos();
        let kind = match self.peek().clone() {
            Tok::If => self.if_stmt()?,
            Tok::While => {
                self.next();
                let cond = self.expr()?;
                self.expect(&Tok::Do, "`do`")?;
                let body = self.block(&[Tok::End])?;
                self.next();
                StmtKind::While { cond, body }
            }
            Tok::For => {
                self.next();
                let var = self.ident()?;
                self.expect(&Tok::Assign, "`=`")?;
                let start = self.expr()?;
                self.expect(&Tok::Comma, "`,`")?;
                let end = self.expr()?;
                let step = if self.eat(&Tok::Comma) { Some(self.expr()?) } else { None };
                self.expect(&Tok::Do, "`do`")?;
                let body = self.block(&[Tok::End])?;
                self.next();
                StmtKind::For { var, start, end, step, body }
            }
            Tok::Return => {
                self.next();
                // a return value follows unless the block ends here
                let ends = matches!(
                    self.peek(),
                    Tok::End | Tok::Else | Tok::Elseif | Tok::Eof
                );
                StmtKind::Return(if ends { None } else { Some(self.expr()?) })
            }
            Tok::Break => {
                self.next();
                StmtKind::Break
            }
            Tok::Asm => {
                self.next();
                let body = match self.next() {
                    Tok::AsmBody(b) => b,
                    _ => return Err(Diag::new(pos, "malformed asm block")),
                };
                self.expect(&Tok::End, "`end`")?;
                StmtKind::Asm(body)
            }
            Tok::Asset => {
                self.next();
                let name = self.ident()?;
                self.expect(&Tok::Assign, "`=`")?;
                match self.next() {
                    Tok::Str(path) => StmtKind::Asset { name, path, image: false },
                    Tok::Ident(f) if f == "image" => {
                        self.expect(&Tok::LParen, "`(`")?;
                        let Tok::Str(path) = self.next() else {
                            return Err(Diag::new(pos, "expected a file name: `image(\"picture.png\")`"));
                        };
                        self.expect(&Tok::RParen, "`)`")?;
                        StmtKind::Asset { name, path, image: true }
                    }
                    _ => {
                        return Err(Diag::new(
                            pos,
                            "expected a file name after `asset name =` (or `image(\"file.png\")`)",
                        ))
                    }
                }
            }
            Tok::Const | Tok::Var => {
                let is_const = self.next() == Tok::Const;
                let name = self.ident()?;
                self.expect(&Tok::Assign, "`=`")?;
                let init = Some(self.expr()?);
                StmtKind::VarDecl { ty: None, name, array: None, init, is_const }
            }
            // `Type name ...` -- two identifiers in a row is always a declaration
            Tok::Ident(_) if matches!(self.peek_at(1), Tok::Ident(_)) => {
                let ty = self.type_ref()?;
                let name = self.ident()?;
                let array = self.array_suffix()?;
                let init = if self.eat(&Tok::Assign) {
                    if self.at(&Tok::LBrace) { Some(self.list()?) } else { Some(self.expr()?) }
                } else {
                    None
                };
                match (&array, &init) {
                    (Some(_), Some(Expr { kind: ExprKind::List(_), .. })) => {}
                    (Some(_), Some(_)) => {
                        return Err(Diag::new(pos, "an array is initialised with a list: `= {1, 2, 3}`"));
                    }
                    (None, Some(Expr { kind: ExprKind::List(_), pos })) => {
                        return Err(Diag::new(*pos, "a `{...}` list needs an array: `name[] = {...}`"));
                    }
                    (Some(Expr { kind: ExprKind::List(v), pos }), None)
                        if v.is_empty() || matches!(&v[0].kind, ExprKind::List(w) if w.is_empty()) =>
                    {
                        return Err(Diag::new(*pos, "`[]` needs an initializer list to take its size from"));
                    }
                    _ => {}
                }
                StmtKind::VarDecl { ty: Some(ty), name, array, init, is_const: false }
            }
            _ => {
                let e = self.expr()?;
                let op = match self.peek() {
                    Tok::Assign => Some(None),
                    Tok::PlusEq => Some(Some(BinOp::Add)),
                    Tok::MinusEq => Some(Some(BinOp::Sub)),
                    Tok::StarEq => Some(Some(BinOp::Mul)),
                    Tok::AmpEq => Some(Some(BinOp::And)),
                    Tok::PipeEq => Some(Some(BinOp::Or)),
                    Tok::SlashEq => Some(Some(BinOp::Div)),
                    Tok::PercentEq => Some(Some(BinOp::Mod)),
                    Tok::ShlEq => Some(Some(BinOp::Shl)),
                    Tok::ShrEq => Some(Some(BinOp::Shr)),
                    _ => None,
                };
                match op {
                    Some(op) => {
                        self.next();
                        if !matches!(e.kind, ExprKind::Name(_) | ExprKind::Index(..) | ExprKind::Field(..)) {
                            return Err(Diag::new(pos, "invalid assignment target"));
                        }
                        let value = self.expr()?;
                        StmtKind::Assign { target: e, op, value }
                    }
                    None => {
                        if !matches!(e.kind, ExprKind::Call(..) | ExprKind::MethodCall(..)) {
                            return Err(Diag::new(pos, "expression statement must be a function call"));
                        }
                        StmtKind::Expr(e)
                    }
                }
            }
        };
        Ok(Stmt { kind, pos })
    }

    fn if_stmt(&mut self) -> Result<StmtKind, Diag> {
        self.next(); // if
        let mut arms = Vec::new();
        let mut otherwise = None;
        loop {
            let cond = self.expr()?;
            self.expect(&Tok::Then, "`then`")?;
            let body = self.block(&[Tok::End, Tok::Else, Tok::Elseif])?;
            arms.push((cond, body));
            match self.next() {
                Tok::Elseif => continue,
                Tok::Else => {
                    otherwise = Some(self.block(&[Tok::End])?);
                    self.next();
                    break;
                }
                _ => break, // End
            }
        }
        Ok(StmtKind::If { arms, otherwise })
    }

    // ---- expressions (precedence climbing, lowest first) ----

    pub fn expr(&mut self) -> Result<Expr, Diag> {
        self.binary(0)
    }

    fn binop_info(t: &Tok) -> Option<(BinOp, u8)> {
        Some(match t {
            Tok::Or => (BinOp::LOr, 1),
            Tok::And => (BinOp::LAnd, 2),
            Tok::EqEq => (BinOp::Eq, 3),
            Tok::NotEq => (BinOp::Ne, 3),
            Tok::Lt => (BinOp::Lt, 3),
            Tok::Le => (BinOp::Le, 3),
            Tok::Gt => (BinOp::Gt, 3),
            Tok::Ge => (BinOp::Ge, 3),
            Tok::Pipe => (BinOp::Or, 4),
            Tok::Tilde => (BinOp::Xor, 5),
            Tok::Amp => (BinOp::And, 6),
            Tok::Shl => (BinOp::Shl, 7),
            Tok::Shr => (BinOp::Shr, 7),
            Tok::DotDot => (BinOp::Concat, 8),
            Tok::Plus => (BinOp::Add, 9),
            Tok::Minus => (BinOp::Sub, 9),
            Tok::Star => (BinOp::Mul, 10),
            Tok::Slash => (BinOp::Div, 10),
            Tok::Percent => (BinOp::Mod, 10),
            _ => return None,
        })
    }

    fn binary(&mut self, min: u8) -> Result<Expr, Diag> {
        let mut lhs = self.unary()?;
        while let Some((op, prec)) = Self::binop_info(self.peek()) {
            if prec <= min {
                break;
            }
            let pos = self.pos();
            self.next();
            let rhs = self.binary(prec)?;
            lhs = Expr { kind: ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)), pos };
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Expr, Diag> {
        let pos = self.pos();
        let op = match self.peek() {
            Tok::Minus => UnOp::Neg,
            Tok::Not => UnOp::Not,
            Tok::Tilde => UnOp::BitNot,
            _ => return self.postfix(),
        };
        self.next();
        let e = self.unary()?;
        // fold negative literals right away
        if let (UnOp::Neg, ExprKind::Int(v)) = (op, &e.kind) {
            return Ok(Expr { kind: ExprKind::Int(-v), pos });
        }
        if let (UnOp::Neg, ExprKind::Fixed(v)) = (op, &e.kind) {
            return Ok(Expr { kind: ExprKind::Fixed(-v), pos });
        }
        Ok(Expr { kind: ExprKind::Unary(op, Box::new(e)), pos })
    }

    fn postfix(&mut self) -> Result<Expr, Diag> {
        let mut e = self.primary()?;
        loop {
            let pos = self.pos();
            if self.eat(&Tok::LBracket) {
                let idx = self.expr()?;
                self.expect(&Tok::RBracket, "`]`")?;
                e = Expr { kind: ExprKind::Index(Box::new(e), Box::new(idx)), pos };
            } else if self.at(&Tok::LParen) {
                let name = match &e.kind {
                    ExprKind::Name(n) => n.clone(),
                    _ => return Err(self.err("only named functions can be called")),
                };
                self.next();
                let args = self.args()?;
                e = Expr { kind: ExprKind::Call(name, args), pos: e.pos };
            } else if self.eat(&Tok::Dot) {
                let name = self.ident()?;
                if self.eat(&Tok::LParen) {
                    let args = self.args()?;
                    e = Expr { kind: ExprKind::MethodCall(Box::new(e), name, args), pos };
                } else {
                    e = Expr { kind: ExprKind::Field(Box::new(e), name), pos };
                }
            } else {
                return Ok(e);
            }
        }
    }

    /// Arguments after `(`, consuming the closing `)`.
    fn args(&mut self) -> Result<Vec<Expr>, Diag> {
        let mut args = Vec::new();
        if !self.at(&Tok::RParen) {
            loop {
                args.push(self.expr()?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(&Tok::RParen, "`)`")?;
        Ok(args)
    }

    fn primary(&mut self) -> Result<Expr, Diag> {
        let pos = self.pos();
        let kind = match self.next() {
            Tok::Int(v) => ExprKind::Int(v),
            Tok::Fixed(v) => ExprKind::Fixed(v),
            Tok::True => ExprKind::Bool(true),
            Tok::False => ExprKind::Bool(false),
            Tok::Str(s) => ExprKind::Str(s),
            Tok::Ident(s) => ExprKind::Name(s),
            Tok::LParen => {
                let e = self.expr()?;
                self.expect(&Tok::RParen, "`)`")?;
                return Ok(e);
            }
            t => return Err(Diag::new(pos, format!("expected expression, found {}", tok_name(&t)))),
        };
        Ok(Expr { kind, pos })
    }
}
