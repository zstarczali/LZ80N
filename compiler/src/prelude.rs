//! Standard library modules written in LZ80N itself. A module is compiled
//! in front of the user program when the program refers to it.

/// Hardware sprites (Next sprite engine, 8-bit 16x16 patterns).
///
/// `sprite[0..127]` mirror the hardware sprites. Fields are a shadow copy;
/// `update()` (or `position`, `show`, `hide`) uploads them through NextReg
/// $34-$39. Coordinates are hardware coordinates: the 256x192 display area
/// starts at (32, 32); X is 0..511 (visible 0..319), Y 0..511 (visible 0..255).
pub const SPRITES: &str = r#"
class Sprite
    word x
    word y
    byte id
    byte pattern
    byte palette
    bool visible
    bool mirror_x
    bool mirror_y
    bool rotate
    byte scale

    function position(word px, word py)
        self.x = px
        self.y = py
        self.update()
    end

    function show(byte pat)
        self.pattern = pat
        self.visible = true
        self.update()
    end

    function hide()
        self.visible = false
        self.update()
    end

    function update()
        nextreg[0x34] = self.id
        nextreg[0x35] = byte(self.x)
        nextreg[0x36] = byte(self.y)
        byte a2 = (self.palette << 4) | byte(self.x >> 8) & 1
        if self.mirror_x then a2 |= 8 end
        if self.mirror_y then a2 |= 4 end
        if self.rotate then a2 |= 2 end
        nextreg[0x37] = a2
        byte a3 = self.pattern & 63 | 0x40
        if self.visible then a3 |= 0x80 end
        nextreg[0x38] = a3
        byte s = self.scale & 3
        nextreg[0x39] = s << 3 | s << 1 | byte(self.y >> 8) & 1
    end
end

Sprite sprite[128]
for i = 0, 127 do
    sprite[i].id = i
end

-- sprite layer on/off (NextReg $15 bit 0) and sprites over the border (bit 1)
function sprites_visible(bool on, bool over_border)
    byte r = nextreg[0x15] & 0xFC
    if on then r |= 1 end
    if over_border then r |= 2 end
    nextreg[0x15] = r
end
"#;

/// Layer 2 drawing (256x192, colours are RRRGGGBB; $E3 = transparent).
/// `plot`, `point`, `hline` and `layer2_clear` are built in (assembly).
pub const LAYER2: &str = r#"
const TRANSPARENT = 0xE3

function layer2_visible(bool on)
    if on then port[0x123B] = 2 else port[0x123B] = 0 end
end

function rgb(byte r, byte g, byte b) -> byte
    return (r & 7) << 5 | (g & 7) << 2 | b & 3
end

function vline(byte x, byte y, byte len, byte c)
    while len > 0 do
        plot(x, y, c)
        y += 1
        len -= 1
    end
end

function fill_rect(byte x, byte y, word w, byte h, byte c)
    while h > 0 do
        hline(x, y, w, c)
        y += 1
        h -= 1
    end
end

function rect(byte x, byte y, word w, byte h, byte c)
    if w == 0 or h == 0 then return end
    hline(x, y, w, c)
    hline(x, y + h - 1, w, c)
    vline(x, y, h, c)
    vline(byte(x + w - 1), y, h, c)
end

-- Bresenham line
function line(byte x0, byte y0, byte x1, byte y1, byte c)
    sword dx = x1 - x0
    sword dy = y1 - y0
    sbyte sx = 1
    sbyte sy = 1
    if dx < 0 then
        dx = -dx
        sx = -1
    end
    if dy < 0 then
        dy = -dy
        sy = -1
    end
    sword err = dx - dy
    while true do
        plot(x0, y0, c)
        if x0 == x1 and y0 == y1 then break end
        sword e2 = err + err
        if e2 > -dy then
            err -= dy
            x0 += sx
        end
        if e2 < dx then
            err += dx
            y0 += sy
        end
    end
end
"#;

/// Palettes. `palette(layer, index, colour)` with an 8-bit RRRGGGBB colour,
/// `palette9(layer, index, colour)` with a 9-bit RRRGGGBBB colour.
/// Layers: PAL_ULA, PAL_LAYER2, PAL_SPRITES, PAL_TEXT (first palettes).
pub const PALETTE: &str = r#"
const PAL_ULA = 0
const PAL_LAYER2 = 1
const PAL_SPRITES = 2
const PAL_TEXT = 3

function palette(byte layer, byte index, byte colour)
    nextreg[0x43] = (layer & 3) << 4
    nextreg[0x40] = index
    nextreg[0x41] = colour
end

function palette9(byte layer, byte index, word colour)
    nextreg[0x43] = (layer & 3) << 4
    nextreg[0x40] = index
    nextreg[0x44] = byte(colour >> 1)
    nextreg[0x44] = byte(colour) & 1
end

-- r, g, b 0..7 -> 9-bit RRRGGGBBB
function rgb9(byte r, byte g, byte b) -> word
    return word(r & 7) << 6 | (g & 7) << 3 | b & 7
end
"#;

/// Copper: a list of WAIT / MOVE instructions run by the Next's copper
/// coprocessor in sync with the raster, restarted every frame.
/// Build it with copper_begin(), copper_wait(), copper_move(), then
/// copper_start(). Lines use the raster numbering (0 = first display line).
pub const COPPER: &str = r#"
function copper_begin()
    nextreg[0x62] = 0       -- stop, write index 0
    nextreg[0x61] = 0
end

function copper_wait(word line)
    nextreg[0x60] = 0x80 | byte(line >> 8) & 1
    nextreg[0x60] = byte(line)
end

function copper_move(byte reg, byte value)
    nextreg[0x60] = reg & 0x7F
    nextreg[0x60] = value
end

-- end the list and run it from the top of every frame
function copper_start()
    nextreg[0x60] = 0xFF
    nextreg[0x60] = 0xFF
    nextreg[0x62] = 0xC0
end

function copper_stop()
    nextreg[0x62] = 0
end
"#;

/// Sound: 9 channels on the three AY chips (channel = chip * 3 + voice).
/// tone / noise / volume control the chips directly; beep and play are
/// non-blocking and run in the background driver `sound_tick`, which the
/// frame interrupt calls. Notes: n = octave * 12 + semitone (A4 = 57).
pub const SOUND: &str = r#"
byte snd_mixer[3]
byte snd_mode[9]          -- 0 idle, 1 beep, 2 tune, 3 looping tune
byte snd_timer[9]
byte snd_pos[9]
byte snd_oct[9]
byte snd_dur[9]
byte snd_vol[9]
byte snd_buf[2304]        -- 9 x 256: a copy of each channel's tune

-- silence everything and stop all tunes
function sound_reset()
    for ch = 0, 8 do
        snd_mode[ch] = 0
    end
    for c = 0, 2 do
        snd_mixer[c] = 0x3F
        ay_write(c, 7, 0x3F)
        for v = 0, 2 do
            ay_write(c, 8 + v, 0)
        end
    end
end

nextreg[0x08] = nextreg[0x08] & 0x7F | 0x02    -- TurboSound: all three AY chips
sound_reset()

-- mixer: tone and/or noise on a channel
function sound_mix(byte ch, bool tone_on, bool noise_on)
    byte chip = ch / 3
    byte v = ch % 3
    byte m = snd_mixer[chip] | 9 << v
    if tone_on then m = m & ~(1 << v) end
    if noise_on then m = m & ~(8 << v) end
    snd_mixer[chip] = m
    ay_write(chip, 7, m)
end

-- tone with an AY period (frequency = 109375 / period Hz)
function tone(byte ch, word period)
    byte chip = ch / 3
    byte v = ch % 3
    ay_write(chip, v * 2, byte(period))
    ay_write(chip, v * 2 + 1, byte(period >> 8) & 15)
    sound_mix(ch, true, false)
end

-- noise (period 0..31; the noise generator is shared by a chip's voices)
function noise(byte ch, byte period)
    ay_write(ch / 3, 6, period & 31)
    sound_mix(ch, false, true)
end

function volume(byte ch, byte v)
    ay_write(ch / 3, 8 + ch % 3, v & 15)
end

function sound_off(byte ch)
    snd_mode[ch] = 0
    volume(ch, 0)
    sound_mix(ch, false, false)
end

-- play a note for a number of frames (does not wait)
function beep(byte ch, byte note, byte frames)
    snd_mode[ch] = 0
    tone(ch, note_period(note))
    volume(ch, 12)
    snd_timer[ch] = frames
    snd_mode[ch] = 1
end

-- play a tune in the background: notes c d e f g a b (+ # or - ), r = rest,
-- an optional length in frames after a note (c8), o4 octave, > < octave
-- up / down, l10 default length, v12 volume; other characters are skipped
function play(byte ch, string tune)
    snd_mode[ch] = 0
    dma_copy(addr(tune), addr(snd_buf[word(ch) << 8]), word(len(tune)) + 1)
    snd_pos[ch] = 0
    snd_oct[ch] = 4
    snd_dur[ch] = 10
    snd_vol[ch] = 12
    snd_timer[ch] = 0
    sound_mix(ch, true, false)
    snd_mode[ch] = 2
end

function play_loop(byte ch, string tune)
    play(ch, tune)
    snd_mode[ch] = 3
end

function playing(byte ch) -> bool
    return snd_mode[ch] ~= 0
end

-- ---- background driver (runs in the frame interrupt) ----

-- character p of the tune on channel ch (0 past the end)
function snd_char(byte ch, byte p) -> byte
    word base = word(ch) << 8
    if p >= snd_buf[base] then return 0 end
    return snd_buf[base + p + 1]
end

-- digits at the current tune position, or `fallback`
function snd_num(byte ch, byte fallback) -> byte
    byte p = snd_pos[ch]
    byte c = snd_char(ch, p)
    if c < '0' or c > '9' then return fallback end
    byte n = 0
    while c >= '0' and c <= '9' do
        n = n * 10 + (c - '0')
        p += 1
        c = snd_char(ch, p)
    end
    snd_pos[ch] = p
    return n
end

-- read tune commands until the next note or rest
function snd_step(byte ch, byte chip, byte v)
    byte wraps = 0
    while true do
        byte p = snd_pos[ch]
        if p >= snd_buf[word(ch) << 8] then
            if snd_mode[ch] == 3 and wraps < 2 then
                snd_pos[ch] = 0
                wraps += 1
            else
                snd_mode[ch] = 0
                ay_write(chip, 8 + v, 0)
                return
            end
        else
            byte c = snd_char(ch, p) | 0x20          -- lower case
            snd_pos[ch] = p + 1
            if c >= 'a' and c <= 'g' or c == 'r' then
                byte semi = 0
                if c ~= 'r' then
                    semi = asc("JLACEFH", c - 'a') - 'A'
                    byte acc = snd_char(ch, snd_pos[ch])
                    if acc == '#' or acc == '+' then
                        semi += 1
                        snd_pos[ch] += 1
                    elseif acc == '-' then
                        semi -= 1
                        snd_pos[ch] += 1
                    end
                end
                byte dur = snd_num(ch, snd_dur[ch])
                if c == 'r' then
                    ay_write(chip, 8 + v, 0)
                else
                    word per = note_period(snd_oct[ch] * 12 + semi)
                    ay_write(chip, v * 2, byte(per))
                    ay_write(chip, v * 2 + 1, byte(per >> 8))
                    ay_write(chip, 8 + v, snd_vol[ch])
                end
                snd_timer[ch] = dur
                return
            elseif c == 'o' then
                snd_oct[ch] = min(snd_num(ch, 4), 8)
            elseif c == '>' then
                if snd_oct[ch] < 8 then snd_oct[ch] += 1 end
            elseif c == '<' then
                if snd_oct[ch] > 0 then snd_oct[ch] -= 1 end
            elseif c == 'l' then
                snd_dur[ch] = max(snd_num(ch, 10), 1)
            elseif c == 'v' then
                snd_vol[ch] = min(snd_num(ch, 12), 15)
            end
        end
    end
end

function sound_tick()
    byte ch = 0
    for chip = 0, 2 do
        for v = 0, 2 do
            byte m = snd_mode[ch]
            if m ~= 0 then
                byte t = snd_timer[ch]
                if t > 1 then
                    snd_timer[ch] = t - 1
                    -- a short gap at the end of each tune note
                    if t == 2 and m >= 2 then ay_write(chip, 8 + v, 0) end
                elseif m == 1 then
                    snd_mode[ch] = 0
                    ay_write(chip, 8 + v, 0)
                else
                    snd_step(ch, chip, v)
                end
            end
            ch += 1
        end
    end
end
"#;

pub fn uses_sound(src: &str) -> bool {
    ["play(", "play_loop(", "beep(", "tone(", "noise(", "volume(", "sound_", "playing("]
        .iter()
        .any(|w| src.contains(w))
}

pub fn uses_copper(src: &str) -> bool {
    src.contains("copper")
}

pub fn uses_palette(src: &str) -> bool {
    src.contains("palette") || src.contains("PAL_") || src.contains("rgb9")
}

/// Does the program use the Layer 2 module?
pub fn uses_layer2(src: &str) -> bool {
    ["layer2", "plot", "point", "hline", "vline", "line(", "rect", "rgb(", "TRANSPARENT"]
        .iter()
        .any(|w| src.contains(w))
}

/// Does the program use the sprite module?
pub fn uses_sprites(src: &str) -> bool {
    src.contains("sprite") || src.contains("Sprite")
}

/// Public names of the standard modules. They are reserved in every program,
/// whether or not the module is compiled in, so a name never changes meaning
/// when a program starts to use a module. Names starting with `snd_` are
/// reserved for the sound module's internals.
pub const LIBRARY_NAMES: &[(&str, &[&str])] = &[
    ("sprite", &["Sprite", "sprite", "sprites_visible"]),
    ("Layer 2", &["layer2_visible", "rgb", "vline", "line", "rect", "fill_rect", "TRANSPARENT"]),
    ("palette", &["palette", "palette9", "rgb9", "PAL_ULA", "PAL_LAYER2", "PAL_SPRITES", "PAL_TEXT"]),
    ("copper", &["copper_begin", "copper_wait", "copper_move", "copper_start", "copper_stop"]),
    (
        "sound",
        &[
            "sound_reset", "sound_mix", "tone", "noise", "volume", "sound_off", "beep", "play",
            "play_loop", "playing", "sound_tick",
        ],
    ),
];

/// The module that reserves `name`, if any.
pub fn library_module(name: &str) -> Option<&'static str> {
    if name.starts_with("snd_") {
        return Some("sound");
    }
    LIBRARY_NAMES.iter().find(|(_, names)| names.contains(&name)).map(|(m, _)| *m)
}
