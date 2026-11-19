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
    /// signed 32-bit integer
    Long,
    /// 16.16 signed fixed point (32 bits): -32768 .. 32767.99998
    Fixed16,
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
            "long" => Ty::Long,
            "fixed16" => Ty::Fixed16,
            _ => return None,
        })
    }
    pub fn size(self) -> u32 {
        match self {
            Ty::Bool | Ty::Byte | Ty::SByte => 1,
            Ty::Word | Ty::SWord | Ty::Fixed => 2,
            Ty::Long | Ty::Fixed16 => 4,
        }
    }
    pub fn signed(self) -> bool {
        matches!(self, Ty::SByte | Ty::SWord | Ty::Fixed | Ty::Long | Ty::Fixed16)
    }
    /// 32-bit: computed in DE:HL with the runtime's 32-bit routines.
    pub fn is_wide(self) -> bool {
        self.size() == 4
    }
    pub fn int(size: u32, signed: bool) -> Ty {
        match (size, signed) {
            (1, false) => Ty::Byte,
            (1, true) => Ty::SByte,
            (2, false) => Ty::Word,
            (2, true) => Ty::SWord,
            _ => Ty::Long,
        }
    }
    /// Range of values representable (accepting both signed and unsigned
    /// interpretation of the bit pattern for literals).
    pub fn literal_fits(self, v: i64) -> bool {
        match self.size() {
            1 => (-128..=255).contains(&v),
            2 => (-32768..=65535).contains(&v),
            _ => (-0x8000_0000..=0xFFFF_FFFF).contains(&v),
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
            Ty::Long => "long",
            Ty::Fixed16 => "fixed16",
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
    /// `{a, b, c}`: array initializer (only in a declaration);
    /// also `[]` (size taken from the initializer) as an array size
    List(Vec<Expr>),
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    /// `far byte name[n]`: an array in its own memory pages (a VarDecl inside)
    Far(Box<Stmt>),
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
    /// `image`: `asset name = image("file.png")` (a Layer 2 picture).
    Asset { name: String, path: String, image: bool },
}

#[derive(Debug, Clone)]
pub struct Func {
    pub name: String,
    pub params: Vec<(String, TypeRef)>,
    pub ret: Option<Ty>,
    /// `-> string`
    pub ret_str: bool,
    /// `-> StructName`
    pub ret_named: Option<String>,
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
    /// `class B extends A`
    pub parent: Option<String>,
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
