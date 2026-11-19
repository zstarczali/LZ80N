//! Compile-time asset conversion (`asset name = "file"`).
//!
//! * `.png`  -> 8-bit hardware sprite patterns: the image is cut into 16x16
//!   cells (left to right, top to bottom), each pixel becomes an RRRGGGBB
//!   index for the Next's default sprite palette. Pixels with alpha < 128
//!   become the transparency index $E3; opaque pixels that would map to $E3
//!   are nudged to $E2 so they stay visible.
//! * `.spr`, `.bin`, `.nxp`, anything else -> raw bytes, embedded unchanged.

use std::path::Path;

pub const TRANSPARENT: u8 = 0xE3;

pub struct Asset {
    pub bytes: Vec<u8>,
    /// Number of 256-byte sprite patterns (only for sprite assets).
    pub patterns: Option<u32>,
}

pub fn load(path: &Path) -> Result<Asset, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let data = std::fs::read(path).map_err(|e| format!("cannot read `{}`: {e}", path.display()))?;
    match ext.as_str() {
        "png" => png_to_patterns(&data).map_err(|e| format!("`{}`: {e}", path.display())),
        "spr" => {
            if data.len() % 256 != 0 || data.is_empty() {
                return Err(format!("`{}`: .spr size must be a multiple of 256 bytes", path.display()));
            }
            Ok(Asset { patterns: Some((data.len() / 256) as u32), bytes: data })
        }
        _ => Ok(Asset { bytes: data, patterns: None }),
    }
}

pub fn rgb_to_332(r: u8, g: u8, b: u8) -> u8 {
    let v = (r & 0xE0) | ((g >> 3) & 0x1C) | (b >> 6);
    if v == TRANSPARENT { 0xE2 } else { v }
}

pub fn png_to_patterns(data: &[u8]) -> Result<Asset, String> {
    let mut dec = png::Decoder::new(data);
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let (w, h) = (info.width as usize, info.height as usize);
    if w % 16 != 0 || h % 16 != 0 || w == 0 || h == 0 {
        return Err(format!("image is {w}x{h}; sprite sheets must be multiples of 16x16"));
    }
    let channels = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Grayscale => 1,
        png::ColorType::Indexed => return Err("unexpected indexed output".into()),
    };
    let px = |x: usize, y: usize| -> u8 {
        let o = (y * w + x) * channels;
        let p = &buf[o..o + channels];
        let (r, g, b, a) = match channels {
            4 => (p[0], p[1], p[2], p[3]),
            3 => (p[0], p[1], p[2], 255),
            2 => (p[0], p[0], p[0], p[1]),
            _ => (p[0], p[0], p[0], 255),
        };
        if a < 128 { TRANSPARENT } else { rgb_to_332(r, g, b) }
    };
    let mut bytes = Vec::with_capacity(w * h);
    for cy in (0..h).step_by(16) {
        for cx in (0..w).step_by(16) {
            for y in 0..16 {
                for x in 0..16 {
                    bytes.push(px(cx + x, cy + y));
                }
            }
        }
    }
    let n = (bytes.len() / 256) as u32;
    if n > 64 {
        return Err(format!("{n} patterns; the Next holds at most 64 8-bit sprite patterns"));
    }
    Ok(Asset { bytes, patterns: Some(n) })
}

/// Encode an RGBA image as PNG (used by tests and tools).
#[cfg(test)]
pub fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().unwrap();
        wr.write_image_data(rgba).unwrap();
    }
    out
}
