//! Type checking + Z80N code generation (sjasmplus syntax).
//!
//! v0 model (see docs/spec-v0.md):
//! * 8-bit values are computed in `A`, 16-bit values in `HL`.
//! * Binary operators: right operand in `B` (8-bit) or `DE` (16-bit).
//! * Functions use static frames: parameters and locals live at fixed
//!   addresses, arguments are stored there by the caller. Recursion is
//!   rejected at compile time.
//! * No value is kept in a register across statements, so inline `asm`
//!   blocks may clobber every register except `SP` (and `I`/interrupt mode).

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use crate::ast::*;
use crate::diag::{Diag, Pos};
use crate::peephole;
use crate::runtime;

type R<T> = Result<T, Diag>;

/// Storage layout of a value: primitive or struct (index into `structs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lay {
    Prim(Ty),
    Struct(usize),
    /// `string`: length byte + up to STR_CAP characters, copied by value
    Str,
}

/// Maximum string length (characters).
const STR_CAP: usize = 64;
const STR_SIZE: u32 = STR_CAP as u32 + 1;

/// A storage object: a value of layout `lay`, or an array of `len` of them.
/// `sub` holds the inner dimensions of a multi-dimensional array (0 = none):
/// `byte m[4][5][6]` is `len 4, sub [5, 6, 0]`, and `m[i]` is `len 5, sub [6, 0, 0]`.
#[derive(Debug, Clone, Copy)]
struct Obj {
    lay: Lay,
    len: Option<u32>,
    sub: [u32; 3],
}

impl Obj {
    fn new(lay: Lay, len: Option<u32>) -> Obj {
        Obj { lay, len, sub: [0; 3] }
    }

    /// Number of elements of `lay` in one step of the outermost index.
    fn inner(&self) -> u32 {
        self.sub.iter().filter(|&&d| d > 0).product()
    }

    /// Total number of `lay` elements (all dimensions), None for a scalar.
    fn total(&self) -> Option<u32> {
        self.len.map(|n| n * self.inner())
    }
}

#[derive(Debug, Clone)]
enum Sym {
    /// `by_ref`: the label holds the *address* of the object (struct params, `self`).
    Var { obj: Obj, label: String, by_ref: bool },
    Const(i64),
    /// `fixed` constant (raw 8.8 value)
    FConst(i64),
    /// string constant
    SConst(String),
    /// Embedded data; `patterns` is set for sprite pattern assets.
    Asset { label: String, patterns: Option<u32>, image: Option<(u32, u32)> },
    /// `far` array: elements of `ty` from 8K page `page` on.
    Far { ty: Ty, len: u32, sub: [u32; 3], page: u32 },
}

#[derive(Debug, Clone)]
struct FieldDef {
    name: String,
    obj: Obj,
    offset: u32,
    init: Option<i64>,
}

#[derive(Debug, Clone)]
struct StructDef {
    name: String,
    fields: Vec<FieldDef>,
    size: u32,
    /// method name -> function key ("Type.method")
    methods: HashMap<String, String>,
}

/// Where an object lives after `place()`.
enum Place {
    /// A link-time constant address expression.
    Static(String),
    /// Address computed into HL.
    Dyn,
}

#[derive(Debug, Clone)]
struct FnSig {
    label: String,
    /// (name, layout, slot); struct parameters hold an address.
    params: Vec<(String, Lay, String)>,
    ret: Option<Ty>,
    /// `-> string`: label of the static return buffer
    ret_str: Option<String>,
}

/// Right-hand operand of a binary operation after `gen_operands`.
#[derive(Clone, Copy)]
enum Rhs {
    /// In `B` (8-bit) or `DE` (16-bit).
    Reg,
    /// Immediate, already masked to the operand width.
    Imm(i64),
}

#[derive(Default)]
pub struct Options {
    pub nex_name: String,
    pub optimize: bool,
    /// Directory that relative asset paths are resolved against.
    pub base_dir: std::path::PathBuf,
    /// Put every function into its own code page (tests of code banking).
    pub bank_all: bool,
}

pub struct Output {
    pub asm: String,
}

pub fn compile(prog: &Program, opts: &Options) -> Result<Output, Vec<Diag>> {
    let mut g = Gen::default();
    g.run(prog, opts);
    if g.errors.is_empty() {
        Ok(Output { asm: g.finish_banked(opts) })
    } else {
        Err(g.errors)
    }
}

#[derive(Default)]
struct Gen {
    /// note period table needed
    uses_notes: bool,
    /// length-prefixed string literals: text -> label
    pstrings: HashMap<String, String>,
    /// string temporaries: next index in the current statement, allocated labels
    str_tmp: u32,
    str_tmps: HashSet<String>,
    /// return buffer of the function being compiled (`-> string`)
    cur_ret_str: Option<String>,
    /// sin/cos/atan2 tables needed
    uses_trig: bool,
    /// Interrupt handlers: (raster line or None = frame, label, key)
    irqs: Vec<(Option<u16>, String, String)>,
    code: String,
    data: String,
    n: u32,
    globals: HashMap<String, Sym>,
    funcs: HashMap<String, FnSig>,
    scopes: Vec<HashMap<String, Sym>>,
    cur_fn: String,
    cur_ret: Option<Option<Ty>>, // None = main program
    loop_exits: Vec<String>,
    calls: HashMap<String, (Pos, HashSet<String>)>,
    errors: Vec<Diag>,
    strings: HashMap<String, String>,
    structs: Vec<StructDef>,
    struct_ids: HashMap<String, usize>,
    assets: Vec<(String, Vec<u8>)>,
    /// Layer 2 pictures: (label, image, first 8K page).
    images: Vec<(String, crate::assets::Image, u32)>,
    /// Contents of the banked 8K pages (pictures, far arrays): (page, bytes, note).
    far_pages: Vec<(u32, Vec<u8>, String)>,
    /// Compiled functions, kept apart so they can be banked: (key, label, code).
    fn_chunks: Vec<(String, String, String)>,
    base_dir: std::path::PathBuf,
}

const INTRINSICS: &[&str] = &[
    "print", "write", "vsync", "border", "sprite_load", "memory", "nextreg", "port",
    "plot", "point", "hline", "layer2_clear", "layer2_flip", "dma_copy", "dma_fill", "addr", "key", "kempston", "frames", "cls", "at", "ink",
    "cpu_speed", "random", "seed", "pause", "any_key", "wait_key",
    "abs", "min", "max", "clamp", "sqrt", "sin", "cos", "atan2", "round",
    "len", "sub", "find", "val", "asc", "chr", "str", "upper", "lower",
    "ay_write", "note_period", "draw_image", "image_width", "image_height",
];

/// Built-in constants. Keys are encoded as `row * 8 + bit` of the
/// Spectrum keyboard matrix (row = address line A8..A15 of port $xxFE).
fn builtin_consts() -> Vec<(String, i64)> {
    let rows = [
        ["CAPS", "Z", "X", "C", "V"],
        ["A", "S", "D", "F", "G"],
        ["Q", "W", "E", "R", "T"],
        ["1", "2", "3", "4", "5"],
        ["0", "9", "8", "7", "6"],
        ["P", "O", "I", "U", "Y"],
        ["ENTER", "L", "K", "J", "H"],
        ["SPACE", "SYM", "M", "N", "B"],
    ];
    let mut v = Vec::new();
    for (r, keys) in rows.iter().enumerate() {
        for (b, k) in keys.iter().enumerate() {
            v.push((format!("KEY_{k}"), (r * 8 + b) as i64));
        }
    }
    for (n, val) in [("JOY_RIGHT", 1), ("JOY_LEFT", 2), ("JOY_DOWN", 4), ("JOY_UP", 8), ("JOY_FIRE", 16)] {
        v.push((n.to_string(), val));
    }
    let colours = ["BLACK", "BLUE", "RED", "MAGENTA", "GREEN", "CYAN", "YELLOW", "WHITE"];
    for (i, c) in colours.iter().enumerate() {
        v.push((c.to_string(), i as i64));
        v.push((format!("BRIGHT_{c}"), i as i64 + 8));
    }
    for (n, val) in [("MHZ_3", 0), ("MHZ_7", 1), ("MHZ_14", 2), ("MHZ_28", 3), ("NOT_FOUND", 255)] {
        v.push((n.to_string(), val));
    }
    v
}

fn mask(v: i64, ty: Ty) -> i64 {
    if ty.size() == 1 { v & 0xFF } else { v & 0xFFFF }
}

fn literal_ty(v: i64) -> Ty {
    if (0..=255).contains(&v) {
        Ty::Byte
    } else if (-128..0).contains(&v) {
        Ty::SByte
    } else if (0..=65535).contains(&v) {
        Ty::Word
    } else {
        Ty::SWord
    }
}

fn join(a: Option<Ty>, b: Option<Ty>) -> Option<Ty> {
    match (a, b) {
        (None, x) | (x, None) => x,
        (Some(Ty::Bool), Some(Ty::Bool)) => Some(Ty::Bool),
        (Some(Ty::Fixed), _) | (_, Some(Ty::Fixed)) => Some(Ty::Fixed),
        (Some(a), Some(b)) => Some(Ty::int(a.size().max(b.size()), a.signed() || b.signed())),
    }
}

impl Gen {
    // ---------------------------------------------------------------- emit
    fn e(&mut self, s: &str) {
        self.code.push_str("    ");
        self.code.push_str(s);
        self.code.push('\n');
    }
    fn label(&mut self, l: &str) {
        self.code.push_str(l);
        self.code.push_str(":\n");
    }
    fn new_label(&mut self) -> String {
        self.n += 1;
        format!("__L{}", self.n)
    }
    fn comment(&mut self, pos: Pos, what: &str) {
        let _ = writeln!(self.code, "    ; line {}: {}", pos.line, what);
    }

    // ------------------------------------------------------------- symbols
    fn lookup(&self, name: &str) -> Option<Sym> {
        for s in self.scopes.iter().rev() {
            if let Some(v) = s.get(name) {
                return Some(v.clone());
            }
        }
        self.globals.get(name).cloned()
    }

    fn check_new_name(&self, name: &str, pos: Pos) -> R<()> {
        if Ty::from_name(name).is_some()
            || INTRINSICS.contains(&name)
            || self.funcs.contains_key(name)
            || self.struct_ids.contains_key(name)
            || name == "self"
        {
            return Err(Diag::new(pos, format!("`{name}` is a reserved name")));
        }
        if let Some(s) = self.scopes.last() {
            if s.contains_key(name) {
                return Err(Diag::new(pos, format!("`{name}` is already declared in this scope")));
            }
        } else if self.globals.contains_key(name) {
            return Err(Diag::new(pos, format!("global `{name}` is already declared")));
        }
        Ok(())
    }

    /// Evaluate an array size (a positive compile-time constant).
    fn array_len(&self, e: &Option<Expr>) -> R<Option<u32>> {
        let Some(e) = e else { return Ok(None) };
        match self.const_eval(e) {
            Some(n) if (1..=0xFFFF).contains(&n) => Ok(Some(n as u32)),
            Some(n) => Err(Diag::new(e.pos, format!("array size {n} must be 1..65535"))),
            None => Err(Diag::new(e.pos, "array size must be a compile-time constant")),
        }
    }

    /// Dimensions of a declaration: (outer length, inner sizes). `[]` as the
    /// first size takes it from the initializer list.
    fn decl_dims(&self, array: &Option<Expr>, init: &Option<Expr>) -> R<Option<(u32, [u32; 3])>> {
        let Some(a) = array else { return Ok(None) };
        let sizes: Vec<&Expr> = match &a.kind {
            ExprKind::List(v) if v.len() >= 2 => v.iter().collect(),
            _ => vec![a],
        };
        if sizes.len() > 4 {
            return Err(Diag::new(a.pos, "arrays have at most 4 dimensions"));
        }
        let mut sub = [0u32; 3];
        for (k, d) in sizes[1..].iter().enumerate() {
            if matches!(&d.kind, ExprKind::List(v) if v.is_empty()) {
                return Err(Diag::new(d.pos, "only the first size can be `[]`"));
            }
            sub[k] = self.array_len(&Some((*d).clone()))?.unwrap();
        }
        let inner: u32 = sub.iter().filter(|&&d| d > 0).product();
        let first = sizes[0];
        let n = if matches!(&first.kind, ExprKind::List(v) if v.is_empty()) {
            match init {
                Some(Expr { kind: ExprKind::List(items), .. }) => {
                    if sizes.len() > 1 && items.iter().any(|e| matches!(e.kind, ExprKind::List(_))) {
                        items.len() as u32
                    } else {
                        (items.len() as u32).div_ceil(inner)
                    }
                }
                _ => return Err(Diag::new(first.pos, "`[]` needs an initializer list to take its size from")),
            }
        } else {
            self.array_len(&Some(first.clone()))?.unwrap()
        };
        if n as u64 * inner as u64 > 0xFFFF {
            return Err(Diag::new(a.pos, "array too large"));
        }
        Ok(Some((n, sub)))
    }

    /// An initializer list flattened to the element order of `dims`
    /// (nested lists are rows; short rows are padded).
    fn flat_init(&self, items: &[Expr], dims: &[u32], lay: Lay) -> R<Vec<Expr>> {
        let nested = items.iter().any(|e| matches!(e.kind, ExprKind::List(_)));
        if !nested {
            return Ok(items.to_vec());
        }
        if dims.len() < 2 {
            return Err(Diag::new(items[0].pos, "too many levels of `{...}` for this array"));
        }
        let row: u32 = dims[1..].iter().product();
        let mut out = Vec::new();
        for it in items {
            let ExprKind::List(v) = &it.kind else {
                return Err(Diag::new(it.pos, "write every row as a `{...}` list"));
            };
            let mut r = self.flat_init(v, &dims[1..], lay)?;
            if r.len() as u32 > row {
                return Err(Diag::new(it.pos, format!("{} values for a row of {row}", r.len())));
            }
            let zero = match lay {
                Lay::Prim(Ty::Bool) => ExprKind::Bool(false),
                Lay::Str => ExprKind::Str(String::new()),
                _ => ExprKind::Int(0),
            };
            while (r.len() as u32) < row {
                r.push(Expr { kind: zero.clone(), pos: it.pos });
            }
            out.extend(r);
        }
        Ok(out)
    }

    fn mk_obj(lay: Lay, dims: Option<(u32, [u32; 3])>) -> Obj {
        match dims {
            None => Obj::new(lay, None),
            Some((n, sub)) => Obj { lay, len: Some(n), sub },
        }
    }

    /// The initializer of a declaration as data lines.
    fn decl_data(&self, lay: Lay, dims: (u32, [u32; 3]), items: &[Expr], pos: Pos) -> R<String> {
        let mut all = vec![dims.0];
        all.extend(dims.1.iter().filter(|&&d| d > 0));
        let flat = self.flat_init(items, &all, lay)?;
        let total = all.iter().product();
        self.array_data(lay, total, &flat, pos)
    }

    /// `db` / `dw` lines for an array initializer (missing values are 0 / "").
    /// Initializer values of a numeric/bool array, masked to the type, padded with 0.
    fn prim_values(&self, t: Ty, len: u32, items: &[Expr], pos: Pos) -> R<Vec<i64>> {
        if items.len() as u32 > len {
            return Err(Diag::new(pos, format!("{} values for an array of {len}", items.len())));
        }
        let mut vals = Vec::new();
        for e in items {
            let v = match t {
                Ty::Fixed => self.fixed_const(e).filter(|v| (-32768..=32767).contains(v)),
                Ty::Bool => match &e.kind {
                    ExprKind::Bool(b) => Some(*b as i64),
                    _ => None,
                },
                _ => self.const_eval(e).filter(|&v| t.literal_fits(v)),
            }
            .ok_or_else(|| Diag::new(e.pos, format!("initializer values must be `{}` constants that fit", t.name())))?;
            vals.push(mask(v, t));
        }
        vals.resize(len as usize, 0);
        Ok(vals)
    }

    fn array_data(&self, lay: Lay, len: u32, items: &[Expr], pos: Pos) -> R<String> {
        if items.len() as u32 > len {
            return Err(Diag::new(pos, format!("{} values for an array of {len}", items.len())));
        }
        let mut out = String::new();
        let mut emit = |kw: &str, vals: &[i64]| {
            for c in vals.chunks(16) {
                let v: Vec<String> = c.iter().map(|x| x.to_string()).collect();
                let _ = writeln!(out, "    {kw} {}", v.join(","));
            }
        };
        match lay {
            Lay::Prim(t) => {
                let mut vals = Vec::new();
                for e in items {
                    let v = match t {
                        Ty::Fixed => self.fixed_const(e).filter(|v| (-32768..=32767).contains(v)),
                        Ty::Bool => match &e.kind {
                            ExprKind::Bool(b) => Some(*b as i64),
                            _ => None,
                        },
                        _ => self.const_eval(e).filter(|&v| t.literal_fits(v)),
                    }
                    .ok_or_else(|| {
                        Diag::new(e.pos, format!("initializer values must be `{}` constants that fit", t.name()))
                    })?;
                    vals.push(mask(v, t));
                }
                vals.resize(len as usize, 0);
                emit(if t.size() == 1 { "db" } else { "dw" }, &vals);
            }
            Lay::Str => {
                for i in 0..len as usize {
                    let text = match items.get(i).map(|e| (&e.kind, e.pos)) {
                        None => String::new(),
                        Some((ExprKind::Str(t), _)) => t.clone(),
                        Some((ExprKind::Name(n), p)) => match self.lookup(n) {
                            Some(Sym::SConst(t)) => t,
                            _ => return Err(Diag::new(p, "string array values must be string constants")),
                        },
                        Some((_, p)) => return Err(Diag::new(p, "string array values must be string constants")),
                    };
                    // a string variable holds the first STR_CAP characters
                    let b: Vec<u8> = text.bytes().take(STR_CAP as usize).collect();
                    let mut vals = vec![b.len() as i64];
                    vals.extend(b.iter().map(|&c| c as i64));
                    vals.resize(STR_SIZE as usize, 0);
                    emit("db", &vals);
                }
            }
            Lay::Struct(_) => return Err(Diag::new(pos, "arrays of structs cannot have an initializer (yet)")),
        }
        Ok(out)
    }

    fn lay_size(&self, lay: Lay) -> u32 {
        match lay {
            Lay::Prim(t) => t.size(),
            Lay::Struct(id) => self.structs[id].size,
            Lay::Str => STR_SIZE,
        }
    }

    fn lay_name(&self, lay: Lay) -> String {
        match lay {
            Lay::Prim(t) => t.name().to_string(),
            Lay::Struct(id) => self.structs[id].name.clone(),
            Lay::Str => "string".to_string(),
        }
    }

    fn resolve_type(&self, t: &TypeRef, pos: Pos) -> R<Lay> {
        match t {
            TypeRef::Prim(t) => Ok(Lay::Prim(*t)),
            TypeRef::Named(n) if n == "string" => Ok(Lay::Str),
            TypeRef::Named(n) => self
                .struct_ids
                .get(n)
                .map(|&id| Lay::Struct(id))
                .ok_or_else(|| Diag::new(pos, format!("unknown type `{n}`"))),
        }
    }

    fn alloc_bytes(&mut self, label: &str, bytes: u32) {
        let _ = writeln!(self.data, "{label}: ds {}", bytes.max(1));
    }

    fn safe(name: &str) -> String {
        name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect()
    }

    fn declare_local_obj(&mut self, name: &str, obj: Obj) -> String {
        self.n += 1;
        let label = format!("l_{}_{}_{}", Self::safe(&self.cur_fn), Self::safe(name), self.n);
        let bytes = self.lay_size(obj.lay) * obj.total().unwrap_or(1);
        self.alloc_bytes(&label, bytes);
        self.scopes
            .last_mut()
            .expect("scope")
            .insert(name.to_string(), Sym::Var { obj, label: label.clone(), by_ref: false });
        label
    }

    fn declare_local(&mut self, name: &str, ty: Ty, len: Option<u32>) -> String {
        self.declare_local_obj(name, Obj::new(Lay::Prim(ty), len))
    }

    // ----------------------------------------------------------- top level
    fn run(&mut self, prog: &Program, opts: &Options) {
        self.base_dir = opts.base_dir.clone();
        for (n, v) in builtin_consts() {
            self.globals.insert(n, Sym::Const(v));
        }
        // pass 0a: top-level constants (usable in struct array sizes) and
        // assets (so `image_width(img)` can be part of a constant), in order
        for item in &prog.items {
            let Item::Stmt(s) = item else { continue };
            let r = match &s.kind {
                StmtKind::VarDecl { is_const: true, .. } => self.declare_global(s),
                StmtKind::Asset { name, path, image } => self.declare_asset(name, path, *image, s.pos),
                _ => Ok(()),
            };
            if let Err(d) = r {
                self.errors.push(d);
            }
        }
        // pass 0b: struct / class layouts (in declaration order)
        for item in &prog.items {
            if let Item::Struct(sd) = item {
                if let Err(d) = self.declare_struct(sd) {
                    self.errors.push(d);
                }
            }
        }
        // pass 1: function and method signatures
        for item in &prog.items {
            let r = match item {
                Item::Func(f) => self.declare_func(f, None),
                Item::Struct(sd) => sd.methods.iter().try_for_each(|m| {
                    let id = self.struct_ids[&sd.name];
                    self.declare_func(m, Some(id))
                }),
                Item::Interrupt(line, _, pos) => self.declare_interrupt(line.as_ref(), *pos),
                _ => Ok(()),
            };
            if let Err(d) = r {
                self.errors.push(d);
            }
        }
        // pass 2: globals (direct top-level declarations), in order
        for item in &prog.items {
            if let Item::Stmt(s) = item {
                let r = match &s.kind {
                    StmtKind::VarDecl { is_const: false, .. } => self.declare_global(s),
                    StmtKind::Far(inner) => self.declare_far(inner),
                    _ => Ok(()),
                };
                if let Err(d) = r {
                    self.errors.push(d);
                }
            }
        }
        // main program
        self.cur_fn = "main".into();
        self.cur_ret = None;
        self.cur_ret_str = None;
        if !self.zero_pages().is_empty() {
            self.e("call rt_far_zero");
        }
        for item in &prog.items {
            if let Item::Stmt(s) = item {
                let r = match &s.kind {
                    StmtKind::VarDecl { name, init, is_const: false, .. } => self.init_global(name, init, s.pos),
                    StmtKind::VarDecl { .. } | StmtKind::Asset { .. } | StmtKind::Far(_) => Ok(()),
                    _ => self.stmt(s),
                };
                if let Err(d) = r {
                    self.errors.push(d);
                }
            }
        }
        self.e("jp __main_end");
        // functions and methods
        for item in &prog.items {
            let r = match item {
                // skip functions whose declaration failed (already reported) and duplicates
                Item::Func(f) if !self.funcs.contains_key(&f.name) || self.calls.contains_key(&f.name) => Ok(()),
                Item::Func(f) => self.func(f, f.name.clone()),
                Item::Struct(sd) => sd.methods.iter().try_for_each(|m| self.func(m, format!("{}.{}", sd.name, m.name))),
                Item::Interrupt(line, body, pos) => match self.irq_key(line.as_ref()) {
                    Some(key) if self.funcs.contains_key(&key) && !self.calls.contains_key(&key) => {
                        let f = Func { name: key.clone(), params: vec![], ret: None, ret_str: false, body: body.clone(), pos: *pos };
                        self.func(&f, key)
                    }
                    _ => Ok(()),
                },
                _ => Ok(()),
            };
            if let Err(d) = r {
                self.errors.push(d);
            }
        }
        self.check_recursion();
        self.check_interrupt_sharing();
    }

    fn declare_asset(&mut self, name: &str, path: &str, image: bool, pos: Pos) -> R<()> {
        self.check_new_name(name, pos)?;
        let full = self.base_dir.join(path);
        let label = format!("a_{name}");
        if image {
            let img = crate::assets::load_image(&full).map_err(|e| Diag::new(pos, e))?;
            // pictures live in 8K pages from 28 (16K bank 14) upwards, above
            // the two Layer 2 buffers (banks 8-13)
            let first = self.alloc_pages(img.pages.len() as u32, pos)?;
            for (k, p) in img.pages.iter().enumerate() {
                self.far_pages.push((first + k as u32, p.clone(), format!("picture {name}")));
            }
            let size = Some((img.width, img.height));
            self.images.push((label.clone(), img, first));
            self.globals.insert(name.to_string(), Sym::Asset { label, patterns: None, image: size });
            return Ok(());
        }
        let a = crate::assets::load(&full).map_err(|e| Diag::new(pos, e))?;
        self.assets.push((label.clone(), a.bytes));
        self.globals.insert(name.to_string(), Sym::Asset { label, patterns: a.patterns, image: None });
        Ok(())
    }

    /// `n` consecutive 8K pages from the pool above the Layer 2 buffers
    /// (page 28 = 16K bank 14 up to the 1 MB limit).
    fn alloc_pages(&mut self, n: u32, pos: Pos) -> R<u32> {
        let first = self.far_pages.iter().map(|(p, _, _)| p + 1).max().unwrap_or(28);
        if first + n > 224 {
            return Err(Diag::new(pos, "pictures and `far` arrays need more than the Next's 1 MB of memory"));
        }
        Ok(first)
    }

    fn declare_far(&mut self, s: &Stmt) -> R<()> {
        let StmtKind::VarDecl { ty: Some(tr), name, array, init, .. } = &s.kind else { unreachable!() };
        self.check_new_name(name, s.pos)?;
        let ty = match self.resolve_type(tr, s.pos)? {
            Lay::Prim(t) => t,
            _ => return Err(Diag::new(s.pos, "`far` arrays hold numbers or `bool` (not strings or structs yet)")),
        };
        let (len, sub) = self.decl_dims(array, init)?.unwrap();
        let total = Self::mk_obj(Lay::Prim(ty), Some((len, sub))).total().unwrap();
        let bytes_len = total as usize * ty.size() as usize;
        if bytes_len > 65536 {
            return Err(Diag::new(s.pos, format!("`far` arrays can be at most 64 KB ({bytes_len} bytes)")));
        }
        let mut bytes = vec![0u8; bytes_len];
        if let Some(Expr { kind: ExprKind::List(items), .. }) = init {
            let mut all = vec![len];
            all.extend(sub.iter().filter(|&&d| d > 0));
            let flat = self.flat_init(items, &all, Lay::Prim(ty))?;
            let vals = self.prim_values(ty, total, &flat, s.pos)?;
            for (k, v) in vals.iter().enumerate() {
                if ty.size() == 1 {
                    bytes[k] = *v as u8;
                } else {
                    bytes[2 * k] = *v as u8;
                    bytes[2 * k + 1] = (*v >> 8) as u8;
                }
            }
        } else if init.is_some() {
            return Err(Diag::new(s.pos, "a `far` array is initialised with a list: `= {1, 2, 3}`"));
        }
        let pages = bytes_len.div_ceil(8192).max(1) as u32;
        let first = self.alloc_pages(pages, s.pos)?;
        for k in 0..pages as usize {
            let chunk = bytes[k * 8192..((k + 1) * 8192).min(bytes_len)].to_vec();
            self.far_pages.push((first + k as u32, chunk, format!("far {name}")));
        }
        self.globals.insert(name.clone(), Sym::Far { ty, len, sub, page: first });
        Ok(())
    }

    /// The `far` array an element expression belongs to: (element type, first page, dims).
    fn far_root(&self, e: &Expr) -> Option<(Ty, u32, Vec<u32>)> {
        let mut b = e;
        while let ExprKind::Index(inner, _) = &b.kind {
            b = inner;
        }
        let ExprKind::Name(n) = &b.kind else { return None };
        match self.lookup(n) {
            Some(Sym::Far { ty, len, sub, page }) => {
                let mut dims = vec![len];
                dims.extend(sub.iter().filter(|&&d| d > 0));
                Some((ty, page, dims))
            }
            _ => None,
        }
    }

    /// HL = byte offset of a `far` element, then map it: HL = address in slot 1.
    fn far_element(&mut self, e: &Expr) -> R<Ty> {
        let (ty, page, dims) = self.far_root(e).unwrap();
        let mut idx = Vec::new();
        let mut b = e;
        while let ExprKind::Index(inner, i) = &b.kind {
            idx.push(i.as_ref());
            b = inner;
        }
        idx.reverse();
        if idx.len() != dims.len() {
            return Err(Diag::new(e.pos, "use every index of a `far` array: one element at a time"));
        }
        let mut first = true;
        for (k, i) in idx.iter().enumerate() {
            let stride: u32 = dims[k + 1..].iter().product::<u32>() * ty.size() as u32;
            if !first {
                self.e("push hl");
            }
            match self.const_eval(i) {
                Some(c) => {
                    Self::check_index(Some(dims[k]), c, i.pos)?;
                    self.e(&format!("ld hl,{}", (c * stride as i64) & 0xFFFF));
                }
                None => {
                    self.gen_expr(i, Ty::Word)?;
                    self.mul_const(stride as i64);
                }
            }
            if !first {
                self.e("pop de");
                self.e("add hl,de");
            }
            first = false;
        }
        self.e(&format!("ld a,{page}"));
        self.e("call rt_far_map");
        Ok(ty)
    }

    /// The image asset named by an argument: (label, width, height).
    fn image_arg(&self, e: &Expr) -> R<(String, u32, u32)> {
        if let ExprKind::Name(n) = &e.kind {
            if let Some(Sym::Asset { label, image: Some((w, h)), .. }) = self.lookup(n) {
                return Ok((label, w, h));
            }
        }
        Err(Diag::new(e.pos, "expected an image asset: `asset name = image(\"file.png\")`"))
    }

    fn init_global(&mut self, name: &str, init: &Option<Expr>, pos: Pos) -> R<()> {
        if let Some(Expr { kind: ExprKind::List(_), .. }) = init {
            return Ok(()); // already in the data
        }
        let Some(Sym::Var { obj, label, .. }) = self.lookup(name) else { return Ok(()) };
        match (obj.lay, init) {
            (Lay::Prim(ty), Some(init)) => {
                self.comment(pos, &format!("init {name}"));
                self.gen_expr(init, ty)?;
                self.store(&label, ty);
            }
            (Lay::Struct(id), None) => self.init_struct(&label, id, obj.total(), pos)?,
            (Lay::Struct(_), Some(_)) => return Err(Diag::new(pos, "struct variables cannot have an initializer (yet)")),
            (Lay::Str, Some(init)) if obj.len.is_none() => {
                self.comment(pos, &format!("init {name}"));
                self.gen_str(init)?;
                self.e(&format!("ld de,{label}"));
                self.e("call rt_scopy");
            }
            (Lay::Str, Some(_)) => return Err(Diag::new(pos, "string arrays cannot have an initializer")),
            _ => {}
        }
        Ok(())
    }

    fn declare_struct(&mut self, sd: &StructDecl) -> R<()> {
        self.check_new_name(&sd.name, sd.pos)?;
        if !sd.name.starts_with(|c: char| c.is_ascii_uppercase()) {
            return Err(Diag::new(sd.pos, format!("type names must start with an upper-case letter (`{}`)", sd.name)));
        }
        let mut fields: Vec<FieldDef> = Vec::new();
        let mut offset = 0;
        for f in &sd.fields {
            if fields.iter().any(|x| x.name == f.name) {
                return Err(Diag::new(f.pos, format!("duplicate field `{}`", f.name)));
            }
            if f.ty == TypeRef::Named(sd.name.clone()) {
                return Err(Diag::new(f.pos, "a struct cannot contain itself"));
            }
            let lay = self.resolve_type(&f.ty, f.pos)?;
            let init = match (&f.init, lay) {
                (None, _) => None,
                (Some(e), Lay::Prim(t)) => {
                    let v = self
                        .const_eval(e)
                        .ok_or_else(|| Diag::new(e.pos, "field defaults must be compile-time constants"))?;
                    if t == Ty::Bool || !t.literal_fits(v) {
                        return Err(Diag::new(e.pos, format!("default {v} does not fit in `{}`", t.name())));
                    }
                    if f.array.is_some() {
                        return Err(Diag::new(e.pos, "array fields cannot have a default"));
                    }
                    Some(v)
                }
                (Some(e), _) => return Err(Diag::new(e.pos, "struct fields cannot have a default")),
            };
            let obj = Self::mk_obj(lay, self.decl_dims(&f.array, &None)?);
            let size = self.lay_size(lay) * obj.total().unwrap_or(1);
            fields.push(FieldDef { name: f.name.clone(), obj, offset, init });
            offset += size;
        }
        if offset > 0xFFFF {
            return Err(Diag::new(sd.pos, "struct too large"));
        }
        let id = self.structs.len();
        self.structs.push(StructDef { name: sd.name.clone(), fields, size: offset, methods: HashMap::new() });
        self.struct_ids.insert(sd.name.clone(), id);
        Ok(())
    }

    /// Does this struct (or a nested one) have field defaults?
    fn has_defaults(&self, id: usize) -> bool {
        self.structs[id].fields.iter().any(|f| {
            f.init.is_some() || matches!(f.obj.lay, Lay::Struct(s) if self.has_defaults(s))
        })
    }

    /// Store the field defaults of a struct object at a static address.
    fn init_struct(&mut self, base: &str, id: usize, len: Option<u32>, pos: Pos) -> R<()> {
        if !self.has_defaults(id) {
            return Ok(());
        }
        let size = self.structs[id].size;
        let count = len.unwrap_or(1);
        if count > 16 {
            return Err(Diag::new(pos, "arrays of more than 16 structs with field defaults are not supported yet"));
        }
        self.comment(pos, &format!("defaults of {}", self.structs[id].name));
        for i in 0..count {
            let elem = format!("{base}+{}", i * size);
            let fields = self.structs[id].fields.clone();
            for f in fields {
                let at = format!("{elem}+{}", f.offset);
                match (f.obj.lay, f.init) {
                    (Lay::Prim(t), Some(v)) => {
                        self.load_imm(v, t);
                        self.store(&at, t);
                    }
                    (Lay::Struct(s), _) => self.init_struct(&at, s, f.obj.total(), pos)?,
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn declare_func(&mut self, f: &Func, owner: Option<usize>) -> R<()> {
        let key = match owner {
            None => f.name.clone(),
            Some(id) => format!("{}.{}", self.structs[id].name, f.name),
        };
        if self.funcs.contains_key(&key)
            || (owner.is_none()
                && (INTRINSICS.contains(&f.name.as_str())
                    || Ty::from_name(&f.name).is_some()
                    || self.struct_ids.contains_key(&f.name)))
        {
            return Err(Diag::new(f.pos, format!("function `{key}` is already defined or reserved")));
        }
        let mut params = Vec::new();
        let mut seen = HashSet::new();
        let safe = Self::safe(&key);
        if let Some(id) = owner {
            if self.structs[id].fields.iter().any(|x| x.name == f.name) {
                return Err(Diag::new(f.pos, format!("method `{}` has the same name as a field", f.name)));
            }
            let slot = format!("p_{safe}_self");
            self.alloc_bytes(&slot, 2);
            params.push(("self".to_string(), Lay::Struct(id), slot));
            seen.insert("self".to_string());
            self.structs[id].methods.insert(f.name.clone(), key.clone());
        }
        for (p, ty) in &f.params {
            if !seen.insert(p.clone()) {
                return Err(Diag::new(f.pos, format!("duplicate parameter `{p}`")));
            }
            let lay = self.resolve_type(ty, f.pos)?;
            let slot = format!("p_{safe}_{p}");
            self.alloc_bytes(&slot, match lay {
                Lay::Prim(t) => t.size(),
                Lay::Struct(_) | Lay::Str => 2, // passed by reference
            });
            params.push((p.clone(), lay, slot));
        }
        let label = match owner {
            None => format!("f_{}", f.name),
            Some(_) => format!("m_{safe}"),
        };
        let ret_str = if f.ret_str {
            let l = format!("r_{}", Self::safe(&key));
            self.alloc_bytes(&l, STR_SIZE);
            Some(l)
        } else {
            None
        };
        self.funcs.insert(key, FnSig { label, params, ret: f.ret, ret_str });
        Ok(())
    }

    fn declare_global(&mut self, s: &Stmt) -> R<()> {
        let StmtKind::VarDecl { ty, name, array, init, is_const } = &s.kind else { unreachable!() };
        self.check_new_name(name, s.pos)?;
        let dims = self.decl_dims(array, init)?;
        let array = &dims.map(|(n, sub)| Self::mk_obj(Lay::Prim(Ty::Byte), Some((n, sub))).total().unwrap());
        if *is_const {
            let sym = self.const_sym(init.as_ref().unwrap(), s.pos)?;
            self.globals.insert(name.clone(), sym);
            return Ok(());
        }
        let lay = match ty {
            Some(t) => self.resolve_type(t, s.pos)?,
            None if matches!(init, Some(Expr { kind: ExprKind::List(_), .. })) => {
                return Err(Diag::new(s.pos, "an initialised array needs an element type"));
            }
            None if self.is_str(init.as_ref().unwrap()) => Lay::Str,
            None => Lay::Prim(self.infer_decl(init.as_ref().unwrap())?),
        };
        let label = format!("g_{name}");
        if let Some(Expr { kind: ExprKind::List(items), .. }) = init {
            // initial values are part of the program image: no code at all
            let data = self.decl_data(lay, dims.unwrap(), items, s.pos)?;
            let _ = write!(self.data, "{label}:\n{data}");
        } else {
            let bytes = self.lay_size(lay) * array.unwrap_or(1);
            self.alloc_bytes(&label, bytes);
        }
        self.globals.insert(name.clone(), Sym::Var { obj: Self::mk_obj(lay, dims), label, by_ref: false });
        Ok(())
    }

    fn infer_decl(&self, init: &Expr) -> R<Ty> {
        if let ExprKind::Str(_) = init.kind {
            return Err(Diag::new(init.pos, "string variables are not supported yet"));
        }
        Ok(match self.natural(init)? {
            Some(t) => t,
            None => literal_ty(self.const_eval(init).unwrap_or(0)),
        })
    }

    fn func(&mut self, f: &Func, key: String) -> R<()> {
        let sig = self.funcs[&key].clone();
        self.cur_fn = key.clone();
        self.cur_ret = Some(sig.ret);
        self.cur_ret_str = sig.ret_str.clone();
        self.calls.entry(key.clone()).or_insert((f.pos, HashSet::new()));
        let start = self.code.len();
        let _ = writeln!(self.code, "\n; function {key}");
        self.label(&sig.label);
        let mut params = HashMap::new();
        for (p, lay, slot) in &sig.params {
            let by_ref = matches!(lay, Lay::Struct(_) | Lay::Str);
            params.insert(p.clone(), Sym::Var { obj: Obj::new(*lay, None), label: slot.clone(), by_ref });
        }
        self.scopes.push(params);
        self.scopes.push(HashMap::new());
        let r = self.block_noscope(&f.body);
        self.scopes.clear();
        self.e("ret");
        let text = self.code.split_off(start);
        self.fn_chunks.push((key, sig.label.clone(), text));
        r
    }

    fn irq_key(&self, line: Option<&Expr>) -> Option<String> {
        match line {
            None => Some("interrupt frame".into()),
            Some(e) => self.const_eval(e).map(|n| format!("interrupt raster({n})")),
        }
    }

    fn declare_interrupt(&mut self, line: Option<&Expr>, pos: Pos) -> R<()> {
        let n = match line {
            None => None,
            Some(e) => match self.const_eval(e) {
                Some(n) if (0..=311).contains(&n) => Some(n as u16),
                Some(_) => return Err(Diag::new(e.pos, "raster line must be 0..311")),
                None => return Err(Diag::new(e.pos, "raster line must be a constant")),
            },
        };
        let key = self.irq_key(line).unwrap();
        if self.funcs.contains_key(&key) {
            return Err(Diag::new(pos, format!("`{key}` is defined twice")));
        }
        let label = match n {
            Some(n) => format!("i_raster_{n}"),
            None => "i_frame".to_string(),
        };
        self.funcs.insert(key.clone(), FnSig { label: label.clone(), params: vec![], ret: None, ret_str: None });
        self.irqs.push((n, label, key));
        Ok(())
    }

    /// Functions have static frames: one called both from the main program
    /// and from an interrupt handler could be re-entered and corrupted.
    fn check_interrupt_sharing(&mut self) {
        let reach = |root: &str| -> HashSet<String> {
            let mut seen = HashSet::new();
            let mut stack = vec![root.to_string()];
            while let Some(n) = stack.pop() {
                if let Some((_, cs)) = self.calls.get(&n) {
                    for c in cs {
                        if seen.insert(c.clone()) {
                            stack.push(c.clone());
                        }
                    }
                }
            }
            seen
        };
        let main = reach("main");
        let mut irqs = self.irqs.clone();
        if self.funcs.contains_key("sound_tick") {
            irqs.push((None, String::new(), "sound_tick".into()));
        }
        irqs.sort();
        for (_, _, key) in irqs {
            let mut shared: Vec<_> = reach(&key).intersection(&main).cloned().collect();
            shared.sort();
            if let Some(f) = shared.first() {
                let pos = self.calls.get(&key).map(|c| c.0).unwrap_or_default();
                self.errors.push(Diag::new(
                    pos,
                    format!(
                        "function `{f}` is called both from the main program and from `{key}`; \
                         functions use static frames, so an interrupt would corrupt it (use separate functions)"
                    ),
                ));
            }
        }
    }

    /// The interrupt service routine and its startup code.
    /// Without raster handlers the ULA frame interrupt drives everything;
    /// with them, the ULA interrupt is switched off and the Next line
    /// interrupt is chained through the handler lines plus line 192 (frame).
    fn gen_isr(&self, code_banked: bool) -> (String, String) {
        let frame = self.irqs.iter().find(|i| i.0.is_none()).map(|i| i.1.clone());
        // background sound driver of the sound module
        let driver = self.funcs.get("sound_tick").map(|f| f.label.clone());
        let mut lines: Vec<(u16, String)> = self.irqs.iter().filter_map(|i| i.0.map(|n| (n, i.1.clone()))).collect();
        let mut isr = String::from("__isr:\n    push af\n    push bc\n    push de\n    push hl\n");
        // slot 1 (far data, pictures) may be remapped by the handler
        let slot1 = !self.far_pages.is_empty();
        if slot1 {
            isr.push_str("    ld a,(rt_farpage)\n    push af\n");
        }
        if code_banked {
            isr.push_str("    ld a,(rt_codepage)\n    push af\n    ld a,(rt_bret)\n    push af\n");
        }
        let tick = |isr: &mut String| {
            isr.push_str("    ld hl,(rt_frames)\n    inc hl\n    ld (rt_frames),hl\n");
            if let Some(d) = &driver {
                isr.push_str(&format!("    call {d}\n"));
            }
            if let Some(f) = &frame {
                isr.push_str(&format!("    call {f}\n"));
            }
        };
        let mut init = String::new();
        if lines.is_empty() {
            tick(&mut isr);
        } else {
            const FRAME_LINE: u16 = 192;
            lines.sort();
            let mut entries: Vec<(u16, Vec<Option<String>>)> = Vec::new();
            for (n, l) in lines {
                entries.push((n, vec![Some(l)]));
            }
            match entries.iter_mut().find(|e| e.0 == FRAME_LINE) {
                Some(e) => e.1.insert(0, None),
                None => {
                    entries.push((FRAME_LINE, vec![None]));
                    entries.sort_by_key(|e| e.0);
                }
            }
            let k = entries.len();
            isr.push_str("    ld a,(rt_irq_idx)\n");
            for (i, (_, hs)) in entries.iter().enumerate() {
                isr.push_str(&format!(".e{i}:\n"));
                if i + 1 < k {
                    isr.push_str(&format!("    cp {i}\n    jr nz,.e{}\n", i + 1));
                }
                for h in hs {
                    match h {
                        None => tick(&mut isr),
                        Some(l) => isr.push_str(&format!("    call {l}\n")),
                    }
                }
                let (next, nline) = ((i + 1) % k, entries[(i + 1) % k].0);
                isr.push_str(&format!("    ld a,{next}\n    ld de,{nline}\n    jp .set\n"));
            }
            isr.push_str(
                ".set:\n    ld (rt_irq_idx),a\n    ld a,e\n    nextreg $23,a\n    ld a,d\n    and 1\n    or %00000110\n    nextreg $22,a\n",
            );
            let first = entries[0].0;
            init = format!(
                "    ; raster interrupts: ULA interrupt off, line interrupt at {first}\n    nextreg $23,{}\n    nextreg $22,{}\n",
                first & 0xFF,
                0b110 | (first >> 8)
            );
        }
        if code_banked {
            isr.push_str("    pop af\n    ld (rt_bret),a\n    pop af\n    ld (rt_codepage),a\n    nextreg $52,a\n");
        }
        if slot1 {
            isr.push_str("    pop af\n    ld (rt_farpage),a\n    nextreg $51,a\n");
        }
        isr.push_str("    pop hl\n    pop de\n    pop bc\n    pop af\n    ei\n    reti\n");
        (isr, init)
    }

    fn check_recursion(&mut self) {
        fn dfs(
            n: &str,
            calls: &HashMap<String, (Pos, HashSet<String>)>,
            state: &mut HashMap<String, u8>,
            path: &mut Vec<String>,
        ) -> Option<Vec<String>> {
            match state.get(n) {
                Some(1) => {
                    let i = path.iter().position(|p| p == n).unwrap();
                    let mut cyc = path[i..].to_vec();
                    cyc.push(n.to_string());
                    return Some(cyc);
                }
                Some(2) => return None,
                _ => {}
            }
            state.insert(n.to_string(), 1);
            path.push(n.to_string());
            if let Some((_, cs)) = calls.get(n) {
                let mut cs: Vec<_> = cs.iter().collect();
                cs.sort();
                for c in cs {
                    if let Some(cyc) = dfs(c, calls, state, path) {
                        return Some(cyc);
                    }
                }
            }
            path.pop();
            state.insert(n.to_string(), 2);
            None
        }
        let mut names: Vec<_> = self.calls.keys().cloned().collect();
        names.sort();
        let mut state = HashMap::new();
        for n in names {
            if let Some(cyc) = dfs(&n, &self.calls, &mut state, &mut Vec::new()) {
                let pos = self.calls[&cyc[0]].0;
                self.errors.push(Diag::new(
                    pos,
                    format!(
                        "recursion is not supported in v0 (static frames): {}",
                        cyc.join(" -> ")
                    ),
                ));
                return;
            }
        }
    }

    // ---------------------------------------------------------- statements
    fn block(&mut self, body: &[Stmt]) -> R<()> {
        self.scopes.push(HashMap::new());
        let r = self.block_noscope(body);
        self.scopes.pop();
        r
    }

    fn block_noscope(&mut self, body: &[Stmt]) -> R<()> {
        for s in body {
            if let Err(d) = self.stmt(s) {
                self.errors.push(d);
            }
        }
        Ok(())
    }

    fn stmt(&mut self, s: &Stmt) -> R<()> {
        self.str_tmp = 0;
        match &s.kind {
            StmtKind::VarDecl { ty, name, array, init, is_const } => {
                let dims = self.decl_dims(array, init)?;
                // `array`: total number of elements over all dimensions
                let array = &dims.map(|(n, sub)| Self::mk_obj(Lay::Prim(Ty::Byte), Some((n, sub))).total().unwrap());
                if self.scopes.is_empty() {
                    // nested block in main program: give it a scope of its own
                    unreachable!("top-level declarations are handled in run()");
                }
                self.check_new_name(name, s.pos)?;
                if *is_const {
                    let sym = self.const_sym(init.as_ref().unwrap(), s.pos)?;
                    self.scopes.last_mut().unwrap().insert(name.clone(), sym);
                    return Ok(());
                }
                if let Some(Expr { kind: ExprKind::List(items), .. }) = init {
                    // static frame: copy the initial values in on every entry
                    let Some(t) = ty else {
                        return Err(Diag::new(s.pos, "an initialised array needs an element type"));
                    };
                    let lay = self.resolve_type(t, s.pos)?;
                    let n = array.unwrap();
                    self.comment(s.pos, &format!("local {name}"));
                    let data = self.decl_data(lay, dims.unwrap(), items, s.pos)?;
                    self.n += 1;
                    let tab = format!("ai_{}", self.n);
                    let _ = write!(self.data, "{tab}:\n{data}");
                    let label = self.declare_local_obj(name, Self::mk_obj(lay, dims));
                    self.e(&format!("ld hl,{tab}"));
                    self.e(&format!("ld de,{label}"));
                    self.e(&format!("ld bc,{}", self.lay_size(lay) * n));
                    self.e("ldir");
                    return Ok(());
                }
                let is_string = match ty {
                    Some(t) => self.resolve_type(t, s.pos)? == Lay::Str,
                    None => self.is_str(init.as_ref().unwrap()),
                };
                if is_string {
                    self.comment(s.pos, &format!("local {name}"));
                    if let (Some(e), Some(_)) = (init, array) {
                        let _ = e;
                        return Err(Diag::new(s.pos, "string arrays cannot have an initializer"));
                    }
                    if let Some(e) = init {
                        self.gen_str(e)?;
                    }
                    let label = self.declare_local_obj(name, Self::mk_obj(Lay::Str, dims));
                    match (init, *array) {
                        (Some(_), _) => {
                            self.e(&format!("ld de,{label}"));
                            self.e("call rt_scopy");
                        }
                        (None, None) => {
                            self.e("xor a");
                            self.e(&format!("ld ({label}),a"));
                        }
                        (None, Some(n)) => {
                            // static frame: empty every element on each entry
                            if n > 255 {
                                return Err(Diag::new(s.pos, "local string arrays are limited to 255 elements"));
                            }
                            let l = self.new_label();
                            self.e(&format!("ld hl,{label}"));
                            self.e(&format!("ld de,{STR_SIZE}"));
                            self.e(&format!("ld b,{n}"));
                            self.label(&l);
                            self.e("ld (hl),0");
                            self.e("add hl,de");
                            self.e(&format!("djnz {l}"));
                        }
                    }
                    return Ok(());
                }
                let ty = match ty {
                    Some(t) => match self.resolve_type(t, s.pos)? {
                        Lay::Prim(t) => t,
                        Lay::Str => unreachable!(),
                        Lay::Struct(id) => {
                            if init.is_some() {
                                return Err(Diag::new(s.pos, "struct variables cannot have an initializer (yet)"));
                            }
                            self.comment(s.pos, &format!("local {name}"));
                            let label = self.declare_local_obj(name, Self::mk_obj(Lay::Struct(id), dims));
                            // locals live in static frames: re-apply defaults on every entry
                            return self.init_struct(&label, id, *array, s.pos);
                        }
                    },
                    None => self.infer_decl(init.as_ref().unwrap())?,
                };
                self.comment(s.pos, &format!("local {name}"));
                match init {
                    Some(e) => self.gen_expr(e, ty)?,
                    None if array.is_none() => self.load_imm(0, ty),
                    None => {}
                }
                let label = self.declare_local_obj(name, Self::mk_obj(Lay::Prim(ty), dims));
                if init.is_some() || array.is_none() {
                    self.store(&label, ty);
                }
                Ok(())
            }
            StmtKind::Assign { target, op, value } => {
                self.comment(s.pos, "assign");
                let value = match op {
                    Some(op) => Expr {
                        kind: ExprKind::Binary(*op, Box::new(target.clone()), Box::new(value.clone())),
                        pos: s.pos,
                    },
                    None => value.clone(),
                };
                self.assign(target, &value)
            }
            StmtKind::If { arms, otherwise } => {
                self.comment(s.pos, "if");
                let end = self.new_label();
                for (cond, body) in arms {
                    let next = self.new_label();
                    self.branch_false(cond, &next)?;
                    self.block(body)?;
                    self.e(&format!("jp {end}"));
                    self.label(&next);
                }
                if let Some(body) = otherwise {
                    self.block(body)?;
                }
                self.label(&end);
                Ok(())
            }
            StmtKind::While { cond, body } => {
                self.comment(s.pos, "while");
                let top = self.new_label();
                let exit = self.new_label();
                self.label(&top);
                self.branch_false(cond, &exit)?;
                self.loop_exits.push(exit.clone());
                let r = self.block(body);
                self.loop_exits.pop();
                r?;
                self.e(&format!("jp {top}"));
                self.label(&exit);
                Ok(())
            }
            StmtKind::For { var, start, end, step, body } => self.for_loop(s.pos, var, start, end, step, body),
            StmtKind::Asset { .. } => Err(Diag::new(s.pos, "`asset` declarations must be at the top level")),
            StmtKind::Far(_) => Err(Diag::new(s.pos, "`far` arrays must be declared at the top level")),
            StmtKind::Return(e) => {
                if let Some(buf) = self.cur_ret_str.clone() {
                    let Some(e) = e else { return Err(Diag::new(s.pos, "missing return value")) };
                    self.gen_str(e)?;
                    self.e(&format!("ld de,{buf}"));
                    self.e("call rt_scopy");
                    self.e("ret");
                    return Ok(());
                }
                match (self.cur_ret, e) {
                    (None, None) => self.e("jp __main_end"),
                    (None, Some(_)) => return Err(Diag::new(s.pos, "the main program cannot return a value")),
                    (Some(None), None) => self.e("ret"),
                    (Some(None), Some(_)) => {
                        return Err(Diag::new(s.pos, "function has no return type (add `-> type`)"))
                    }
                    (Some(Some(_)), None) => return Err(Diag::new(s.pos, "missing return value")),
                    (Some(Some(t)), Some(e)) => {
                        self.gen_expr(e, t)?;
                        self.e("ret");
                    }
                }
                Ok(())
            }
            StmtKind::Break => {
                let l = self
                    .loop_exits
                    .last()
                    .cloned()
                    .ok_or_else(|| Diag::new(s.pos, "`break` outside of a loop"))?;
                self.e(&format!("jp {l}"));
                Ok(())
            }
            StmtKind::Expr(e) => {
                match &e.kind {
                    ExprKind::Call(name, args) => {
                        self.comment(s.pos, &format!("call {name}"));
                        self.call(name, args, e.pos)?;
                    }
                    ExprKind::MethodCall(obj, m, args) => {
                        self.comment(s.pos, &format!("method {m}"));
                        self.method_call(obj, m, args, e.pos)?;
                    }
                    _ => unreachable!(),
                }
                Ok(())
            }
            StmtKind::Asm(body) => {
                let _ = writeln!(self.code, "    ; line {}: inline asm
    ; asm begin", s.pos.line);
                for line in body.lines() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    self.code.push_str(line);
                    self.code.push('\n');
                }
                Ok(())
            }
        }
    }

    fn for_loop(&mut self, pos: Pos, var: &str, start: &Expr, end: &Expr, step: &Option<Expr>, body: &[Stmt]) -> R<()> {
        self.comment(pos, &format!("for {var}"));
        let step_v = match step {
            None => 1,
            Some(e) => self
                .const_eval(e)
                .ok_or_else(|| Diag::new(e.pos, "`for` step must be a compile-time constant"))?,
        };
        if step_v == 0 {
            return Err(Diag::new(pos, "`for` step cannot be zero"));
        }
        let ty = match join(self.natural(start)?, self.natural(end)?) {
            Some(Ty::Bool) => return Err(Diag::new(pos, "`for` bounds must be integers")),
            Some(t) => t,
            None => {
                let a = self.const_eval(start).unwrap_or(0);
                let b = self.const_eval(end).unwrap_or(0);
                join(Some(literal_ty(a)), Some(literal_ty(b))).unwrap()
            }
        };
        if !ty.literal_fits(step_v) {
            return Err(Diag::new(pos, "`for` step does not fit the loop variable type"));
        }
        self.scopes.push(HashMap::new());
        self.gen_expr(start, ty)?;
        let ivar = self.declare_local(var, ty, None);
        self.store(&ivar, ty);
        let end_slot = format!("$end{}", self.n);
        self.gen_expr(end, ty)?;
        let end_lbl = self.declare_local(&end_slot, ty, None);
        self.store(&end_lbl, ty);

        let top = self.new_label();
        let exit = self.new_label();
        // initial check: enter only if i <= end (step > 0) / i >= end (step < 0)
        let op = if step_v > 0 { BinOp::Le } else { BinOp::Ge };
        let cmp = Expr {
            kind: ExprKind::Binary(
                op,
                Box::new(Expr { kind: ExprKind::Name(var.to_string()), pos }),
                Box::new(Expr { kind: ExprKind::Name(end_slot.clone()), pos }),
            ),
            pos,
        };
        // constant bounds: the entry check is known at compile time
        let entered = match (self.const_eval(start), self.const_eval(end)) {
            (Some(a), Some(b)) => Some(if step_v > 0 { a <= b } else { a >= b }),
            _ => None,
        };
        match entered {
            Some(true) => {}
            Some(false) => self.e(&format!("jp {exit}")),
            None => self.branch_false(&cmp, &exit)?,
        }
        let mag = step_v.unsigned_abs() as i64;
        let end_const = self.const_eval(end);
        let word = ty.size() == 2;
        if mag == 1 && end_const.is_none() {
            // the slot now holds the stop value end + step
            let (r, op) = if word { ("hl", if step_v > 0 { "inc" } else { "dec" }) } else { ("a", if step_v > 0 { "inc" } else { "dec" }) };
            self.e(&format!("ld {r},({end_lbl})"));
            self.e(&format!("{op} {r}"));
            self.e(&format!("ld ({end_lbl}),{r}"));
        }
        self.label(&top);
        self.loop_exits.push(exit.clone());
        let r = self.block(body);
        self.loop_exits.pop();
        r?;
        if mag == 1 {
            // step +-1: advance, then loop while i != end + step
            let op = if step_v > 0 { "inc" } else { "dec" };
            let stop = end_const.map(|v| mask(v + step_v, ty));
            if word {
                self.e(&format!("ld hl,({ivar})"));
                self.e(&format!("{op} hl"));
                self.e(&format!("ld ({ivar}),hl"));
                if let Some(v) = stop {
                    self.e("ld a,l");
                    self.e(&format!("cp {}", v & 0xFF));
                    self.e(&format!("jp nz,{top}"));
                    self.e("ld a,h");
                    self.e(&format!("cp {}", v >> 8));
                } else {
                    self.e(&format!("ld de,({end_lbl})"));
                    self.e("or a");
                    self.e("sbc hl,de");
                }
            } else {
                self.e(&format!("ld a,({ivar})"));
                self.e(&format!("{op} a"));
                self.e(&format!("ld ({ivar}),a"));
                if let Some(v) = stop {
                    self.e(&format!("cp {v}"));
                } else {
                    self.e(&format!("ld hl,{end_lbl}"));
                    self.e("cp (hl)");
                }
            }
            self.e(&format!("jp nz,{top}"));
            self.label(&exit);
            self.scopes.pop();
            return Ok(());
        }
        // remaining = distance to end (unsigned, never negative here)
        let (a, b) = if step_v > 0 { (&end_lbl, &ivar) } else { (&ivar, &end_lbl) };
        if ty.size() == 1 {
            self.e(&format!("ld a,({b})"));
            self.e("ld b,a");
            self.e(&format!("ld a,({a})"));
            self.e("sub b");
            if mag == 1 {
                self.e(&format!("jp z,{exit}"));
            } else {
                self.e(&format!("cp {mag}"));
                self.e(&format!("jp c,{exit}"));
            }
            self.e(&format!("ld a,({ivar})"));
            self.e(&format!("add a,{}", mask(step_v, ty)));
            self.e(&format!("ld ({ivar}),a"));
        } else {
            self.e(&format!("ld hl,({a})"));
            self.e(&format!("ld de,({b})"));
            self.e("or a");
            self.e("sbc hl,de");
            if mag == 1 {
                self.e(&format!("jp z,{exit}"));
            } else {
                self.e(&format!("ld de,{mag}"));
                self.e("or a");
                self.e("sbc hl,de");
                self.e(&format!("jp c,{exit}"));
            }
            self.e(&format!("ld hl,({ivar})"));
            self.e(&format!("add hl,{}", mask(step_v, ty)));
            self.e(&format!("ld ({ivar}),hl"));
        }
        self.e(&format!("jp {top}"));
        self.label(&exit);
        self.scopes.pop();
        Ok(())
    }

    // ------------------------------------------------------------ objects
    fn is_hw(e: &Expr, name: &str) -> bool {
        matches!(&e.kind, ExprKind::Index(b, _) if matches!(&b.kind, ExprKind::Name(n) if n == name))
    }

    /// Layout of a storage expression (variable, array element, field). No code.
    fn obj_of(&self, e: &Expr) -> R<Obj> {
        match &e.kind {
            ExprKind::Name(n) => match self.lookup(n) {
                Some(Sym::Var { obj, .. }) => Ok(obj),
                Some(Sym::Far { ty, len, sub, .. }) => Ok(Obj { lay: Lay::Prim(ty), len: Some(len), sub }),
                Some(Sym::Const(_) | Sym::FConst(_) | Sym::SConst(_)) => {
                    Err(Diag::new(e.pos, format!("`{n}` is a constant")))
                }
                Some(Sym::Asset { .. }) => {
                    Err(Diag::new(e.pos, format!("`{n}` is an asset (use it with `sprite_load`)")))
                }
                None if n == "memory" || n == "nextreg" || n == "port" => {
                    Err(Diag::new(e.pos, format!("`{n}` must be indexed: `{n}[...]`")))
                }
                None => Err(Diag::new(e.pos, format!("undefined variable `{n}`"))),
            },
            ExprKind::Index(b, _) => {
                if Self::is_hw(e, "memory") || Self::is_hw(e, "nextreg") || Self::is_hw(e, "port") {
                    return Ok(Obj::new(Lay::Prim(Ty::Byte), None));
                }
                let o = self.obj_of(b)?;
                if o.len.is_none() {
                    return Err(Diag::new(e.pos, "indexing something that is not an array"));
                }
                // drop the outermost dimension
                let len = if o.sub[0] > 0 { Some(o.sub[0]) } else { None };
                Ok(Obj { lay: o.lay, len, sub: [o.sub[1], o.sub[2], 0] })
            }
            ExprKind::Field(b, f) => {
                let (id, fd) = self.field(b, f, e.pos)?;
                let _ = id;
                Ok(fd.obj)
            }
            _ => Err(Diag::new(e.pos, "expected a variable, array element or field")),
        }
    }

    fn field(&self, base: &Expr, f: &str, pos: Pos) -> R<(usize, FieldDef)> {
        let o = self.obj_of(base)?;
        let (Lay::Struct(id), None) = (o.lay, o.len) else {
            return Err(Diag::new(pos, format!("`.{f}` used on something that is not a struct")));
        };
        let fd = self.structs[id].fields.iter().find(|x| x.name == f).cloned().ok_or_else(|| {
            let s = &self.structs[id];
            if s.methods.contains_key(f) {
                Diag::new(pos, format!("`{f}` is a method of `{}`; call it with `(...)`", s.name))
            } else {
                Diag::new(pos, format!("`{}` has no field `{f}`", s.name))
            }
        })?;
        Ok((id, fd))
    }

    fn check_index(len: Option<u32>, k: i64, pos: Pos) -> R<()> {
        match len {
            Some(n) if k < 0 || k >= n as i64 => {
                Err(Diag::new(pos, format!("index {k} out of bounds (array has {n} elements)")))
            }
            _ => Ok(()),
        }
    }

    /// Link-time address of a storage expression, if it has one. No code.
    fn static_addr(&self, e: &Expr) -> R<Option<String>> {
        Ok(match &e.kind {
            ExprKind::Name(n) => match self.lookup(n) {
                Some(Sym::Var { label, by_ref: false, .. }) => Some(label),
                _ => None,
            },
            ExprKind::Index(_, i) if Self::is_hw(e, "memory") => match self.const_eval(i) {
                Some(k) if (0..=0xFFFF).contains(&k) => Some(k.to_string()),
                Some(_) => return Err(Diag::new(i.pos, "memory address must be 0..65535")),
                None => None,
            },
            ExprKind::Index(_, _) if Self::is_hw(e, "nextreg") || Self::is_hw(e, "port") => None,
            ExprKind::Index(b, i) => {
                let o = self.obj_of(b)?;
                match self.const_eval(i) {
                    Some(k) => {
                        Self::check_index(o.len, k, i.pos)?;
                        let esz = (self.lay_size(o.lay) * o.inner()) as i64;
                        self.static_addr(b)?.map(|bs| format!("{bs}+{}", k * esz))
                    }
                    None => None,
                }
            }
            ExprKind::Field(b, f) => {
                let (_, fd) = self.field(b, f, e.pos)?;
                self.static_addr(b)?.map(|bs| format!("{bs}+{}", fd.offset))
            }
            _ => None,
        })
    }

    fn add_hl_const(&mut self, n: i64) {
        let n = n & 0xFFFF;
        match n {
            0 => {}
            1..=3 => {
                for _ in 0..n {
                    self.e("inc hl");
                }
            }
            _ => self.e(&format!("add hl,{n}")),
        }
    }

    /// HL = index expression * element size.
    fn scaled_index(&mut self, i: &Expr, esz: u32) -> R<()> {
        if !esz.is_power_of_two() && esz < 256 && self.natural(i)? == Some(Ty::Byte) {
            // Z80N: one MUL D,E for byte index * small element size
            self.gen_expr(i, Ty::Byte)?;
            self.e("ld d,a");
            self.e(&format!("ld e,{esz}"));
            self.e("mul d,e");
            self.e("ex de,hl");
            return Ok(());
        }
        self.gen_expr(i, Ty::Word)?;
        if esz.is_power_of_two() {
            for _ in 0..esz.trailing_zeros() {
                self.e("add hl,hl");
            }
        } else {
            self.e(&format!("ld de,{esz}"));
            self.e("call rt_mul16");
        }
        Ok(())
    }

    /// Locate a storage expression: static address, or address in HL.
    fn place(&mut self, e: &Expr) -> R<Place> {
        if self.far_root(e).is_some() {
            return Err(Diag::new(e.pos, "a `far` array element can only be read or assigned"));
        }
        if let Some(a) = self.static_addr(e)? {
            return Ok(Place::Static(a));
        }
        match &e.kind {
            ExprKind::Name(n) => {
                let Some(Sym::Var { label, .. }) = self.lookup(n) else { unreachable!() };
                self.e(&format!("ld hl,({label})"));
            }
            ExprKind::Index(_, i) if Self::is_hw(e, "memory") => self.gen_expr(i, Ty::Word)?,
            ExprKind::Index(b, i) => {
                let o = self.obj_of(b)?;
                let esz = self.lay_size(o.lay) * o.inner();
                if let Some(k) = self.const_eval(i) {
                    Self::check_index(o.len, k, i.pos)?;
                    self.place_hl(b)?;
                    self.add_hl_const(k * esz as i64);
                } else if let Some(bs) = self.static_addr(b)? {
                    self.scaled_index(i, esz)?;
                    self.e(&format!("add hl,{bs}"));
                } else {
                    self.scaled_index(i, esz)?;
                    self.e("push hl");
                    self.place_hl(b)?;
                    self.e("pop de");
                    self.e("add hl,de");
                }
            }
            ExprKind::Field(b, f) => {
                let (_, fd) = self.field(b, f, e.pos)?;
                self.place_hl(b)?;
                self.add_hl_const(fd.offset as i64);
            }
            _ => return Err(Diag::new(e.pos, "expected a variable, array element or field")),
        }
        Ok(Place::Dyn)
    }

    /// HL = address of a storage expression.
    fn place_hl(&mut self, e: &Expr) -> R<()> {
        if let Place::Static(a) = self.place(e)? {
            self.e(&format!("ld hl,{a}"));
        }
        Ok(())
    }

    /// BC = 16-bit port number (clobbers A, HL).
    fn port_addr(&mut self, idx: &Expr) -> R<()> {
        match self.const_eval(idx) {
            Some(p) if (0..=0xFFFF).contains(&p) => self.e(&format!("ld bc,{p}")),
            Some(_) => return Err(Diag::new(idx.pos, "port number must be 0..65535")),
            None => {
                self.gen_expr(idx, Ty::Word)?;
                self.e("ld b,h");
                self.e("ld c,l");
            }
        }
        Ok(())
    }

    /// Load a primitive storage expression into A/HL; returns its type.
    fn load_obj(&mut self, e: &Expr) -> R<Ty> {
        if let ExprKind::Index(_, idx) = &e.kind {
            if Self::is_hw(e, "nextreg") {
                match self.const_eval(idx) {
                    Some(r) if (0..=255).contains(&r) => self.e(&format!("ld a,{r}")),
                    Some(_) => return Err(Diag::new(idx.pos, "NextReg number must be 0..255")),
                    None => self.gen_expr(idx, Ty::Byte)?,
                }
                self.e("ld bc,$243B");
                self.e("out (c),a");
                self.e("inc b");
                self.e("in a,(c)");
                return Ok(Ty::Byte);
            }
            if Self::is_hw(e, "port") {
                self.port_addr(idx)?;
                self.e("in a,(c)");
                return Ok(Ty::Byte);
            }
        }
        let Obj { lay: Lay::Prim(ty), len: None, .. } = self.obj_of(e)? else {
            return Err(Diag::new(e.pos, "not a simple value"));
        };
        if self.far_root(e).is_some() {
            self.far_element(e)?;
            self.e("ld a,(hl)");
            if ty.size() == 2 {
                self.e("inc hl");
                self.e("ld h,(hl)");
                self.e("ld l,a");
            }
            return Ok(ty);
        }
        match self.place(e)? {
            Place::Static(a) => self.load(&a, ty),
            Place::Dyn if ty.size() == 1 => self.e("ld a,(hl)"),
            Place::Dyn => {
                self.e("ld a,(hl)");
                self.e("inc hl");
                self.e("ld h,(hl)");
                self.e("ld l,a");
            }
        }
        Ok(ty)
    }

    fn assign(&mut self, target: &Expr, value: &Expr) -> R<()> {
        if Self::is_hw(target, "nextreg") {
            let ExprKind::Index(_, idx) = &target.kind else { unreachable!() };
            return self.nextreg_write(idx, value);
        }
        if Self::is_hw(target, "port") {
            let ExprKind::Index(_, idx) = &target.kind else { unreachable!() };
            self.gen_expr(value, Ty::Byte)?;
            if self.const_eval(idx).is_some() {
                self.e("ld e,a");
                self.port_addr(idx)?;
                self.e("out (c),e");
            } else {
                self.e("push af");
                self.port_addr(idx)?;
                self.e("pop af");
                self.e("out (c),a");
            }
            return Ok(());
        }
        let obj = self.obj_of(target)?;
        if obj.len.is_some() {
            return Err(Diag::new(target.pos, "cannot assign to a whole array"));
        }
        if self.far_root(target).is_some() {
            let Lay::Prim(ty) = obj.lay else { unreachable!() };
            self.gen_expr(value, ty)?;
            self.push(ty);
            self.far_element(target)?;
            if ty.size() == 1 {
                self.e("pop af");
                self.e("ld (hl),a");
            } else {
                self.e("pop de");
                self.e("ld (hl),e");
                self.e("inc hl");
                self.e("ld (hl),d");
            }
            return Ok(());
        }
        match obj.lay {
            Lay::Prim(ty) => {
                if let Some(a) = self.static_addr(target)? {
                    self.gen_expr(value, ty)?;
                    self.store(&a, ty);
                    return Ok(());
                }
                // constant or plain variable of the same type: address first
                let cv = if matches!(ty, Ty::Byte | Ty::SByte | Ty::Word | Ty::SWord) { self.const_eval(value).filter(|&v| ty.literal_fits(v)) } else { None };
                let simple = match &value.kind {
                    ExprKind::Name(_) if self.natural(value)? == Some(ty) => self.static_addr(value)?,
                    _ => None,
                };
                if cv.is_some() || simple.is_some() {
                    self.place(target)?;
                    match (cv, ty.size()) {
                        (Some(v), 1) => self.e(&format!("ld (hl),{}", mask(v, ty))),
                        (Some(v), _) => {
                            let v = mask(v, ty);
                            self.e(&format!("ld (hl),{}", v & 0xFF));
                            self.e("inc hl");
                            self.e(&format!("ld (hl),{}", v >> 8));
                        }
                        (None, 1) => {
                            self.e(&format!("ld a,({})", simple.unwrap()));
                            self.e("ld (hl),a");
                        }
                        (None, _) => {
                            self.e(&format!("ld de,({})", simple.unwrap()));
                            self.e("ld (hl),e");
                            self.e("inc hl");
                            self.e("ld (hl),d");
                        }
                    }
                    return Ok(());
                }
                self.gen_expr(value, ty)?;
                self.push(ty);
                self.place(target)?;
                if ty.size() == 1 {
                    self.e("pop af");
                    self.e("ld (hl),a");
                } else {
                    self.e("pop de");
                    self.e("ld (hl),e");
                    self.e("inc hl");
                    self.e("ld (hl),d");
                }
            }
            Lay::Str => {
                self.gen_str(value)?;
                if let Some(a) = self.static_addr(target)? {
                    self.e(&format!("ld de,{a}"));
                } else {
                    self.e("push hl");
                    self.place_hl(target)?;
                    self.e("ex de,hl");
                    self.e("pop hl");
                }
                self.e("call rt_scopy");
            }
            Lay::Struct(id) => {
                // whole-struct copy
                let src = self.obj_of(value)?;
                if src.lay != obj.lay || src.len.is_some() {
                    return Err(Diag::new(value.pos, format!("expected a `{}` value", self.structs[id].name)));
                }
                let size = self.structs[id].size;
                self.place_hl(value)?;
                self.e("push hl");
                self.place_hl(target)?;
                self.e("ex de,hl");
                self.e("pop hl");
                self.e(&format!("ld bc,{size}"));
                self.e("ldir");
            }
        }
        Ok(())
    }

    fn nextreg_write(&mut self, reg: &Expr, value: &Expr) -> R<()> {
        if let Some(r) = self.const_eval(reg) {
            if !(0..=255).contains(&r) {
                return Err(Diag::new(reg.pos, "NextReg number must be 0..255"));
            }
            if let Some(v) = self.const_eval(value) {
                if !Ty::Byte.literal_fits(v) {
                    return Err(Diag::new(value.pos, "NextReg value must fit in a byte"));
                }
                self.e(&format!("nextreg {r},{}", v & 0xFF));
            } else {
                self.gen_expr(value, Ty::Byte)?;
                self.e(&format!("nextreg {r},a"));
            }
            return Ok(());
        }
        self.gen_expr(value, Ty::Byte)?;
        self.e("push af");
        self.gen_expr(reg, Ty::Byte)?;
        self.e("ld bc,$243B");
        self.e("out (c),a");
        self.e("inc b");
        self.e("pop af");
        self.e("out (c),a");
        Ok(())
    }

    // ------------------------------------------------------- loads/stores
    fn load_imm(&mut self, v: i64, ty: Ty) {
        if ty.size() == 1 {
            if v & 0xFF == 0 {
                self.e("xor a");
            } else {
                self.e(&format!("ld a,{}", v & 0xFF));
            }
        } else {
            self.e(&format!("ld hl,{}", v & 0xFFFF));
        }
    }
    fn load(&mut self, label: &str, ty: Ty) {
        if ty.size() == 1 {
            self.e(&format!("ld a,({label})"));
        } else {
            self.e(&format!("ld hl,({label})"));
        }
    }
    fn store(&mut self, label: &str, ty: Ty) {
        if ty.size() == 1 {
            self.e(&format!("ld ({label}),a"));
        } else {
            self.e(&format!("ld ({label}),hl"));
        }
    }
    fn push(&mut self, ty: Ty) {
        self.e(if ty.size() == 1 { "push af" } else { "push hl" });
    }

    /// Convert the value in A/HL from `from` to `to` (truncating if needed).
    fn convert(&mut self, from: Ty, to: Ty) {
        match (from.size(), to.size()) {
            (1, 2) => {
                self.e("ld l,a");
                if from.signed() {
                    self.e("rla");
                    self.e("sbc a,a");
                    self.e("ld h,a");
                } else {
                    self.e("ld h,0");
                }
            }
            (2, 1) => self.e("ld a,l"),
            _ => {}
        }
    }

    // ------------------------------------------------------- type queries
    fn const_eval(&self, e: &Expr) -> Option<i64> {
        match &e.kind {
            ExprKind::Int(v) => Some(*v),
            ExprKind::Name(n) => match self.lookup(n) {
                Some(Sym::Const(v)) => Some(v),
                _ => None,
            },
            ExprKind::Unary(UnOp::Neg, x) => self.const_eval(x).map(|v| -v),
            ExprKind::Unary(UnOp::BitNot, x) => self.const_eval(x).map(|v| !v),
            ExprKind::Binary(op, l, r) if !op.is_compare() && !op.is_logic() => {
                let a = self.const_eval(l)?;
                let b = self.const_eval(r)?;
                Some(match op {
                    BinOp::Add => a + b,
                    BinOp::Sub => a - b,
                    BinOp::Mul => a.checked_mul(b)?,
                    BinOp::Div if b != 0 => a / b,
                    BinOp::Mod if b != 0 => a % b,
                    BinOp::And => a & b,
                    BinOp::Or => a | b,
                    BinOp::Xor => a ^ b,
                    BinOp::Shl if (0..32).contains(&b) => a << b,
                    BinOp::Shr if (0..32).contains(&b) => a >> b,
                    _ => return None,
                })
            }
            ExprKind::Call(n, args) if (n == "image_width" || n == "image_height") && args.len() == 1 => {
                let (_, w, h) = self.image_arg(&args[0]).ok()?;
                Some(if n == "image_width" { w } else { h } as i64)
            }
            ExprKind::Call(n, args) if matches!(n.as_str(), "abs" | "min" | "max" | "clamp") => {
                let v: Option<Vec<i64>> = args.iter().map(|a| self.const_eval(a)).collect();
                let v = v?;
                match (n.as_str(), v.as_slice()) {
                    ("abs", [a]) => Some(a.abs()),
                    ("min", [a, b]) => Some(*a.min(b)),
                    ("max", [a, b]) => Some(*a.max(b)),
                    ("clamp", [x, lo, hi]) => Some(*x.max(lo).min(hi)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// The type an expression has on its own; `None` for untyped integer
    /// constants (which adapt to the context).
    fn natural(&self, e: &Expr) -> R<Option<Ty>> {
        Ok(match &e.kind {
            ExprKind::Int(_) => None,
            ExprKind::Fixed(_) => Some(Ty::Fixed),
            ExprKind::Name(n) if matches!(self.lookup(n), Some(Sym::FConst(_))) => Some(Ty::Fixed),
            ExprKind::Name(n) if matches!(self.lookup(n), Some(Sym::SConst(_))) => {
                return Err(Diag::new(e.pos, "a string cannot be used as a number here"))
            }
            ExprKind::Bool(_) => Some(Ty::Bool),
            ExprKind::Str(_) => return Err(Diag::new(e.pos, "a string cannot be used as a number here")),
            ExprKind::List(_) => {
                return Err(Diag::new(e.pos, "a `{...}` list is only allowed as an array initializer"));
            }
            ExprKind::Binary(BinOp::Concat, ..) => {
                return Err(Diag::new(e.pos, "a string cannot be used as a number here"))
            }
            ExprKind::Name(n) if matches!(self.lookup(n), Some(Sym::Const(_))) => None,
            ExprKind::Name(_) | ExprKind::Index(..) | ExprKind::Field(..) => match self.obj_of(e)? {
                Obj { lay: Lay::Prim(t), len: None, .. } => Some(t),
                Obj { len: Some(_), .. } => return Err(Diag::new(e.pos, "array used without an index")),
                Obj { lay: Lay::Str, .. } => return Err(Diag::new(e.pos, "a string cannot be used as a number here")),
                Obj { lay, .. } => {
                    return Err(Diag::new(e.pos, format!("a whole `{}` cannot be used as a value here", self.lay_name(lay))))
                }
            },
            ExprKind::MethodCall(obj, m, _) => {
                let key = self.method_key(obj, m, e.pos)?;
                match self.funcs[&key].ret {
                    Some(t) => Some(t),
                    None => return Err(Diag::new(e.pos, format!("method `{key}` does not return a value"))),
                }
            }
            ExprKind::Call(n, args) => {
                if let Some(t) = Ty::from_name(n) {
                    Some(t)
                } else if self.funcs.get(n).is_some_and(|f| f.ret_str.is_some()) {
                    return Err(Diag::new(e.pos, format!("`{n}` returns a string, not a number")));
                } else if let Some(f) = self.funcs.get(n) {
                    match f.ret {
                        Some(t) => Some(t),
                        None => return Err(Diag::new(e.pos, format!("function `{n}` does not return a value"))),
                    }
                } else if n == "key" {
                    Some(Ty::Bool)
                } else if n == "kempston" {
                    Some(Ty::Byte)
                } else if n == "frames" {
                    Some(Ty::Word)
                } else if n == "point" {
                    Some(Ty::Byte)
                } else if matches!(n.as_str(), "addr" | "random" | "image_width" | "image_height") {
                    Some(Ty::Word)
                } else if n == "any_key" {
                    Some(Ty::Bool)
                } else if matches!(n.as_str(), "abs" | "min" | "max" | "clamp") {
                    let mut t = None;
                    for a in args {
                        t = join(t, self.natural(a)?);
                    }
                    if t == Some(Ty::Bool) {
                        return Err(Diag::new(e.pos, format!("`{n}` of `bool`")));
                    }
                    t
                } else if n == "sqrt" {
                    match args.first().map(|a| self.natural(a)).transpose()?.flatten() {
                        Some(Ty::Fixed) => Some(Ty::Fixed),
                        _ => Some(Ty::Byte),
                    }
                } else if n == "sin" || n == "cos" {
                    Some(Ty::Fixed)
                } else if n == "atan2" {
                    Some(Ty::Byte)
                } else if n == "note_period" {
                    Some(Ty::Word)
                } else if n == "round" || n == "val" {
                    Some(Ty::SWord)
                } else if matches!(n.as_str(), "len" | "find" | "asc") {
                    Some(Ty::Byte)
                } else if matches!(n.as_str(), "sub" | "chr" | "str" | "upper" | "lower") {
                    return Err(Diag::new(e.pos, format!("`{n}` returns a string, not a number")));
                } else if INTRINSICS.contains(&n.as_str()) {
                    return Err(Diag::new(e.pos, format!("`{n}` does not return a value")));
                } else {
                    return Err(Diag::new(e.pos, format!("undefined function `{n}`")));
                }
            }
            ExprKind::Unary(UnOp::Not, _) => Some(Ty::Bool),
            ExprKind::Unary(_, x) => self.natural(x)?,
            ExprKind::Binary(op, l, r) => {
                if op.is_compare() || op.is_logic() {
                    Some(Ty::Bool)
                } else if matches!(op, BinOp::Shl | BinOp::Shr) {
                    // `1 << v`: a constant shifted by a variable has the
                    // constant's natural type (and widens to the destination)
                    match self.natural(l)? {
                        None if self.const_eval(e).is_none() => {
                            Some(literal_ty(self.const_eval(l).unwrap_or(0)))
                        }
                        t => t,
                    }
                } else {
                    let (a, b) = (self.natural(l)?, self.natural(r)?);
                    if a == Some(Ty::Bool) || b == Some(Ty::Bool) {
                        return Err(Diag::new(e.pos, "arithmetic on `bool` values"));
                    }
                    join(a, b)
                }
            }
        })
    }

    // --------------------------------------------------------- expressions
    /// Generate code leaving the value of `e`, converted to `want`, in A
    /// (8-bit / bool) or HL (16-bit).
    fn gen_expr(&mut self, e: &Expr, want: Ty) -> R<()> {
        let nat = self.natural(e)?;
        if want == Ty::Bool {
            if nat != Some(Ty::Bool) {
                return Err(Diag::new(e.pos, format!("expected `bool`, found `{}` (use a comparison, e.g. `x ~= 0`)", nat.map_or("integer", |t| t.name()))));
            }
            return self.gen_bool(e);
        }
        if nat == Some(Ty::Bool) {
            return Err(Diag::new(e.pos, format!("expected `{}`, found `bool`", want.name())));
        }
        if want == Ty::Fixed {
            return self.gen_fixed(e, nat);
        }
        if nat == Some(Ty::Fixed) {
            return Err(Diag::new(
                e.pos,
                format!("expected `{}`, found `fixed` (convert with `{}(x)` or `round(x)`)", want.name(), want.name()),
            ));
        }
        if let Some(v) = self.const_eval(e) {
            if !want.literal_fits(v) {
                return Err(Diag::new(e.pos, format!("constant {v} does not fit in `{}`", want.name())));
            }
            self.load_imm(v, want);
            return Ok(());
        }
        let Some(nat) = nat else {
            return Err(Diag::new(e.pos, "invalid constant expression (division by zero or overflow?)"));
        };
        if nat.size() > want.size() {
            return Err(Diag::new(
                e.pos,
                format!("cannot implicitly narrow `{}` to `{}` (use `{}(...)`)", nat.name(), want.name(), want.name()),
            ));
        }
        match &e.kind {
            ExprKind::Name(_) | ExprKind::Index(..) | ExprKind::Field(..) => {
                let ty = self.load_obj(e)?;
                self.convert(ty, want);
            }
            ExprKind::Call(n, args) => {
                let ty = self.call(n, args, e.pos)?.expect("checked by natural()");
                self.convert(ty, want);
            }
            ExprKind::MethodCall(obj, m, args) => {
                let ty = self.method_call(obj, m, args, e.pos)?.expect("checked by natural()");
                self.convert(ty, want);
            }
            ExprKind::Unary(op, x) => {
                let ty = Ty::int(want.size(), nat.signed() || want.signed());
                self.gen_expr(x, ty)?;
                match (op, ty.size()) {
                    (UnOp::Neg, 1) => self.e("neg"),
                    (UnOp::BitNot, 1) => self.e("cpl"),
                    (UnOp::Neg, _) => {
                        self.e("xor a");
                        self.e("sub l");
                        self.e("ld l,a");
                        self.e("sbc a,a");
                        self.e("sub h");
                        self.e("ld h,a");
                    }
                    (UnOp::BitNot, _) => {
                        self.e("ld a,h");
                        self.e("cpl");
                        self.e("ld h,a");
                        self.e("ld a,l");
                        self.e("cpl");
                        self.e("ld l,a");
                    }
                    (UnOp::Not, _) => unreachable!(),
                }
            }
            ExprKind::Binary(op, l, r) => {
                let ty = Ty::int(want.size(), nat.signed() || want.signed());
                self.gen_arith(*op, l, r, ty, e.pos)?;
            }
            ExprKind::Int(_) | ExprKind::Fixed(_) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::List(_) => {
                unreachable!()
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------- strings
    /// Is `e` a string-valued expression?
    fn is_str(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Str(_) | ExprKind::Binary(BinOp::Concat, ..) => true,
            ExprKind::Name(n) if matches!(self.lookup(n), Some(Sym::SConst(_))) => true,
            ExprKind::Name(_) | ExprKind::Index(..) | ExprKind::Field(..) => {
                matches!(self.obj_of(e), Ok(Obj { lay: Lay::Str, len: None, .. }))
            }
            ExprKind::Call(n, _) => {
                matches!(n.as_str(), "sub" | "chr" | "str" | "upper" | "lower")
                    || self.funcs.get(n).is_some_and(|f| f.ret_str.is_some())
            }
            ExprKind::MethodCall(obj, m, _) => self
                .method_key(obj, m, e.pos)
                .ok()
                .is_some_and(|k| self.funcs[&k].ret_str.is_some()),
            _ => false,
        }
    }

    /// A fresh string temporary for the current statement (per function,
    /// so calls made while evaluating do not clobber it).
    fn str_temp(&mut self) -> String {
        let l = format!("t_{}_{}", Self::safe(&self.cur_fn), self.str_tmp);
        self.str_tmp += 1;
        if self.str_tmps.insert(l.clone()) {
            self.alloc_bytes(&l, STR_SIZE);
        }
        l
    }

    fn pstring_label(&mut self, s: &str, pos: Pos) -> R<String> {
        // literals may be up to 255 characters (print, play, find ...);
        // copying one into a string variable keeps the first STR_CAP
        if s.len() > 255 {
            return Err(Diag::new(pos, "string literal longer than 255 characters"));
        }
        if let Some(l) = self.pstrings.get(s) {
            return Ok(l.clone());
        }
        let l = format!("ps_{}", self.pstrings.len());
        self.pstrings.insert(s.to_string(), l.clone());
        Ok(l)
    }

    /// HL = address of the (length-prefixed) string value of `e`.
    fn gen_str(&mut self, e: &Expr) -> R<()> {
        match &e.kind {
            ExprKind::Str(s) => {
                let l = self.pstring_label(s, e.pos)?;
                self.e(&format!("ld hl,{l}"));
            }
            ExprKind::Name(n) if matches!(self.lookup(n), Some(Sym::SConst(_))) => {
                let Some(Sym::SConst(t)) = self.lookup(n) else { unreachable!() };
                let l = self.pstring_label(&t, e.pos)?;
                self.e(&format!("ld hl,{l}"));
            }
            ExprKind::Name(_) | ExprKind::Index(..) | ExprKind::Field(..) if self.is_str(e) => self.place_hl(e)?,
            ExprKind::Binary(BinOp::Concat, ..) => {
                let t = self.str_temp();
                self.gen_str_into(&t, e)?;
                self.e(&format!("ld hl,{t}"));
            }
            ExprKind::Call(n, args) if matches!(n.as_str(), "sub" | "chr" | "str" | "upper" | "lower") => {
                let argc = |k: usize| -> R<()> {
                    if args.len() == k {
                        Ok(())
                    } else {
                        Err(Diag::new(e.pos, format!("`{n}` expects {k} argument(s), got {}", args.len())))
                    }
                };
                let t = self.str_temp();
                match n.as_str() {
                    "str" => {
                        argc(1)?;
                        self.gen_str_into(&t, &args[0])?;
                    }
                    "chr" => {
                        argc(1)?;
                        self.gen_expr(&args[0], Ty::Byte)?;
                        self.e(&format!("ld de,{t}"));
                        self.e("ld c,a");
                        self.e("xor a");
                        self.e("ld (de),a");
                        self.e("ld a,c");
                        self.e("call rt_sapp_chr");
                    }
                    "sub" => {
                        // sub(s, from, count), 0-based
                        argc(3)?;
                        self.gen_expr(&args[2], Ty::Byte)?;
                        self.e("push af");
                        self.gen_expr(&args[1], Ty::Byte)?;
                        self.e("push af");
                        self.gen_str(&args[0])?;
                        self.e("pop af");
                        self.e("ld b,a");
                        self.e("pop af");
                        self.e("ld c,a");
                        self.e(&format!("ld de,{t}"));
                        self.e("call rt_ssub");
                    }
                    _ => {
                        argc(1)?;
                        self.gen_str(&args[0])?;
                        self.e(&format!("ld de,{t}"));
                        self.e("call rt_scopy");
                        self.e(&format!("ld hl,{t}"));
                        self.e(if n == "upper" { "call rt_supper" } else { "call rt_slower" });
                    }
                }
                self.e(&format!("ld hl,{t}"));
            }
            ExprKind::Call(n, args) if self.funcs.get(n).is_some_and(|f| f.ret_str.is_some()) => {
                self.call(n, args, e.pos)?;
                let buf = self.funcs[n].ret_str.clone().unwrap();
                self.e(&format!("ld hl,{buf}"));
            }
            ExprKind::MethodCall(obj, m, args) if self.is_str(e) => {
                let key = self.method_key(obj, m, e.pos)?;
                self.method_call(obj, m, args, e.pos)?;
                let buf = self.funcs[&key].ret_str.clone().unwrap();
                self.e(&format!("ld hl,{buf}"));
            }
            _ => {
                let what = match self.natural(e) {
                    Ok(Some(t)) => format!("`{}`", t.name()),
                    _ => "a number".into(),
                };
                return Err(Diag::new(e.pos, format!("expected a string, found {what} (use `str(x)`)")));
            }
        }
        Ok(())
    }

    /// Build the string value of `e` (string or number) in buffer `t`.
    fn gen_str_into(&mut self, t: &str, e: &Expr) -> R<()> {
        match &e.kind {
            ExprKind::Binary(BinOp::Concat, l, r) => {
                self.gen_str_into(t, l)?;
                self.gen_append(t, r)
            }
            _ if self.is_str(e) => {
                self.gen_str(e)?;
                self.e(&format!("ld de,{t}"));
                self.e("call rt_scopy");
                Ok(())
            }
            _ => {
                self.e("xor a");
                self.e(&format!("ld ({t}),a"));
                self.gen_append(t, e)
            }
        }
    }

    /// Append the string or number `e` to buffer `t` (numbers are formatted
    /// like `print` does).
    fn gen_append(&mut self, t: &str, e: &Expr) -> R<()> {
        if let ExprKind::Binary(BinOp::Concat, l, r) = &e.kind {
            self.gen_append(t, l)?;
            return self.gen_append(t, r);
        }
        if self.is_str(e) {
            self.gen_str(e)?;
            self.e(&format!("ld de,{t}"));
            self.e("call rt_scat");
            return Ok(());
        }
        let routine = match self.natural(e)? {
            Some(Ty::Bool) => {
                self.gen_expr(e, Ty::Bool)?;
                "rt_print_bool"
            }
            Some(Ty::Fixed) => {
                self.gen_expr(e, Ty::Fixed)?;
                "rt_print_fixed"
            }
            Some(ty) => {
                self.gen_expr(e, ty)?;
                self.convert(ty, Ty::int(2, ty.signed()));
                if ty.signed() { "rt_print_s16" } else { "rt_print_u16" }
            }
            None => {
                let v = self.const_eval(e).ok_or_else(|| Diag::new(e.pos, "invalid constant expression"))?;
                self.gen_expr(e, if v < 0 { Ty::SWord } else { Ty::Word })?;
                if v < 0 { "rt_print_s16" } else { "rt_print_u16" }
            }
        };
        self.e(&format!("ld de,{t}"));
        self.e("call rt_fmt_on");
        self.e(&format!("call {routine}"));
        self.e("call rt_fmt_off");
        Ok(())
    }

    /// HL = first string, DE = second (the first is copied to a temporary
    /// when evaluating the second could overwrite it).
    fn gen_str_pair(&mut self, a: &Expr, b: &Expr) -> R<()> {
        self.gen_str(a)?;
        if contains_call(b) {
            let t = self.str_temp();
            self.e(&format!("ld de,{t}"));
            self.e("call rt_scopy");
            self.e(&format!("ld hl,{t}"));
        }
        self.e("push hl");
        self.gen_str(b)?;
        self.e("ex de,hl");
        self.e("pop hl");
        Ok(())
    }

    fn const_sym(&self, init: &Expr, pos: Pos) -> R<Sym> {
        if let ExprKind::Str(t) = &init.kind {
            return Ok(Sym::SConst(t.clone()));
        }
        if let Some(v) = self.const_eval(init) {
            return Ok(Sym::Const(v));
        }
        if self.natural(init).ok().flatten() == Some(Ty::Fixed) {
            if let Some(v) = self.fixed_const(init) {
                Self::check_fixed(v, init.pos)?;
                return Ok(Sym::FConst(v));
            }
        }
        Err(Diag::new(pos, "`const` needs a compile-time constant value"))
    }

    /// Value of a constant expression as raw 8.8 fixed point (integer
    /// constants are scaled by 256). Rounds toward zero like the runtime.
    fn fixed_const(&self, e: &Expr) -> Option<i64> {
        if let Some(v) = self.const_eval(e) {
            return v.checked_mul(256);
        }
        match &e.kind {
            ExprKind::Fixed(v) => Some(*v),
            ExprKind::Name(n) => match self.lookup(n) {
                Some(Sym::FConst(v)) => Some(v),
                _ => None,
            },
            ExprKind::Unary(UnOp::Neg, x) => self.fixed_const(x).map(|v| -v),
            ExprKind::Binary(op, l, r) => {
                let (a, b) = (self.fixed_const(l)?, self.fixed_const(r)?);
                match op {
                    BinOp::Add => Some(a + b),
                    BinOp::Sub => Some(a - b),
                    BinOp::Mul => Some(a * b / 256),
                    BinOp::Div if b != 0 => Some(a * 256 / b),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn check_fixed(v: i64, pos: Pos) -> R<()> {
        if (-32768..=32767).contains(&v) {
            Ok(())
        } else {
            Err(Diag::new(pos, format!("constant {} does not fit in `fixed` (-128 .. 127.99)", v as f64 / 256.0)))
        }
    }

    fn is_fixed(&self, e: &Expr) -> R<bool> {
        Ok(self.natural(e)? == Some(Ty::Fixed))
    }

    /// Generate a `fixed` value in HL.
    fn gen_fixed(&mut self, e: &Expr, nat: Option<Ty>) -> R<()> {
        if let Some(v) = self.fixed_const(e) {
            Self::check_fixed(v, e.pos)?;
            self.load_imm(v, Ty::Fixed);
            return Ok(());
        }
        match nat {
            Some(Ty::Fixed) => {}
            Some(t) => {
                return Err(Diag::new(e.pos, format!("expected `fixed`, found `{}` (convert with `fixed(x)`)", t.name())))
            }
            None => return Err(Diag::new(e.pos, "invalid constant expression (overflow?)")),
        }
        match &e.kind {
            ExprKind::Name(_) | ExprKind::Index(..) | ExprKind::Field(..) => {
                self.load_obj(e)?;
            }
            ExprKind::Call(n, args) => {
                self.call(n, args, e.pos)?;
            }
            ExprKind::MethodCall(obj, m, args) => {
                self.method_call(obj, m, args, e.pos)?;
            }
            ExprKind::Unary(UnOp::Neg, x) => {
                self.gen_expr(x, Ty::Fixed)?;
                self.e("call rt_neghl");
            }
            ExprKind::Unary(..) => return Err(Diag::new(e.pos, "bitwise `~` on a `fixed` value")),
            ExprKind::Binary(op, l, r) => self.gen_fixed_binary(*op, l, r, e.pos)?,
            _ => unreachable!(),
        }
        Ok(())
    }

    fn gen_fixed_binary(&mut self, op: BinOp, l: &Expr, r: &Expr, pos: Pos) -> R<()> {
        let (lf, rf) = (self.is_fixed(l)?, self.is_fixed(r)?);
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mod => self.gen_arith(op, l, r, Ty::Fixed, pos),
            BinOp::Shl | BinOp::Shr => self.gen_arith(op, l, r, Ty::Fixed, pos),
            BinOp::And | BinOp::Or | BinOp::Xor => Err(Diag::new(pos, "bitwise operators on `fixed` values")),
            BinOp::Mul | BinOp::Div if lf && rf => {
                if op == BinOp::Div && self.fixed_const(r) == Some(0) {
                    return Err(Diag::new(pos, "division by zero"));
                }
                let rhs = self.gen_operands(l, r, Ty::Fixed, Ty::Fixed)?;
                if let Rhs::Imm(v) = rhs {
                    self.e(&format!("ld de,{v}"));
                }
                self.e(if op == BinOp::Mul { "call rt_fmul" } else { "call rt_fdiv" });
                Ok(())
            }
            BinOp::Mul | BinOp::Div => {
                // fixed * integer, integer * fixed, fixed / integer: plain 16-bit arithmetic
                let (f, i) = if lf { (l, r) } else { (r, l) };
                if op == BinOp::Div && !lf {
                    return Err(Diag::new(pos, "integer / fixed: convert the left side with `fixed(x)`"));
                }
                match self.const_eval(i) {
                    Some(0) if op == BinOp::Div => return Err(Diag::new(pos, "division by zero")),
                    Some(v) if op == BinOp::Mul && v > 0 && v & (v - 1) == 0 => {
                        self.gen_expr(f, Ty::Fixed)?;
                        for _ in 0..v.trailing_zeros() {
                            self.e("add hl,hl");
                        }
                        return Ok(());
                    }
                    Some(v) => {
                        if !Ty::SWord.literal_fits(v) {
                            return Err(Diag::new(i.pos, format!("constant {v} does not fit in `sword`")));
                        }
                        self.gen_expr(f, Ty::Fixed)?;
                        self.e(&format!("ld de,{}", v & 0xFFFF));
                    }
                    None => {
                        let it = self.natural(i)?.unwrap_or(Ty::SWord);
                        let it = if it.size() == 1 { Ty::SWord } else { it };
                        self.gen_expr(i, it)?;
                        self.e("push hl");
                        self.gen_expr(f, Ty::Fixed)?;
                        self.e("pop de");
                    }
                }
                self.e(if op == BinOp::Mul { "call rt_mul16" } else { "call rt_divs16" });
                Ok(())
            }
            _ => unreachable!("comparisons are handled by gen_bool"),
        }
    }

    /// True if loading `e` cannot clobber B/C/D/E.
    fn is_simple(&self, e: &Expr) -> bool {
        if self.const_eval(e).is_some() {
            return true;
        }
        matches!(e.kind, ExprKind::Name(_) | ExprKind::Index(..) | ExprKind::Field(..))
            && !Self::is_hw(e, "nextreg")
            && !Self::is_hw(e, "port")
            && matches!(self.obj_of(e), Ok(Obj { lay: Lay::Prim(_), len: None, .. }))
            && matches!(self.static_addr(e), Ok(Some(_)))
    }

    /// Left operand in A/HL (as `lty`); right in B/DE (as `rty`) or immediate.
    fn gen_operands(&mut self, l: &Expr, r: &Expr, lty: Ty, rty: Ty) -> R<Rhs> {
        if rty == Ty::Fixed {
            if let Some(v) = self.fixed_const(r) {
                Self::check_fixed(v, r.pos)?;
                self.gen_expr(l, lty)?;
                return Ok(Rhs::Imm(v & 0xFFFF));
            }
        } else if let Some(v) = self.const_eval(r) {
            if !rty.literal_fits(v) {
                return Err(Diag::new(r.pos, format!("constant {v} does not fit in `{}`", rty.name())));
            }
            self.gen_expr(l, lty)?;
            return Ok(Rhs::Imm(mask(v, rty)));
        }
        self.gen_expr(r, rty)?;
        if self.is_simple(l) {
            self.e(if rty.size() == 1 { "ld b,a" } else { "ex de,hl" });
            self.gen_expr(l, lty)?;
        } else {
            self.push(rty);
            self.gen_expr(l, lty)?;
            self.e(if rty.size() == 1 { "pop bc" } else { "pop de" });
        }
        Ok(Rhs::Reg)
    }

    fn gen_arith(&mut self, op: BinOp, l: &Expr, r: &Expr, ty: Ty, pos: Pos) -> R<()> {
        // Z80N: byte * byte -> word with a single MUL D,E
        if op == BinOp::Mul && ty.size() == 2 {
            let (a, b) = (self.natural(l)?, self.natural(r)?);
            // only for leaf operands: a compound byte expression must still
            // be evaluated at the destination width (no 8-bit overflow)
            let small = |t: Option<Ty>, e: &Expr| match t {
                Some(t) => {
                    t == Ty::Byte
                        && matches!(e.kind, ExprKind::Name(_) | ExprKind::Index(..) | ExprKind::Field(..) | ExprKind::Call(..) | ExprKind::MethodCall(..))
                }
                None => matches!(self.const_eval(e), Some(0..=255)),
            };
            if small(a, l) && small(b, r) {
                let rhs = self.gen_operands(l, r, Ty::Byte, Ty::Byte)?;
                self.e("ld d,a");
                self.e(&format!("ld e,{}", self.rhs8(rhs)));
                self.e("mul d,e");
                self.e("ex de,hl");
                return Ok(());
            }
        }
        if matches!(op, BinOp::Shl | BinOp::Shr) {
            let rhs = self.gen_operands(l, r, ty, Ty::Byte)?;
            return self.gen_shift(op, ty, rhs);
        }
        if matches!(op, BinOp::Div | BinOp::Mod) && self.const_eval(r) == Some(0) {
            return Err(Diag::new(pos, "division by zero"));
        }
        let rhs = self.gen_operands(l, r, ty, ty)?;
        if ty.size() == 1 {
            let x = self.rhs8(rhs);
            match op {
                BinOp::Add => self.e(&format!("add a,{x}")),
                BinOp::Sub => self.e(&format!("sub {x}")),
                BinOp::And => self.e(&format!("and {x}")),
                BinOp::Or => self.e(&format!("or {x}")),
                BinOp::Xor => self.e(&format!("xor {x}")),
                BinOp::Mul => {
                    self.e("ld d,a");
                    self.e(&format!("ld e,{x}"));
                    self.e("mul d,e");
                    self.e("ld a,e");
                }
                BinOp::Div | BinOp::Mod if !ty.signed() && matches!(rhs, Rhs::Imm(_)) => {
                    let Rhs::Imm(v) = rhs else { unreachable!() };
                    if v & (v - 1) == 0 {
                        if op == BinOp::Div {
                            for _ in 0..v.trailing_zeros() {
                                self.e("srl a");
                            }
                        } else {
                            self.e(&format!("and {}", v - 1));
                        }
                    } else {
                        self.e("ld l,a");
                        self.e("ld h,0");
                        self.udiv_const(v, op == BinOp::Mod);
                        self.e("ld a,l");
                    }
                }
                BinOp::Div | BinOp::Mod => {
                    // widen both to 16 bits and use the 16-bit routine
                    self.convert(ty, Ty::int(2, ty.signed()));
                    match rhs {
                        Rhs::Imm(v) => {
                            let v = if ty.signed() && v >= 128 { v - 256 } else { v };
                            self.e(&format!("ld de,{}", v & 0xFFFF));
                        }
                        Rhs::Reg => {
                            self.e("ld a,b");
                            self.e("ld e,a");
                            if ty.signed() {
                                self.e("rla");
                                self.e("sbc a,a");
                                self.e("ld d,a");
                            } else {
                                self.e("ld d,0");
                            }
                        }
                    }
                    self.e(if ty.signed() { "call rt_divs16" } else { "call rt_divu16" });
                    self.e(if op == BinOp::Div { "ld a,l" } else { "ld a,e" });
                }
                _ => unreachable!(),
            }
        } else {
            if let Rhs::Imm(v) = rhs {
                match op {
                    BinOp::Add if v <= 3 => {
                        for _ in 0..v {
                            self.e("inc hl");
                        }
                        return Ok(());
                    }
                    BinOp::Sub if v <= 3 => {
                        for _ in 0..v {
                            self.e("dec hl");
                        }
                        return Ok(());
                    }
                    BinOp::Add => {
                        self.e(&format!("add hl,{v}"));
                        return Ok(());
                    }
                    BinOp::Sub => {
                        self.e(&format!("add hl,{}", (-v) & 0xFFFF));
                        return Ok(());
                    }
                    BinOp::And | BinOp::Or | BinOp::Xor => {
                        let m = match op {
                            BinOp::And => "and",
                            BinOp::Or => "or",
                            _ => "xor",
                        };
                        self.e("ld a,h");
                        self.e(&format!("{m} {}", v >> 8));
                        self.e("ld h,a");
                        self.e("ld a,l");
                        self.e(&format!("{m} {}", v & 0xFF));
                        self.e("ld l,a");
                        return Ok(());
                    }
                    BinOp::Mul if v != 0 => {
                        self.mul_const(v);
                        return Ok(());
                    }
                    BinOp::Div | BinOp::Mod if !ty.signed() => {
                        self.udiv_const(v, op == BinOp::Mod);
                        return Ok(());
                    }
                    _ => self.e(&format!("ld de,{v}")),
                }
            }
            match op {
                BinOp::Add => self.e("add hl,de"),
                BinOp::Sub => {
                    self.e("or a");
                    self.e("sbc hl,de");
                }
                BinOp::And | BinOp::Or | BinOp::Xor => {
                    let m = match op {
                        BinOp::And => "and",
                        BinOp::Or => "or",
                        _ => "xor",
                    };
                    self.e("ld a,h");
                    self.e(&format!("{m} d"));
                    self.e("ld h,a");
                    self.e("ld a,l");
                    self.e(&format!("{m} e"));
                    self.e("ld l,a");
                }
                BinOp::Mul => self.e("call rt_mul16"),
                BinOp::Div => self.e(if ty.signed() { "call rt_divs16" } else { "call rt_divu16" }),
                BinOp::Mod => {
                    self.e(if ty.signed() { "call rt_divs16" } else { "call rt_divu16" });
                    self.e("ex de,hl");
                }
                _ => unreachable!(),
            }
        }
        Ok(())
    }

    /// HL = HL * v (low 16 bits), v != 0
    fn mul_const(&mut self, v: i64) {
        let v = (v & 0xFFFF) as u64;
        let bits = 64 - v.leading_zeros();
        if v & (v - 1) == 0 {
            for _ in 0..v.trailing_zeros() {
                self.e("add hl,hl");
            }
        } else if bits - 1 + v.count_ones() - 1 <= 9 {
            // shift and add
            self.e("ld d,h");
            self.e("ld e,l");
            for b in (0..bits - 1).rev() {
                self.e("add hl,hl");
                if v >> b & 1 == 1 {
                    self.e("add hl,de");
                }
            }
        } else {
            self.e(&format!("ld de,{v}"));
            self.e("call rt_mul16");
        }
    }

    /// HL = HL / v or HL % v (unsigned 16-bit, constant v != 0) without a
    /// division loop: q = (x * M) >> (16 + s) with a magic multiplier M,
    /// chosen here and checked for every 16-bit x.
    fn udiv_const(&mut self, v: i64, modulo: bool) {
        let d = (v & 0xFFFF) as u64;
        if d == 1 {
            if modulo {
                self.e("ld hl,0");
            }
            return;
        }
        if d & (d - 1) == 0 {
            if modulo {
                let m = d - 1;
                self.e("ld a,h");
                self.e(&format!("and {}", m >> 8));
                self.e("ld h,a");
                self.e("ld a,l");
                self.e(&format!("and {}", m & 0xFF));
                self.e("ld l,a");
            } else {
                self.shr_hl(d.trailing_zeros());
            }
            return;
        }
        if modulo {
            self.e("push hl");
        }
        match udiv_magic(d) {
            Some((false, m, sh)) => {
                self.e(&format!("ld de,{m}"));
                self.e("call rt_mulhi16");
                self.shr_hl(sh);
            }
            Some((true, m, sh)) => {
                // 17-bit multiplier: t = hi(x * m); q = ((x - t) / 2 + t) >> (s - 1)
                self.e("push hl");
                self.e(&format!("ld de,{m}"));
                self.e("call rt_mulhi16");
                self.e("pop de");
                self.e("ex de,hl");
                self.e("or a");
                self.e("sbc hl,de");
                self.e("srl h");
                self.e("rr l");
                self.e("add hl,de");
                self.shr_hl(sh - 1);
            }
            None => {
                self.e(&format!("ld de,{d}"));
                self.e("call rt_divu16");
            }
        }
        if modulo {
            // x - q * d
            self.mul_const(d as i64);
            self.e("ex de,hl");
            self.e("pop hl");
            self.e("or a");
            self.e("sbc hl,de");
        }
    }

    /// HL >>= n (logical), Z80N barrel shifter
    fn shr_hl(&mut self, n: u32) {
        match n {
            0 => {}
            1 => {
                self.e("srl h");
                self.e("rr l");
            }
            8 => {
                self.e("ld l,h");
                self.e("ld h,0");
            }
            _ => {
                self.e("ex de,hl");
                self.e(&format!("ld b,{n}"));
                self.e("bsrl de,b");
                self.e("ex de,hl");
            }
        }
    }

    /// `ld a,<operand>` source for a byte constant or a plain byte variable
    fn trivial8(&self, e: &Expr) -> R<Option<String>> {
        if let Some(v) = self.const_eval(e) {
            if (0..=255).contains(&v) && self.natural(e)? != Some(Ty::Fixed) {
                return Ok(Some(v.to_string()));
            }
            return Ok(None);
        }
        if let ExprKind::Name(_) = &e.kind {
            if self.natural(e)? == Some(Ty::Byte) {
                return Ok(self.static_addr(e)?.map(|a| format!("({a})")));
            }
        }
        Ok(None)
    }

    fn rhs8(&self, r: Rhs) -> String {
        match r {
            Rhs::Reg => "b".into(),
            Rhs::Imm(v) => format!("{v}"),
        }
    }

    fn gen_shift(&mut self, op: BinOp, ty: Ty, rhs: Rhs) -> R<()> {
        let left = op == BinOp::Shl;
        let barrel = if left { "bsla" } else if ty.signed() { "bsra" } else { "bsrl" };
        match (ty.size(), rhs) {
            (1, Rhs::Imm(n)) => {
                if n >= 8 && (left || !ty.signed()) {
                    self.e("xor a");
                } else {
                    let ins = if left { "add a,a" } else if ty.signed() { "sra a" } else { "srl a" };
                    for _ in 0..n.min(7) {
                        self.e(ins);
                    }
                }
            }
            (1, Rhs::Reg) => {
                // Z80N barrel shifter on DE, count in B
                self.e("ld e,a");
                if ty.signed() {
                    self.e("rla");
                    self.e("sbc a,a");
                    self.e("ld d,a");
                } else {
                    self.e("ld d,0");
                }
                self.e(&format!("{barrel} de,b"));
                self.e("ld a,e");
            }
            (_, Rhs::Imm(n)) if n <= 2 => {
                for _ in 0..n {
                    if left {
                        self.e("add hl,hl");
                    } else {
                        self.e(if ty.signed() { "sra h" } else { "srl h" });
                        self.e("rr l");
                    }
                }
            }
            (_, Rhs::Imm(n)) if n >= 16 && (left || !ty.signed()) => self.e("ld hl,0"),
            (_, rhs) => {
                if let Rhs::Imm(n) = rhs {
                    self.e(&format!("ld b,{}", n.min(15)));
                }
                self.e("ex de,hl");
                self.e(&format!("{barrel} de,b"));
                self.e("ex de,hl");
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------- booleans
    /// Compare l and r, leaving the result in the flags as if `left - right`
    /// were computed unsigned (signed operands are bias-adjusted).
    fn gen_compare_flags(&mut self, l: &Expr, r: &Expr, pos: Pos) -> R<()> {
        if self.is_str(l) || self.is_str(r) {
            if !(self.is_str(l) && self.is_str(r)) {
                return Err(Diag::new(pos, "cannot compare a string with a number (use `str(x)` or `val(s)`)"));
            }
            self.gen_str_pair(l, r)?;
            self.e("call rt_scmp");
            return Ok(());
        }
        let (a, b) = (self.natural(l)?, self.natural(r)?);
        let ty = match join(a, b) {
            Some(t) => t,
            None => join(
                Some(literal_ty(self.const_eval(l).unwrap_or(0))),
                Some(literal_ty(self.const_eval(r).unwrap_or(0))),
            )
            .unwrap(),
        };
        if (a == Some(Ty::Bool)) != (b == Some(Ty::Bool)) {
            return Err(Diag::new(pos, "cannot compare `bool` with a number"));
        }
        let rhs = self.gen_operands(l, r, ty, ty)?;
        if ty.size() == 1 {
            match (rhs, ty.signed()) {
                (Rhs::Imm(v), false) => self.e(&format!("cp {v}")),
                (Rhs::Reg, false) => self.e("cp b"),
                (Rhs::Imm(v), true) => {
                    self.e("xor $80");
                    self.e(&format!("cp {}", v ^ 0x80));
                }
                (Rhs::Reg, true) => {
                    self.e("xor $80");
                    self.e("ld c,a");
                    self.e("ld a,b");
                    self.e("xor $80");
                    self.e("ld b,a");
                    self.e("ld a,c");
                    self.e("cp b");
                }
            }
        } else {
            if let Rhs::Imm(v) = rhs {
                let v = if ty.signed() { v ^ 0x8000 } else { v };
                self.e(&format!("ld de,{v}"));
            } else if ty.signed() {
                self.e("ld a,d");
                self.e("xor $80");
                self.e("ld d,a");
            }
            if ty.signed() {
                self.e("ld a,h");
                self.e("xor $80");
                self.e("ld h,a");
            }
            self.e("or a");
            self.e("sbc hl,de");
        }
        Ok(())
    }

    /// Materialize a boolean expression as 0/1 in A.
    fn gen_bool(&mut self, e: &Expr) -> R<()> {
        match &e.kind {
            ExprKind::Bool(b) => self.e(if *b { "ld a,1" } else { "xor a" }),
            ExprKind::Unary(UnOp::Not, x) => {
                self.gen_bool(x)?;
                self.e("xor 1");
            }
            ExprKind::Binary(op, l, r) if op.is_logic() => {
                let done = self.new_label();
                self.gen_bool(l)?;
                self.e("or a");
                self.e(&format!("jp {},{done}", if *op == BinOp::LAnd { "z" } else { "nz" }));
                self.gen_bool(r)?;
                self.label(&done);
            }
            ExprKind::Binary(op, l, r) if op.is_compare() => {
                self.gen_compare_flags(l, r, e.pos)?;
                let done = self.new_label();
                match op {
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Ge => {
                        let cc = match op {
                            BinOp::Eq => "z",
                            BinOp::Ne => "nz",
                            BinOp::Lt => "c",
                            _ => "nc",
                        };
                        self.e("ld a,1");
                        self.e(&format!("jp {cc},{done}"));
                        self.e("xor a");
                    }
                    BinOp::Gt => {
                        self.e("ld a,0");
                        self.e(&format!("jp c,{done}"));
                        self.e(&format!("jp z,{done}"));
                        self.e("inc a");
                    }
                    _ => {
                        // Le
                        self.e("ld a,1");
                        self.e(&format!("jp c,{done}"));
                        self.e(&format!("jp z,{done}"));
                        self.e("xor a");
                    }
                }
                self.label(&done);
            }
            ExprKind::Name(_) | ExprKind::Index(..) | ExprKind::Field(..) => {
                self.load_obj(e)?;
            }
            ExprKind::Call(n, args) => {
                self.call(n, args, e.pos)?;
            }
            ExprKind::MethodCall(obj, m, args) => {
                self.method_call(obj, m, args, e.pos)?;
            }
            _ => return Err(Diag::new(e.pos, "expected a boolean expression")),
        }
        Ok(())
    }

    /// Jump to `target` if the boolean `cond` is false.
    fn branch_false(&mut self, cond: &Expr, target: &str) -> R<()> {
        if self.natural(cond)? != Some(Ty::Bool) {
            return Err(Diag::new(cond.pos, "condition must be `bool` (use a comparison, e.g. `x ~= 0`)"));
        }
        match &cond.kind {
            ExprKind::Bool(true) => {}
            ExprKind::Bool(false) => self.e(&format!("jp {target}")),
            ExprKind::Binary(BinOp::LAnd, l, r) => {
                self.branch_false(l, target)?;
                self.branch_false(r, target)?;
            }
            ExprKind::Binary(op, l, r) if op.is_compare() => {
                self.gen_compare_flags(l, r, cond.pos)?;
                match op {
                    BinOp::Eq => self.e(&format!("jp nz,{target}")),
                    BinOp::Ne => self.e(&format!("jp z,{target}")),
                    BinOp::Lt => self.e(&format!("jp nc,{target}")),
                    BinOp::Ge => self.e(&format!("jp c,{target}")),
                    BinOp::Gt => {
                        self.e(&format!("jp c,{target}"));
                        self.e(&format!("jp z,{target}"));
                    }
                    _ => {
                        // Le: false iff nc and nz
                        let ok = self.new_label();
                        self.e(&format!("jp c,{ok}"));
                        self.e(&format!("jp nz,{target}"));
                        self.label(&ok);
                    }
                }
            }
            _ => {
                self.gen_bool(cond)?;
                self.e("or a");
                self.e(&format!("jp z,{target}"));
            }
        }
        Ok(())
    }

    // ---------------------------------------------------------------- calls
    fn call(&mut self, name: &str, args: &[Expr], pos: Pos) -> R<Option<Ty>> {
        let argc = |n: usize| -> R<()> {
            if args.len() != n {
                Err(Diag::new(pos, format!("`{name}` expects {n} argument(s), got {}", args.len())))
            } else {
                Ok(())
            }
        };
        if let Some(target) = Ty::from_name(name) {
            argc(1)?;
            let a = &args[0];
            if target == Ty::Fixed {
                if let Some(v) = self.fixed_const(a) {
                    Self::check_fixed(v, a.pos)?;
                    self.load_imm(v, Ty::Fixed);
                    return Ok(Some(Ty::Fixed));
                }
                let from = self.natural(a)?.unwrap();
                if from == Ty::Bool {
                    return Err(Diag::new(pos, "cannot convert `bool` to `fixed`"));
                }
                if from == Ty::Fixed {
                    self.gen_expr(a, Ty::Fixed)?;
                } else {
                    self.gen_expr(a, from)?;
                    if from.size() == 1 {
                        self.e("ld h,a");
                    } else {
                        self.e("ld h,l");
                    }
                    self.e("ld l,0");
                }
                return Ok(Some(Ty::Fixed));
            }
            if self.natural(a)? == Some(Ty::Fixed) && target != Ty::Bool {
                // fixed -> integer: floor (arithmetic shift right by 8)
                self.gen_expr(a, Ty::Fixed)?;
                self.e("ld l,h");
                self.e("ld a,h");
                self.e("rla");
                self.e("sbc a,a");
                self.e("ld h,a");
                self.convert(Ty::SWord, target);
                return Ok(Some(target));
            }
            if let Some(v) = self.const_eval(a) {
                if target == Ty::Bool {
                    self.load_imm((v != 0) as i64, Ty::Byte);
                } else {
                    self.load_imm(v, target);
                }
                return Ok(Some(target));
            }
            let from = self.natural(a)?.unwrap();
            if target == Ty::Bool {
                return Err(Diag::new(pos, "use a comparison instead of `bool(...)`"));
            }
            if from == Ty::Bool {
                self.gen_expr(a, Ty::Bool)?;
                self.convert(Ty::Byte, target);
            } else {
                self.gen_expr(a, from)?;
                self.convert(from, target);
            }
            return Ok(Some(target));
        }
        match name {
            "print" | "write" => {
                let is_print = name == "print";
                for (i, a) in args.iter().enumerate() {
                    if i > 0 && is_print {
                        self.e("call rt_space");
                    }
                    if let ExprKind::Str(s) = &a.kind {
                        let l = self.string_label(s);
                        self.e(&format!("ld hl,{l}"));
                        self.e("call rt_print_str");
                        continue;
                    }
                    if self.is_str(a) {
                        self.gen_str(a)?;
                        self.e("call rt_print_pstr");
                        continue;
                    }
                    match self.natural(a)? {
                        Some(Ty::Bool) => {
                            self.gen_expr(a, Ty::Bool)?;
                            self.e("call rt_print_bool");
                        }
                        Some(Ty::Fixed) => {
                            self.gen_expr(a, Ty::Fixed)?;
                            self.e("call rt_print_fixed");
                        }
                        // evaluated at the expression's own type, then widened
                        Some(t) => {
                            self.gen_expr(a, t)?;
                            let w = Ty::int(2, t.signed());
                            self.convert(t, w);
                            self.e(if t.signed() { "call rt_print_s16" } else { "call rt_print_u16" });
                        }
                        None => {
                            let v = self.const_eval(a).unwrap();
                            let t = if v < 0 { Ty::SWord } else { Ty::Word };
                            self.gen_expr(a, t)?;
                            self.e(if v < 0 { "call rt_print_s16" } else { "call rt_print_u16" });
                        }
                    }
                }
                if is_print {
                    self.e("call rt_newline");
                }
                return Ok(None);
            }
            "vsync" => {
                argc(0)?;
                self.e("call rt_vsync");
                return Ok(None);
            }
            "ay_write" => {
                // ay_write(chip 0..2, register 0..15, value): atomic w.r.t. interrupts
                argc(3)?;
                self.gen_expr(&args[2], Ty::Byte)?;
                self.e("push af");
                self.gen_expr(&args[1], Ty::Byte)?;
                self.e("push af");
                self.gen_expr(&args[0], Ty::Byte)?;
                self.e("ld d,a");
                self.e("pop af");
                self.e("pop hl");
                self.e("ld e,h");
                self.e("call rt_ay_write");
                return Ok(None);
            }
            "note_period" => {
                // AY tone period of note n (n = octave * 12 + semitone, A4 = 57)
                argc(1)?;
                self.uses_notes = true;
                self.gen_expr(&args[0], Ty::Byte)?;
                self.e("call rt_note_period");
                return Ok(Some(Ty::Word));
            }
            "len" => {
                argc(1)?;
                self.gen_str(&args[0])?;
                self.e("ld a,(hl)");
                return Ok(Some(Ty::Byte));
            }
            "asc" => {
                argc(2)?;
                self.gen_expr(&args[1], Ty::Byte)?;
                self.e("push af");
                self.gen_str(&args[0])?;
                self.e("pop af");
                self.e("call rt_sasc");
                return Ok(Some(Ty::Byte));
            }
            "find" => {
                argc(2)?;
                self.gen_str_pair(&args[0], &args[1])?;
                self.e("call rt_sfind");
                return Ok(Some(Ty::Byte));
            }
            "val" => {
                argc(1)?;
                self.gen_str(&args[0])?;
                self.e("call rt_sval");
                return Ok(Some(Ty::SWord));
            }
            "sub" | "chr" | "str" | "upper" | "lower" => {
                return Err(Diag::new(pos, format!("the string returned by `{name}` is not used")));
            }
            "abs" => {
                argc(1)?;
                let Some(ty) = self.natural(&args[0])? else {
                    return Err(Diag::new(pos, "invalid constant expression"));
                };
                self.gen_expr(&args[0], ty)?;
                if ty.signed() {
                    if ty.size() == 1 {
                        let l = self.new_label();
                        self.e("bit 7,a");
                        self.e(&format!("jr z,{l}"));
                        self.e("neg");
                        self.label(&l);
                    } else {
                        self.e("bit 7,h");
                        self.e("call nz,rt_neghl");
                    }
                }
                return Ok(Some(ty));
            }
            "min" | "max" => {
                argc(2)?;
                let Some(ty) = join(self.natural(&args[0])?, self.natural(&args[1])?) else {
                    return Err(Diag::new(pos, "invalid constant expression"));
                };
                let wide = if ty == Ty::Fixed { ty } else { Ty::int(2, ty.signed()) };
                let rhs = self.gen_operands(&args[0], &args[1], wide, wide)?;
                if let Rhs::Imm(v) = rhs {
                    self.e(&format!("ld de,{v}"));
                }
                let sign = if ty.signed() { "s" } else { "u" };
                self.e(&format!("call rt_{name}_{sign}16"));
                if ty.size() == 1 {
                    self.e("ld a,l");
                }
                return Ok(Some(ty));
            }
            "clamp" => {
                argc(3)?;
                let inner = Expr { kind: ExprKind::Call("max".into(), vec![args[0].clone(), args[1].clone()]), pos };
                return self.call("min", &[inner, args[2].clone()], pos);
            }
            "sqrt" => {
                argc(1)?;
                if self.is_fixed(&args[0])? {
                    // sqrt(x) * 256 = sqrt(raw * 256): 24-bit integer square root
                    self.gen_expr(&args[0], Ty::Fixed)?;
                    let l = self.new_label();
                    self.e("bit 7,h");
                    self.e(&format!("jr z,{l}"));
                    self.e("ld hl,0");
                    self.label(&l);
                    self.e("ld a,h");
                    self.e("ld h,l");
                    self.e("ld l,0");
                    self.e("call rt_isqrt24");
                    self.e("ex de,hl");
                    return Ok(Some(Ty::Fixed));
                }
                self.gen_expr(&args[0], Ty::Word)?;
                self.e("xor a");
                self.e("call rt_isqrt24");
                self.e("ld a,e");
                return Ok(Some(Ty::Byte));
            }
            "sin" | "cos" => {
                // angle: byte, 256 units per turn
                argc(1)?;
                self.uses_trig = true;
                self.gen_expr(&args[0], Ty::Byte)?;
                if name == "cos" {
                    self.e("add a,64");
                }
                self.e("ld l,a");
                self.e("ld h,0");
                self.e("add hl,hl");
                self.e("ld de,rt_sin_tab");
                self.e("add hl,de");
                self.e("ld a,(hl)");
                self.e("inc hl");
                self.e("ld h,(hl)");
                self.e("ld l,a");
                return Ok(Some(Ty::Fixed));
            }
            "atan2" => {
                argc(2)?;
                self.uses_trig = true;
                let ty = match join(self.natural(&args[0])?, self.natural(&args[1])?) {
                    Some(Ty::Fixed) => Ty::Fixed,
                    _ => Ty::SWord,
                };
                self.gen_expr(&args[1], ty)?;
                self.e("push hl");
                self.gen_expr(&args[0], ty)?;
                self.e("pop de");
                self.e("call rt_atan2");
                return Ok(Some(Ty::Byte));
            }
            "round" => {
                argc(1)?;
                self.gen_expr(&args[0], Ty::Fixed)?;
                self.e("ld de,128");
                self.e("add hl,de");
                self.e("ld l,h");
                self.e("ld a,h");
                self.e("rla");
                self.e("sbc a,a");
                self.e("ld h,a");
                return Ok(Some(Ty::SWord));
            }
            "cpu_speed" => {
                argc(1)?;
                self.gen_expr(&args[0], Ty::Byte)?;
                self.e("and 3");
                self.e("nextreg $07,a");
                return Ok(None);
            }
            "random" => {
                // random(n): 0..n-1 (n = 0: full 16-bit value)
                argc(1)?;
                self.gen_expr(&args[0], Ty::Word)?;
                self.e("call rt_random");
                return Ok(Some(Ty::Word));
            }
            "seed" => {
                argc(1)?;
                self.gen_expr(&args[0], Ty::Word)?;
                self.e("call rt_seed");
                return Ok(None);
            }
            "pause" => {
                argc(1)?;
                self.gen_expr(&args[0], Ty::Word)?;
                self.e("call rt_pause");
                return Ok(None);
            }
            "any_key" => {
                argc(0)?;
                self.e("call rt_any_key");
                return Ok(Some(Ty::Bool));
            }
            "wait_key" => {
                argc(0)?;
                self.e("call rt_wait_key");
                return Ok(None);
            }
            "border" => {
                // Next: the ULA is off; the "border" is the fallback colour
                argc(1)?;
                self.gen_expr(&args[0], Ty::Byte)?;
                self.e("call rt_border");
                return Ok(None);
            }
            "plot" | "point" | "hline" => {
                // D = y, E = x, A = colour, BC = length (hline)
                let n = if name == "point" { 2 } else if name == "plot" { 3 } else { 4 };
                argc(n)?;
                if name != "hline" {
                    // constants and plain byte variables load straight into registers
                    let colour = if name == "plot" { self.trivial8(&args[2])? } else { None };
                    if name == "plot" && colour.is_none() {
                        self.gen_expr(&args[2], Ty::Byte)?;
                        self.e("push af");
                    }
                    if let Some(x) = self.trivial8(&args[0])? {
                        self.gen_expr(&args[1], Ty::Byte)?;
                        self.e("ld d,a");
                        self.e(&format!("ld a,{x}"));
                        self.e("ld e,a");
                    } else {
                        self.gen_expr(&args[1], Ty::Byte)?;
                        self.e("push af");
                        self.gen_expr(&args[0], Ty::Byte)?;
                        self.e("ld e,a");
                        self.e("pop af");
                        self.e("ld d,a");
                    }
                    if name == "point" {
                        self.e("call rt_point");
                        return Ok(Some(Ty::Byte));
                    }
                    match colour {
                        Some(c) => self.e(&format!("ld a,{c}")),
                        None => self.e("pop af"),
                    }
                    self.e("call rt_plot");
                    return Ok(None);
                }
                if name == "hline" {
                    self.gen_expr(&args[3], Ty::Byte)?;
                    self.e("push af");
                    self.gen_expr(&args[2], Ty::Word)?;
                    self.e("push hl");
                }
                self.gen_expr(&args[1], Ty::Byte)?;
                self.e("push af");
                self.gen_expr(&args[0], Ty::Byte)?;
                self.e("ld e,a");
                self.e("pop af");
                self.e("ld d,a");
                match name {
                    "plot" => {
                        self.e("pop af");
                        self.e("call rt_plot");
                        return Ok(None);
                    }
                    "hline" => {
                        self.e("pop bc");
                        self.e("pop af");
                        self.e("call rt_hline");
                        return Ok(None);
                    }
                    _ => {
                        self.e("call rt_point");
                        return Ok(Some(Ty::Byte));
                    }
                }
            }
            "addr" => {
                argc(1)?;
                self.obj_of(&args[0])?;
                if Self::is_hw(&args[0], "nextreg") || Self::is_hw(&args[0], "port") {
                    return Err(Diag::new(args[0].pos, "registers and ports have no memory address"));
                }
                self.place_hl(&args[0])?;
                return Ok(Some(Ty::Word));
            }
            "dma_copy" => {
                // HL = source, DE = destination, BC = length
                argc(3)?;
                self.gen_expr(&args[2], Ty::Word)?;
                self.e("push hl");
                self.gen_expr(&args[1], Ty::Word)?;
                self.e("push hl");
                self.gen_expr(&args[0], Ty::Word)?;
                self.e("pop de");
                self.e("pop bc");
                self.e("call rt_dma_copy");
                return Ok(None);
            }
            "dma_fill" => {
                // DE = destination, BC = length, A = value
                argc(3)?;
                self.gen_expr(&args[2], Ty::Byte)?;
                self.e("push af");
                self.gen_expr(&args[1], Ty::Word)?;
                self.e("push hl");
                self.gen_expr(&args[0], Ty::Word)?;
                self.e("ex de,hl");
                self.e("pop bc");
                self.e("pop af");
                self.e("call rt_dma_fill");
                return Ok(None);
            }
            "layer2_flip" => {
                argc(0)?;
                self.e("call rt_l2_flip");
                return Ok(None);
            }
            "layer2_clear" => {
                argc(1)?;
                self.gen_expr(&args[0], Ty::Byte)?;
                self.e("call rt_l2_clear");
                return Ok(None);
            }
            "key" => {
                argc(1)?;
                if let Some(k) = self.const_eval(&args[0]) {
                    if !(0..64).contains(&k) || k % 8 > 4 {
                        return Err(Diag::new(args[0].pos, "invalid key code (use the KEY_ constants)"));
                    }
                    let row = 0xFF ^ (1 << (k / 8));
                    self.e(&format!("ld a,{row}"));
                    self.e("in a,($fe)");
                    self.e("cpl");
                    self.e(&format!("and {}", 1 << (k % 8)));
                    let done = self.new_label();
                    self.e(&format!("jp z,{done}"));
                    self.e("ld a,1");
                    self.label(&done);
                } else {
                    self.gen_expr(&args[0], Ty::Byte)?;
                    self.e("call rt_key");
                }
                return Ok(Some(Ty::Bool));
            }
            "sprite_load" => {
                argc(2)?;
                let found = match &args[1].kind {
                    ExprKind::Name(n) => self.lookup(n),
                    _ => None,
                };
                let Some(Sym::Asset { label, patterns: Some(count), .. }) = found else {
                    return Err(Diag::new(args[1].pos, "second argument must be a sprite asset (.png / .spr)"));
                };
                if let Some(first) = self.const_eval(&args[0]) {
                    if first < 0 || first + count as i64 > 64 {
                        return Err(Diag::new(
                            args[0].pos,
                            format!("patterns {first}..{} do not fit the 64 pattern slots", first + count as i64 - 1),
                        ));
                    }
                }
                self.gen_expr(&args[0], Ty::Byte)?;
                self.e(&format!("ld hl,{label}"));
                self.e(&format!("ld d,{count}"));
                self.e("call rt_sprite_upload");
                return Ok(None);
            }
            "draw_image" => {
                argc(3)?;
                let (label, _, _) = self.image_arg(&args[0])?;
                self.gen_expr(&args[1], Ty::SWord)?;
                self.e("ld (rt_img_x),hl");
                self.gen_expr(&args[2], Ty::SWord)?;
                self.e("ld (rt_img_y),hl");
                self.e(&format!("ld hl,{label}"));
                self.e("call rt_draw_image");
                return Ok(None);
            }
            "image_width" | "image_height" => {
                argc(1)?;
                let (_, w, h) = self.image_arg(&args[0])?;
                let v = if name == "image_width" { w } else { h };
                self.e(&format!("ld hl,{v}"));
                return Ok(Some(Ty::Word));
            }
            "kempston" => {
                argc(0)?;
                self.e("in a,($1f)");
                return Ok(Some(Ty::Byte));
            }
            "frames" => {
                argc(0)?;
                self.e("ld hl,(rt_frames)");
                return Ok(Some(Ty::Word));
            }
            "cls" => {
                argc(0)?;
                self.e("call rt_cls");
                return Ok(None);
            }
            "at" => {
                argc(2)?;
                self.gen_expr(&args[0], Ty::Byte)?;
                self.e("call rt_set_row");
                self.gen_expr(&args[1], Ty::Byte)?;
                self.e("call rt_set_col");
                return Ok(None);
            }
            "ink" => {
                argc(1)?;
                self.gen_expr(&args[0], Ty::Byte)?;
                self.e("call rt_set_ink");
                return Ok(None);
            }
            "memory" | "nextreg" | "port" => {
                return Err(Diag::new(pos, format!("`{name}` is accessed with `{name}[...]`, not called")))
            }
            _ => {}
        }
        if !self.funcs.contains_key(name) {
            return Err(Diag::new(pos, format!("undefined function `{name}`")));
        }
        let args: Vec<&Expr> = args.iter().collect();
        self.call_fn(name, &args, pos)
    }

    fn method_key(&self, obj: &Expr, m: &str, pos: Pos) -> R<String> {
        let o = self.obj_of(obj)?;
        let (Lay::Struct(id), None) = (o.lay, o.len) else {
            return Err(Diag::new(pos, format!("`.{m}()` called on something that is not a struct/class value")));
        };
        self.structs[id]
            .methods
            .get(m)
            .cloned()
            .ok_or_else(|| Diag::new(pos, format!("`{}` has no method `{m}`", self.structs[id].name)))
    }

    fn method_call(&mut self, obj: &Expr, m: &str, args: &[Expr], pos: Pos) -> R<Option<Ty>> {
        let key = self.method_key(obj, m, pos)?;
        let mut all: Vec<&Expr> = vec![obj];
        all.extend(args.iter());
        self.call_fn(&key, &all, pos)
    }

    /// Evaluate an argument for a parameter of layout `lay` into A/HL.
    /// Struct arguments are passed by reference (address in HL).
    fn gen_arg(&mut self, a: &Expr, lay: Lay) -> R<Ty> {
        match lay {
            Lay::Prim(t) => {
                self.gen_expr(a, t)?;
                Ok(t)
            }
            Lay::Str => {
                self.gen_str(a)?;
                Ok(Ty::Word)
            }
            Lay::Struct(id) => {
                let o = self.obj_of(a)?;
                if o.lay != lay || o.len.is_some() {
                    return Err(Diag::new(a.pos, format!("expected a `{}` variable", self.structs[id].name)));
                }
                self.place_hl(a)?;
                Ok(Ty::Word)
            }
        }
    }

    fn call_fn(&mut self, key: &str, args: &[&Expr], pos: Pos) -> R<Option<Ty>> {
        let sig = self.funcs[key].clone();
        let shown = if sig.params.first().is_some_and(|p| p.0 == "self") { sig.params.len() - 1 } else { sig.params.len() };
        if args.len() != sig.params.len() {
            return Err(Diag::new(
                pos,
                format!("`{key}` expects {shown} argument(s), got {}", args.len() + shown - sig.params.len()),
            ));
        }
        let caller = self.cur_fn.clone();
        self.calls.entry(caller).or_insert((pos, HashSet::new())).1.insert(key.to_string());
        // If no argument contains a call, store each one directly into the
        // callee's frame; otherwise evaluate all onto the stack first.
        let has_call = args.iter().any(|a| contains_call(a));
        if !has_call {
            for (a, (_, lay, slot)) in args.iter().zip(&sig.params) {
                let t = self.gen_arg(a, *lay)?;
                self.store(slot, t);
            }
        } else {
            let mut tys = Vec::new();
            for (a, (_, lay, _)) in args.iter().zip(&sig.params) {
                let t = self.gen_arg(a, *lay)?;
                self.push(t);
                tys.push(t);
            }
            for ((_, _, slot), t) in sig.params.iter().zip(tys).rev() {
                self.e(if t.size() == 1 { "pop af" } else { "pop hl" });
                self.store(slot, t);
            }
        }
        self.e(&format!("call {}", sig.label));
        Ok(sig.ret)
    }

    fn string_label(&mut self, s: &str) -> String {
        if let Some(l) = self.strings.get(s) {
            return l.clone();
        }
        let l = format!("s_{}", self.strings.len());
        self.strings.insert(s.to_string(), l.clone());
        l
    }

    // --------------------------------------------------------------- output
    /// Output with automatic code banking: when the resident part would not
    /// fit below $FD00, the largest functions move into 8K code pages.
    fn finish_banked(&mut self, opts: &Options) -> String {
        let bankable: Vec<usize> =
            (0..self.fn_chunks.len()).filter(|&i| !self.fn_chunks[i].0.starts_with("interrupt")).collect();
        if opts.bank_all {
            let all: HashSet<String> = bankable.iter().map(|&i| self.fn_chunks[i].0.clone()).collect();
            return self.finish(opts, &all);
        }
        let mut banked = HashSet::new();
        let out = self.finish(opts, &banked);
        let mut size = resident_size(&out);
        if size <= RESIDENT_LIMIT {
            return out;
        }
        let mut cands: Vec<(usize, String)> = bankable
            .iter()
            .map(|&i| (self.chunk_size(i, opts), self.fn_chunks[i].0.clone()))
            .filter(|(sz, _)| *sz <= PAGE_CAP)
            .collect();
        cands.sort_by(|a, b| b.0.cmp(&a.0));
        for (sz, key) in cands {
            if size <= RESIDENT_LIMIT {
                break;
            }
            banked.insert(key);
            size = size.saturating_sub(sz) + STUB_SIZE;
        }
        self.finish(opts, &banked)
    }

    /// The code of function chunk `i` after the peephole pass.
    fn chunk_code(&self, i: usize, opts: &Options) -> String {
        let text = &self.fn_chunks[i].2;
        if opts.optimize { peephole::optimize(text) } else { text.clone() }
    }

    fn chunk_size(&self, i: usize, opts: &Options) -> usize {
        asm_size(&self.chunk_code(i, opts))
    }

    /// Resident code (main program and resident functions), the stubs of the
    /// banked functions and the banked code pages.
    fn code_layout(&self, opts: &Options, banked: &HashSet<String>) -> (String, String, String) {
        let mut resident = if opts.optimize { peephole::optimize(&self.code) } else { self.code.clone() };
        let mut stubs = String::new();
        let mut pages_text = String::new();
        // first fit, largest first; with bank_all one function per page
        let mut items: Vec<(usize, usize)> = Vec::new();
        for i in 0..self.fn_chunks.len() {
            if banked.contains(&self.fn_chunks[i].0) {
                items.push((self.chunk_size(i, opts), i));
            } else {
                resident.push_str(&self.chunk_code(i, opts));
            }
        }
        if items.is_empty() {
            return (resident, stubs, pages_text);
        }
        items.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut pages: Vec<(usize, Vec<usize>)> = Vec::new();
        for (sz, i) in items {
            match pages.iter_mut().find(|p| !opts.bank_all && p.0 + sz <= PAGE_CAP) {
                Some(p) => {
                    p.0 += sz;
                    p.1.push(i);
                }
                None => pages.push((sz, vec![i])),
            }
        }
        let first = self.far_pages.iter().map(|(p, _, _)| p + 1).max().unwrap_or(28) as usize;
        for (k, (_, members)) in pages.iter().enumerate() {
            let page = first + k;
            let _ = writeln!(pages_text, "; code page {page}\n    MMU 2 e, {page}\n    ORG $4000");
            for &i in members {
                let (_, label, _) = &self.fn_chunks[i];
                let code = self.chunk_code(i, opts).replacen(&format!("\n{label}:\n"), &format!("\n{label}_bank:\n"), 1);
                pages_text.push_str(&code);
                let _ = writeln!(stubs, "{label}:\n    ld hl,{label}_bank\n    ld a,{page}\n    jp rt_bcall");
            }
        }
        (resident, stubs, pages_text)
    }

    fn finish(&self, opts: &Options, banked: &HashSet<String>) -> String {
        let (code, stubs, code_pages) = self.code_layout(opts, banked);
        let code_banked = !stubs.is_empty();
        let mut out = String::new();
        out.push_str("; Generated by lz80nc (LZ80N compiler v0) -- do not edit\n");
        out.push_str("    OPT --zxnext\n    DEVICE ZXSPECTRUMNEXT\n    ORG $8000\n\n");
        let (isr, irq_init) = self.gen_isr(code_banked);
        let startup = runtime::STARTUP.trim_end();
        let startup = startup.strip_suffix("ei").expect("startup ends with ei");
        out.push_str(startup);
        out.push_str(&irq_init);
        out.push_str("    ei\n");
        out.push_str("\n; ---- main program ----\n");
        out.push_str(&code);
        if code_banked {
            out.push_str("\n; ---- stubs of the banked functions ----\n");
            out.push_str(&stubs);
            out.push_str(runtime::BANKED);
        }
        out.push_str(runtime::RUNTIME);
        out.push_str("\n; ---- interrupt service routine ----\n");
        out.push_str(&isr);
        out.push_str("rt_font:
");
        for glyph in crate::font::bytes().chunks(8) {
            let row: Vec<String> = glyph.iter().map(|b| format!("${b:02X}")).collect();
            let _ = writeln!(out, "    db {}", row.join(","));
        }
        out.push_str("\n; ---- data ----\n");
        let mut strings: Vec<_> = self.strings.iter().collect();
        strings.sort_by(|a, b| a.1.cmp(b.1));
        for (s, l) in strings {
            let bytes: Vec<String> = s.bytes().chain(std::iter::once(0)).map(|b| b.to_string()).collect();
            let _ = writeln!(out, "{l}: db {}", bytes.join(","));
        }
        let mut ps: Vec<_> = self.pstrings.iter().collect();
        ps.sort_by(|a, b| a.1.cmp(b.1));
        for (s, l) in ps {
            let mut bytes = vec![s.len().to_string()];
            bytes.extend(s.bytes().map(|b| b.to_string()));
            let _ = writeln!(out, "{l}: db {}", bytes.join(","));
        }
        for (l, bytes) in &self.assets {
            let _ = writeln!(out, "{l}:");
            for chunk in bytes.chunks(16) {
                let row: Vec<String> = chunk.iter().map(|b| b.to_string()).collect();
                let _ = writeln!(out, "    db {}", row.join(","));
            }
        }
        if self.uses_notes {
            out.push_str(runtime::NOTES);
            out.push_str("; AY tone periods (1.75 MHz clock), notes 0..107 = C0..B8\nrt_note_tab:\n");
            let per: Vec<String> = (0..108)
                .map(|n| {
                    let f = 440.0 * 2f64.powf((n as f64 - 57.0) / 12.0);
                    ((1_750_000.0 / (16.0 * f)).round() as i64).min(4095).to_string()
                })
                .collect();
            for c in per.chunks(12) {
                let _ = writeln!(out, "    dw {}", c.join(","));
            }
        }
        let zero = self.zero_pages();
        if !zero.is_empty() {
            out.push_str(
                "; clear the banked pages that are not stored in the .nex\nrt_far_zero:\n    ld hl,rt_far_zero_tab\n.p:\n    ld a,(hl)\n    or a\n    ret z\n    push hl\n    ld (rt_farpage),a\n    nextreg $51,a\n    xor a\n    ld de,$2000\n    ld bc,8192\n    call rt_dma_fill\n    pop hl\n    inc hl\n    jr .p\n",
            );
            let list: Vec<String> = zero.iter().map(|p| p.to_string()).collect();
            let _ = writeln!(out, "rt_far_zero_tab: db {},0", list.join(","));
        }
        if !self.images.is_empty() {
            out.push_str(runtime::IMAGES);
            for (label, img, first) in &self.images {
                let _ = writeln!(
                    out,
                    "{label}: db {first}\n    dw {}\n    db {},{},{}",
                    img.width, img.height, img.rows_per_page, img.masked as u8
                );
            }
        }
        if self.uses_trig {
            out.push_str(runtime::TRIG);
            out.push_str("rt_sin_tab:\n");
            for chunk in (0..256).collect::<Vec<_>>().chunks(8) {
                let row: Vec<String> = chunk
                    .iter()
                    .map(|&i| {
                        let v = ((i as f64) * std::f64::consts::PI * 2.0 / 256.0).sin() * 256.0;
                        format!("{}", (v.round() as i64) & 0xFFFF)
                    })
                    .collect();
                let _ = writeln!(out, "    dw {}", row.join(","));
            }
            out.push_str("rt_atan_tab:\n");
            let row: Vec<String> = (0..=64)
                .map(|i| format!("{}", ((i as f64 / 64.0).atan() * 128.0 / std::f64::consts::PI).round() as i64))
                .collect();
            let _ = writeln!(out, "    db {}", row.join(","));
        }
        out.push_str(&self.data);
        let _ = write!(
            out,
            "\n__program_end:\n    ASSERT __program_end <= $FD00, \"main program and data too large: move code into functions (banked automatically) and big arrays into far arrays\"\n\n\
             {}\
             \x20   SAVENEX OPEN \"{}\", __start, $FDFD\n\
             \x20   SAVENEX CORE 3,0,0\n\
             \x20   SAVENEX CFG 0\n\
             \x20   SAVENEX AUTO\n\
             \x20   SAVENEX CLOSE\n",
            self.image_pages() + &code_pages,
            opts.nex_name
        );
        out
    }
}

impl Gen {
    /// Banked data: each 8K page assembled into MMU slot 1 at $2000.
    /// All-zero pages are not stored (SAVENEX skips empty banks, which then
    /// hold whatever was in memory); `rt_far_zero` clears them at startup.
    fn image_pages(&self) -> String {
        let mut out = String::new();
        for (page, bytes, note) in &self.far_pages {
            if Self::zero_page(bytes) {
                continue;
            }
            let _ = writeln!(out, "; {note}\n    MMU 1 e, {page}\n    ORG $2000");
            for chunk in bytes.chunks(32) {
                let row: Vec<String> = chunk.iter().map(|b| b.to_string()).collect();
                let _ = writeln!(out, "    db {}", row.join(","));
            }
        }
        out
    }

    fn zero_page(bytes: &[u8]) -> bool {
        bytes.iter().all(|&b| b == 0)
    }

    /// Pages cleared at startup.
    fn zero_pages(&self) -> Vec<u32> {
        self.far_pages.iter().filter(|(_, b, _)| Self::zero_page(b)).map(|(p, _, _)| *p).collect()
    }
}

/// Code banking: the resident area ends at $FD00 (32000 bytes from $8000);
/// sizes are estimates, so leave a margin. A code page holds 8192 bytes.
const RESIDENT_LIMIT: usize = 30500;
const PAGE_CAP: usize = 7800;
const STUB_SIZE: usize = 8;

/// Estimated size of the resident part of an output (up to `__program_end`).
pub(crate) fn resident_size(out: &str) -> usize {
    asm_size(out.split("\n__program_end:").next().unwrap_or(out))
}

/// Estimated number of bytes the assembler makes of `text`. Exact for data,
/// slightly high rather than low for instructions.
fn asm_size(text: &str) -> usize {
    let mut n = 0;
    for raw in text.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() || line.ends_with(':') {
            continue;
        }
        let line = match line.split_once(": ") {
            Some((lbl, rest)) if !lbl.contains(' ') && !lbl.contains('"') => rest.trim(),
            _ => line,
        };
        let (op, args) = match line.split_once(char::is_whitespace) {
            Some((o, a)) => (o.to_ascii_lowercase(), a.trim()),
            None => (line.to_ascii_lowercase(), ""),
        };
        n += match op.as_str() {
            "db" | "defb" | "dm" => data_items(args, 1),
            "dw" | "defw" => data_items(args, 2),
            "ds" | "defs" | "block" => args.split(',').next().and_then(|v| v.trim().parse().ok()).unwrap_or(0),
            "org" | "assert" | "mmu" | "device" | "opt" | "savenex" | "equ" | "align" | "if" | "endif" | "macro"
            | "endm" | "output" | "include" | "incbin" => 0,
            _ => insn_size(&op, args),
        };
    }
    n
}

fn strip_comment(line: &str) -> &str {
    let mut in_str = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_str = !in_str,
            ';' if !in_str => return &line[..i],
            _ => {}
        }
    }
    line
}

fn data_items(args: &str, unit: usize) -> usize {
    let mut n = 0;
    let mut cur = String::new();
    let mut in_str = false;
    for c in args.chars().chain(std::iter::once(',')) {
        match c {
            '"' => {
                in_str = !in_str;
                cur.push(c);
            }
            ',' if !in_str => {
                let t = cur.trim();
                if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
                    n += t.len() - 2;
                } else if !t.is_empty() {
                    n += unit;
                }
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    n
}

fn insn_size(op: &str, args: &str) -> usize {
    let a = args.to_ascii_lowercase();
    let ops: Vec<&str> = a.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    const R8: &[&str] = &["a", "b", "c", "d", "e", "h", "l", "i", "r", "ixh", "ixl", "iyh", "iyl", "f"];
    const R16: &[&str] = &["bc", "de", "hl", "sp", "ix", "iy", "af", "af'"];
    const CC: &[&str] = &["z", "nz", "c", "nc", "p", "m", "pe", "po"];
    const IND: &[&str] = &["(hl)", "(bc)", "(de)", "(sp)", "(c)", "(ix)", "(iy)"];
    let is_reg = |o: &str| R8.contains(&o) || R16.contains(&o) || IND.contains(&o);
    let index = a.contains("ix") || a.contains("iy");
    let disp = a.contains("(ix+") || a.contains("(ix-") || a.contains("(iy+") || a.contains("(iy-");
    let mut size = match op {
        "jr" | "djnz" => return 2,
        "jp" | "call" => {
            if ops.last().is_some_and(|o| IND.contains(o)) {
                1
            } else {
                3
            }
        }
        "push" => {
            if ops.first().is_some_and(|o| R16.contains(o)) {
                1
            } else {
                4 // Z80N push nn
            }
        }
        "ret" | "rst" | "pop" | "ex" | "exx" | "di" | "ei" | "halt" | "nop" | "scf" | "ccf" | "cpl"
        | "daa" | "rla" | "rra" | "rlca" | "rrca" | "inc" | "dec" => 1,
        "nextreg" => {
            if ops.get(1) == Some(&"a") {
                3
            } else {
                4
            }
        }
        "ldir" | "lddr" | "ldi" | "ldd" | "cpir" | "cpdr" | "cpi" | "cpd" | "ini" | "outi" | "inir" | "otir"
        | "neg" | "reti" | "retn" | "im" | "rld" | "rrd" | "mul" | "swapnib" | "mirror" | "ldix" | "ldirx"
        | "lddx" | "lddrx" | "ldpirx" | "ldws" | "pixeldn" | "pixelad" | "setae" | "outinb" | "bsla" | "bsra"
        | "bsrl" | "bsrf" | "brlc" => 2,
        "test" => 3,
        "bit" | "set" | "res" | "rl" | "rr" | "rlc" | "rrc" | "sla" | "sra" | "srl" | "sll" | "sli" => 2,
        "in" | "out" => 2,
        "adc" | "sbc" if ops.first() == Some(&"hl") => 2,
        "add" if ops.first() == Some(&"hl") || ops.first() == Some(&"de") || ops.first() == Some(&"bc") => {
            match ops.get(1) {
                Some(&"a") => 2,
                Some(o) if R16.contains(o) => 1,
                _ => 4, // Z80N add rr,nn
            }
        }
        _ => {
            // ld / alu with an optional immediate or address
            let imm: Vec<&&str> = ops.iter().filter(|o| !is_reg(o) && !CC.contains(o)).collect();
            if imm.is_empty() {
                1
            } else {
                let wide = ops.iter().any(|o| R16.contains(o) && *o != "af")
                    || imm.iter().any(|o| o.starts_with('(') && !o.starts_with("(ix") && !o.starts_with("(iy"));
                if op == "ld" && ops.iter().any(|o| ["bc", "de", "sp"].contains(o)) && imm.iter().any(|o| o.starts_with('(')) {
                    4
                } else if wide {
                    3
                } else {
                    2
                }
            }
        }
    };
    if index {
        size += 1;
    }
    if disp {
        size += 1;
    }
    size
}

fn contains_call(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Call(n, args) => Ty::from_name(n).is_none() || args.iter().any(contains_call),
        ExprKind::MethodCall(..) => true,
        ExprKind::Index(b, i) => contains_call(b) || contains_call(i),
        ExprKind::Field(b, _) => contains_call(b),
        ExprKind::Unary(_, x) => contains_call(x),
        ExprKind::Binary(_, l, r) => contains_call(l) || contains_call(r),
        _ => false,
    }
}

/// Magic multiplier for unsigned 16-bit division by `d`:
/// (17-bit?, M, s) with q = (x * M) >> (16 + s), checked for every x.
fn udiv_magic(d: u64) -> Option<(bool, u64, u32)> {
    for s in 0..=16u32 {
        let m = ((1u64 << (16 + s)) + d - 1) / d;
        if m < 0x10000 && (0..0x10000u64).all(|x| (x * m) >> (16 + s) == x / d) {
            return Some((false, m, s));
        }
    }
    for s in 1..=16u32 {
        let m = ((1u64 << (16 + s)) + d - 1) / d;
        if (0x10000..0x20000).contains(&m) {
            let mp = m - 0x10000;
            let ok = (0..0x10000u64).all(|x| {
                let t = (x * mp) >> 16;
                (((x - t) >> 1) + t) >> (s - 1) == x / d
            });
            if ok {
                return Some((true, mp, s));
            }
        }
    }
    None
}
