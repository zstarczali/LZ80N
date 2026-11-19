//! Abstract syntax tree.

use crate::diag::Pos;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    Bool,
    Byte,
    SByte,
    Word,
    SWord,
    /// 8.8 signed fixed point (16 bits): -128 .. 127.996, step 1/256
    Fixed,
}

impl Ty {
    pub fn from_name(s: &str) -> Option<Ty> {
        Some(match s {
            "bool" => Ty::Bool,
            "byte" => Ty::Byte,
            "sbyte" => Ty::SByte,
            "word" => Ty::Word,
            "sword" => Ty::SWord,
            "fixed" => Ty::Fixed,
            _ => return None,
        })
    }
    pub fn size(self) -> u32 {
        match self {
            Ty::Bool | Ty::Byte | Ty::SByte => 1,
            Ty::Word | Ty::SWord | Ty::Fixed => 2,
        }
    }
    pub fn signed(self) -> bool {
        matches!(self, Ty::SByte | Ty::SWord | Ty::Fixed)
    }
    pub fn int(size: u32, signed: bool) -> Ty {
        match (size, signed) {
            (1, false) => Ty::Byte,
            (1, true) => Ty::SByte,
            (_, false) => Ty::Word,
            (_, true) => Ty::SWord,
        }
    }
    /// Range of values representable (accepting both signed and unsigned
    /// interpretation of the bit pattern for literals).
    pub fn literal_fits(self, v: i64) -> bool {
        match self.size() {
            1 => (-128..=255).contains(&v),
            _ => (-32768..=65535).contains(&v),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Ty::Bool => "bool",
            Ty::Byte => "byte",
            Ty::SByte => "sbyte",
            Ty::Word => "word",
            Ty::SWord => "sword",
            Ty::Fixed => "fixed",
        }
    }
}

/// A type as written in the source: primitive or a struct/class name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRef {
    Prim(Ty),
    Named(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod,
    /// string concatenation `..`
    Concat,
    And, Or, Xor, Shl, Shr,
    Eq, Ne, Lt, Le, Gt, Ge,
    LAnd, LOr,
}

impl BinOp {
    pub fn is_compare(self) -> bool {
        matches!(self, BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge)
    }
    pub fn is_logic(self) -> bool {
        matches!(self, BinOp::LAnd | BinOp::LOr)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    BitNot,
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Int(i64),
    /// fixed-point literal, raw 8.8 value
    Fixed(i64),
    Bool(bool),
    Str(String),
    Name(String),
    Index(Box<Expr>, Box<Expr>),
    Field(Box<Expr>, String),
    Call(String, Vec<Expr>),
    MethodCall(Box<Expr>, String, Vec<Expr>),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    /// `byte x = 1`, `word buf[10]`, `var x = 5`, `const k = 3`
    VarDecl {
        ty: Option<TypeRef>,
        name: String,
        array: Option<Expr>,
        init: Option<Expr>,
        is_const: bool,
    },
    Assign { target: Expr, op: Option<BinOp>, value: Expr },
    If { arms: Vec<(Expr, Vec<Stmt>)>, otherwise: Option<Vec<Stmt>> },
    While { cond: Expr, body: Vec<Stmt> },
    For { var: String, start: Expr, end: Expr, step: Option<Expr>, body: Vec<Stmt> },
    Return(Option<Expr>),
    Break,
    Expr(Expr),
    Asm(String),
    /// `asset name = "file"` (top level only)
    Asset { name: String, path: String },
}

#[derive(Debug, Clone)]
pub struct Func {
    pub name: String,
    pub params: Vec<(String, TypeRef)>,
    pub ret: Option<Ty>,
    /// `-> string`
    pub ret_str: bool,
    pub body: Vec<Stmt>,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub name: String,
    pub ty: TypeRef,
    pub array: Option<Expr>,
    pub init: Option<Expr>,
    pub pos: Pos,
}

/// `struct` or `class` declaration (a class is a struct with methods).
#[derive(Debug, Clone)]
pub struct StructDecl {
    pub name: String,
    pub fields: Vec<FieldDecl>,
    pub methods: Vec<Func>,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub enum Item {
    Func(Func),
    /// `interrupt raster(line) ... end` (Some(line)) or `interrupt frame ... end` (None)
    Interrupt(Option<Expr>, Vec<Stmt>, Pos),
    Struct(StructDecl),
    Stmt(Stmt),
}

#[derive(Debug, Clone, Default)]
pub struct Program {
    pub items: Vec<Item>,
}
