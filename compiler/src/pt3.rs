//! ProTracker 3 / Vortex Tracker II modules (.pt3), rendered when compiling
//! into AY register values for every 50 Hz frame. The playback rules follow
//! the Vortex Tracker II engine as implemented by zxtune (LGPL).
//!
//! Stream format (what the runtime player reads, one entry per frame):
//! `db mask R0-R7, mask R8-R13` then the values of the registers whose bit is
//! set, in register order. Mask byte 2 bit 7 = end of the song.

/// One frame: registers R0-R13; `env` = R13 is written this frame (writing
/// R13 restarts the envelope, so it is only sent when the tune sets it).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Frame {
    pub regs: [u8; 14],
    pub env: bool,
}

pub struct Song {
    pub frames: Vec<Frame>,
    /// frame the song continues from after its end
    pub loop_frame: usize,
}

const TABLE_PT3_3: [u16; 96] = [
    0x0c21, 0x0b73, 0x0ace, 0x0a33, 0x09a0, 0x0916, 0x0893, 0x0818, 0x07a4, 0x0736, 0x06ce, 0x066d, 0x0610, 0x05b9,
    0x0567, 0x0519, 0x04d0, 0x048b, 0x0449, 0x040c, 0x03d2, 0x039b, 0x0367, 0x0336, 0x0308, 0x02dc, 0x02b3, 0x028c,
    0x0268, 0x0245, 0x0224, 0x0206, 0x01e9, 0x01cd, 0x01b3, 0x019b, 0x0184, 0x016e, 0x0159, 0x0146, 0x0134, 0x0122,
    0x0112, 0x0103, 0x00f4, 0x00e6, 0x00d9, 0x00cd, 0x00c2, 0x00b7, 0x00ac, 0x00a3, 0x009a, 0x0091, 0x0089, 0x0081,
    0x007a, 0x0073, 0x006c, 0x0066, 0x0061, 0x005b, 0x0056, 0x0051, 0x004d, 0x0048, 0x0044, 0x0040, 0x003d, 0x0039,
    0x0036, 0x0033, 0x0030, 0x002d, 0x002b, 0x0028, 0x0026, 0x0024, 0x0022, 0x0020, 0x001e, 0x001c, 0x001b, 0x0019,
    0x0018, 0x0016, 0x0015, 0x0014, 0x0013, 0x0012, 0x0011, 0x0010, 0x000f, 0x000e, 0x000d, 0x000c,
];
const TABLE_PT3_4: [u16; 96] = [
    0x0c22, 0x0b73, 0x0acf, 0x0a33, 0x09a1, 0x0917, 0x0894, 0x0819, 0x07a4, 0x0737, 0x06cf, 0x066d, 0x0611, 0x05ba,
    0x0567, 0x051a, 0x04d0, 0x048b, 0x044a, 0x040c, 0x03d2, 0x039b, 0x0367, 0x0337, 0x0308, 0x02dd, 0x02b4, 0x028d,
    0x0268, 0x0246, 0x0225, 0x0206, 0x01e9, 0x01ce, 0x01b4, 0x019b, 0x0184, 0x016e, 0x015a, 0x0146, 0x0134, 0x0123,
    0x0112, 0x0103, 0x00f5, 0x00e7, 0x00da, 0x00ce, 0x00c2, 0x00b7, 0x00ad, 0x00a3, 0x009a, 0x0091, 0x0089, 0x0082,
    0x007a, 0x0073, 0x006d, 0x0067, 0x0061, 0x005c, 0x0056, 0x0052, 0x004d, 0x0049, 0x0045, 0x0041, 0x003d, 0x003a,
    0x0036, 0x0033, 0x0031, 0x002e, 0x002b, 0x0029, 0x0027, 0x0024, 0x0022, 0x0020, 0x001f, 0x001d, 0x001b, 0x001a,
    0x0018, 0x0017, 0x0016, 0x0014, 0x0013, 0x0012, 0x0011, 0x0010, 0x000f, 0x000e, 0x000d, 0x000c,
];
const TABLE_ST: [u16; 96] = [
    0x0EF8, 0x0E10, 0x0D60, 0x0C80, 0x0BD8, 0x0B28, 0x0A88, 0x09F0, 0x0960, 0x08E0, 0x0858, 0x07E0, 0x077C, 0x0708,
    0x06B0, 0x0640, 0x05EC, 0x0594, 0x0544, 0x04F8, 0x04B0, 0x0470, 0x042C, 0x03FD, 0x03BE, 0x0384, 0x0358, 0x0320,
    0x02F6, 0x02CA, 0x02A2, 0x027C, 0x0258, 0x0238, 0x0216, 0x01F8, 0x01DF, 0x01C2, 0x01AC, 0x0190, 0x017B, 0x0165,
    0x0151, 0x013E, 0x012C, 0x011C, 0x010A, 0x00FC, 0x00EF, 0x00E1, 0x00D6, 0x00C8, 0x00BD, 0x00B2, 0x00A8, 0x009F,
    0x0096, 0x008E, 0x0085, 0x007E, 0x0077, 0x0070, 0x006B, 0x0064, 0x005E, 0x0059, 0x0054, 0x004F, 0x004B, 0x0047,
    0x0042, 0x003F, 0x003B, 0x0038, 0x0035, 0x0032, 0x002F, 0x002C, 0x002A, 0x0027, 0x0025, 0x0023, 0x0021, 0x001F,
    0x001D, 0x001C, 0x001A, 0x0019, 0x0017, 0x0016, 0x0015, 0x0013, 0x0012, 0x0011, 0x0010, 0x000F,
];
const TABLE_ASM_3: [u16; 96] = [
    0x0d3e, 0x0c80, 0x0bcc, 0x0b22, 0x0a82, 0x09ec, 0x095c, 0x08d6, 0x0858, 0x07e0, 0x076e, 0x0704, 0x069f, 0x0640,
    0x05e6, 0x0591, 0x0541, 0x04f6, 0x04ae, 0x046b, 0x042c, 0x03f0, 0x03b7, 0x0382, 0x034f, 0x0320, 0x02f3, 0x02c8,
    0x02a1, 0x027b, 0x0257, 0x0236, 0x0216, 0x01f8, 0x01dc, 0x01c1, 0x01a8, 0x0190, 0x0179, 0x0164, 0x0150, 0x013d,
    0x012c, 0x011b, 0x010b, 0x00fc, 0x00ee, 0x00e0, 0x00d4, 0x00c8, 0x00bd, 0x00b2, 0x00a8, 0x009f, 0x0096, 0x008d,
    0x0085, 0x007e, 0x0077, 0x0070, 0x006a, 0x0064, 0x005e, 0x0059, 0x0054, 0x0050, 0x004b, 0x0047, 0x0043, 0x003f,
    0x003c, 0x0038, 0x0035, 0x0032, 0x002f, 0x002d, 0x002a, 0x0028, 0x0026, 0x0024, 0x0022, 0x0020, 0x001e, 0x001d,
    0x001b, 0x001a, 0x0019, 0x0018, 0x0015, 0x0014, 0x0013, 0x0012, 0x0011, 0x0010, 0x000f, 0x000e,
];
const TABLE_ASM_4: [u16; 96] = [
    0x0d10, 0x0c55, 0x0ba4, 0x0afc, 0x0a5f, 0x09ca, 0x093d, 0x08b8, 0x083b, 0x07c5, 0x0755, 0x06ec, 0x0688, 0x062a,
    0x05d2, 0x057e, 0x052f, 0x04e5, 0x049e, 0x045c, 0x041d, 0x03e2, 0x03ab, 0x0376, 0x0344, 0x0315, 0x02e9, 0x02bf,
    0x0298, 0x0272, 0x024f, 0x022e, 0x020f, 0x01f1, 0x01d5, 0x01bb, 0x01a2, 0x018b, 0x0174, 0x0160, 0x014c, 0x0139,
    0x0128, 0x0117, 0x0107, 0x00f9, 0x00eb, 0x00dd, 0x00d1, 0x00c5, 0x00ba, 0x00b0, 0x00a6, 0x009d, 0x0094, 0x008c,
    0x0084, 0x007c, 0x0075, 0x006f, 0x0069, 0x0063, 0x005d, 0x0058, 0x0053, 0x004e, 0x004a, 0x0046, 0x0042, 0x003e,
    0x003b, 0x0037, 0x0034, 0x0031, 0x002f, 0x002c, 0x0029, 0x0027, 0x0025, 0x0023, 0x0021, 0x001f, 0x001d, 0x001c,
    0x001a, 0x0019, 0x0017, 0x0016, 0x0015, 0x0014, 0x0012, 0x0011, 0x0010, 0x000f, 0x000e, 0x000d,
];
const TABLE_REAL_3: [u16; 96] = [
    0x0cda, 0x0c22, 0x0b73, 0x0acf, 0x0a33, 0x09a1, 0x0917, 0x0894, 0x0819, 0x07a4, 0x0737, 0x06cf, 0x066d, 0x0611,
    0x05ba, 0x0567, 0x051a, 0x04d0, 0x048b, 0x044a, 0x040c, 0x03d2, 0x039b, 0x0367, 0x0337, 0x0308, 0x02dd, 0x02b4,
    0x028d, 0x0268, 0x0246, 0x0225, 0x0206, 0x01e9, 0x01ce, 0x01b4, 0x019b, 0x0184, 0x016e, 0x015a, 0x0146, 0x0134,
    0x0123, 0x0113, 0x0103, 0x00f5, 0x00e7, 0x00da, 0x00ce, 0x00c2, 0x00b7, 0x00ad, 0x00a3, 0x009a, 0x0091, 0x0089,
    0x0082, 0x007a, 0x0073, 0x006d, 0x0067, 0x0061, 0x005c, 0x0056, 0x0052, 0x004d, 0x0049, 0x0045, 0x0041, 0x003d,
    0x003a, 0x0036, 0x0033, 0x0031, 0x002e, 0x002b, 0x0029, 0x0027, 0x0024, 0x0022, 0x0020, 0x001f, 0x001d, 0x001b,
    0x001a, 0x0018, 0x0017, 0x0016, 0x0014, 0x0013, 0x0012, 0x0011, 0x0010, 0x000f, 0x000e, 0x000d,
];
/// the 3.4 "real" table is the 3.3 one except one entry
const TABLE_REAL_4: [u16; 96] = {
    let mut t = TABLE_REAL_3;
    t[43] = 0x0112;
    t
};
const TABLE_NATURAL: [u16; 96] = [
    2880, 2700, 2560, 2400, 2304, 2160, 2025, 1920, 1800, 1728, 1620, 1536, 1440, 1350, 1280, 1200, 1152, 1080, 1013,
    960, 900, 864, 810, 768, 720, 675, 640, 600, 576, 540, 506, 480, 450, 432, 405, 384, 360, 338, 320, 300, 288, 270,
    253, 240, 225, 216, 203, 192, 180, 169, 160, 150, 144, 135, 127, 120, 113, 108, 101, 96, 90, 84, 80, 75, 72, 68, 63,
    60, 56, 54, 51, 48, 45, 42, 40, 38, 36, 34, 32, 30, 28, 27, 25, 24, 23, 21, 20, 19, 18, 17, 16, 15, 14, 14, 13, 12,
];

/// Volume tables (index volume * 16 + level): PT 3.0-3.4 and 3.5+.
const VOL_33_34: [u8; 256] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x02, 0x02, 0x02, 0x02, 0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x01, 0x01, 0x02,
    0x02, 0x02, 0x02, 0x03, 0x03, 0x03, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x01, 0x02, 0x02, 0x02, 0x03, 0x03,
    0x03, 0x04, 0x04, 0x04, 0x00, 0x00, 0x00, 0x01, 0x01, 0x01, 0x02, 0x02, 0x03, 0x03, 0x03, 0x04, 0x04, 0x04, 0x05,
    0x05, 0x00, 0x00, 0x00, 0x01, 0x01, 0x02, 0x02, 0x03, 0x03, 0x03, 0x04, 0x04, 0x05, 0x05, 0x06, 0x06, 0x00, 0x00,
    0x01, 0x01, 0x02, 0x02, 0x03, 0x03, 0x04, 0x04, 0x05, 0x05, 0x06, 0x06, 0x07, 0x07, 0x00, 0x00, 0x01, 0x01, 0x02,
    0x02, 0x03, 0x03, 0x04, 0x05, 0x05, 0x06, 0x06, 0x07, 0x07, 0x08, 0x00, 0x00, 0x01, 0x01, 0x02, 0x03, 0x03, 0x04,
    0x05, 0x05, 0x06, 0x06, 0x07, 0x08, 0x08, 0x09, 0x00, 0x00, 0x01, 0x02, 0x02, 0x03, 0x04, 0x04, 0x05, 0x06, 0x06,
    0x07, 0x08, 0x08, 0x09, 0x0a, 0x00, 0x00, 0x01, 0x02, 0x03, 0x03, 0x04, 0x05, 0x06, 0x06, 0x07, 0x08, 0x09, 0x09,
    0x0a, 0x0b, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x04, 0x05, 0x06, 0x07, 0x08, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x00,
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x00, 0x00, 0x01, 0x02,
    0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06,
    0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
];
const VOL_35: [u8; 256] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x02, 0x02, 0x02, 0x02, 0x00, 0x00, 0x00, 0x01, 0x01, 0x01, 0x01, 0x01, 0x02,
    0x02, 0x02, 0x02, 0x02, 0x03, 0x03, 0x03, 0x00, 0x00, 0x01, 0x01, 0x01, 0x01, 0x02, 0x02, 0x02, 0x02, 0x03, 0x03,
    0x03, 0x03, 0x04, 0x04, 0x00, 0x00, 0x01, 0x01, 0x01, 0x02, 0x02, 0x02, 0x03, 0x03, 0x03, 0x04, 0x04, 0x04, 0x05,
    0x05, 0x00, 0x00, 0x01, 0x01, 0x02, 0x02, 0x02, 0x03, 0x03, 0x04, 0x04, 0x04, 0x05, 0x05, 0x06, 0x06, 0x00, 0x00,
    0x01, 0x01, 0x02, 0x02, 0x03, 0x03, 0x04, 0x04, 0x05, 0x05, 0x06, 0x06, 0x07, 0x07, 0x00, 0x01, 0x01, 0x02, 0x02,
    0x03, 0x03, 0x04, 0x04, 0x05, 0x05, 0x06, 0x06, 0x07, 0x07, 0x08, 0x00, 0x01, 0x01, 0x02, 0x02, 0x03, 0x04, 0x04,
    0x05, 0x05, 0x06, 0x07, 0x07, 0x08, 0x08, 0x09, 0x00, 0x01, 0x01, 0x02, 0x03, 0x03, 0x04, 0x05, 0x05, 0x06, 0x07,
    0x07, 0x08, 0x09, 0x09, 0x0a, 0x00, 0x01, 0x01, 0x02, 0x03, 0x04, 0x04, 0x05, 0x06, 0x07, 0x07, 0x08, 0x09, 0x0a,
    0x0a, 0x0b, 0x00, 0x01, 0x02, 0x02, 0x03, 0x04, 0x05, 0x06, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0a, 0x0b, 0x0c, 0x00,
    0x01, 0x02, 0x03, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0a, 0x0b, 0x0c, 0x0d, 0x00, 0x01, 0x02, 0x03,
    0x04, 0x05, 0x06, 0x07, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06,
    0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
];

/// about 20 minutes: a module that never ends is an error
const MAX_FRAMES: usize = 60_000;

#[derive(Clone, Copy)]
struct Line {
    level: i32,
    vol_slide: i32,
    noise_env: i32,
    env_mask: bool,
    noise_mask: bool,
    tone_mask: bool,
    keep_tone: bool,
    keep_noise_env: bool,
    tone: i32,
}

struct Sample {
    looping: usize,
    lines: Vec<Line>,
}

struct Ornament {
    looping: usize,
    offsets: Vec<i32>,
}

#[derive(Clone, Copy, Default)]
struct Slider {
    period: i32,
    counter: i32,
    delta: i32,
    value: i32,
}

impl Slider {
    fn update(&mut self) -> bool {
        if self.counter != 0 {
            self.counter -= 1;
            if self.counter == 0 {
                self.value += self.delta;
                self.counter = self.period;
                return true;
            }
        }
        false
    }
    fn reset(&mut self) {
        self.counter = 0;
        self.value = 0;
    }
}

const NO_TARGET: i32 = -1;

#[derive(Clone)]
struct Chan {
    // pattern reader
    ptr: usize,
    skip: u32,
    left: u32,
    // playback
    enabled: bool,
    envelope: bool,
    note: i32,
    prev_note: i32,
    sample: usize,
    pos_sample: usize,
    ornament: usize,
    pos_orn: usize,
    volume: i32,
    vol_slide: i32,
    slider: Slider,
    target: i32,
    slide_delta: i32,
    tone_acc: i32,
    env_sliding: i32,
    noise_sliding: i32,
    vib_counter: i32,
    vib_on: i32,
    vib_off: i32,
}

impl Chan {
    fn new() -> Self {
        Chan {
            ptr: 0,
            skip: 1,
            left: 0,
            enabled: false,
            envelope: false,
            note: 0,
            prev_note: 0,
            sample: 1,
            pos_sample: 0,
            ornament: 0,
            pos_orn: 0,
            volume: 15,
            vol_slide: 0,
            slider: Slider::default(),
            target: NO_TARGET,
            slide_delta: 0,
            tone_acc: 0,
            env_sliding: 0,
            noise_sliding: 0,
            vib_counter: 0,
            vib_on: 0,
            vib_off: 0,
        }
    }
}

struct Player<'a> {
    d: &'a [u8],
    version: u32,
    table: &'static [u16; 96],
    vol: &'static [u8; 256],
    samples: Vec<Sample>,
    ornaments: Vec<Ornament>,
    tempo: u32,
    ch: [Chan; 3],
    env_base: i32,
    env_slider: Slider,
    noise_base: i32,
    noise_addon: i32,
    regs: [u8; 14],
    env_write: bool,
}

fn err<T>(m: &str) -> Result<T, String> {
    Err(format!("not a valid .pt3 module ({m})"))
}

impl<'a> Player<'a> {
    fn byte(&self, o: usize) -> Result<u8, String> {
        match self.d.get(o) {
            Some(&b) => Ok(b),
            None => err("data past the end of the file"),
        }
    }
    fn le16(&self, o: usize) -> Result<usize, String> {
        Ok(self.byte(o)? as usize | (self.byte(o + 1)? as usize) << 8)
    }
    fn next(&mut self, c: usize) -> Result<u8, String> {
        let b = self.byte(self.ch[c].ptr)?;
        self.ch[c].ptr += 1;
        Ok(b)
    }
    fn next_le16(&mut self, c: usize) -> Result<i32, String> {
        let lo = self.next(c)? as i32;
        Ok(lo | (self.next(c)? as i32) << 8)
    }

    fn tone_of(&self, half: i32) -> i32 {
        self.table[half.clamp(0, 95) as usize] as i32
    }

    fn start_pattern(&mut self, pattern: usize) -> Result<(), String> {
        let base = self.le16(103)? + pattern * 6;
        for c in 0..3 {
            self.ch[c].ptr = self.le16(base + c * 2)?;
            self.ch[c].skip = 1;
            self.ch[c].left = 0;
        }
        Ok(())
    }

    /// Read the next cell of channel `c` and apply it.
    fn cell(&mut self, c: usize) -> Result<(), String> {
        let mut note: Option<i32> = None;
        let mut enabled: Option<bool> = None;
        let mut fx: Vec<u8> = Vec::new();
        let mut env: Option<Option<(u8, i32)>> = None;
        let mut sample = None;
        let mut ornament = None;
        let mut volume = None;
        let mut noise = None;
        loop {
            let cmd = self.next(c)?;
            match cmd {
                0x00..=0x0F => fx.push(cmd),
                0x10..=0x1F | 0xB2..=0xBF | 0xF0..=0xFF => {
                    if (0x11..=0xBF).contains(&cmd) {
                        let ty = cmd - if cmd >= 0xB2 { 0xB1 } else { 0x10 };
                        let hi = self.next(c)? as i32;
                        let per = hi << 8 | self.next(c)? as i32;
                        env = Some(Some((ty, per)));
                    } else {
                        env = Some(None);
                    }
                    if cmd >= 0xF0 {
                        ornament = Some((cmd - 0xF0) as usize);
                    }
                    if !(0xB2..=0xBF).contains(&cmd) {
                        let s = self.next(c)?;
                        sample = Some(if s < 64 && s & 1 == 0 { s as usize / 2 } else { 0 });
                    }
                }
                0x20..=0x3F => noise = Some((cmd - 0x20) as i32),
                0x40..=0x4F => ornament = Some((cmd - 0x40) as usize),
                0x50..=0xAF => {
                    note = Some((cmd - 0x50) as i32);
                    enabled = Some(true);
                    break;
                }
                0xB0 => env = Some(None),
                0xB1 => {
                    let n = self.next(c)?;
                    self.ch[c].skip = (n as u32).max(1);
                }
                0xC0 => {
                    enabled = Some(false);
                    break;
                }
                0xC1..=0xCF => volume = Some((cmd - 0xC0) as i32),
                0xD0 => break,
                0xD1..=0xEF => sample = Some((cmd - 0xD0) as usize),
            }
        }
        // effect parameters follow the note, last effect first
        let mut effects: Vec<(u8, i32, i32)> = Vec::new();
        for &e in fx.iter().rev() {
            match e {
                1 => {
                    let p = self.next(c)? as i32;
                    let d = self.next_le16(c)? as u16 as i16 as i32;
                    effects.push((1, p, d));
                }
                2 => {
                    let p = self.next(c)? as i32;
                    self.next_le16(c)?;
                    let d = self.next_le16(c)? as u16 as i16 as i32;
                    effects.push((2, p, d));
                }
                3 | 4 => {
                    let p = self.next(c)? as i32;
                    effects.push((e, p, 0));
                }
                5 => {
                    let on = self.next(c)? as i32;
                    let off = self.next(c)? as i32;
                    effects.push((5, on, off));
                }
                8 => {
                    let p = self.next(c)? as i32;
                    let d = self.next_le16(c)? as u16 as i16 as i32;
                    effects.push((8, p, d));
                }
                9 => {
                    let t = self.next(c)?;
                    if t > 0 {
                        self.tempo = t as u32;
                    }
                }
                _ => {}
            }
        }

        let version = self.version;
        let tone_diff = |p: &Player, a: i32, b: i32| p.tone_of(b) - p.tone_of(a);
        let ch = &mut self.ch[c];
        let prev_slide = ch.slider.value;
        if let Some(en) = enabled {
            ch.pos_sample = 0;
            ch.pos_orn = 0;
            ch.vol_slide = 0;
            ch.env_sliding = 0;
            ch.noise_sliding = 0;
            ch.slider.reset();
            ch.tone_acc = 0;
            ch.vib_counter = 0;
            ch.enabled = en;
        }
        if let Some(n) = note {
            if effects.iter().any(|e| e.0 == 2) {
                if ch.target == NO_TARGET {
                    ch.prev_note = ch.note;
                }
            } else {
                ch.prev_note = n;
            }
            ch.note = n;
        }
        if let Some(s) = sample {
            ch.sample = s;
        }
        if let Some(o) = ornament {
            ch.ornament = o;
            ch.pos_orn = 0;
        }
        if let Some(v) = volume {
            ch.volume = v;
        }
        if let Some(n) = noise {
            self.noise_base = n;
        }
        match env {
            Some(Some((ty, per))) => {
                self.regs[13] = ty;
                self.env_write = true;
                self.env_base = per;
                self.env_slider.reset();
                self.ch[c].envelope = true;
                self.ch[c].pos_orn = 0;
            }
            Some(None) => {
                self.ch[c].envelope = false;
                self.ch[c].pos_orn = 0;
            }
            None => {}
        }
        for (e, p1, p2) in effects {
            match e {
                1 => {
                    let ch = &mut self.ch[c];
                    ch.slider.period = p1;
                    ch.slider.counter = p1;
                    ch.slider.delta = p2;
                    ch.target = NO_TARGET;
                    ch.vib_counter = 0;
                    if ch.slider.counter == 0 && version >= 7 {
                        ch.slider.counter += 1;
                    }
                }
                2 => {
                    let (from, to) = (self.ch[c].prev_note, self.ch[c].note);
                    let diff = tone_diff(self, from, to);
                    let ch = &mut self.ch[c];
                    ch.vib_counter = 0;
                    ch.slider.period = p1;
                    ch.slider.counter = p1;
                    ch.slider.delta = p2.abs();
                    ch.target = ch.note;
                    ch.slide_delta = diff;
                    if version >= 6 {
                        ch.slider.value = prev_slide;
                    }
                    if ch.slide_delta - ch.slider.value < 0 {
                        ch.slider.delta = -ch.slider.delta;
                    }
                }
                3 => self.ch[c].pos_sample = p1 as usize,
                4 => self.ch[c].pos_orn = p1 as usize,
                5 => {
                    let ch = &mut self.ch[c];
                    ch.vib_counter = p1;
                    ch.vib_on = p1;
                    ch.vib_off = p2;
                    ch.slider.value = 0;
                    ch.slider.counter = 0;
                }
                8 => {
                    self.env_slider.period = p1;
                    self.env_slider.counter = p1;
                    self.env_slider.delta = p2;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// One channel's registers for this frame; returns its envelope offset.
    fn synth(&mut self, c: usize) -> i32 {
        if !self.ch[c].enabled {
            self.regs[8 + c] = 0;
            return 0;
        }
        let smp = &self.samples[self.ch[c].sample];
        let orn = &self.ornaments[self.ch[c].ornament];
        let line = *smp.lines.get(self.ch[c].pos_sample).unwrap_or(&smp.lines[smp.looping]);
        let orn_off = *orn.offsets.get(self.ch[c].pos_orn).unwrap_or(&orn.offsets[orn.looping]);
        let (smp_len, smp_loop, orn_len, orn_loop) = (smp.lines.len(), smp.looping, orn.offsets.len(), orn.looping);
        let mut env_addon = 0;

        let tone_add = line.tone + self.ch[c].tone_acc;
        if line.keep_tone {
            self.ch[c].tone_acc = tone_add;
        }
        let half = self.ch[c].prev_note + orn_off;
        let tone = ((self.tone_of(half) + self.ch[c].slider.value + tone_add) & 0xFFF) as u16;
        self.regs[c * 2] = tone as u8;
        self.regs[c * 2 + 1] = (tone >> 8) as u8;
        if line.tone_mask {
            self.regs[7] |= 1 << c;
        }
        let ch = &mut self.ch[c];
        ch.vol_slide = (ch.vol_slide + line.vol_slide).clamp(-15, 15);
        let level = (ch.vol_slide + line.level).clamp(0, 15);
        let mut v = self.vol[(ch.volume * 16 + level) as usize];
        if ch.envelope && !line.env_mask {
            v |= 16;
        }
        self.regs[8 + c] = v;
        if line.noise_mask {
            let a = line.noise_env + ch.env_sliding;
            if line.keep_noise_env {
                ch.env_sliding = a;
            }
            env_addon = a;
            self.regs[7] |= 8 << c;
        } else {
            self.noise_addon = line.noise_env + ch.noise_sliding;
            if line.keep_noise_env {
                ch.noise_sliding = self.noise_addon;
            }
        }
        let diff_target = self.ch[c].target;
        let ch = &mut self.ch[c];
        if ch.slider.update() && diff_target != NO_TARGET {
            let d = ch.slider;
            if (d.delta < 0 && d.value <= ch.slide_delta) || (d.delta >= 0 && d.value >= ch.slide_delta) {
                ch.note = ch.target;
                ch.prev_note = ch.target;
                ch.target = NO_TARGET;
                ch.slider.value = 0;
                ch.slider.counter = 0;
            }
        }
        ch.pos_sample += 1;
        if ch.pos_sample >= smp_len {
            ch.pos_sample = smp_loop;
        }
        ch.pos_orn += 1;
        if ch.pos_orn >= orn_len {
            ch.pos_orn = orn_loop;
        }
        env_addon
    }

    fn frame(&mut self) -> Frame {
        self.regs[7] = 0;
        let mut env_addon = 0;
        for c in 0..3 {
            env_addon += self.synth(c);
            let ch = &mut self.ch[c];
            if ch.vib_counter > 0 {
                ch.vib_counter -= 1;
                if ch.vib_counter == 0 {
                    ch.enabled = !ch.enabled;
                    ch.vib_counter = if ch.enabled { ch.vib_on } else { ch.vib_off };
                }
            }
        }
        self.regs[6] = ((self.noise_base + self.noise_addon) & 31) as u8;
        let env = env_addon + self.env_slider.value + self.env_base;
        self.regs[11] = env as u8;
        self.regs[12] = (env >> 8) as u8;
        self.env_slider.update();
        let f = Frame { regs: self.regs, env: self.env_write };
        self.env_write = false;
        f
    }
}

fn load_sample(d: &[u8], off: usize) -> Result<Sample, String> {
    let silent = Sample {
        looping: 0,
        lines: vec![Line {
            level: 0,
            vol_slide: 0,
            noise_env: 0,
            env_mask: true,
            noise_mask: true,
            tone_mask: true,
            keep_tone: false,
            keep_noise_env: false,
            tone: 0,
        }],
    };
    if off == 0 {
        return Ok(silent);
    }
    let (Some(&lp), Some(&size)) = (d.get(off), d.get(off + 1)) else { return err("sample past the end") };
    if size == 0 {
        return Ok(silent);
    }
    let mut lines = Vec::new();
    for i in 0..size as usize {
        let o = off + 2 + i * 4;
        let Some(b) = d.get(o..o + 4) else { return err("sample past the end") };
        let ne = ((b[0] >> 1) & 31) as i32;
        lines.push(Line {
            level: (b[1] & 15) as i32,
            vol_slide: if b[0] & 0x80 == 0 { 0 } else if b[0] & 0x40 != 0 { 1 } else { -1 },
            noise_env: if ne & 16 != 0 { ne - 32 } else { ne },
            env_mask: b[0] & 1 != 0,
            noise_mask: b[1] & 0x80 != 0,
            tone_mask: b[1] & 0x10 != 0,
            keep_tone: b[1] & 0x40 != 0,
            keep_noise_env: b[1] & 0x20 != 0,
            tone: i16::from_le_bytes([b[2], b[3]]) as i32,
        });
    }
    Ok(Sample { looping: (lp as usize).min(lines.len() - 1), lines })
}

fn load_ornament(d: &[u8], off: usize) -> Result<Ornament, String> {
    if off == 0 {
        return Ok(Ornament { looping: 0, offsets: vec![0] });
    }
    let (Some(&lp), Some(&size)) = (d.get(off), d.get(off + 1)) else { return err("ornament past the end") };
    let Some(b) = d.get(off + 2..off + 2 + size as usize) else { return err("ornament past the end") };
    let offsets: Vec<i32> = if b.is_empty() { vec![0] } else { b.iter().map(|&x| x as i8 as i32).collect() };
    Ok(Ornament { looping: (lp as usize).min(offsets.len() - 1), offsets })
}

/// Play a module through once, frame by frame.
pub fn render(d: &[u8]) -> Result<Song, String> {
    if d.len() < 202 {
        return err("too short");
    }
    let version = if d.starts_with(b"ProTracker 3.") && d[13].is_ascii_digit() {
        (d[13] - b'0') as u32
    } else if d.starts_with(b"ProTracker 3.") || d.starts_with(b"Vortex Tracker II") {
        6
    } else {
        return err("no `ProTracker 3.` header");
    };
    let table: &'static [u16; 96] = match d[99] {
        1 => &TABLE_ST,
        2 if version <= 3 => &TABLE_ASM_3,
        2 => &TABLE_ASM_4,
        3 if version <= 3 => &TABLE_REAL_3,
        3 => &TABLE_REAL_4,
        4 => &TABLE_NATURAL,
        _ if version <= 3 => &TABLE_PT3_3,
        _ => &TABLE_PT3_4,
    };
    let positions: Vec<usize> = d[201..].iter().take_while(|&&b| b != 0xFF).map(|&b| b as usize / 3).collect();
    if positions.is_empty() || 201 + positions.len() >= d.len() {
        return err("no positions");
    }
    let le = |o: usize| d[o] as usize | (d[o + 1] as usize) << 8;
    let samples = (0..32).map(|i| load_sample(d, le(105 + i * 2))).collect::<Result<Vec<_>, _>>()?;
    let ornaments = (0..16).map(|i| load_ornament(d, le(169 + i * 2))).collect::<Result<Vec<_>, _>>()?;
    let mut p = Player {
        d,
        version,
        table,
        vol: if version <= 4 { &VOL_33_34 } else { &VOL_35 },
        samples,
        ornaments,
        tempo: d[100].max(1) as u32,
        ch: [Chan::new(), Chan::new(), Chan::new()],
        env_base: 0,
        env_slider: Slider::default(),
        noise_base: 0,
        noise_addon: 0,
        regs: [0; 14],
        env_write: false,
    };
    let loop_pos = (d[102] as usize).min(positions.len() - 1);
    let mut frames = Vec::new();
    let mut loop_frame = 0;
    for (k, &pat) in positions.iter().enumerate() {
        if k == loop_pos {
            loop_frame = frames.len();
        }
        p.start_pattern(pat)?;
        let mut line = 0usize;
        loop {
            // channel A's next cell starting with 0: the end of the pattern
            if p.ch[0].left == 0 && p.byte(p.ch[0].ptr)? == 0 {
                break;
            }
            if line == 0 {
                p.noise_base = 0;
            }
            for c in 0..3 {
                if p.ch[c].left == 0 {
                    p.cell(c)?;
                    p.ch[c].left = p.ch[c].skip;
                }
                p.ch[c].left -= 1;
            }
            for _ in 0..p.tempo {
                frames.push(p.frame());
            }
            line += 1;
            if line > 256 || frames.len() > MAX_FRAMES {
                return err("a pattern does not end");
            }
        }
    }
    if frames.is_empty() {
        return err("the song is empty");
    }
    Ok(Song { frames, loop_frame })
}

/// The register stream for the runtime player (format: see the top), and
/// the byte offset of the loop frame in it.
pub fn encode(song: &Song) -> (Vec<u8>, usize) {
    let mut out = Vec::new();
    let mut loop_off = 0;
    let mut prev: Option<[u8; 14]> = None;
    for (i, f) in song.frames.iter().enumerate() {
        let full = i == song.loop_frame || prev.is_none();
        if i == song.loop_frame {
            loop_off = out.len();
        }
        let mut mask = 0u16;
        for r in 0..13 {
            if full || prev.is_some_and(|p| p[r] != f.regs[r]) {
                mask |= 1 << r;
            }
        }
        if f.env {
            mask |= 1 << 13;
        }
        out.push(mask as u8);
        out.push((mask >> 8) as u8);
        for r in 0..14 {
            if mask & 1 << r != 0 {
                out.push(f.regs[r]);
            }
        }
        prev = Some(f.regs);
    }
    out.extend([0, 0x80]);
    (out, loop_off)
}

pub fn load(path: &std::path::Path) -> Result<Song, String> {
    let d = std::fs::read(path).map_err(|e| format!("cannot read `{}`: {e}", path.display()))?;
    render(&d).map_err(|e| format!("`{}`: {e}", path.display()))
}
