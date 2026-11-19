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

/// A picture for Layer 2: RRRGGGBB pixels, row by row. Rows are packed into
/// 8K pages so that no row crosses a page boundary.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rows_per_page: u32,
    /// Has TRANSPARENT pixels (drawn with a masked copy).
    pub masked: bool,
    /// One entry per 8K page (each at most 8192 bytes).
    pub pages: Vec<Vec<u8>>,
}

pub fn load_image(path: &Path) -> Result<Image, String> {
    let data = std::fs::read(path).map_err(|e| format!("cannot read `{}`: {e}", path.display()))?;
    let (w, h, px) = decode_png(&data).map_err(|e| format!("`{}`: {e}", path.display()))?;
    if w == 0 || h == 0 || w > 256 || h > 255 {
        return Err(format!("`{}` is {w}x{h}; images can be at most 256x255 pixels", path.display()));
    }
    let rows_per_page = (8192 / w).min(255);
    let masked = px.contains(&TRANSPARENT);
    let pages = px
        .chunks(w * h)
        .next()
        .unwrap()
        .chunks(w * rows_per_page)
        .map(|c| c.to_vec())
        .collect();
    Ok(Image { width: w as u32, height: h as u32, rows_per_page: rows_per_page as u32, masked, pages })
}

/// Decode a PNG into RRRGGGBB pixels (alpha < 128 -> TRANSPARENT).
fn decode_png(data: &[u8]) -> Result<(usize, usize, Vec<u8>), String> {
    let mut dec = png::Decoder::new(data);
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let (w, h) = (info.width as usize, info.height as usize);
    let channels = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Grayscale => 1,
        png::ColorType::Indexed => return Err("unexpected indexed output".into()),
    };
    let px = (0..w * h)
        .map(|i| {
            let p = &buf[i * channels..i * channels + channels];
            let (r, g, b, a) = match channels {
                4 => (p[0], p[1], p[2], p[3]),
                3 => (p[0], p[1], p[2], 255),
                2 => (p[0], p[0], p[0], p[1]),
                _ => (p[0], p[0], p[0], 255),
            };
            if a < 128 { TRANSPARENT } else { rgb_to_332(r, g, b) }
        })
        .collect();
    Ok((w, h, px))
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

/// A sound sample for the DAC: unsigned 8-bit mono, played at
/// 875 kHz / `prescaler` by the zxnDMA.
pub struct Sample {
    pub prescaler: u8,
    pub bytes: Vec<u8>,
}

/// The playback rate of a prescaler value.
pub fn sample_rate(prescaler: u8) -> u32 {
    875_000 / prescaler.max(1) as u32
}

/// Load a PCM .wav file (8 or 16 bit, mono or stereo). Rates from 4 kHz to
/// 31.25 kHz are kept (to the nearest DMA rate); others are resampled to
/// 15.6 kHz.
pub fn load_wav(path: &Path) -> Result<Sample, String> {
    let data = std::fs::read(path).map_err(|e| format!("cannot read `{}`: {e}", path.display()))?;
    wav_to_sample(&data).map_err(|e| format!("`{}`: {e}", path.display()))
}

pub fn wav_to_sample(data: &[u8]) -> Result<Sample, String> {
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err("not a .wav file".into());
    }
    let u16_at = |o: usize| u16::from_le_bytes([data[o], data[o + 1]]);
    let u32_at = |o: usize| u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
    let (mut fmt, mut pcm): (Option<(u16, u16, u32, u16)>, Option<&[u8]>) = (None, None);
    let mut o = 12;
    while o + 8 <= data.len() {
        let size = u32_at(o + 4) as usize;
        let body = &data[o + 8..(o + 8 + size).min(data.len())];
        match &data[o..o + 4] {
            b"fmt " if body.len() >= 16 => {
                fmt = Some((u16::from_le_bytes([body[0], body[1]]), u16::from_le_bytes([body[2], body[3]]),
                    u32::from_le_bytes([body[4], body[5], body[6], body[7]]), u16::from_le_bytes([body[14], body[15]])))
            }
            b"data" => pcm = Some(body),
            _ => {}
        }
        o += 8 + size + (size & 1);
    }
    let _ = u16_at;
    let (format, channels, rate, bits) = fmt.ok_or("no `fmt ` chunk")?;
    let pcm = pcm.ok_or("no `data` chunk")?;
    if format != 1 || !(bits == 8 || bits == 16) || channels == 0 {
        return Err(format!("only uncompressed 8 or 16 bit PCM is supported (format {format}, {bits} bits)"));
    }
    // to unsigned 8-bit mono
    let frame = channels as usize * bits as usize / 8;
    let mono: Vec<u8> = pcm
        .chunks_exact(frame)
        .map(|f| {
            let sum: i32 = (0..channels as usize)
                .map(|c| {
                    if bits == 8 {
                        f[c] as i32 - 128
                    } else {
                        i16::from_le_bytes([f[2 * c], f[2 * c + 1]]) as i32 >> 8
                    }
                })
                .sum();
            (sum / channels as i32 + 128).clamp(0, 255) as u8
        })
        .collect();
    if mono.is_empty() {
        return Err("the sample is empty".into());
    }
    let (prescaler, bytes) = if (4000..=31250).contains(&rate) {
        (((875_000 + rate / 2) / rate) as u8, mono)
    } else {
        // resample to 15.6 kHz (prescaler 56), linear interpolation
        let p = 56u8;
        let out_rate = sample_rate(p) as f64;
        let n = ((mono.len() as f64) * out_rate / rate as f64).max(1.0) as usize;
        let step = rate as f64 / out_rate;
        let v = (0..n)
            .map(|i| {
                let x = i as f64 * step;
                let k = x as usize;
                let a = mono[k.min(mono.len() - 1)] as f64;
                let b = mono[(k + 1).min(mono.len() - 1)] as f64;
                (a + (b - a) * (x - k as f64)).round() as u8
            })
            .collect();
        (p, v)
    };
    if bytes.len() >= 1 << 24 {
        return Err("the sample is longer than 16 MB".into());
    }
    Ok(Sample { prescaler, bytes })
}

/// A .wav file (8-bit mono PCM) for tests.
#[cfg(test)]
pub fn encode_wav(rate: u32, samples: &[u8]) -> Vec<u8> {
    let mut w = Vec::new();
    w.extend(b"RIFF");
    w.extend(&(36 + samples.len() as u32).to_le_bytes());
    w.extend(b"WAVEfmt ");
    w.extend(&16u32.to_le_bytes());
    w.extend(&1u16.to_le_bytes());
    w.extend(&1u16.to_le_bytes());
    w.extend(&rate.to_le_bytes());
    w.extend(&rate.to_le_bytes());
    w.extend(&1u16.to_le_bytes());
    w.extend(&8u16.to_le_bytes());
    w.extend(b"data");
    w.extend(&(samples.len() as u32).to_le_bytes());
    w.extend(samples);
    w
}
