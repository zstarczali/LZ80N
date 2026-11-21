//! Z80 + Z80N instruction encoder.
//!
//! `encode` is called in every pass. Instruction sizes never depend on the
//! values of the operands, so the early passes (where forward labels are
//! still 0) lay out the same addresses as the final one; range checks
//! (`jr`, 8-bit immediates) are only reported in the final pass.

use super::expr::{self, Scope, Value};

/// An 8-bit register operand: `code` is the 3-bit register field, `prefix`
/// $DD/$FD for IXH/IXL/IYH/IYL and `(ix+d)`/`(iy+d)`, `disp` the displacement.
#[derive(Debug, Clone)]
struct R8 {
    code: u8,
    prefix: Option<u8>,
    disp: Option<String>,
}

#[derive(Debug, Clone)]
enum Op {
    R8(R8),
    /// bc de hl sp af ix iy (`ix`/`iy` carry their prefix)
    R16(&'static str),
    AfAlt,
    I,
    R,
    IndBc,
    IndDe,
    IndSp,
    IndC,
    /// `(expr)`
    Mem(String),
    Imm(String),
}

pub struct Ctx<'a> {
    pub scope: &'a dyn Scope,
    /// Report range errors and undefined symbols (final pass).
    pub strict: bool,
}

impl Ctx<'_> {
    fn val(&self, e: &str) -> Result<i64, String> {
        match expr::eval(e, self.scope)? {
            Value::Known(v) => Ok(v),
            Value::Undefined(n) => {
                if self.strict {
                    Err(format!("undefined symbol `{n}`"))
                } else {
                    Ok(0)
                }
            }
        }
    }

    pub fn byte(&self, e: &str) -> Result<u8, String> {
        let v = self.val(e)?;
        if self.strict && !(-128..=255).contains(&v) {
            return Err(format!("value {v} does not fit in a byte (`{}`)", e.trim()));
        }
        Ok(v as u8)
    }

    pub fn word(&self, e: &str) -> Result<[u8; 2], String> {
        let v = self.val(e)?;
        if self.strict && !(-32768..=65535).contains(&v) {
            return Err(format!("value {v} does not fit in a word (`{}`)", e.trim()));
        }
        Ok((v as u16).to_le_bytes())
    }

    fn disp(&self, d: &Option<String>) -> Result<Vec<u8>, String> {
        let Some(d) = d else { return Ok(vec![]) };
        let v = self.val(d)?;
        if self.strict && !(-128..=127).contains(&v) {
            return Err(format!("index offset {v} out of range -128..127"));
        }
        Ok(vec![v as u8])
    }
}

/// Split an operand list at top-level commas (not inside brackets or quotes).
pub fn split_operands(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut depth = 0i32;
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut prev = '\0';
    for c in s.chars() {
        if let Some(q) = quote {
            cur.push(c);
            if c == q && prev != '\\' {
                quote = None;
            }
            prev = if prev == '\\' && c == '\\' { '\0' } else { c };
            continue;
        }
        match c {
            '"' => quote = Some('"'),
            // `af'` is a register, not the start of a character constant
            '\'' if !cur.trim().eq_ignore_ascii_case("af") => quote = Some('\''),
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
                prev = c;
                continue;
            }
            _ => {}
        }
        cur.push(c);
        prev = c;
    }
    if !cur.trim().is_empty() || !out.is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// `(` ... `)` wrapping the whole operand?
fn bracketed(s: &str) -> Option<&str> {
    let b = s.as_bytes();
    if b.first() != Some(&b'(') || b.last() != Some(&b')') {
        return None;
    }
    let mut depth = 0;
    let mut quote = None;
    for (i, &c) in b.iter().enumerate() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            b'\'' | b'"' => quote = Some(c),
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 && i != b.len() - 1 {
                    return None; // `(a) | (b)`
                }
            }
            _ => {}
        }
    }
    Some(&s[1..s.len() - 1])
}

fn reg8(name: &str) -> Option<R8> {
    let (code, prefix) = match name {
        "b" => (0, None),
        "c" => (1, None),
        "d" => (2, None),
        "e" => (3, None),
        "h" => (4, None),
        "l" => (5, None),
        "a" => (7, None),
        "ixh" | "xh" | "hx" => (4, Some(0xDD)),
        "ixl" | "xl" | "lx" => (5, Some(0xDD)),
        "iyh" | "yh" | "hy" => (4, Some(0xFD)),
        "iyl" | "yl" | "ly" => (5, Some(0xFD)),
        _ => return None,
    };
    Some(R8 { code, prefix, disp: None })
}

fn parse_op(raw: &str) -> Op {
    let s = raw.trim();
    let l = s.to_ascii_lowercase();
    if let Some(r) = reg8(&l) {
        return Op::R8(r);
    }
    match l.as_str() {
        "bc" => return Op::R16("bc"),
        "de" => return Op::R16("de"),
        "hl" => return Op::R16("hl"),
        "sp" => return Op::R16("sp"),
        "af" => return Op::R16("af"),
        "ix" => return Op::R16("ix"),
        "iy" => return Op::R16("iy"),
        "af'" => return Op::AfAlt,
        "i" => return Op::I,
        "r" => return Op::R,
        _ => {}
    }
    if let Some(inner) = bracketed(s) {
        let il = inner.trim().to_ascii_lowercase();
        match il.as_str() {
            "hl" => return Op::R8(R8 { code: 6, prefix: None, disp: None }),
            "bc" => return Op::IndBc,
            "de" => return Op::IndDe,
            "sp" => return Op::IndSp,
            "c" => return Op::IndC,
            _ => {}
        }
        for (name, px) in [("ix", 0xDD), ("iy", 0xFD)] {
            if il == name {
                return Op::R8(R8 { code: 6, prefix: Some(px), disp: Some("0".into()) });
            }
            if il.starts_with(name) {
                let rest = inner.trim()[2..].trim_start();
                if rest.starts_with('+') || rest.starts_with('-') {
                    return Op::R8(R8 { code: 6, prefix: Some(px), disp: Some(format!("0{rest}")) });
                }
            }
        }
        return Op::Mem(inner.to_string());
    }
    Op::Imm(s.to_string())
}

fn cond(s: &str) -> Option<u8> {
    Some(match s.trim().to_ascii_lowercase().as_str() {
        "nz" => 0,
        "z" => 1,
        "nc" => 2,
        "c" => 3,
        "po" => 4,
        "pe" => 5,
        "p" => 6,
        "m" => 7,
        _ => return None,
    })
}

fn rr_code(r: &str) -> Option<(u8, Option<u8>)> {
    Some(match r {
        "bc" => (0, None),
        "de" => (1, None),
        "hl" => (2, None),
        "sp" => (3, None),
        "ix" => (2, Some(0xDD)),
        "iy" => (2, Some(0xFD)),
        _ => return None,
    })
}

fn px(p: Option<u8>) -> Vec<u8> {
    p.into_iter().collect()
}

/// `prefix opcode [disp]` for an instruction with one 8-bit register operand.
fn r8_insn(c: &Ctx, r: &R8, op: u8) -> Result<Vec<u8>, String> {
    let mut v = px(r.prefix);
    v.push(op);
    v.extend(c.disp(&r.disp)?);
    Ok(v)
}

fn bad(mn: &str, ops: &[String]) -> String {
    format!("invalid instruction `{mn} {}`", ops.join(","))
}

/// Encode one instruction. `pc` is its address (for `jr`/`djnz`).
pub fn encode(mn: &str, raw: &[String], pc: i64, c: &Ctx) -> Result<Vec<u8>, String> {
    let ops: Vec<Op> = raw.iter().map(|s| parse_op(s)).collect();
    let err = || bad(mn, raw);
    let single = |b: &[u8]| -> Result<Vec<u8>, String> {
        if !raw.is_empty() {
            return Err(format!("`{mn}` takes no operands"));
        }
        Ok(b.to_vec())
    };
    match mn {
        "nop" => single(&[0x00]),
        "rlca" => single(&[0x07]),
        "rrca" => single(&[0x0F]),
        "rla" => single(&[0x17]),
        "rra" => single(&[0x1F]),
        "daa" => single(&[0x27]),
        "cpl" => single(&[0x2F]),
        "scf" => single(&[0x37]),
        "ccf" => single(&[0x3F]),
        "halt" => single(&[0x76]),
        "di" => single(&[0xF3]),
        "ei" => single(&[0xFB]),
        "exx" => single(&[0xD9]),
        "neg" => single(&[0xED, 0x44]),
        "retn" => single(&[0xED, 0x45]),
        "reti" => single(&[0xED, 0x4D]),
        "rrd" => single(&[0xED, 0x67]),
        "rld" => single(&[0xED, 0x6F]),
        "ldi" => single(&[0xED, 0xA0]),
        "cpi" => single(&[0xED, 0xA1]),
        "ini" => single(&[0xED, 0xA2]),
        "outi" => single(&[0xED, 0xA3]),
        "ldd" => single(&[0xED, 0xA8]),
        "cpd" => single(&[0xED, 0xA9]),
        "ind" => single(&[0xED, 0xAA]),
        "outd" => single(&[0xED, 0xAB]),
        "ldir" => single(&[0xED, 0xB0]),
        "cpir" => single(&[0xED, 0xB1]),
        "inir" => single(&[0xED, 0xB2]),
        "otir" => single(&[0xED, 0xB3]),
        "lddr" => single(&[0xED, 0xB8]),
        "cpdr" => single(&[0xED, 0xB9]),
        "indr" => single(&[0xED, 0xBA]),
        "otdr" => single(&[0xED, 0xBB]),
        // ---- Z80N ----
        "swapnib" => single(&[0xED, 0x23]),
        "outinb" => single(&[0xED, 0x90]),
        "pixeldn" => single(&[0xED, 0x93]),
        "pixelad" => single(&[0xED, 0x94]),
        "setae" => single(&[0xED, 0x95]),
        "ldix" => single(&[0xED, 0xA4]),
        "ldws" => single(&[0xED, 0xA5]),
        "lddx" => single(&[0xED, 0xAC]),
        "ldirx" => single(&[0xED, 0xB4]),
        "ldpirx" => single(&[0xED, 0xB7]),
        "lddrx" => single(&[0xED, 0xBC]),
        "mirror" => match ops.as_slice() {
            [] | [Op::R8(R8 { code: 7, prefix: None, .. })] => Ok(vec![0xED, 0x24]),
            _ => Err(err()),
        },
        "mul" => match ops.as_slice() {
            [] => Ok(vec![0xED, 0x30]),
            [Op::R8(R8 { code: 2, prefix: None, .. }), Op::R8(R8 { code: 3, prefix: None, .. })] => {
                Ok(vec![0xED, 0x30])
            }
            _ => Err(err()),
        },
        "bsla" | "bsra" | "bsrl" | "bsrf" | "brlc" => {
            let op = match mn {
                "bsla" => 0x28,
                "bsra" => 0x29,
                "bsrl" => 0x2A,
                "bsrf" => 0x2B,
                _ => 0x2C,
            };
            match ops.as_slice() {
                [Op::R16("de"), Op::R8(R8 { code: 0, prefix: None, .. })] => Ok(vec![0xED, op]),
                _ => Err(err()),
            }
        }
        "test" => match ops.as_slice() {
            [Op::Imm(e)] | [Op::Mem(e)] => Ok(vec![0xED, 0x27, c.byte(e)?]),
            _ => Err(err()),
        },
        "nextreg" => match ops.as_slice() {
            [Op::Imm(r) | Op::Mem(r), Op::R8(R8 { code: 7, prefix: None, .. })] => Ok(vec![0xED, 0x92, c.byte(r)?]),
            [Op::Imm(r) | Op::Mem(r), Op::Imm(v) | Op::Mem(v)] => Ok(vec![0xED, 0x91, c.byte(r)?, c.byte(v)?]),
            _ => Err(err()),
        },
        // ---- loads ----
        "ld" => ld(&ops, c).ok_or_else(err)?,
        "push" | "pop" => {
            let base = if mn == "push" { 0xC5 } else { 0xC1 };
            let mut out = vec![];
            if ops.is_empty() {
                return Err(err());
            }
            // sjasmplus: `push bc,de` pushes both
            for o in &ops {
                match o {
                    Op::R16("af") => out.push(base + 0x30),
                    Op::R16(r) => {
                        let (code, p) = rr_code(r).ok_or_else(err)?;
                        if code == 3 {
                            return Err(err()); // sp
                        }
                        out.extend(px(p));
                        out.push(base + (code << 4));
                    }
                    // Z80N `push nn` (big-endian operand)
                    Op::Imm(e) if mn == "push" => {
                        let w = c.word(e)?;
                        out.extend([0xED, 0x8A, w[1], w[0]]);
                    }
                    _ => return Err(err()),
                }
            }
            Ok(out)
        }
        "ex" => match ops.as_slice() {
            [Op::R16("de"), Op::R16("hl")] | [Op::R16("hl"), Op::R16("de")] => Ok(vec![0xEB]),
            [Op::R16("af"), Op::AfAlt] | [Op::R16("af"), Op::R16("af")] => Ok(vec![0x08]),
            [Op::IndSp, Op::R16(r)] | [Op::R16(r), Op::IndSp] => match rr_code(r) {
                Some((2, p)) => Ok([px(p), vec![0xE3]].concat()),
                _ => Err(err()),
            },
            _ => Err(err()),
        },
        // ---- arithmetic ----
        "add" | "adc" | "sub" | "sbc" | "and" | "xor" | "or" | "cp" => alu(mn, &ops, c).ok_or_else(err)?,
        "inc" | "dec" => {
            let inc = mn == "inc";
            match ops.as_slice() {
                [Op::R8(r)] => r8_insn(c, r, (if inc { 0x04 } else { 0x05 }) | (r.code << 3)),
                [Op::R16(r)] => {
                    let (code, p) = rr_code(r).ok_or_else(err)?;
                    Ok([px(p), vec![(if inc { 0x03 } else { 0x0B }) | (code << 4)]].concat())
                }
                _ => Err(err()),
            }
        }
        // ---- rotates, shifts, bits ----
        "rlc" | "rrc" | "rl" | "rr" | "sla" | "sra" | "sll" | "sli" | "srl" => {
            let base = match mn {
                "rlc" => 0x00,
                "rrc" => 0x08,
                "rl" => 0x10,
                "rr" => 0x18,
                "sla" => 0x20,
                "sra" => 0x28,
                "sll" | "sli" => 0x30,
                _ => 0x38,
            };
            match ops.as_slice() {
                [Op::R8(r)] => cb(c, r, base),
                _ => Err(err()),
            }
        }
        "bit" | "res" | "set" => {
            let base = match mn {
                "bit" => 0x40,
                "res" => 0x80,
                _ => 0xC0,
            };
            match ops.as_slice() {
                [Op::Imm(b), Op::R8(r)] => {
                    let n = c.byte(b)?;
                    if c.strict && n > 7 {
                        return Err(format!("bit number {n} out of range 0..7"));
                    }
                    cb(c, r, base | ((n & 7) << 3))
                }
                _ => Err(err()),
            }
        }
        // ---- jumps ----
        "jp" => match ops.as_slice() {
            [Op::Imm(e)] => Ok([vec![0xC3], c.word(e)?.to_vec()].concat()),
            [Op::R8(R8 { code: 6, prefix, .. })] => Ok([px(*prefix), vec![0xE9]].concat()),
            [Op::R16("hl")] => Ok(vec![0xE9]),
            [Op::R16("ix")] => Ok(vec![0xDD, 0xE9]),
            [Op::R16("iy")] => Ok(vec![0xFD, 0xE9]),
            [Op::IndC] => Ok(vec![0xED, 0x98]),
            [_, Op::Imm(e)] => {
                let cc = cond(&raw[0]).ok_or_else(err)?;
                Ok([vec![0xC2 | (cc << 3)], c.word(e)?.to_vec()].concat())
            }
            _ => Err(err()),
        },
        "call" => match ops.as_slice() {
            [Op::Imm(e)] => Ok([vec![0xCD], c.word(e)?.to_vec()].concat()),
            [_, Op::Imm(e)] => {
                let cc = cond(&raw[0]).ok_or_else(err)?;
                Ok([vec![0xC4 | (cc << 3)], c.word(e)?.to_vec()].concat())
            }
            _ => Err(err()),
        },
        "jr" | "djnz" => {
            let (op, target) = match (mn, ops.as_slice()) {
                ("djnz", [Op::Imm(e)]) => (0x10, e),
                ("jr", [Op::Imm(e)]) => (0x18, e),
                ("jr", [_, Op::Imm(e)]) => match cond(&raw[0]) {
                    Some(cc @ 0..=3) => (0x20 | (cc << 3), e),
                    _ => return Err(err()),
                },
                _ => return Err(err()),
            };
            let t = c.val(target)?;
            let d = t - (pc + 2);
            if c.strict && !(-128..=127).contains(&d) {
                return Err(format!("`{mn}` target out of range ({d} bytes)"));
            }
            Ok(vec![op, d as u8])
        }
        "ret" => match raw {
            [] => Ok(vec![0xC9]),
            [cc] => Ok(vec![0xC0 | (cond(cc).ok_or_else(err)? << 3)]),
            _ => Err(err()),
        },
        "rst" => match ops.as_slice() {
            [Op::Imm(e)] => {
                let v = c.byte(e)?;
                if c.strict && (v & !0x38) != 0 {
                    return Err(format!("bad rst address {v}"));
                }
                Ok(vec![0xC7 | (v & 0x38)])
            }
            _ => Err(err()),
        },
        "im" => match ops.as_slice() {
            [Op::Imm(e)] => match c.byte(e)? {
                0 => Ok(vec![0xED, 0x46]),
                1 => Ok(vec![0xED, 0x56]),
                2 => Ok(vec![0xED, 0x5E]),
                _ if !c.strict => Ok(vec![0xED, 0x46]),
                n => Err(format!("bad interrupt mode {n}")),
            },
            _ => Err(err()),
        },
        // ---- I/O ----
        "in" => match ops.as_slice() {
            [Op::R8(R8 { code: 7, prefix: None, .. }), Op::Mem(e)] => Ok(vec![0xDB, c.byte(e)?]),
            [Op::R8(r @ R8 { prefix: None, .. }), Op::IndC] if r.code != 6 => Ok(vec![0xED, 0x40 | (r.code << 3)]),
            [Op::IndC] => Ok(vec![0xED, 0x70]),
            [Op::Imm(f), Op::IndC] if f.eq_ignore_ascii_case("f") => Ok(vec![0xED, 0x70]),
            _ => Err(err()),
        },
        "out" => match ops.as_slice() {
            [Op::Mem(e), Op::R8(R8 { code: 7, prefix: None, .. })] => Ok(vec![0xD3, c.byte(e)?]),
            [Op::IndC, Op::R8(r @ R8 { prefix: None, .. })] if r.code != 6 => Ok(vec![0xED, 0x41 | (r.code << 3)]),
            [Op::IndC, Op::Imm(z)] if c.val(z).ok() == Some(0) => Ok(vec![0xED, 0x71]),
            _ => Err(err()),
        },
        _ => Err(format!("unknown instruction `{mn}`")),
    }
}

fn cb(c: &Ctx, r: &R8, op: u8) -> Result<Vec<u8>, String> {
    match (&r.prefix, &r.disp) {
        // DD CB d op
        (Some(p), Some(_)) => Ok([vec![*p, 0xCB], c.disp(&r.disp)?, vec![op | 6]].concat()),
        (Some(_), None) => Err("IXH/IXL/IYH/IYL cannot be used here".into()),
        (None, _) => Ok(vec![0xCB, op | r.code]),
    }
}

fn alu(mn: &str, ops: &[Op], c: &Ctx) -> Option<Result<Vec<u8>, String>> {
    // 16-bit forms
    if let [Op::R16(d), s] = ops {
        let r = (|| -> Result<Vec<u8>, String> {
            match (mn, *d, s) {
                ("add", "hl" | "ix" | "iy", Op::R16(sr)) => {
                    let (dc, dp) = rr_code(d).unwrap();
                    let (sc, sp) = rr_code(sr).ok_or("bad register")?;
                    if sc == 2 && sp != dp {
                        return Err("bad register".into());
                    }
                    let _ = dc;
                    Ok([px(dp), vec![0x09 | (sc << 4)]].concat())
                }
                ("adc" | "sbc", "hl", Op::R16(sr)) => {
                    let (sc, sp) = rr_code(sr).ok_or("bad register")?;
                    if sp.is_some() {
                        return Err("bad register".into());
                    }
                    Ok(vec![0xED, (if mn == "adc" { 0x4A } else { 0x42 }) | (sc << 4)])
                }
                // Z80N
                ("add", "hl" | "de" | "bc", Op::R8(R8 { code: 7, prefix: None, .. })) => {
                    Ok(vec![0xED, match *d { "hl" => 0x31, "de" => 0x32, _ => 0x33 }])
                }
                ("add", "hl" | "de" | "bc", Op::Imm(e)) => {
                    let w = c.word(e)?;
                    Ok(vec![0xED, match *d { "hl" => 0x34, "de" => 0x35, _ => 0x36 }, w[0], w[1]])
                }
                _ => Err("bad operands".into()),
            }
        })();
        return Some(r.map_err(|e| format!("invalid operands for `{mn}`: {e}")));
    }
    let base: u8 = match mn {
        "add" => 0x80,
        "adc" => 0x88,
        "sub" => 0x90,
        "sbc" => 0x98,
        "and" => 0xA0,
        "xor" => 0xA8,
        "or" => 0xB0,
        _ => 0xB8,
    };
    // `add a,x` / `sub x` / `sub a,x`
    let src = match ops {
        [s] => s,
        [Op::R8(R8 { code: 7, prefix: None, .. }), s] => s,
        _ => return None,
    };
    Some(match src {
        Op::R8(r) => r8_insn(c, r, base | r.code),
        Op::Imm(e) | Op::Mem(e) => c.byte(e).map(|b| vec![base + 0x46, b]),
        _ => return None,
    })
}

fn ld(ops: &[Op], c: &Ctx) -> Option<Result<Vec<u8>, String>> {
    let [d, s] = ops else { return None };
    let r = |v: Result<Vec<u8>, String>| Some(v);
    match (d, s) {
        (Op::R8(d), Op::R8(s)) => {
            if d.code == 6 && s.code == 6 {
                return None; // that would be halt
            }
            // one prefix at most; with (ix+d) the other operand is a plain register
            let prefix = match (d.prefix, s.prefix) {
                (Some(a), Some(b)) if a != b => return None,
                (Some(_), Some(_)) if d.disp.is_some() || s.disp.is_some() => return None,
                (Some(a), _) | (_, Some(a)) => Some(a),
                (None, None) => None,
            };
            if prefix.is_some() {
                let idx_mem = d.disp.is_some() || s.disp.is_some();
                let other_hl = |x: &R8| x.prefix.is_none() && (x.code == 4 || x.code == 5);
                if !idx_mem && (other_hl(d) || other_hl(s)) {
                    return None; // ld ixh,h
                }
            }
            let mut v = px(prefix);
            v.push(0x40 | (d.code << 3) | s.code);
            match c.disp(if d.disp.is_some() { &d.disp } else { &s.disp }) {
                Ok(dd) => v.extend(dd),
                Err(e) => return r(Err(e)),
            }
            r(Ok(v))
        }
        (Op::R8(d), Op::Imm(e)) => {
            let mut v = px(d.prefix);
            v.push(0x06 | (d.code << 3));
            let res = (|| {
                v.extend(c.disp(&d.disp)?);
                v.push(c.byte(e)?);
                Ok(v)
            })();
            r(res)
        }
        (Op::R8(R8 { code: 7, prefix: None, .. }), Op::IndBc) => r(Ok(vec![0x0A])),
        (Op::R8(R8 { code: 7, prefix: None, .. }), Op::IndDe) => r(Ok(vec![0x1A])),
        (Op::R8(R8 { code: 7, prefix: None, .. }), Op::Mem(e)) => r(c.word(e).map(|w| vec![0x3A, w[0], w[1]])),
        (Op::IndBc, Op::R8(R8 { code: 7, prefix: None, .. })) => r(Ok(vec![0x02])),
        (Op::IndDe, Op::R8(R8 { code: 7, prefix: None, .. })) => r(Ok(vec![0x12])),
        (Op::Mem(e), Op::R8(R8 { code: 7, prefix: None, .. })) => r(c.word(e).map(|w| vec![0x32, w[0], w[1]])),
        (Op::R8(R8 { code: 7, prefix: None, .. }), Op::I) => r(Ok(vec![0xED, 0x57])),
        (Op::R8(R8 { code: 7, prefix: None, .. }), Op::R) => r(Ok(vec![0xED, 0x5F])),
        (Op::I, Op::R8(R8 { code: 7, prefix: None, .. })) => r(Ok(vec![0xED, 0x47])),
        (Op::R, Op::R8(R8 { code: 7, prefix: None, .. })) => r(Ok(vec![0xED, 0x4F])),
        (Op::R16("sp"), Op::R16(s @ ("hl" | "ix" | "iy"))) => {
            let (_, p) = rr_code(s).unwrap();
            r(Ok([px(p), vec![0xF9]].concat()))
        }
        (Op::R16(d), Op::Imm(e)) => {
            let (code, p) = rr_code(d)?;
            r(c.word(e).map(|w| [px(p), vec![0x01 | (code << 4), w[0], w[1]]].concat()))
        }
        (Op::R16(d), Op::Mem(e)) => {
            let (code, p) = rr_code(d)?;
            r(c.word(e).map(|w| {
                if code == 2 {
                    [px(p), vec![0x2A, w[0], w[1]]].concat()
                } else {
                    vec![0xED, 0x4B | (code << 4), w[0], w[1]]
                }
            }))
        }
        (Op::Mem(e), Op::R16(s)) => {
            let (code, p) = rr_code(s)?;
            r(c.word(e).map(|w| {
                if code == 2 {
                    [px(p), vec![0x22, w[0], w[1]]].concat()
                } else {
                    vec![0xED, 0x43 | (code << 4), w[0], w[1]]
                }
            }))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct S(HashMap<String, i64>);
    impl Scope for S {
        fn symbol(&self, n: &str) -> Option<i64> {
            self.0.get(n).copied()
        }
        fn here(&self) -> i64 {
            0x8000
        }
        fn qualify(&self, n: &str) -> String {
            n.to_string()
        }
    }

    fn enc(line: &str) -> Vec<u8> {
        let s = S(HashMap::from([("lab".to_string(), 0x8010i64), ("len".to_string(), 7)]));
        let c = Ctx { scope: &s, strict: true };
        let line = line.trim();
        let (mn, rest) = line.split_once(' ').unwrap_or((line, ""));
        encode(&mn.to_ascii_lowercase(), &split_operands(rest), 0x8000, &c)
            .unwrap_or_else(|e| panic!("{line}: {e}"))
    }

    #[test]
    fn encodings() {
        let cases: &[(&str, &[u8])] = &[
            ("nop", &[0x00]),
            ("ld a,b", &[0x78]),
            ("ld (hl),a", &[0x77]),
            ("ld a,(hl)", &[0x7E]),
            ("ld b,(ix+5)", &[0xDD, 0x46, 5]),
            ("ld (iy-2),c", &[0xFD, 0x71, 0xFE]),
            ("ld (ix+3),$12", &[0xDD, 0x36, 3, 0x12]),
            ("ld ixh,a", &[0xDD, 0x67]),
            ("ld a,ixl", &[0xDD, 0x7D]),
            ("ld a,$FF", &[0x3E, 0xFF]),
            ("ld (hl),' '", &[0x36, 0x20]),
            ("ld hl,$1234", &[0x21, 0x34, 0x12]),
            ("ld ix,lab", &[0xDD, 0x21, 0x10, 0x80]),
            ("ld hl,(lab)", &[0x2A, 0x10, 0x80]),
            ("ld de,(lab)", &[0xED, 0x5B, 0x10, 0x80]),
            ("ld (lab),sp", &[0xED, 0x73, 0x10, 0x80]),
            ("ld (lab),ix", &[0xDD, 0x22, 0x10, 0x80]),
            ("ld a,(lab)", &[0x3A, 0x10, 0x80]),
            ("ld (lab),a", &[0x32, 0x10, 0x80]),
            ("ld a,(de)", &[0x1A]),
            ("ld (bc),a", &[0x02]),
            ("ld sp,hl", &[0xF9]),
            ("ld sp,$FDFD", &[0x31, 0xFD, 0xFD]),
            ("ld i,a", &[0xED, 0x47]),
            ("ld a,r", &[0xED, 0x5F]),
            ("ld bc,(len << 8) | $6B", &[0x01, 0x6B, 0x07]),
            ("ld bc,'a' << 8 | ('z' + 1)", &[0x01, b'z' + 1, b'a']),
            ("push af", &[0xF5]),
            ("pop ix", &[0xDD, 0xE1]),
            ("push $1234", &[0xED, 0x8A, 0x12, 0x34]),
            ("ex de,hl", &[0xEB]),
            ("ex af,af'", &[0x08]),
            ("ex (sp),hl", &[0xE3]),
            ("ex (sp),ix", &[0xDD, 0xE3]),
            ("add a,b", &[0x80]),
            ("add a,'0'", &[0xC6, 0x30]),
            ("sub '0'", &[0xD6, 0x30]),
            ("sbc a,0", &[0xDE, 0]),
            ("and $0F", &[0xE6, 0x0F]),
            ("or a", &[0xB7]),
            ("xor (hl)", &[0xAE]),
            ("cp (ix+1)", &[0xDD, 0xBE, 1]),
            ("add hl,de", &[0x19]),
            ("add hl,sp", &[0x39]),
            ("add ix,bc", &[0xDD, 0x09]),
            ("add ix,ix", &[0xDD, 0x29]),
            ("adc hl,bc", &[0xED, 0x4A]),
            ("sbc hl,de", &[0xED, 0x52]),
            ("add hl,a", &[0xED, 0x31]),
            ("add de,a", &[0xED, 0x32]),
            ("add bc,$0102", &[0xED, 0x36, 0x02, 0x01]),
            ("inc a", &[0x3C]),
            ("dec (hl)", &[0x35]),
            ("inc (ix+2)", &[0xDD, 0x34, 2]),
            ("inc hl", &[0x23]),
            ("dec sp", &[0x3B]),
            ("dec iy", &[0xFD, 0x2B]),
            ("rlc b", &[0xCB, 0x00]),
            ("srl a", &[0xCB, 0x3F]),
            ("sla (hl)", &[0xCB, 0x26]),
            ("rr (ix+4)", &[0xDD, 0xCB, 4, 0x1E]),
            ("bit 7,h", &[0xCB, 0x7C]),
            ("set 0,(hl)", &[0xCB, 0xC6]),
            ("res 3,(iy+1)", &[0xFD, 0xCB, 1, 0x9E]),
            ("jp lab", &[0xC3, 0x10, 0x80]),
            ("jp nz,lab", &[0xC2, 0x10, 0x80]),
            ("jp m,lab", &[0xFA, 0x10, 0x80]),
            ("jp (hl)", &[0xE9]),
            ("jp (ix)", &[0xDD, 0xE9]),
            ("jp (c)", &[0xED, 0x98]),
            ("call lab", &[0xCD, 0x10, 0x80]),
            ("call c,lab", &[0xDC, 0x10, 0x80]),
            ("jr lab", &[0x18, 0x0E]),
            ("jr nc,lab", &[0x30, 0x0E]),
            ("jr $", &[0x18, 0xFE]),
            ("djnz lab", &[0x10, 0x0E]),
            ("ret", &[0xC9]),
            ("ret z", &[0xC8]),
            ("ret po", &[0xE0]),
            ("rst $38", &[0xFF]),
            ("im 2", &[0xED, 0x5E]),
            ("in a,($FE)", &[0xDB, 0xFE]),
            ("in e,(c)", &[0xED, 0x58]),
            ("out ($FE),a", &[0xD3, 0xFE]),
            ("out (c),a", &[0xED, 0x79]),
            ("out (c),0", &[0xED, 0x71]),
            ("ldir", &[0xED, 0xB0]),
            ("otir", &[0xED, 0xB3]),
            ("neg", &[0xED, 0x44]),
            ("reti", &[0xED, 0x4D]),
            ("mul d,e", &[0xED, 0x30]),
            ("swapnib", &[0xED, 0x23]),
            ("mirror a", &[0xED, 0x24]),
            ("test $80", &[0xED, 0x27, 0x80]),
            ("bsla de,b", &[0xED, 0x28]),
            ("bsrl de,b", &[0xED, 0x2A]),
            ("nextreg $07,3", &[0xED, 0x91, 0x07, 0x03]),
            ("nextreg $15,%00001000", &[0xED, 0x91, 0x15, 0x08]),
            ("nextreg $56,a", &[0xED, 0x92, 0x56]),
            ("nextreg $6E,high(lab) & $3F", &[0xED, 0x91, 0x6E, 0x00]),
            ("ldirx", &[0xED, 0xB4]),
            ("pixelad", &[0xED, 0x94]),
        ];
        for (src, want) in cases {
            assert_eq!(enc(src), *want, "{src}");
        }
    }

    #[test]
    fn operand_split() {
        assert_eq!(split_operands("a,','"), vec!["a", "','"]);
        assert_eq!(split_operands("af,af'"), vec!["af", "af'"]);
        assert_eq!(split_operands("bc,(x << 8) | $6B"), vec!["bc", "(x << 8) | $6B"]);
        assert_eq!(split_operands("\"a,b\",0"), vec!["\"a,b\"", "0"]);
    }
}
