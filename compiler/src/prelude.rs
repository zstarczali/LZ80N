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

    -- the five attribute registers, written from the fields through IX
    -- (field offsets: x 0, y 2, id 4, pattern 5, palette 6, visible 7,
    -- mirror_x 8, mirror_y 9, rotate 10, scale 11)
    function update()
        asm
            ld ix,(p_Sprite_update_self)
            ld a,(ix+4)
            nextreg $34,a           ; sprite number
            ld a,(ix+0)
            nextreg $35,a           ; x low
            ld a,(ix+2)
            nextreg $36,a           ; y low
            ld a,(ix+8)             ; mirror x * 8 + mirror y * 4 + rotate * 2
            add a,a
            add a,(ix+9)
            add a,a
            add a,(ix+10)
            add a,a
            ld b,a
            ld a,(ix+1)
            and 1                   ; x bit 8
            or b
            ld b,a
            ld a,(ix+6)
            swapnib
            and $F0                 ; palette offset
            or b
            nextreg $37,a
            ld a,(ix+7)
            rrca
            and $80                 ; visible
            ld b,a
            ld a,(ix+5)
            and 63
            or $40                  ; pattern, 4th attribute byte follows
            or b
            nextreg $38,a
            ld a,(ix+11)
            and 3
            ld b,a
            add a,a
            add a,a
            or b
            add a,a                 ; x and y scale
            ld b,a
            ld a,(ix+3)
            and 1                   ; y bit 8
            or b
            nextreg $39,a
        end
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

/// Layer 2 drawing (256x192, 320x256 or 640x256; colours are RRRGGGBB,
/// $E3 = transparent; 640x256: palette indices 0..15). `plot`, `point`,
/// `hline`, `layer2_clear`, `layer2_mode`, `layer2_width` and
/// `layer2_height` are built in (assembly).
pub const LAYER2: &str = r#"
const TRANSPARENT = 0xE3
const L2_256 = 0
const L2_320 = 1
const L2_640 = 2

function layer2_visible(bool on)
    if on then port[0x123B] = 2 else port[0x123B] = 0 end
end

function rgb(byte r, byte g, byte b) -> byte
    return (r & 7) << 5 | (g & 7) << 2 | b & 3
end

function vline(word x, byte y, word len, byte c)
    while len > 0 do
        plot(x, y, c)
        if y == 255 then return end
        y += 1
        len -= 1
    end
end

function fill_rect(word x, byte y, word w, word h, byte c)
    while h > 0 do
        hline(x, y, w, c)
        if y == 255 then return end
        y += 1
        h -= 1
    end
end

function rect(word x, byte y, word w, word h, byte c)
    if w == 0 or h == 0 then return end
    hline(x, y, w, c)
    hline(x, byte(y + h - 1), w, c)
    vline(x, y, h, c)
    vline(x + w - 1, y, h, c)
end

-- Bresenham line
function line(word x0, byte y0, word x1, byte y1, byte c)
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

-- ---- circles and ellipses (clipped; the centre may be off screen) ----

function l2_plot(sword x, sword y, byte c)
    if x >= 0 and y >= 0 and x < sword(layer2_width()) and y < sword(layer2_height()) then
        plot(word(x), byte(y), c)
    end
end

-- horizontal span x0..x1 on row y, clipped to the screen
function l2_span(sword x0, sword x1, sword y, byte c)
    if y < 0 or y >= sword(layer2_height()) then return end
    if x0 < 0 then x0 = 0 end
    sword right = sword(layer2_width()) - 1
    if x1 > right then x1 = right end
    if x0 > x1 then return end
    hline(word(x0), byte(y), word(x1 - x0 + 1), c)
end

-- half width of an ellipse dy rows from its centre
function l2_halfw(byte rx, byte ry, byte dy) -> byte
    if dy > ry then return 0 end
    if ry == 0 then return rx end
    -- Pythagoras in the space of the larger radius, then scale down, so
    -- the integer square root's rounding is never magnified
    if rx >= ry then
        word k = (word(dy) * rx + ry / 2) / ry
        return sqrt(word(rx) * rx - k * k)
    end
    byte h = sqrt(word(ry) * ry - word(dy) * dy)
    return byte((word(rx) * h + ry / 2) / ry)
end

function fill_ellipse(sword x, sword y, byte rx, byte ry, byte c)
    for dy = 0, ry do
        byte w = l2_halfw(rx, ry, dy)
        l2_span(x - w, x + w, y + dy, c)
        if dy ~= 0 then l2_span(x - w, x + w, y - dy, c) end
    end
end

function ellipse(sword x, sword y, byte rx, byte ry, byte c)
    for dy = 0, ry do
        byte w = l2_halfw(rx, ry, dy)
        -- connect to the next row: from just outside its half width to ours
        byte inner = w
        if dy < ry then inner = min(l2_halfw(rx, ry, dy + 1) + 1, w) end
        if dy == ry then inner = 0 end
        l2_span(x + inner, x + w, y + dy, c)
        l2_span(x - w, x - inner, y + dy, c)
        if dy ~= 0 then
            l2_span(x + inner, x + w, y - dy, c)
            l2_span(x - w, x - inner, y - dy, c)
        end
    end
end

function circle(sword x, sword y, byte r, byte c)
    ellipse(x, y, r, r, c)
end

function fill_circle(sword x, sword y, byte r, byte c)
    fill_ellipse(x, y, r, r, c)
end

-- ---- triangles ----

function triangle(word x0, byte y0, word x1, byte y1, word x2, byte y2, byte c)
    line(x0, y0, x1, y1, c)
    line(x1, y1, x2, y2, c)
    line(x2, y2, x0, y0, c)
end

-- a + (b - a) * t / n, rounded towards a
function l2_lerp(sword a, sword b, sword t, sword n) -> sword
    if n == 0 then return a end
    sword d = b - a
    sword m = sword(word(abs(d)) * word(t) / word(n))
    if d < 0 then return a - m end
    return a + m
end

function fill_triangle(sword x0, sword y0, sword x1, sword y1, sword x2, sword y2, byte c)
    sword t = 0
    -- sort the corners by y
    if y1 < y0 then
        t = x0  x0 = x1  x1 = t
        t = y0  y0 = y1  y1 = t
    end
    if y2 < y0 then
        t = x0  x0 = x2  x2 = t
        t = y0  y0 = y2  y2 = t
    end
    if y2 < y1 then
        t = x1  x1 = x2  x2 = t
        t = y1  y1 = y2  y2 = t
    end
    sword ya = max(y0, 0)
    sword yb = min(y2, sword(layer2_height()) - 1)
    sword y = ya
    while y <= yb do
        sword xa = l2_lerp(x0, x2, y - y0, y2 - y0)
        sword xb = 0
        if y < y1 then
            xb = l2_lerp(x0, x1, y - y0, y1 - y0)
        else
            xb = l2_lerp(x1, x2, y - y1, y2 - y1)
        end
        if xa <= xb then l2_span(xa, xb, y, c) else l2_span(xb, xa, y, c) end
        y += 1
    end
end
"#;

/// Palettes. `palette(layer, index, colour)` with an 8-bit RRRGGGBB colour,
/// `palette9(layer, index, colour)` with a 9-bit RRRGGGBBB colour.
/// Layers: PAL_ULA, PAL_LAYER2, PAL_SPRITES, PAL_TEXT (first palettes) and
/// PAL_ULA_2 .. PAL_TEXT_2 (second palettes). `palette_select(layer, second)`
/// chooses which of the two a layer shows.
pub const PALETTE: &str = r#"
const PAL_ULA = 0
const PAL_LAYER2 = 1
const PAL_SPRITES = 2
const PAL_TEXT = 3
const PAL_ULA_2 = 4
const PAL_LAYER2_2 = 5
const PAL_SPRITES_2 = 6
const PAL_TEXT_2 = 7

-- NextReg $43 bits 6-4 select the palette written; bits 3-1 (which
-- palette sprites / Layer 2 / ULA show) are kept
function palette(byte layer, byte index, byte colour)
    nextreg[0x43] = nextreg[0x43] & 0x0F | (layer & 7) << 4
    nextreg[0x40] = index
    nextreg[0x41] = colour
end

function palette9(byte layer, byte index, word colour)
    nextreg[0x43] = nextreg[0x43] & 0x0F | (layer & 7) << 4
    nextreg[0x40] = index
    nextreg[0x44] = byte(colour >> 1)
    nextreg[0x44] = byte(colour) & 1
end

-- show the first (false) or the second (true) palette of a layer
function palette_select(byte layer, bool second)
    byte which = layer & 3
    if which == PAL_TEXT then
        -- tilemap (text): NextReg $6B bit 4
        byte t = nextreg[0x6B] & 0xEF
        if second then t |= 0x10 end
        nextreg[0x6B] = t
        return
    end
    byte bit = 2                                   -- ULA: bit 1
    if which == PAL_LAYER2 then bit = 4 end        -- Layer 2: bit 2
    if which == PAL_SPRITES then bit = 8 end       -- sprites: bit 3
    byte r = nextreg[0x43] & 0x7F
    if second then r |= bit else r &= 0xFF - bit end
    nextreg[0x43] = r
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

byte snd_ch
byte snd_chip
byte snd_v
byte snd_wraps
byte snd_semi
word snd_bufp

-- every channel: count down the note, or read the next tune commands
function sound_tick()
    asm
            xor a
            ld (g_snd_ch),a
            ld (g_snd_chip),a
            ld (g_snd_v),a
.chan:
            ld hl,g_snd_mode
            call .ix
            ld a,(hl)
            or a
            jr z,.next
            ld c,a                  ; mode
            ld hl,g_snd_timer
            call .ix
            ld a,(hl)
            cp 2
            jr c,.expired
            dec (hl)
            cp 2                    ; a short gap at the end of each tune note
            jr nz,.next
            ld a,c
            cp 2
            jr c,.next
            call .silence
            jr .next
.expired:
            ld a,c
            dec a
            jr nz,.step
            ld hl,g_snd_mode        ; a beep has ended
            call .ix
            ld (hl),0
            call .silence
            jr .next
.step:
            call .stepch
.next:
            ld a,(g_snd_v)
            inc a
            cp 3
            jr c,.same
            ld a,(g_snd_chip)
            inc a
            ld (g_snd_chip),a
            xor a
.same:
            ld (g_snd_v),a
            ld a,(g_snd_ch)
            inc a
            ld (g_snd_ch),a
            cp 9
            jp c,.chan
            jp .out

; HL += channel
.ix:
            ld a,(g_snd_ch)
            add hl,a
            ret

; volume register of the channel = 0
.silence:
            ld a,(g_snd_chip)
            ld d,a
            ld a,(g_snd_v)
            add a,8
            ld e,0
            jp rt_ay_write

; A = the tune character at the channel's position (0 past the end)
.peek:
            ld hl,g_snd_pos
            call .ix
            ld a,(hl)
            ld hl,(g_snd_bufp)
            cp (hl)
            jr nc,.peek0
            inc a
            add hl,a
            ld a,(hl)
            ret
.peek0:
            xor a
            ret

.adv:
            ld hl,g_snd_pos
            call .ix
            inc (hl)
            ret

; A = the number at the position (advanced past it), or E if no digit
.num:
            call .peek
            sub '0'
            cp 10
            jr c,.ndig
            ld a,e
            ret
.ndig:
            ld e,a
.nloop:
            call .adv
            call .peek
            sub '0'
            cp 10
            jr nc,.ndone
            ld d,a
            ld a,e
            add a,a
            ld b,a
            add a,a
            add a,a
            add a,b                 ; n * 10
            add a,d
            ld e,a
            jr .nloop
.ndone:
            ld a,e
            ret

; read tune commands until the next note or rest
.stepch:
            ld a,(g_snd_ch)
            ld h,a
            ld l,0
            ld de,g_snd_buf
            add hl,de
            ld (g_snd_bufp),hl
            xor a
            ld (g_snd_wraps),a
.sl:
            ld hl,g_snd_pos
            call .ix
            ld a,(hl)
            ld hl,(g_snd_bufp)
            cp (hl)
            jr c,.sc
            ld hl,g_snd_mode        ; the end of the tune
            call .ix
            ld a,(hl)
            cp 3
            jr nz,.sstop
            ld a,(g_snd_wraps)
            cp 2
            jr nc,.sstop
            inc a
            ld (g_snd_wraps),a
            ld hl,g_snd_pos
            call .ix
            ld (hl),0
            jr .sl
.sstop:
            ld hl,g_snd_mode
            call .ix
            ld (hl),0
            jp .silence
.sc:
            call .peek
            or $20                  ; lower case
            ld c,a
            call .adv
            ld a,c
            cp 'r'
            jr z,.note
            sub 'a'
            cp 7
            jp nc,.cmd
            ld hl,.semis
            add hl,a
            ld a,(hl)
            ld (g_snd_semi),a
            call .peek
            cp '#'
            jr z,.sharp
            cp '+'
            jr z,.sharp
            cp '-'
            jr nz,.note
            ld hl,g_snd_semi
            dec (hl)
            call .adv
            jr .note
.sharp:
            ld hl,g_snd_semi
            inc (hl)
            call .adv
.note:
            push bc
            ld hl,g_snd_dur
            call .ix
            ld e,(hl)
            call .num               ; duration
            pop bc
            push af
            ld a,c
            cp 'r'
            jr nz,.tone
            call .silence
            jr .settime
.tone:
            ld hl,g_snd_oct
            call .ix
            ld a,(hl)
            ld b,a
            add a,a
            add a,b
            add a,a
            add a,a                 ; octave * 12
            ld hl,g_snd_semi
            add a,(hl)
            call rt_note_period
            push hl
            ld a,(g_snd_chip)
            ld d,a
            ld e,l
            ld a,(g_snd_v)
            add a,a
            call rt_ay_write        ; period low
            pop hl
            ld a,(g_snd_chip)
            ld d,a
            ld e,h
            ld a,(g_snd_v)
            add a,a
            inc a
            call rt_ay_write        ; period high
            ld hl,g_snd_vol
            call .ix
            ld e,(hl)
            ld a,(g_snd_chip)
            ld d,a
            ld a,(g_snd_v)
            add a,8
            call rt_ay_write        ; volume
.settime:
            pop af
            ld b,a
            ld hl,g_snd_timer
            call .ix
            ld (hl),b
            ret
.cmd:
            ld a,c
            cp 'o'
            jr nz,.c1
            ld e,4
            call .num
            cp 9
            jr c,.o1
            ld a,8
.o1:
            ld b,a
            ld hl,g_snd_oct
            call .ix
            ld (hl),b
            jp .sl
.c1:
            cp '>'
            jr nz,.c2
            ld hl,g_snd_oct
            call .ix
            ld a,(hl)
            cp 8
            jp nc,.sl
            inc (hl)
            jp .sl
.c2:
            cp '<'
            jr nz,.c3
            ld hl,g_snd_oct
            call .ix
            ld a,(hl)
            or a
            jp z,.sl
            dec (hl)
            jp .sl
.c3:
            cp 'l'
            jr nz,.c4
            ld e,10
            call .num
            or a
            jr nz,.l1
            inc a
.l1:
            ld b,a
            ld hl,g_snd_dur
            call .ix
            ld (hl),b
            jp .sl
.c4:
            cp 'v'
            jp nz,.sl
            ld e,12
            call .num
            cp 16
            jr c,.v1
            ld a,15
.v1:
            ld b,a
            ld hl,g_snd_vol
            call .ix
            ld (hl),b
            jp .sl
.semis:
            db 9, 11, 0, 2, 4, 5, 7  ; a b c d e f g
.out:
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
    ["layer2", "plot", "point", "hline", "vline", "line(", "rect", "rgb(", "TRANSPARENT", "circle", "ellipse", "triangle"]
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
    (
        "Layer 2",
        &[
            "layer2_visible", "rgb", "vline", "line", "rect", "fill_rect", "TRANSPARENT", "circle",
            "L2_256", "L2_320", "L2_640",
            "fill_circle", "ellipse", "fill_ellipse", "triangle", "fill_triangle",
        ],
    ),
    (
        "palette",
        &[
            "palette", "palette9", "palette_select", "rgb9", "PAL_ULA", "PAL_LAYER2", "PAL_SPRITES", "PAL_TEXT",
            "PAL_ULA_2", "PAL_LAYER2_2", "PAL_SPRITES_2", "PAL_TEXT_2",
        ],
    ),
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
    if name.starts_with("l2_") {
        return Some("Layer 2");
    }
    LIBRARY_NAMES.iter().find(|(_, names)| names.contains(&name)).map(|(m, _)| *m)
}
