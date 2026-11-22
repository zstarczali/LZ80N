//! NEX file writer (format V1.2): a 512-byte header, then the 16K banks in
//! NEX order (5, 2, 0, 1, 3, 4, 6, 7, 8 ... 111).

pub const BANKS: usize = 112;

/// Banks in the order a NEX file stores them.
pub fn nex_order() -> impl Iterator<Item = u8> {
    [5u8, 2, 0, 1, 3, 4].into_iter().chain(6..BANKS as u8)
}

fn order_index(bank: u8) -> usize {
    nex_order().position(|b| b == bank).unwrap()
}

#[derive(Debug, Clone, Default)]
pub struct Nex {
    pub file: String,
    pub pc: u16,
    pub sp: u16,
    pub entry_bank: u8,
    pub core: [u8; 3],
    pub border: u8,
    pub file_handle: u16,
    pub preserve_regs: bool,
    pub ram_2mb: bool,
    /// (bank, 16384 bytes)
    pub banks: Vec<(u8, Vec<u8>)>,
}

impl Nex {
    pub fn has_bank(&self, bank: u8) -> bool {
        self.banks.iter().any(|(b, _)| *b == bank)
    }

    pub fn build(&self) -> Vec<u8> {
        let mut banks = self.banks.clone();
        banks.sort_by_key(|(b, _)| order_index(*b));
        let mut h = vec![0u8; 512];
        h[0..4].copy_from_slice(b"Next");
        h[4..8].copy_from_slice(b"V1.2");
        h[8] = (self.ram_2mb || banks.iter().any(|(b, _)| *b >= 48)) as u8;
        h[9] = banks.len() as u8;
        h[10] = 0; // no loading screen
        h[11] = self.border & 7;
        h[12..14].copy_from_slice(&self.sp.to_le_bytes());
        h[14..16].copy_from_slice(&self.pc.to_le_bytes());
        // 16..18: extra files = 0
        for (b, _) in &banks {
            h[18 + *b as usize] = 1;
        }
        h[134] = self.preserve_regs as u8;
        h[135..138].copy_from_slice(&self.core);
        h[139] = self.entry_bank;
        h[140..142].copy_from_slice(&self.file_handle.to_le_bytes());
        // 142..: V1.3 fields (bank offset, checksum ...) stay 0 in a V1.2
        // header, as SjASMPlus writes it
        let mut out = h;
        for (_, data) in banks {
            out.extend_from_slice(&data);
        }
        out
    }
}
