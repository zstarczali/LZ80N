//! Built-in assembler: turns the generated Z80N assembly (sjasmplus syntax,
//! the subset `codegen`/`runtime` and inline `asm` blocks use) straight into
//! a NEX file, so building no longer needs an external SjASMPlus.
//!
//! Supported: all Z80 and Z80N instructions; labels (`name:`, `name`, local
//! `.name` belonging to the last global label); `equ`; `OPT` and `DEVICE
//! ZXSPECTRUMNEXT`; `ORG`; `MMU slot[ slot2] [e|n|w], page[, address]`;
//! `DB`/`DEFB`/`DM`/`DZ` (numbers and strings), `DW`/`DEFW`, `DD`/`DWORD`, `DS`/`DEFS`/`BLOCK`;
//! `ALIGN`; `ASSERT expr[, "message"]`; `INCBIN "file"[, offset[, length]]`;
//! `SAVENEX OPEN/CORE/CFG/BANK/AUTO/CLOSE`.
//!
//! Instruction sizes never depend on operand values, so the assembler repeats
//! the layout pass until the symbol values stop changing, then runs a final,
//! strict pass that writes the memory image and reports errors.

pub mod encode;
pub mod expr;
pub mod nex;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use encode::{split_operands, Ctx};
use expr::{is_ident_char, is_ident_start, Scope, Value};

const PAGE: usize = 8192;
/// 8K pages of the ZXSPECTRUMNEXT device (1.75 MB)
const PAGES: usize = 224;
/// Pages mapped into the eight 8K slots at the start (banks 7, 5, 2, 0)
const DEFAULT_SLOTS: [u8; 8] = [14, 15, 10, 11, 4, 5, 0, 1];
const MAX_PASSES: usize = 12;

#[derive(Debug, Clone)]
pub struct AsmError {
    /// 1-based line in the assembly source
    pub line: usize,
    pub msg: String,
    pub text: String,
}

impl fmt::Display for AsmError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: error: {}\n    {}", self.line, self.msg, self.text.trim())
    }
}

pub struct Assembled {
    /// (file name from `SAVENEX OPEN`, NEX bytes)
    pub nex: Option<(String, Vec<u8>)>,
    pub symbols: HashMap<String, i64>,
}

impl Assembled {
    /// Symbols as 16-bit addresses (for the test emulator).
    #[cfg(test)]
    pub fn symbols16(&self) -> HashMap<String, u16> {
        self.symbols.iter().map(|(k, v)| (k.clone(), *v as u16)).collect()
    }

    /// A symbol file in the sjasmplus `--sym` format.
    pub fn sym_file(&self) -> String {
        let mut v: Vec<_> = self.symbols.iter().collect();
        v.sort();
        v.iter().map(|(k, val)| format!("{k}: EQU 0x{:08X}\n", **val as u32)).collect()
    }
}

/// Assemble `src`. `base_dir` resolves `INCBIN` paths.
pub fn assemble(src: &str, base_dir: &Path) -> Result<Assembled, Vec<AsmError>> {
    let lines: Vec<Line> = src.lines().enumerate().map(|(i, t)| Line::parse(i + 1, t)).collect();
    let mut syms: HashMap<String, i64> = HashMap::new();
    let mut prev: Option<HashMap<String, i64>> = None;
    for _ in 0..MAX_PASSES {
        let mut p = Pass::new(&mut syms, false, base_dir);
        p.run(&lines);
        if prev.as_ref() == Some(&*p.syms) {
            break;
        }
        prev = Some(p.syms.clone());
    }
    let mut p = Pass::new(&mut syms, true, base_dir);
    p.run(&lines);
    if !p.errors.is_empty() {
        return Err(p.errors);
    }
    let nex = p.nex_out.take();
    Ok(Assembled { nex, symbols: syms })
}

/// Tests: a folder INCBIN looks in when the file is not next to the source
/// (the self-hosted compiler's `lib/rt_core.bin`).
#[cfg(test)]
pub static TEST_FALLBACK: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// Assemble a block of code at `ORG $8000` (bank 2, as mapped at the start)
/// up to the symbol `end`: its bytes and its symbols. The self-hosted
/// compiler takes the runtime this way, ready-made.
pub fn assemble_block(src: &str, base_dir: &Path, end: &str) -> Result<(Vec<u8>, HashMap<String, i64>), Vec<AsmError>> {
    let lines: Vec<Line> = src.lines().enumerate().map(|(i, t)| Line::parse(i + 1, t)).collect();
    let mut syms: HashMap<String, i64> = HashMap::new();
    let mut prev: Option<HashMap<String, i64>> = None;
    for _ in 0..MAX_PASSES {
        let mut p = Pass::new(&mut syms, false, base_dir);
        p.run(&lines);
        if prev.as_ref() == Some(&*p.syms) {
            break;
        }
        prev = Some(p.syms.clone());
    }
    let mut p = Pass::new(&mut syms, true, base_dir);
    p.run(&lines);
    if !p.errors.is_empty() {
        return Err(p.errors);
    }
    let len = (p.syms[end] - 0x8000) as usize;
    let bytes = p.bank_bytes(2)[..len].to_vec();
    Ok((bytes, syms))
}

// ---------------------------------------------------------------- lines

struct Line {
    no: usize,
    text: String,
    label: Option<String>,
    /// lower-case mnemonic / directive
    op: String,
    args: String,
}

/// Strip a `;` comment (outside quotes).
fn strip_comment(s: &str) -> &str {
    let b = s.as_bytes();
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if let Some(q) = quote {
            if c == b'\\' && q == b'"' {
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
        } else if c == b';' {
            return &s[..i];
        } else if c == b'"' {
            quote = Some(c);
        } else if c == b'\'' {
            // `af'` is a register name, not a quote
            let af = i >= 2 && b[i - 2..i].eq_ignore_ascii_case(b"af") && (i < 3 || !is_ident_char(b[i - 3]));
            if !af {
                quote = Some(c);
            }
        }
        i += 1;
    }
    s
}

impl Line {
    fn parse(no: usize, text: &str) -> Line {
        let body = strip_comment(text);
        let mut label = None;
        let mut rest = body;
        let b = body.as_bytes();
        if !b.is_empty() && is_ident_start(b[0]) {
            // label in column 0 (with or without a colon)
            let mut i = 0;
            while i < b.len() && is_ident_char(b[i]) {
                i += 1;
            }
            label = Some(body[..i].to_string());
            rest = &body[i..];
            rest = rest.strip_prefix(':').unwrap_or(rest);
        } else {
            // an indented `name:` is a label too
            let t = body.trim_start();
            let tb = t.as_bytes();
            if !tb.is_empty() && is_ident_start(tb[0]) {
                let mut i = 0;
                while i < tb.len() && is_ident_char(tb[i]) {
                    i += 1;
                }
                if tb.get(i) == Some(&b':') {
                    label = Some(t[..i].to_string());
                    rest = &t[i + 1..];
                }
            }
        }
        let rest = rest.trim();
        let (op, args) = match rest.find(|c: char| c.is_whitespace()) {
            Some(i) => (&rest[..i], rest[i..].trim()),
            None => (rest, ""),
        };
        Line { no, text: text.to_string(), label, op: op.to_ascii_lowercase(), args: args.to_string() }
    }
}

// ---------------------------------------------------------------- passes

struct Pass<'a> {
    syms: &'a mut HashMap<String, i64>,
    defined: HashSet<String>,
    strict: bool,
    base_dir: &'a Path,
    pc: i64,
    line_pc: i64,
    last_global: String,
    slots: [u8; 8],
    /// per slot: MMU option (`e` error / `n` next page / `w` warning)
    guard: [Option<u8>; 8],
    last_write: Option<i64>,
    mem: Vec<u8>,
    nex: Option<nex::Nex>,
    nex_out: Option<(String, Vec<u8>)>,
    errors: Vec<AsmError>,
    cur_line: usize,
    cur_text: String,
}

struct View<'a> {
    syms: &'a HashMap<String, i64>,
    here: i64,
    last_global: &'a str,
}

impl Scope for View<'_> {
    fn symbol(&self, name: &str) -> Option<i64> {
        self.syms.get(name).copied()
    }
    fn here(&self) -> i64 {
        self.here
    }
    fn qualify(&self, name: &str) -> String {
        qualify(self.last_global, name)
    }
}

fn qualify(last_global: &str, name: &str) -> String {
    if name.starts_with('.') {
        format!("{last_global}{name}")
    } else {
        name.trim_start_matches('@').to_string()
    }
}

/// A whole-item string literal of a `db`: `"text"` or `'text'` (2+ chars).
fn string_lit(item: &str) -> Option<Vec<u8>> {
    let b = item.as_bytes();
    if b.len() < 2 {
        return None;
    }
    let q = b[0];
    if (q != b'"' && q != b'\'') || b[b.len() - 1] != q {
        return None;
    }
    let inner = &b[1..b.len() - 1];
    let mut out = vec![];
    let mut i = 0;
    while i < inner.len() {
        let c = inner[i];
        if c == q {
            return None; // `"a" + "b"`: not one literal
        }
        if c == b'\\' && q == b'"' && i + 1 < inner.len() {
            out.push(expr::unescape(inner[i + 1]));
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }
    if q == b'\'' && out.len() < 2 {
        return None; // 'c' is a number
    }
    Some(out)
}

fn unquote(s: &str) -> Option<&str> {
    let s = s.trim();
    (s.len() >= 2 && s.starts_with('"') && s.ends_with('"')).then(|| &s[1..s.len() - 1])
}

impl<'a> Pass<'a> {
    fn new(syms: &'a mut HashMap<String, i64>, strict: bool, base_dir: &'a Path) -> Self {
        Pass {
            syms,
            defined: HashSet::new(),
            strict,
            base_dir,
            pc: 0,
            line_pc: 0,
            last_global: String::new(),
            slots: DEFAULT_SLOTS,
            guard: [None; 8],
            last_write: None,
            mem: if strict { vec![0; PAGES * PAGE] } else { vec![] },
            nex: None,
            nex_out: None,
            errors: vec![],
            cur_line: 0,
            cur_text: String::new(),
        }
    }

    fn error(&mut self, msg: impl Into<String>) {
        if self.strict {
            self.errors.push(AsmError { line: self.cur_line, msg: msg.into(), text: self.cur_text.clone() });
        }
    }

    fn view(&self) -> View<'_> {
        View { syms: self.syms, here: self.line_pc, last_global: &self.last_global }
    }

    /// Evaluate; undefined symbols are 0 outside the final pass.
    fn eval(&mut self, e: &str) -> Option<i64> {
        let r = expr::eval(e, &self.view());
        match r {
            Ok(Value::Known(v)) => Some(v),
            Ok(Value::Undefined(n)) => {
                if self.strict {
                    self.error(format!("undefined symbol `{n}`"));
                    None
                } else {
                    Some(0)
                }
            }
            Err(m) => {
                self.error(m);
                None
            }
        }
    }

    fn run(&mut self, lines: &[Line]) {
        for l in lines {
            self.cur_line = l.no;
            if self.strict {
                self.cur_text = l.text.clone();
            }
            self.line_pc = self.pc;
            self.line(l);
            if !self.errors.is_empty() && self.errors.len() > 50 {
                return;
            }
        }
    }

    fn define(&mut self, name: &str, value: i64) {
        let full = qualify(&self.last_global, name);
        if !name.starts_with('.') {
            self.last_global = full.clone();
        }
        if !self.defined.insert(full.clone()) {
            self.error(format!("label `{full}` defined twice"));
        }
        self.syms.insert(full, value);
    }

    fn line(&mut self, l: &Line) {
        if let Some(label) = &l.label {
            if matches!(l.op.as_str(), "equ" | "=" | "defl") {
                let v = self.eval(&l.args).unwrap_or(0);
                self.define(label, v);
                return;
            }
            self.define(label, self.pc);
        }
        if l.op.is_empty() {
            return;
        }
        match l.op.as_str() {
            "opt" | "output" | "display" | "end" => {}
            "device" => {
                if !l.args.trim().eq_ignore_ascii_case("zxspectrumnext") {
                    self.error(format!("only DEVICE ZXSPECTRUMNEXT is supported, not `{}`", l.args));
                }
            }
            "org" => {
                let a = split_operands(&l.args);
                if let Some(v) = a.first().and_then(|e| self.eval(e)) {
                    self.pc = v;
                    self.last_write = None;
                }
            }
            "mmu" => self.mmu(&l.args),
            "db" | "defb" | "byte" | "dm" | "defm" | "dz" => {
                for item in split_operands(&l.args) {
                    if let Some(bytes) = string_lit(&item) {
                        for b in bytes {
                            self.emit(b);
                        }
                    } else {
                        let v = self.eval(&item).unwrap_or(0);
                        if self.strict && !(-128..=255).contains(&v) {
                            self.error(format!("value {v} does not fit in a byte"));
                        }
                        self.emit(v as u8);
                    }
                }
                if l.op == "dz" {
                    self.emit(0);
                }
            }
            "dw" | "defw" | "word" => {
                for item in split_operands(&l.args) {
                    let v = self.eval(&item).unwrap_or(0);
                    if self.strict && !(-32768..=65535).contains(&v) {
                        self.error(format!("value {v} does not fit in a word"));
                    }
                    for b in (v as u16).to_le_bytes() {
                        self.emit(b);
                    }
                }
            }
            "dd" | "defd" | "dword" => {
                for item in split_operands(&l.args) {
                    let v = self.eval(&item).unwrap_or(0);
                    if self.strict && !(-(1i64 << 31)..(1i64 << 32)).contains(&v) {
                        self.error(format!("value {v} does not fit in 32 bits"));
                    }
                    for b in (v as u32).to_le_bytes() {
                        self.emit(b);
                    }
                }
            }
            "ds" | "defs" | "block" => {
                let a = split_operands(&l.args);
                let n = a.first().and_then(|e| self.eval(e)).unwrap_or(0);
                let fill = a.get(1).and_then(|e| self.eval(e)).unwrap_or(0) as u8;
                if !(0..=0x10000).contains(&n) {
                    self.error(format!("bad DS size {n}"));
                    return;
                }
                for _ in 0..n {
                    self.emit(fill);
                }
            }
            "align" => {
                let a = split_operands(&l.args);
                let n = a.first().and_then(|e| self.eval(e)).unwrap_or(1).max(1);
                let fill = a.get(1).and_then(|e| self.eval(e)).unwrap_or(0) as u8;
                while self.pc % n != 0 {
                    self.emit(fill);
                }
            }
            "assert" => {
                if self.strict {
                    let a = split_operands(&l.args);
                    if let Some(v) = a.first().and_then(|e| self.eval(e)) {
                        if v == 0 {
                            let msg = a.get(1).and_then(|m| unquote(m)).unwrap_or("assertion failed");
                            self.error(msg.to_string());
                        }
                    }
                }
            }
            "incbin" => self.incbin(&l.args),
            "savenex" => self.savenex(&l.args),
            mn => {
                let ops = split_operands(&l.args);
                let ctx_view = self.view();
                let ctx = Ctx { scope: &ctx_view, strict: self.strict };
                match encode::encode(mn, &ops, self.line_pc, &ctx) {
                    Ok(bytes) => {
                        for b in bytes {
                            self.emit(b);
                        }
                    }
                    Err(e) => {
                        self.error(e);
                    }
                }
            }
        }
    }

    /// Write one byte at `pc` through the current MMU mapping.
    fn emit(&mut self, b: u8) {
        // crossing the end of an MMU slot guarded with `e` / `n`
        if let Some(last) = self.last_write {
            if last + 1 == self.pc && self.pc & 0x1FFF == 0 {
                let slot = (last >> 13) as usize;
                match self.guard[slot] {
                    Some(b'e') => {
                        self.error(format!("data does not fit in MMU slot {slot} (8K page {})", self.slots[slot]));
                        self.guard[slot] = None; // report once
                    }
                    Some(b'n') => {
                        self.slots[slot] = self.slots[slot].wrapping_add(1);
                        self.pc -= 0x2000;
                    }
                    _ => {}
                }
            }
        }
        if !(0..=0xFFFF).contains(&self.pc) {
            self.error(format!("address ${:X} outside the 64K address space", self.pc));
            self.pc &= 0xFFFF;
        }
        if self.strict {
            let page = self.slots[(self.pc >> 13) as usize] as usize;
            if page < PAGES {
                self.mem[page * PAGE + (self.pc as usize & 0x1FFF)] = b;
            }
        }
        self.last_write = Some(self.pc);
        self.pc += 1;
    }

    fn mmu(&mut self, args: &str) {
        // MMU <slot>[ <slot2>] [e|n|w], <page>[, <address>]
        let Some((left, right)) = args.split_once(',') else {
            self.error("MMU needs `slot, page`");
            return;
        };
        let mut words: Vec<&str> = left.split_whitespace().collect();
        let mut opt = None;
        if let Some(w) = words.last() {
            if w.len() == 1 && matches!(w.to_ascii_lowercase().as_str(), "e" | "n" | "w") {
                opt = Some(w.to_ascii_lowercase().as_bytes()[0]);
                words.pop();
            }
        }
        let slots: Vec<i64> = words.iter().filter_map(|w| self.eval(w)).collect();
        let (lo, hi) = match slots.as_slice() {
            [a] => (*a, *a),
            [a, b] => (*a, *b),
            _ => {
                self.error("bad MMU slot");
                return;
            }
        };
        let r = split_operands(right);
        let Some(page) = r.first().and_then(|e| self.eval(e)) else { return };
        if !(0..8).contains(&lo) || !(lo..8).contains(&hi) {
            self.error(format!("bad MMU slot {lo}"));
            return;
        }
        if page < 0 || page + (hi - lo) >= PAGES as i64 {
            self.error(format!("8K page {page} does not exist"));
            return;
        }
        for (k, s) in (lo..=hi).enumerate() {
            self.slots[s as usize] = (page + k as i64) as u8;
            self.guard[s as usize] = opt;
        }
        if let Some(addr) = r.get(1).and_then(|e| self.eval(e)) {
            self.pc = addr;
            self.last_write = None;
        }
    }

    fn incbin(&mut self, args: &str) {
        let a = split_operands(args);
        let Some(name) = a.first().and_then(|n| unquote(n)) else {
            self.error("INCBIN needs a quoted file name");
            return;
        };
        let mut path: PathBuf = self.base_dir.join(name);
        #[cfg(test)]
        if !path.exists() {
            if let Some(dir) = TEST_FALLBACK.lock().unwrap().as_ref() {
                path = dir.join(name);
            }
        }
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                // the size must be known in every pass
                self.errors.push(AsmError { line: self.cur_line, msg: format!("{}: {e}", path.display()), text: self.cur_text.clone() });
                return;
            }
        };
        let off = a.get(1).and_then(|e| self.eval(e)).unwrap_or(0).max(0) as usize;
        let len = a.get(2).and_then(|e| self.eval(e)).map(|l| l as usize).unwrap_or(data.len().saturating_sub(off));
        let Some(slice) = data.get(off..off + len) else {
            self.error("INCBIN offset/length outside the file");
            return;
        };
        for &b in slice {
            self.emit(b);
        }
    }

    fn bank_bytes(&self, bank: u8) -> Vec<u8> {
        let p = bank as usize * 2 * PAGE;
        self.mem[p..p + 2 * PAGE].to_vec()
    }

    fn savenex(&mut self, args: &str) {
        if !self.strict {
            return;
        }
        let args = args.trim();
        let (cmd, rest) = match args.find(char::is_whitespace) {
            Some(i) => (&args[..i], args[i..].trim()),
            None => (args, ""),
        };
        let a = split_operands(rest);
        let cmd = cmd.to_ascii_lowercase();
        if cmd != "open" && self.nex.is_none() {
            self.error("SAVENEX OPEN missing");
            return;
        }
        match cmd.as_str() {
            "open" => {
                let Some(file) = a.first().and_then(|f| unquote(f)) else {
                    self.error("SAVENEX OPEN needs a quoted file name");
                    return;
                };
                let mut n = nex::Nex { file: file.to_string(), sp: 0xFFFE, ..Default::default() };
                if let Some(v) = a.get(1).and_then(|e| self.eval(e)) {
                    n.pc = v as u16;
                }
                if let Some(v) = a.get(2).and_then(|e| self.eval(e)) {
                    n.sp = v as u16;
                }
                if let Some(v) = a.get(3).and_then(|e| self.eval(e)) {
                    n.entry_bank = v as u8;
                }
                self.nex = Some(n);
            }
            "core" => {
                let v: Vec<u8> = a.iter().filter_map(|e| self.eval(e)).map(|v| v as u8).collect();
                if v.len() != 3 {
                    self.error("SAVENEX CORE needs major,minor,subminor");
                    return;
                }
                self.nex.as_mut().unwrap().core = [v[0], v[1], v[2]];
            }
            "cfg" => {
                let v: Vec<i64> = a.iter().filter_map(|e| self.eval(e)).collect();
                let n = self.nex.as_mut().unwrap();
                if let Some(b) = v.first() {
                    n.border = *b as u8;
                }
                if let Some(f) = v.get(1) {
                    n.file_handle = *f as u16;
                }
                if let Some(p) = v.get(2) {
                    n.preserve_regs = *p != 0;
                }
                if let Some(r) = v.get(3) {
                    n.ram_2mb = *r != 0;
                }
            }
            "bank" => {
                for e in &a {
                    let Some(b) = self.eval(e) else { continue };
                    if !(0..nex::BANKS as i64).contains(&b) {
                        self.error(format!("bank {b} cannot be stored in a NEX"));
                        continue;
                    }
                    let data = self.bank_bytes(b as u8);
                    let n = self.nex.as_mut().unwrap();
                    if !n.has_bank(b as u8) {
                        n.banks.push((b as u8, data));
                    }
                }
            }
            "auto" => {
                // banks from..to in NEX order (default: all); all-zero banks are skipped
                let order: Vec<u8> = nex::nex_order().collect();
                let pos = |this: &mut Self, e: Option<&String>, default: usize| -> Option<usize> {
                    match e {
                        None => Some(default),
                        Some(e) => {
                            let b = this.eval(e)?;
                            order.iter().position(|&x| x as i64 == b)
                        }
                    }
                };
                let (Some(i0), Some(i1)) = (pos(self, a.first(), 0), pos(self, a.get(1), order.len() - 1)) else {
                    self.error("bad SAVENEX AUTO bank range");
                    return;
                };
                for &b in &order[i0..=i1] {
                    let data = self.bank_bytes(b);
                    if data.iter().any(|&x| x != 0) && !self.nex.as_ref().unwrap().has_bank(b) {
                        self.nex.as_mut().unwrap().banks.push((b, data));
                    }
                }
            }
            "close" => {
                let n = self.nex.take().unwrap();
                self.nex_out = Some((n.file.clone(), n.build()));
            }
            "screen" | "palette" | "copper" => self.error(format!("SAVENEX {cmd} is not supported by the built-in assembler")),
            _ => self.error(format!("unknown SAVENEX command `{cmd}`")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asm(src: &str) -> Assembled {
        match assemble(src, Path::new(".")) {
            Ok(a) => a,
            Err(e) => panic!("{}", e.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n")),
        }
    }

    #[test]
    fn forward_labels_locals_and_nex() {
        let a = asm(r#"
    OPT --zxnext
    DEVICE ZXSPECTRUMNEXT
    ORG $8000
start:
    jr .fwd
    nop
.fwd:
    ld hl,data + 1
    jp other.fwd
other:
    nop
.fwd: ret
LEN equ end - data
data: db "ab",0,'c'
    dw $1234, LEN
end:
    ASSERT LEN == 8, "len"
    MMU 1 e, 28
    ORG $2000
    db 1,2,3
    SAVENEX OPEN "t.nex", start, $FDFD
    SAVENEX CORE 3,0,0
    SAVENEX CFG 0
    SAVENEX AUTO
    SAVENEX CLOSE
"#);
        assert_eq!(a.symbols["start.fwd"], 0x8003);
        assert_eq!(a.symbols["other.fwd"], 0x800A);
        assert_eq!(a.symbols["LEN"], 8);
        let (name, nex) = a.nex.unwrap();
        assert_eq!(name, "t.nex");
        assert_eq!(&nex[0..8], b"NextV1.2");
        assert_eq!(nex[9], 2); // bank 2 ($8000) and bank 14 (page 28)
        assert_eq!(nex[18 + 2], 1);
        assert_eq!(nex[18 + 14], 1);
        assert_eq!(u16::from_le_bytes([nex[14], nex[15]]), 0x8000);
        assert_eq!(u16::from_le_bytes([nex[12], nex[13]]), 0xFDFD);
        let b2 = &nex[512..512 + 16384];
        assert_eq!(&b2[0..3], &[0x18, 0x01, 0x00]);
        assert_eq!(&b2[3..6], &[0x21, 0x0C, 0x80]);
        assert_eq!(&b2[0x0B..0x11], &[b'a', b'b', 0, b'c', 0x34, 0x12]);
        let b14 = &nex[512 + 16384..512 + 2 * 16384];
        assert_eq!(&b14[0..3], &[1, 2, 3]);
    }

    #[test]
    fn mmu_guard_and_errors() {
        let e = assemble("    MMU 1 e, 28\n    ORG $3FFF\n    db 1,2\n", Path::new(".")).err().unwrap();
        assert!(e[0].msg.contains("does not fit"), "{}", e[0]);
        let e = assemble("    ld a,nosuch\n", Path::new(".")).err().unwrap();
        assert!(e[0].msg.contains("undefined symbol `nosuch`"));
        let e = assemble("x: nop\nx: nop\n", Path::new(".")).err().unwrap();
        assert!(e[0].msg.contains("twice"));
        let e = assemble("    ORG $8000\n    jr far\n    ds 200\nfar: nop\n", Path::new(".")).err().unwrap();
        assert!(e[0].msg.contains("out of range"));
        let e = assemble("    ASSERT 1 > 2, \"too big\"\n", Path::new(".")).err().unwrap();
        assert_eq!(e[0].msg, "too big");
    }
}
