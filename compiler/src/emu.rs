//! Minimal Z80N interpreter used by the test-suite to *run* generated
//! programs. Covers the unprefixed, CB and ED instruction sets plus the
//! Z80N extensions emitted by the compiler. IX/IY (DD/FD) are
//! implemented. Not cycle accurate: raster timing is approximated per
//! instruction count (see `tick`), with IM2 frame/line interrupts and copper.

const SF: u8 = 0x80;
const ZF: u8 = 0x40;
const HF: u8 = 0x10;
const PF: u8 = 0x04;
const NF: u8 = 0x02;
const CF: u8 = 0x01;

fn parity(v: u8) -> bool {
    v.count_ones() % 2 == 0
}
fn sz(v: u8) -> u8 {
    (if v == 0 { ZF } else { 0 }) | (v & SF)
}
fn szp(v: u8) -> u8 {
    sz(v) | if parity(v) { PF } else { 0 }
}

pub struct Cpu {
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub sp: u16,
    pub pc: u16,
    pub i: u8,
    alt: [u8; 8],
    pub mem: Vec<u8>,
    pub nextreg: [u8; 256],
    nr_sel: u8,
    pub border: u8,
    /// Keyboard half-rows (index = address line A8..A15), active low.
    pub keys: [u8; 8],
    pub kempston: u8,
    pub halted: bool,
    /// Sprite engine state: 64 x 256-byte patterns, 128 x 5 attribute bytes.
    pub patterns: Vec<u8>,
    pattern_ptr: usize,
    pub sprite_attr: [[u8; 5]; 128],
    /// Palettes (8-bit RRRGGGBB), indexed by NextReg $43 bits 6-4.
    pub palettes: [[u8; 256]; 8],
    pal_idx: u8,
    /// 8K RAM pages that have been mapped into slot 0 (MMU, NextReg $50).
    pub pages: std::collections::HashMap<u8, Vec<u8>>,
    /// Page in slot 0, or $FF for ROM (= `mem`).
    pub slot0: u8,
    /// Page in slot 1 ($2000-$3FFF), or $FF for ROM.
    pub slot1: u8,
    /// Page in slot 2 ($4000-$5FFF); 10 (bank 5) = `mem`.
    pub slot2: u8,
    /// Last value written to the Layer 2 port $123B.
    pub l2_port: u8,
    /// Bytes written to the zxnDMA port since the last "enable".
    dma_buf: Vec<u8>,
    /// an /INT raised during the current instruction (a long block
    /// instruction spans several lines; real hardware would interrupt it)
    int_fresh: bool,
    /// Bytes sent to the DAC (port $DF, by the CPU or the DMA).
    pub dac: Vec<u8>,
    /// A prescaled DMA transfer to the DAC: (start, length, position,
    /// prescaler, restart at the end, T-state of the next byte)
    smp: Option<(u16, u16, u16, u8, bool, u64)>,
    /// zxnDMA read-back: a mask byte follows / bytes to be read
    dma_mask_next: bool,
    dma_read: std::collections::VecDeque<u8>,
    pub dma_transfers: u32,
    nr44_second: bool,
    /// AY chips: registers, selected chip and register.
    pub ay: [[u8; 16]; 3],
    ay_chip: usize,
    ay_reg: usize,
    /// T-states executed (Z80 timings, no wait states) and DMA bytes moved.
    pub t_states: u64,
    /// (value, T-states) of every write to NextReg $7F
    pub marks: Vec<(u8, u64)>,
    dma_bytes: u64,
    line_t: u64,
    /// Interrupts: IFF1, EI delay, pending /INT.
    pub iff: bool,
    ei_delay: bool,
    int_pending: bool,
    /// Raster timing: current line (0 = first display line, 312 per frame),
    /// instructions executed on it, frames started.
    pub line: u16,
    pub frame_count: u32,
    /// Copper: 2K program memory, write index, program counter.
    pub copper: Vec<u8>,
    copper_idx: usize,
    copper_pc: usize,
    /// Display state (NextRegs, palettes) at the end of each line of the
    /// last frame, for rendering raster effects.
    pub line_state: Vec<Box<([u8; 256], [[u8; 256]; 8])>>,
    /// Index registers, and the (IX+d) / (IY+d) address of the current
    /// indexed instruction (replaces (HL) as operand 6).
    pub ix: u16,
    pub iy: u16,
    mem_override: Option<u16>,
}

/// Instructions per raster line (~1792 T-states at 28 MHz).
pub const LINES: u16 = 312;
/// The frame starts (vertical blank, ULA interrupt, copper restart).
pub const FRAME_START_LINE: u16 = 248;

impl Cpu {
    pub fn new() -> Self {
        let mut cpu = Self::blank();
        // Layer 2 and sprite palettes start as the identity (index = RRRGGGBB)
        for i in 0..256 {
            cpu.palettes[1][i] = i as u8;
            cpu.palettes[2][i] = i as u8;
            cpu.palettes[5][i] = i as u8;
            cpu.palettes[6][i] = i as u8;
        }
        cpu
    }

    fn blank() -> Self {
        Cpu {
            a: 0xFF, f: 0xFF, b: 0, c: 0, d: 0, e: 0, h: 0, l: 0,
            sp: 0xFFFF, pc: 0, i: 0, alt: [0; 8],
            mem: vec![0; 65536],
            nextreg: { let mut n = [0; 256]; n[0x12] = 8; n[0x13] = 11; n }, nr_sel: 0, border: 7,
            keys: [0x1F; 8], kempston: 0, halted: false,
            patterns: vec![0; 16384], pattern_ptr: 0, sprite_attr: [[0; 5]; 128],
            palettes: [[0; 256]; 8], pal_idx: 0,
            pages: std::collections::HashMap::new(), slot0: 0xFF, slot1: 0xFF, slot2: 10, l2_port: 0,
            dma_buf: Vec::new(), dma_transfers: 0, nr44_second: false,
            dac: Vec::new(), smp: None, int_fresh: false, dma_mask_next: false, dma_read: std::collections::VecDeque::new(),
            iff: false, ei_delay: false, int_pending: false,
            ay: [[0; 16]; 3], ay_chip: 0, ay_reg: 0,
            t_states: 0, marks: Vec::new(), dma_bytes: 0, line_t: 0,
            line: 0, frame_count: 0,
            copper: vec![0; 2048], copper_idx: 0, copper_pc: 0, line_state: Vec::new(),
            ix: 0, iy: 0, mem_override: None,
        }
    }

    /// RAM page mapped at `a` through MMU slot 0 or 1, if any.
    fn mapped(&self, a: u16) -> Option<u8> {
        match a {
            0x0000..=0x1FFF if self.slot0 != 0xFF => Some(self.slot0),
            0x2000..=0x3FFF if self.slot1 != 0xFF => Some(self.slot1),
            0x4000..=0x5FFF if self.slot2 != 10 => Some(self.slot2),
            _ => None,
        }
    }
    fn rd(&self, a: u16) -> u8 {
        if let Some(pg) = self.mapped(a) {
            return self.pages.get(&pg).map_or(0, |p| p[(a & 0x1FFF) as usize]);
        }
        self.mem[a as usize]
    }
    fn wr(&mut self, a: u16, v: u8) {
        if let Some(pg) = self.mapped(a) {
            self.pages.entry(pg).or_insert_with(|| vec![0; 8192])[(a & 0x1FFF) as usize] = v;
            return;
        }
        self.mem[a as usize] = v;
    }
    /// Read a Layer 2 pixel (256x192 mode).
    pub fn l2_pixel(&self, x: usize, y: usize) -> u8 {
        let page = self.nextreg[0x12] as usize * 2 + y / 32;
        self.pages.get(&(page as u8)).map_or(0, |p| p[(y % 32) * 256 + x])
    }
    fn rd16(&self, a: u16) -> u16 {
        self.rd(a) as u16 | (self.rd(a.wrapping_add(1)) as u16) << 8
    }
    fn wr16(&mut self, a: u16, v: u16) {
        self.wr(a, v as u8);
        self.wr(a.wrapping_add(1), (v >> 8) as u8);
    }
    fn fetch(&mut self) -> u8 {
        let v = self.rd(self.pc);
        self.pc = self.pc.wrapping_add(1);
        v
    }
    fn fetch16(&mut self) -> u16 {
        let lo = self.fetch() as u16;
        lo | (self.fetch() as u16) << 8
    }
    fn push(&mut self, v: u16) {
        self.sp = self.sp.wrapping_sub(2);
        self.wr16(self.sp, v);
    }
    fn pop(&mut self) -> u16 {
        let v = self.rd16(self.sp);
        self.sp = self.sp.wrapping_add(2);
        v
    }

    pub fn hl(&self) -> u16 {
        (self.h as u16) << 8 | self.l as u16
    }
    pub fn de(&self) -> u16 {
        (self.d as u16) << 8 | self.e as u16
    }
    fn bc(&self) -> u16 {
        (self.b as u16) << 8 | self.c as u16
    }
    fn set_hl(&mut self, v: u16) {
        self.h = (v >> 8) as u8;
        self.l = v as u8;
    }
    fn set_de(&mut self, v: u16) {
        self.d = (v >> 8) as u8;
        self.e = v as u8;
    }
    fn set_bc(&mut self, v: u16) {
        self.b = (v >> 8) as u8;
        self.c = v as u8;
    }

    fn r(&self, i: u8) -> u8 {
        match i {
            0 => self.b, 1 => self.c, 2 => self.d, 3 => self.e,
            4 => self.h, 5 => self.l, 6 => self.rd(self.mem_override.unwrap_or(self.hl())), _ => self.a,
        }
    }
    fn set_r(&mut self, i: u8, v: u8) {
        match i {
            0 => self.b = v, 1 => self.c = v, 2 => self.d = v, 3 => self.e = v,
            4 => self.h = v, 5 => self.l = v, 6 => self.wr(self.mem_override.unwrap_or(self.hl()), v), _ => self.a = v,
        }
    }
    fn rp(&self, p: u8) -> u16 {
        match p {
            0 => self.bc(), 1 => self.de(), 2 => self.hl(), _ => self.sp,
        }
    }
    fn set_rp(&mut self, p: u8, v: u16) {
        match p {
            0 => self.set_bc(v), 1 => self.set_de(v), 2 => self.set_hl(v), _ => self.sp = v,
        }
    }
    fn rp2(&self, p: u8) -> u16 {
        if p == 3 { (self.a as u16) << 8 | self.f as u16 } else { self.rp(p) }
    }
    fn set_rp2(&mut self, p: u8, v: u16) {
        if p == 3 {
            self.a = (v >> 8) as u8;
            self.f = v as u8;
        } else {
            self.set_rp(p, v)
        }
    }
    fn cond(&self, y: u8) -> bool {
        let f = self.f;
        match y {
            0 => f & ZF == 0, 1 => f & ZF != 0, 2 => f & CF == 0, 3 => f & CF != 0,
            4 => f & PF == 0, 5 => f & PF != 0, 6 => f & SF == 0, _ => f & SF != 0,
        }
    }

    fn add8(&mut self, v: u8, carry: bool) {
        let a = self.a;
        let r = a as u16 + v as u16 + carry as u16;
        let r8 = r as u8;
        self.f = sz(r8)
            | ((a ^ v ^ r8) & HF)
            | if (a ^ r8) & (v ^ r8) & 0x80 != 0 { PF } else { 0 }
            | if r > 0xFF { CF } else { 0 };
        self.a = r8;
    }
    fn sub8(&mut self, v: u8, carry: bool, store: bool) {
        let a = self.a;
        let r = a as i16 - v as i16 - carry as i16;
        let r8 = r as u8;
        self.f = sz(r8)
            | ((a ^ v ^ r8) & HF)
            | if (a ^ v) & (a ^ r8) & 0x80 != 0 { PF } else { 0 }
            | NF
            | if r < 0 { CF } else { 0 };
        if store {
            self.a = r8;
        }
    }
    fn alu(&mut self, op: u8, v: u8) {
        let c = self.f & CF != 0;
        match op {
            0 => self.add8(v, false),
            1 => self.add8(v, c),
            2 => self.sub8(v, false, true),
            3 => self.sub8(v, c, true),
            4 => {
                self.a &= v;
                self.f = szp(self.a) | HF;
            }
            5 => {
                self.a ^= v;
                self.f = szp(self.a);
            }
            6 => {
                self.a |= v;
                self.f = szp(self.a);
            }
            _ => self.sub8(v, false, false),
        }
    }
    fn adc16(&mut self, v: u16, sub: bool) {
        let hl = self.hl() as i32;
        let c = (self.f & CF) as i32;
        let v = v as i32;
        let r = if sub { hl - v - c } else { hl + v + c };
        let r16 = (r & 0xFFFF) as u16;
        let ovf = if sub {
            (hl ^ v) & (hl ^ r) & 0x8000 != 0
        } else {
            (hl ^ r) & (v ^ r) & 0x8000 != 0
        };
        self.f = (if r16 == 0 { ZF } else { 0 })
            | ((r16 >> 8) as u8 & SF)
            | if ovf { PF } else { 0 }
            | if sub { NF } else { 0 }
            | if !(0..=0xFFFF).contains(&r) { CF } else { 0 };
        self.set_hl(r16);
    }

    fn write_nextreg(&mut self, r: u8, v: u8) {
        self.nextreg[r as usize] = v;
        if r == 0x7F {
            // user register: benchmarks mark checkpoints here
            self.marks.push((v, self.t_states));
        }
        match r {
            0x50 => self.slot0 = v,
            0x51 => self.slot1 = v,
            0x52 => self.slot2 = v,
            0x60 => {
                self.copper[self.copper_idx] = v;
                self.copper_idx = (self.copper_idx + 1) & 2047;
            }
            0x61 => self.copper_idx = (self.copper_idx & 0x700) | v as usize,
            0x62 => {
                self.copper_idx = (self.copper_idx & 0xFF) | ((v as usize & 7) << 8);
                if v >> 6 != 0 {
                    self.copper_pc = 0;
                }
            }
            0x40 => {
                self.pal_idx = v;
                self.nr44_second = false;
            }
            0x44 => {
                // 9-bit colour: first write RRRGGGBB, second write the low blue bit
                if !self.nr44_second {
                    let sel = ((self.nextreg[0x43] >> 4) & 7) as usize;
                    self.palettes[sel][self.pal_idx as usize] = v;
                    self.nr44_second = true;
                } else {
                    self.nr44_second = false;
                    if self.nextreg[0x43] & 0x80 == 0 {
                        self.pal_idx = self.pal_idx.wrapping_add(1);
                    }
                }
            }
            0x41 => {
                let sel = ((self.nextreg[0x43] >> 4) & 7) as usize;
                self.palettes[sel][self.pal_idx as usize] = v;
                if self.nextreg[0x43] & 0x80 == 0 {
                    self.pal_idx = self.pal_idx.wrapping_add(1);
                }
            }
            0x35..=0x39 => {
                let n = (self.nextreg[0x34] & 0x7F) as usize;
                self.sprite_attr[n][(r - 0x35) as usize] = v;
            }
            0x75..=0x79 => {
                let n = (self.nextreg[0x34] & 0x7F) as usize;
                self.sprite_attr[n][(r - 0x75) as usize] = v;
                if r == 0x79 {
                    self.nextreg[0x34] = (n as u8 + 1) & 0x7F;
                }
            }
            _ => {}
        }
    }

    fn out(&mut self, port: u16, v: u8) {
        match port {
            0x243B => self.nr_sel = v,
            0x253B => self.write_nextreg(self.nr_sel, v),
            0x123B => self.l2_port = v,
            p if p & 0xFF == 0xDF => self.dac.push(v),
            0xFFFD => {
                if v & 0xFC == 0xFC {
                    if v & 3 != 0 {
                        self.ay_chip = 3 - (v & 3) as usize;
                    }
                } else {
                    self.ay_reg = v as usize & 15;
                }
            }
            0xBFFD => self.ay[self.ay_chip][self.ay_reg] = v,
            p if p & 0xFF == 0x6B => self.dma_write(v),
            0x303B => self.pattern_ptr = (v as usize & 63) * 256,
            p if p & 0xFF == 0x5B => {
                self.patterns[self.pattern_ptr & 0x3FFF] = v;
                self.pattern_ptr += 1;
            }
            p if p & 1 == 0 => self.border = v & 7,
            _ => {}
        }
    }
    /// zxnDMA: understands the memory-to-memory program the runtime sends
    /// (WR6 disable, WR0 with address+length, WR1, WR2, WR4, WR5, load, enable).
    fn dma_write(&mut self, v: u8) {
        if self.dma_mask_next {
            self.dma_mask_next = false;
            return;
        }
        // parse by position (data bytes may equal command codes such as $87)
        if self.dma_buf.is_empty() {
            match v {
                0xBB => self.dma_mask_next = true,
                0xA7 => {
                    // read sequence: the port A address (the only mask used)
                    let a = self.smp.map_or(0, |(start, _, pos, ..)| start.wrapping_add(pos));
                    self.dma_read.clear();
                    self.dma_read.extend([a as u8, (a >> 8) as u8]);
                }
                0x83 => {
                    self.smp = None;
                    self.dma_buf.push(v);
                }
                _ => {}
            }
            return;
        }
        if self.dma_buf.len() == 1 && v == 0x83 {
            return;
        }
        self.dma_buf.push(v);
        let n = self.dma_buf.len();
        if n < 16 || (n == 16 && self.dma_buf[8] == 0x68) {
            return;
        }
        let b = std::mem::take(&mut self.dma_buf);
        if n == 17 {
            // memory -> I/O port (the DAC), burst mode with a prescaler
            if b[1] == 0x7D && b[8] == 0x68 && b[16] == 0x87 && u16::from_le_bytes([b[12], b[13]]) & 0xFF == 0xDF {
                let presc = b[10].max(1);
                let next = self.t_states + self.dma_byte_t(presc);
                self.smp = Some((u16::from_le_bytes([b[2], b[3]]), u16::from_le_bytes([b[4], b[5]]), 0, presc, b[14] & 0x20 != 0, next));
            }
            return;
        }
        if b[1] != 0x7D || b[15] != 0x87 {
            return;
        }
        let src = u16::from_le_bytes([b[2], b[3]]);
        let len = u16::from_le_bytes([b[4], b[5]]);
        let a_fixed = (b[6] >> 4) & 3 == 2;
        let dst = u16::from_le_bytes([b[11], b[12]]);
        self.dma_bytes += len as u64;
        for i in 0..len {
            let s = if a_fixed { src } else { src.wrapping_add(i) };
            let v = self.rd(s);
            self.wr(dst.wrapping_add(i), v);
        }
        self.dma_transfers += 1;
    }

    /// T-states per byte of a prescaled DMA transfer (875 kHz / prescaler).
    fn dma_byte_t(&self, presc: u8) -> u64 {
        presc as u64 * (4u64 << (self.nextreg[0x07] & 3))
    }

    /// Advance a prescaled DMA transfer to the DAC up to the current time.
    fn run_sample_dma(&mut self) {
        while let Some((start, len, pos, presc, restart, next)) = self.smp {
            if self.t_states < next {
                break;
            }
            let v = self.rd(start.wrapping_add(pos));
            self.dac.push(v);
            let pos = pos + 1;
            let step = self.dma_byte_t(presc);
            self.smp = match (pos >= len, restart) {
                (false, _) => Some((start, len, pos, presc, restart, next + step)),
                (true, true) => Some((start, len, 0, presc, restart, next + step)),
                (true, false) => None,
            };
        }
    }

    fn inp(&mut self, port: u16) -> u8 {
        match port {
            p if p & 0xFF == 0x6B => self.dma_read.pop_front().unwrap_or(0),
            0xFFFD => self.ay[self.ay_chip][self.ay_reg],
            0x253B => self.nextreg[self.nr_sel as usize],
            p if p & 0xFF == 0x1F => self.kempston,
            p if p & 1 == 0 => {
                let mut v = 0x1F;
                for row in 0..8 {
                    if (p >> 8) & (1 << row) == 0 {
                        v &= self.keys[row];
                    }
                }
                v | 0xE0
            }
            _ => 0xFF,
        }
    }

    /// Execute one instruction (or idle while halted), advance the raster
    /// and deliver interrupts (IM 2, vector from I and $FF on the bus).
    pub fn tick(&mut self) -> Result<(), String> {
        let t0 = self.t_states;
        if self.int_pending && self.iff && !self.ei_delay {
            self.int_pending = false;
            self.halted = false;
            self.iff = false;
            self.push(self.pc);
            self.pc = self.rd16(((self.i as u16) << 8) | 0xFF);
            self.t_states += 19;
        }
        self.ei_delay = false;
        self.int_fresh = false;
        if !self.halted {
            self.step()?;
        } else {
            self.t_states += 4;
        }
        self.run_sample_dma();
        // a raster line is 224 T-states at 3.5 MHz, doubled per speed step
        self.line_t += self.t_states - t0;
        let per_line = 224u64 << (self.nextreg[0x07] & 3);
        while self.line_t >= per_line {
            self.line_t -= per_line;
            self.end_line();
        }
        Ok(())
    }

    fn end_line(&mut self) {
        let l = self.line as usize;
        if self.line_state.len() < LINES as usize {
            self.line_state.resize_with(LINES as usize, || Box::new(([0; 256], [[0; 256]; 8])));
        }
        self.line_state[l].0 = self.nextreg;
        self.line_state[l].1 = self.palettes;
        // an /INT not taken during its line is lost
        if !self.int_fresh {
            self.int_pending = false;
        }
        self.line = (self.line + 1) % LINES;
        if self.line == FRAME_START_LINE {
            self.frame_count += 1;
            if self.nextreg[0x62] >> 6 == 3 {
                self.copper_pc = 0;
            }
            if self.nextreg[0x22] & 4 == 0 {
                self.int_pending = true;
                self.int_fresh = true;
            }
        }
        let target = (((self.nextreg[0x22] & 1) as u16) << 8) | self.nextreg[0x23] as u16;
        if self.nextreg[0x22] & 2 != 0 && self.line == target {
            self.int_pending = true;
            self.int_fresh = true;
        }
        self.run_copper();
    }

    /// Run copper instructions at the start of the current line until a
    /// WAIT for another line or HALT.
    fn run_copper(&mut self) {
        if self.nextreg[0x62] >> 6 == 0 {
            return;
        }
        for _ in 0..1024 {
            if self.copper_pc >= 1024 {
                return;
            }
            let (hi, lo) = (self.copper[self.copper_pc * 2], self.copper[self.copper_pc * 2 + 1]);
            if hi == 0xFF && lo == 0xFF {
                return;
            }
            if hi & 0x80 != 0 {
                let wait = (((hi & 1) as u16) << 8) | lo as u16;
                if wait != self.line {
                    return;
                }
            } else if hi != 0 || lo != 0 {
                self.write_nextreg(hi & 0x7F, lo);
            }
            self.copper_pc += 1;
        }
    }

    /// Execute one instruction and count its T-states.
    pub fn step(&mut self) -> Result<(), String> {
        let pc = self.pc;
        let (b0, b1) = (self.rd(pc), self.rd(pc.wrapping_add(1)));
        let (bc0, b_reg0, dma0) = (self.bc(), self.b, self.dma_bytes);
        self.step_inner()?;
        let t = Self::cycles(b0, b1, pc, self.pc, bc0.wrapping_sub(self.bc()), b_reg0.wrapping_sub(self.b));
        self.t_states += t as u64 + (self.dma_bytes - dma0) * 4;
        Ok(())
    }

    /// T-states of the instruction at `pc` (opcode bytes b0 b1) that moved
    /// the PC to `next`; `bc_used` / `b_used`: iterations of block instructions.
    fn cycles(b0: u8, b1: u8, pc: u16, next: u16, bc_used: u16, b_used: u8) -> u32 {
        let jumped = |len: u16| next != pc.wrapping_add(len);
        let (x, y, z) = (b0 >> 6, (b0 >> 3) & 7, b0 & 7);
        let (p, q) = (y >> 1, y & 1);
        match x {
            0 => match z {
                0 => match y {
                    0 | 1 => 4,
                    2 => if jumped(2) { 13 } else { 8 },
                    3 => 12,
                    _ => if jumped(2) { 12 } else { 7 },
                },
                1 => if q == 0 { 10 } else { 11 },
                2 => match (p, q) {
                    (2, _) => 16,
                    (3, _) => 13,
                    _ => 7,
                },
                3 => 6,
                4 | 5 => if y == 6 { 11 } else { 4 },
                6 => if y == 6 { 10 } else { 7 },
                _ => 4,
            },
            1 => if (y == 6) != (z == 6) { 7 } else { 4 },
            2 => if z == 6 { 7 } else { 4 },
            _ => match z {
                0 => if jumped(1) { 11 } else { 5 },
                1 => match (q, p) {
                    (0, _) => 10,
                    (1, 0) => 10,
                    (1, 3) => 6,
                    _ => 4,
                },
                2 => 10,
                3 => match y {
                    0 => 10,
                    1 => {
                        let (bx, bz) = (b1 >> 6, b1 & 7);
                        match (bx, bz) {
                            (_, 6) if bx == 1 => 12,
                            (_, 6) => 15,
                            _ => 8,
                        }
                    }
                    2 | 3 => 11,
                    4 => 19,
                    _ => 4,
                },
                4 => if jumped(3) { 17 } else { 10 },
                5 => match (q, p) {
                    (0, _) => 11,
                    (1, 0) => 17,
                    (1, 2) => Self::ed_cycles(b1, bc_used, b_used),
                    _ => 4,
                },
                6 => 7,
                _ => 11,
            },
        }
    }

    fn ed_cycles(op: u8, bc_used: u16, b_used: u8) -> u32 {
        match op {
            // Z80N
            0x23 | 0x24 | 0x28..=0x2C | 0x30..=0x33 | 0x93 | 0x94 => 8,
            0x34..=0x36 => 16,
            0x8A => 23,
            0x90 => 16,
            0x91 => 20,
            0x92 => 17,
            0x27 => 11,
            // LDIR / LDDR, OTIR / OTDR: 21 per repeated iteration, 16 for the last
            0xB0 | 0xB8 => {
                let n = if bc_used == 0 { 65536 } else { bc_used as u32 };
                21 * n - 5
            }
            0xB3 | 0xBB => {
                let n = if b_used == 0 { 256 } else { b_used as u32 };
                21 * n - 5
            }
            0xA0 | 0xA8 | 0xA3 | 0xAB => 16,
            0x45 | 0x4D => 14,
            0x44 => 8,
            0x47 | 0x4F | 0x57 | 0x5F => 9,
            op if op & 0xC7 == 0x40 || op & 0xC7 == 0x41 => 12,
            op if op & 0xC7 == 0x42 => 15,
            op if op & 0xC7 == 0x43 => 20,
            _ => 8,
        }
    }

    /// Execute one instruction.
    fn step_inner(&mut self) -> Result<(), String> {
        let pc = self.pc;
        let op = self.fetch();
        self.exec(op, pc)
    }

    /// DD / FD prefix: the next instruction uses IX / IY instead of HL.
    fn indexed(&mut self, use_ix: bool, pc: u16) -> Result<(), String> {
        let base = if use_ix { self.ix } else { self.iy };
        let op = self.fetch();
        let (x, y, z) = (op >> 6, (op >> 3) & 7, op & 7);
        let mem = op == 0xCB
            || (x == 1 && (y == 6 || z == 6) && op != 0x76)
            || (x == 2 && z == 6)
            || (x == 0 && y == 6 && (4..=6).contains(&z));
        if mem {
            // (IX+d): the displacement follows the opcode
            let d = self.fetch() as i8;
            self.mem_override = Some(base.wrapping_add(d as u16));
            let r = if op == 0xCB { self.cb() } else { self.exec(op, pc) };
            self.mem_override = None;
            return r;
        }
        // IX / IY in place of HL (ld ix,nn / add ix,rr / push / pop / jp (ix) ...)
        let saved = self.hl();
        self.set_hl(base);
        let r = self.exec(op, pc);
        let v = self.hl();
        self.set_hl(saved);
        if use_ix {
            self.ix = v;
        } else {
            self.iy = v;
        }
        r
    }

    fn exec(&mut self, op: u8, pc: u16) -> Result<(), String> {
        let (x, y, z) = (op >> 6, (op >> 3) & 7, op & 7);
        let (p, q) = (y >> 1, y & 1);
        match x {
            0 => match z {
                0 => match y {
                    0 => {}
                    1 => {
                        std::mem::swap(&mut self.a, &mut self.alt[0]);
                        std::mem::swap(&mut self.f, &mut self.alt[1]);
                    }
                    _ => {
                        let d = self.fetch() as i8;
                        let take = match y {
                            2 => {
                                self.b = self.b.wrapping_sub(1);
                                self.b != 0
                            }
                            3 => true,
                            _ => self.cond(y - 4),
                        };
                        if take {
                            self.pc = self.pc.wrapping_add(d as u16);
                        }
                    }
                },
                1 => {
                    if q == 0 {
                        let v = self.fetch16();
                        self.set_rp(p, v);
                    } else {
                        let r = self.hl() as u32 + self.rp(p) as u32;
                        self.f = (self.f & (SF | ZF | PF)) | if r > 0xFFFF { CF } else { 0 };
                        self.set_hl(r as u16);
                    }
                }
                2 => match (q, p) {
                    (0, 0) => self.wr(self.bc(), self.a),
                    (0, 1) => self.wr(self.de(), self.a),
                    (0, 2) => {
                        let a = self.fetch16();
                        self.wr16(a, self.hl());
                    }
                    (0, _) => {
                        let a = self.fetch16();
                        self.wr(a, self.a);
                    }
                    (_, 0) => self.a = self.rd(self.bc()),
                    (_, 1) => self.a = self.rd(self.de()),
                    (_, 2) => {
                        let a = self.fetch16();
                        let v = self.rd16(a);
                        self.set_hl(v);
                    }
                    _ => {
                        let a = self.fetch16();
                        self.a = self.rd(a);
                    }
                },
                3 => {
                    let v = self.rp(p);
                    self.set_rp(p, if q == 0 { v.wrapping_add(1) } else { v.wrapping_sub(1) });
                }
                4 => {
                    let v = self.r(y).wrapping_add(1);
                    self.f = (self.f & CF) | sz(v) | if v == 0x80 { PF } else { 0 } | if v & 0xF == 0 { HF } else { 0 };
                    self.set_r(y, v);
                }
                5 => {
                    let v = self.r(y).wrapping_sub(1);
                    self.f = (self.f & CF) | sz(v) | NF | if v == 0x7F { PF } else { 0 } | if v & 0xF == 0xF { HF } else { 0 };
                    self.set_r(y, v);
                }
                6 => {
                    let n = self.fetch();
                    self.set_r(y, n);
                }
                _ => {
                    let a = self.a;
                    let c = self.f & CF;
                    let keep = self.f & (SF | ZF | PF);
                    match y {
                        0 => {
                            self.a = a.rotate_left(1);
                            self.f = keep | (a >> 7);
                        }
                        1 => {
                            self.a = a.rotate_right(1);
                            self.f = keep | (a & 1);
                        }
                        2 => {
                            self.a = (a << 1) | c;
                            self.f = keep | (a >> 7);
                        }
                        3 => {
                            self.a = (a >> 1) | (c << 7);
                            self.f = keep | (a & 1);
                        }
                        4 => return Err(format!("DAA not supported at {pc:04X}")),
                        5 => {
                            self.a = !a;
                            self.f |= HF | NF;
                        }
                        6 => self.f = keep | CF,
                        _ => self.f = keep | (c ^ 1),
                    }
                }
            },
            1 => {
                if op == 0x76 {
                    self.halted = true;
                } else {
                    let v = self.r(z);
                    self.set_r(y, v);
                }
            }
            2 => {
                let v = self.r(z);
                self.alu(y, v);
            }
            _ => match z {
                0 => {
                    if self.cond(y) {
                        self.pc = self.pop();
                    }
                }
                1 => match (q, p) {
                    (0, _) => {
                        let v = self.pop();
                        self.set_rp2(p, v);
                    }
                    (_, 0) => self.pc = self.pop(),
                    (_, 1) => {
                        for (i, r) in [self.b, self.c, self.d, self.e, self.h, self.l].into_iter().enumerate() {
                            let o = self.alt[2 + i];
                            self.alt[2 + i] = r;
                            match i {
                                0 => self.b = o, 1 => self.c = o, 2 => self.d = o,
                                3 => self.e = o, 4 => self.h = o, _ => self.l = o,
                            }
                        }
                    }
                    (_, 2) => self.pc = self.hl(),
                    _ => self.sp = self.hl(),
                },
                2 => {
                    let a = self.fetch16();
                    if self.cond(y) {
                        self.pc = a;
                    }
                }
                3 => match y {
                    0 => self.pc = self.fetch16(),
                    1 => self.cb()?,
                    2 => {
                        let n = self.fetch();
                        self.out((self.a as u16) << 8 | n as u16, self.a);
                    }
                    3 => {
                        let n = self.fetch();
                        self.a = self.inp((self.a as u16) << 8 | n as u16);
                    }
                    4 => {
                        let v = self.rd16(self.sp);
                        self.wr16(self.sp, self.hl());
                        self.set_hl(v);
                    }
                    5 => {
                        let (d, e) = (self.d, self.e);
                        self.d = self.h;
                        self.e = self.l;
                        self.h = d;
                        self.l = e;
                    }
                    6 => self.iff = false,
                    _ => {
                        self.iff = true;
                        self.ei_delay = true;
                    }
                },
                4 => {
                    let a = self.fetch16();
                    if self.cond(y) {
                        self.push(self.pc);
                        self.pc = a;
                    }
                }
                5 => match (q, p) {
                    (0, _) => self.push(self.rp2(p)),
                    (_, 0) => {
                        let a = self.fetch16();
                        self.push(self.pc);
                        self.pc = a;
                    }
                    (_, 2) => self.ed(pc)?,
                    (_, 1) => self.indexed(true, pc)?,
                    _ => self.indexed(false, pc)?,
                },
                6 => {
                    let n = self.fetch();
                    self.alu(y, n);
                }
                _ => {
                    self.push(self.pc);
                    self.pc = (y as u16) * 8;
                }
            },
        }
        Ok(())
    }

    fn cb(&mut self) -> Result<(), String> {
        let op = self.fetch();
        let (x, y, z) = (op >> 6, (op >> 3) & 7, op & 7);
        let v = self.r(z);
        match x {
            0 => {
                let c = self.f & CF;
                let (r, nc) = match y {
                    0 => (v.rotate_left(1), v >> 7),
                    1 => (v.rotate_right(1), v & 1),
                    2 => ((v << 1) | c, v >> 7),
                    3 => ((v >> 1) | (c << 7), v & 1),
                    4 => (v << 1, v >> 7),
                    5 => ((v >> 1) | (v & 0x80), v & 1),
                    6 => ((v << 1) | 1, v >> 7),
                    _ => (v >> 1, v & 1),
                };
                self.f = szp(r) | nc;
                self.set_r(z, r);
            }
            1 => {
                let bit = v & (1 << y);
                self.f = (self.f & CF) | HF | if bit == 0 { ZF | PF } else { 0 } | (bit & SF);
            }
            2 => self.set_r(z, v & !(1 << y)),
            _ => self.set_r(z, v | (1 << y)),
        }
        Ok(())
    }

    fn ed(&mut self, pc: u16) -> Result<(), String> {
        let op = self.fetch();
        // Z80N extensions
        match op {
            0x23 => {
                self.a = self.a.rotate_left(4);
                return Ok(());
            }
            0x24 => {
                self.a = self.a.reverse_bits();
                return Ok(());
            }
            0x28..=0x2C => {
                let n = (self.b & 31) as u32;
                let de = self.de();
                let r = match op {
                    0x28 => de.checked_shl(n).unwrap_or(0),
                    0x29 => ((de as i16) >> n.min(15)) as u16,
                    0x2A => de.checked_shr(n).unwrap_or(0),
                    0x2B => !((!de).checked_shr(n).unwrap_or(0)),
                    _ => de.rotate_left(n & 15),
                };
                self.set_de(r);
                return Ok(());
            }
            0x30 => {
                let r = self.d as u16 * self.e as u16;
                self.set_de(r);
                return Ok(());
            }
            0x31 => {
                let v = self.hl().wrapping_add(self.a as u16);
                self.set_hl(v);
                return Ok(());
            }
            0x32 => {
                let v = self.de().wrapping_add(self.a as u16);
                self.set_de(v);
                return Ok(());
            }
            0x33 => {
                let v = self.bc().wrapping_add(self.a as u16);
                self.set_bc(v);
                return Ok(());
            }
            0x34..=0x36 => {
                let n = self.fetch16();
                match op {
                    0x34 => self.set_hl(self.hl().wrapping_add(n)),
                    0x35 => self.set_de(self.de().wrapping_add(n)),
                    _ => self.set_bc(self.bc().wrapping_add(n)),
                }
                return Ok(());
            }
            0x8A => {
                // PUSH nn (big endian operand)
                let hi = self.fetch() as u16;
                let lo = self.fetch() as u16;
                self.push(hi << 8 | lo);
                return Ok(());
            }
            0x91 => {
                let r = self.fetch();
                let v = self.fetch();
                self.write_nextreg(r, v);
                return Ok(());
            }
            0x92 => {
                let r = self.fetch();
                self.write_nextreg(r, self.a);
                return Ok(());
            }
            _ => {}
        }
        let (x, y, z) = (op >> 6, (op >> 3) & 7, op & 7);
        let (p, q) = (y >> 1, y & 1);
        match (x, z) {
            (1, 0) => {
                let v = self.inp(self.bc());
                self.f = (self.f & CF) | szp(v);
                if y != 6 {
                    self.set_r(y, v);
                }
            }
            (1, 1) => {
                let v = if y == 6 { 0 } else { self.r(y) };
                self.out(self.bc(), v);
            }
            (1, 2) => {
                let v = self.rp(p);
                self.adc16(v, q == 0);
            }
            (1, 3) => {
                let a = self.fetch16();
                if q == 0 {
                    self.wr16(a, self.rp(p));
                } else {
                    let v = self.rd16(a);
                    self.set_rp(p, v);
                }
            }
            (1, 4) => {
                let v = self.a;
                self.a = 0;
                self.sub8(v, false, true);
            }
            (1, 5) => self.pc = self.pop(),
            (1, 6) => {}
            (1, 7) => match y {
                0 => self.i = self.a,
                2 => {
                    // LD A,I: P/V = IFF2
                    self.a = self.i;
                    self.f = (self.f & CF) | sz(self.a) | if self.iff { PF } else { 0 };
                }
                _ => {}
            },
            (2, 0) if y >= 4 => {
                // LDI / LDD / LDIR / LDDR
                loop {
                    let v = self.rd(self.hl());
                    self.wr(self.de(), v);
                    let step: u16 = if y & 1 == 0 { 1 } else { 0xFFFF };
                    self.set_hl(self.hl().wrapping_add(step));
                    self.set_de(self.de().wrapping_add(step));
                    self.set_bc(self.bc().wrapping_sub(1));
                    if y < 6 || self.bc() == 0 {
                        break;
                    }
                }
                self.f = (self.f & (SF | ZF | CF)) | if self.bc() != 0 { PF } else { 0 };
            }
            (2, 3) if y >= 4 => {
                // OUTI / OUTD / OTIR / OTDR
                loop {
                    let v = self.rd(self.hl());
                    self.b = self.b.wrapping_sub(1);
                    self.out(self.bc(), v);
                    let step: u16 = if y & 1 == 0 { 1 } else { 0xFFFF };
                    self.set_hl(self.hl().wrapping_add(step));
                    if y < 6 || self.b == 0 {
                        break;
                    }
                }
                self.f = (self.f & CF) | NF | if self.b == 0 { ZF } else { 0 };
            }
            _ => return Err(format!("unsupported ED {op:02X} at {pc:04X}")),
        }
        Ok(())
    }
}

/// Load the banks of a NEX file into a fresh CPU and return it with PC/SP set.
pub fn load_nex(nex: &[u8]) -> Result<Cpu, String> {
    if nex.len() < 512 || &nex[0..4] != b"Next" {
        return Err("not a NEX file".into());
    }
    let mut cpu = Cpu::new();
    let present = &nex[18..18 + 112];
    let mut order: Vec<usize> = vec![5, 2, 0, 1, 3, 4];
    order.extend(6..112);
    let mut off = 512;
    if nex[10] != 0 {
        return Err("NEX loading screens are not supported by the test loader".into());
    }
    for bank in order {
        if present[bank] == 0 {
            continue;
        }
        let data = nex.get(off..off + 16384).ok_or("truncated NEX")?;
        off += 16384;
        let addr = match bank {
            5 => 0x4000,
            2 => 0x8000,
            0 => 0xC000,
            _ => {
                // other banks: two 8K pages, reachable through the MMU
                cpu.pages.insert(bank as u8 * 2, data[..8192].to_vec());
                cpu.pages.insert(bank as u8 * 2 + 1, data[8192..].to_vec());
                continue;
            }
        };
        cpu.mem[addr..addr + 16384].copy_from_slice(data);
    }
    cpu.sp = u16::from_le_bytes([nex[12], nex[13]]);
    cpu.pc = u16::from_le_bytes([nex[14], nex[15]]);
    Ok(cpu)
}
