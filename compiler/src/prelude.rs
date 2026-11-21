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

    -- do the 16 << scale squares of two visible sprites overlap? (from the
    -- shadow fields; transparent pixels count, no screen-edge wrapping)
    function overlaps(Sprite other) -> bool
        if not self.visible or not other.visible then return false end
        word a = word(16) << (self.scale & 3)
        word b = word(16) << (other.scale & 3)
        return self.x < other.x + b and other.x < self.x + a and self.y < other.y + b and other.y < self.y + a
    end

    -- the five attribute registers, written from the fields through IX
    -- (field offsets: x 0, y 2, id 4, pattern 5, palette 6, visible 7,
    -- mirror_x 8, mirror_y 9, rotate 10, scale 11)
    function update()
        asm
            push ix                 ; user assembly may keep a value in IX
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
            pop ix
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

-- hardware collision flag (port $303B bit 0): two sprites' non-transparent
-- pixels overlapped since the last call; reading the port clears it
function sprite_collision() -> bool
    return port[0x303B] & 1 ~= 0
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
    asm
            ld hl,(p_vline_x)
            ld a,(p_vline_y)
            ld d,a
            ld bc,(p_vline_len)
            ld a,(p_vline_c)
            call rt_vline
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

-- Bresenham line, in assembly (plots through the current mode's rt_plot)
word l2_ln_dx
sword l2_ln_err
byte l2_ln_dy
byte l2_ln_sy

function line(word x0, byte y0, word x1, byte y1, byte c)
    asm
            ld hl,(p_line_x1)       ; dx = |x1 - x0|, x direction
            ld de,(p_line_x0)
            ld a,h
            or d
            ld c,a                  ; C = 0: both ends at x < 256
            or a
            sbc hl,de
            ld a,$23                ; x step: inc hl
            jr nc,.sx
            xor a                   ; dx = -dx
            sub l
            ld l,a
            sbc a,a
            sub h
            ld h,a
            ld a,$2B                ; x step: dec hl
.sx:
            ld (.xstep),a
            ld (g_l2_ln_dx),hl
            ld a,(p_line_y0)        ; dy = |y1 - y0|, sy = direction
            ld b,a
            ld a,(p_line_y1)
            sub b
            ld b,1
            jr nc,.sy
            neg
            ld b,255
            or a                    ; (clear the carry of neg)
.sy:
            ld (g_l2_ln_dy),a
            ld e,a
            ld d,0
            ld a,b
            ld (g_l2_ln_sy),a
            sbc hl,de               ; err = dx - dy (carry clear)
            ld (g_l2_ln_err),hl
            ld a,(rt_l2mode)        ; 256x192 with both ends on the screen:
            or c                    ; the fast loop (no clipping needed)
            jr nz,.slow
            ld a,e                  ; a single pixel? (E = dy; HL = dx - dy
            or l                    ; is zero with dy = 0 when dx = 0)
            or h
            jr z,.dot
            ld a,(p_line_y0)
            cp 192
            jr nc,.loop
            ld a,(p_line_y1)
            cp 192
            jp c,.fast
            jr .loop
.slow:
            ld a,e                  ; a single pixel: plot it
            or l
            or h
            jr nz,.loop
            ld hl,(p_line_x0)
            ld a,(p_line_y0)
            ld d,a
            jr .last
.loop:
            ld de,(p_line_x0)       ; at the end point?
            ld hl,(p_line_x1)
            or a
            sbc hl,de
            ex de,hl                ; HL = x
            ld a,(p_line_y0)
            ld d,a                  ; D = y
            jr nz,.plot
            ld a,(p_line_y1)
            cp d
            jr nz,.plot
.last:
            ld a,(p_line_c)         ; the end point: plot it and stop
            call rt_plot
            jp .out
.dot:
            ld a,(p_line_y0)        ; one pixel, 256x192, x < 256
            cp 192
            jp nc,.out
            ld d,a
            ld a,(p_line_x0)
            ld e,a
            call rt_l2_map
            ld a,(p_line_c)
            ld (hl),a
            jp .out
.plot:
            ld a,(p_line_c)
            call rt_plot
            ld hl,(g_l2_ln_err)
            add hl,hl
            ld b,h
            ld c,l                  ; BC = e2
            ld a,(g_l2_ln_dy)       ; e2 > -dy: e2 + dy > 0
            ld e,a
            ld d,0
            add hl,de
            bit 7,h
            jr nz,.noX
            ld a,h
            or l
            jr z,.noX
            ld hl,(g_l2_ln_err)
            or a
            sbc hl,de
            ld (g_l2_ln_err),hl
            ld hl,(p_line_x0)
.xstep:     inc hl                  ; patched: inc hl / dec hl
            ld (p_line_x0),hl
.noX:
            ld h,b                  ; e2 < dx
            ld l,c
            ld de,(g_l2_ln_dx)
            or a
            sbc hl,de
            jp p,.loop
            ld hl,(g_l2_ln_err)
            add hl,de
            ld (g_l2_ln_err),hl
            ld a,(g_l2_ln_sy)
            ld hl,p_line_y0
            add a,(hl)
            ld (hl),a
            jp .loop

; The same steps with everything in registers. dx <= 255, dy <= 191, so
; the tests on e2 = 2 err become comparisons of err with constants:
;   e2 > -dy  <=>  err >= 1 - (dy + 1) / 2                 (T1)
;   e2 < dx   <=>  err < (dx + 1) / 2                      (T2)
;   after err -= dy (x stepped), the second test on the old err is
;   err < T2 - dy                                         (T3)
; The constants and the step directions are patched into the loop.
; Main set: HL = pixel address (H = y & 31, L = x), B = y, C = colour,
; D = x1, E = y1. Shadow set: HL = err, DE = dy, BC = dx.
.fast:
            ld a,(g_l2_ln_dy)       ; T1 = 1 - (dy + 1) / 2: -95..1
            ld b,a                  ; B = dy
            inc a
            srl a
            neg
            inc a
            ld (.t1l+1),a
            add a,a
            sbc a,a
            ld (.t1h+1),a
            ld a,(g_l2_ln_dx)       ; T2 = (dx + 1) / 2: 0..128 (high byte 0)
            add a,1
            rra
            ld (.t2l+1),a
            sub b                   ; T3 = T2 - dy: -191..128
            ld (.t3l+1),a
            sbc a,a
            ld (.t3h+1),a
            ld a,(.xstep)
            cp $23                  ; inc hl: x grows
            ld a,$2C                ; inc l
            jr z,.fsx
            inc a                   ; dec l
.fsx:
            ld (.xs),a
            ld a,(g_l2_ln_sy)
            dec a
            ld bc,$2404             ; inc h, inc b
            jr z,.fsy
            inc b
            inc c                   ; dec h, dec b
.fsy:
            ld a,c
            ld (.ys1),a
            ld a,b
            ld (.ys2),a
            ld a,(p_line_y0)
            ld d,a
            ld b,a                  ; B = y
            ld a,(p_line_x0)
            ld e,a
            call rt_l2_map          ; HL = address of (x0, y0)
            ld a,(p_line_c)
            ld c,a
            ld a,(p_line_x1)
            ld d,a
            ld a,(p_line_y1)
            ld e,a
            exx
            ld hl,(g_l2_ln_err)
            ld a,(g_l2_ln_dy)
            ld e,a
            ld d,0
            ld bc,(g_l2_ln_dx)
            exx
.floop:
            ld (hl),c
            ld a,l                  ; at the end point?
            cp d
            jr nz,.fgo
            ld a,b
            cp e
            jp z,.out
.fgo:
            exx
            ld a,l                  ; err >= T1: step x
.t1l:       sub 0
            ld a,h
.t1h:       sbc a,0
            jp m,.fnox
            or a
            sbc hl,de               ; err -= dy
            exx
.xs:        inc l
            exx
            ld a,l                  ; old err < T2: err < T3
.t3l:       sub 0
            ld a,h
.t3h:       sbc a,0
            jp p,.fnoy
            jr .fy
.fnox:
            ld a,l                  ; err < T2: step y
.t2l:       sub 0
            ld a,h
.t2h:       sbc a,0                 ; (T2 >= 0: high byte 0, not patched)
            jp p,.fnoy
.fy:
            add hl,bc               ; err += dx
            exx
.ys1:       inc b
.ys2:       inc h
            bit 5,h                 ; left the 32 rows of this 8K page?
            jp z,.floop
            ld a,h
            and 31
            ld h,a
            push hl
            ld a,b                  ; map the page of row B (as rt_l2_map)
            rlca
            rlca
            rlca
            and 7
            ld hl,rt_l2base
            add a,(hl)
            inc hl
            ld (hl),a
            nextreg $50,a
            pop hl
            jp .floop
.fnoy:
            exx
            jp .floop
.out:
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
    asm
            ld hl,(p_l2_span_y)
            ld a,h
            or a
            jr nz,.out              ; y < 0 or y > 255
            ld a,(rt_l2hmax)
            cp l
            jr c,.out               ; below the screen
            ld hl,(p_l2_span_x0)
            bit 7,h
            jr z,.x0
            ld hl,0                 ; x0 = max(x0, 0)
.x0:
            ex de,hl                ; DE = x0
            ld bc,(p_l2_span_x1)
            bit 7,b
            jr nz,.out              ; x1 < 0
            ld hl,(rt_l2wid)
            dec hl                  ; the right edge
            or a
            sbc hl,bc
            jr nc,.x1
            add hl,bc
            ld b,h
            ld c,l                  ; x1 = right
.x1:
            ld h,b
            ld l,c
            or a
            sbc hl,de
            jr c,.out               ; x0 > x1
            inc hl
            ld b,h
            ld c,l                  ; BC = length
            ex de,hl                ; HL = x0
            ld a,(p_l2_span_y)
            ld d,a
            ld a,(p_l2_span_c)
            call rt_hline
.out:
    end
end

-- Half widths of an ellipse, row by row (dy = 0, 1, ... ry). The value is
-- Pythagoras in the space of the larger radius B, then scaled down, so the
-- integer square root's rounding is never magnified:
--   rx >= ry: k = (dy * rx + ry / 2) / ry,  w = isqrt(B*B + B - k*k)
--   rx <  ry: h = isqrt(B*B + B - dy*dy),   w = (rx * h + ry / 2) / ry
-- (radius r + 1/2: (r + 1/2)^2 - dy^2 rounded is r*r + r - dy*dy, so the
-- top, bottom and sides are flat runs, not single-pixel spikes).
-- k, the root and the quotient only ever move one way as dy grows, so each
-- is stepped with additions: no square root, multiplication or division
-- per row. l2_ell_start sets up row 0; each l2_ell_next returns the half
-- width of the next row (the first call: row 0).
byte l2_e_big
byte l2_e_small
bool l2_e_wide
bool l2_e_first
byte l2_e_k
byte l2_e_root
word l2_e_v                 -- B*B + B - k*k
word l2_e_sq                -- root * root
byte l2_e_q                 -- rx < ry: the scaled width
sword l2_e_rem              -- remainder of the stepped division

byte l2_e_fast

-- l2_e_fast = 1 if the ellipse lies on the screen (x - rx >= 0,
-- x + rx < width, y - ry >= 0, y + ry < height), else 0; then
--   wide = rx >= ry; big, small = the larger and the smaller radius
--   sq = big * big, v = sq + big, root = big, k = 0, q = rx, rem = ry / 2
function l2_ell_start(sword x, sword y, byte rx, byte ry)
    asm
            ld a,(p_l2_ell_start_rx)
            ld e,a
            ld d,0
            ld hl,(p_l2_ell_start_x)
            or a
            sbc hl,de
            jp m,.off               ; x - rx < 0
            add hl,de
            add hl,de
            ex de,hl                ; DE = x + rx
            ld hl,(rt_l2wid)
            dec hl
            or a
            sbc hl,de
            jp m,.off               ; x + rx > width - 1
            ld a,(p_l2_ell_start_ry)
            ld e,a
            ld d,0
            ld hl,(p_l2_ell_start_y)
            or a
            sbc hl,de
            jp m,.off               ; y - ry < 0
            add hl,de
            add hl,de
            ex de,hl                ; DE = y + ry
            ld a,(rt_l2hmax)
            ld l,a
            ld h,0
            or a
            sbc hl,de
            jp m,.off               ; y + ry > height - 1
            ld a,1
            jr .fit
.off:
            xor a
.fit:
            ld (g_l2_e_fast),a
            ld a,(p_l2_ell_start_rx)
            ld b,a
            ld (g_l2_e_q),a
            ld a,(p_l2_ell_start_ry)
            ld c,a
            srl a
            ld l,a
            ld h,0
            ld (g_l2_e_rem),hl
            ld a,b
            cp c
            ld a,1
            jr nc,.wide
            ld a,b                  ; rx < ry
            ld b,c
            ld c,a
            xor a
.wide:
            ld (g_l2_e_wide),a
            ld a,b
            ld (g_l2_e_big),a
            ld (g_l2_e_root),a
            ld a,c
            ld (g_l2_e_small),a
            ld d,b
            ld e,b
            mul d,e
            ld (g_l2_e_sq),de
            ex de,hl
            ld a,b
            add hl,a
            ld (g_l2_e_v),hl
            xor a
            ld (g_l2_e_k),a
            inc a
            ld (g_l2_e_first),a
    end
end

byte l2_e_out

-- the steps, as in this LZ80N outline:
--   rx >= ry: rem += B; while rem >= S: rem -= S, v -= 2k + 1, k += 1
--   rx <  ry: v -= 2k + 1, k += 1
--   while sq > v: sq -= 2 root - 1, root -= 1
--   rx <  ry: per root step: rem -= S; while rem < 0: rem += B, q -= 1
-- (rx >= ry: dy * rx + ry / 2 = k * ry + rem, 0 <= rem < ry;
--  rx <  ry: rx * root + ry / 2 = q * ry + rem, 0 <= rem < ry)
function l2_ell_next() -> byte
    asm
            ld a,(g_l2_e_first)
            or a
            jr z,.step
            xor a
            ld (g_l2_e_first),a
            jp .result
.step:
            ld hl,(g_l2_e_v)        ; HL = v
            ld a,(g_l2_e_k)
            ld c,a                  ; C = k
            ld a,(g_l2_e_wide)
            or a
            jr z,.tall
            ld de,(g_l2_e_rem)      ; DE = rem (0..509)
            ld a,(g_l2_e_big)
            add de,a
            ld a,(g_l2_e_small)
            ld b,a                  ; B = S
.kloop:
            ld a,d                  ; rem >= S?
            or a
            jr nz,.kstep
            ld a,e
            cp b
            jr c,.kdone
.kstep:
            ld a,e                  ; rem -= S
            sub b
            ld e,a
            jr nc,.knb
            dec d
.knb:
            ld a,l                  ; v -= 2k + 1
            sub c
            ld l,a
            jr nc,.kv1
            dec h
.kv1:
            ld a,l
            sub c
            ld l,a
            jr nc,.kv2
            dec h
.kv2:
            dec hl
            inc c                   ; k += 1
            jr .kloop
.kdone:
            ld (g_l2_e_rem),de
            jr .root
.tall:
            ld e,c                  ; v -= 2k + 1, k += 1
            ld d,0
            or a
            sbc hl,de
            sbc hl,de
            dec hl
            inc c
.root:
            ld a,c
            ld (g_l2_e_k),a
            ld (g_l2_e_v),hl
            ex de,hl                ; DE = v
            ld hl,(g_l2_e_sq)       ; HL = sq
            ld a,(g_l2_e_root)
            ld c,a                  ; C = root
            ld b,0                  ; B = root steps
.rloop:
            or a                    ; sq > v?
            sbc hl,de
            jr c,.rdone
            jr z,.rdone
            add hl,de
            ld a,l                  ; sq -= 2 root - 1
            sub c
            ld l,a
            jr nc,.r1
            dec h
.r1:
            ld a,l
            sub c
            ld l,a
            jr nc,.r2
            dec h
.r2:
            inc hl
            dec c                   ; root -= 1
            inc b
            jr .rloop
.rdone:
            add hl,de
            ld (g_l2_e_sq),hl
            ld a,c
            ld (g_l2_e_root),a
            ld a,(g_l2_e_wide)
            or a
            jr nz,.result
            ld a,b
            or a
            jr z,.result
            ld hl,(g_l2_e_rem)      ; HL = rem (-255..255)
            ld a,(g_l2_e_small)
            ld e,a
            ld d,0                  ; DE = S
            ld a,(g_l2_e_big)
            ld c,a                  ; C = B
            ld a,(g_l2_e_q)
.qloop:
            or a                    ; rem -= S (keeps A = q: or a clears carry)
            sbc hl,de
.qfix:
            bit 7,h                 ; rem < 0: rem += B, q -= 1
            jr z,.qok
            push af
            ld a,c
            add hl,a
            pop af
            dec a
            jr .qfix
.qok:
            djnz .qloop
            ld (g_l2_e_q),a
            ld (g_l2_e_rem),hl
.result:
            ld a,(g_l2_e_wide)
            or a
            ld a,(g_l2_e_root)
            jr nz,.out
            ld a,(g_l2_e_q)
.out:
            ld (g_l2_e_out),a
    end
    return l2_e_out
end

-- row state of fill_ellipse / ellipse; l2_e_fast: 0 = the ellipse
-- crosses the screen edge (every span through l2_span, which clips),
-- 1 = on the screen (rt_hline), 2 = on the 256x192 screen (direct writes)
byte l2_e_dy
byte l2_e_w
byte l2_e_wn
byte l2_e_inner

-- for dy = 0, ry: w = l2_ell_next(); span x - w .. x + w on rows y + dy
-- and y - dy
function fill_ellipse(sword x, sword y, byte rx, byte ry, byte c)
    if ry == 0 then                 -- one row: no set-up needed
        l2_span(x - sword(rx), x + sword(rx), y, c)
        return
    end
    l2_ell_start(x, y, rx, ry)
    asm
            xor a
            ld (g_l2_e_dy),a
.row:
            call f_l2_ell_next
            ld a,(g_l2_e_out)
            ld (g_l2_e_w),a
            ld bc,(p_fill_ellipse_y)
            ld a,(g_l2_e_dy)
            add bc,a
            call .span              ; row y + dy
            ld a,(g_l2_e_dy)
            or a
            jr z,.next
            ld e,a
            ld d,0
            ld hl,(p_fill_ellipse_y)
            or a
            sbc hl,de
            ld b,h
            ld c,l
            call .span              ; row y - dy
.next:
            ld hl,g_l2_e_dy
            ld a,(p_fill_ellipse_ry)
            cp (hl)
            jp z,.done
            inc (hl)
            jp .row
.span:                              ; BC = row: x - w .. x + w
            ld a,(g_l2_e_w)
            ld e,a
            ld d,0
            ld hl,(p_fill_ellipse_x)
            or a
            sbc hl,de               ; HL = x - w
            ld a,(g_l2_e_fast)
            or a
            jr z,.clip
            ld d,c                  ; D = y
            ld c,e
            ld b,0
            sla c
            rl b
            inc bc                  ; BC = 2 w + 1
            ld a,(p_fill_ellipse_c)
            jp rt_hline
.clip:
            ld (p_l2_span_x0),hl
            add hl,de
            add hl,de
            ld (p_l2_span_x1),hl    ; x + w
            ld (p_l2_span_y),bc
            ld a,(p_fill_ellipse_c)
            ld (p_l2_span_c),a
            jp f_l2_span
.done:
    end
end

-- w = l2_ell_next()
-- for dy = 0, ry:
--     wn = 0, inner = 0; if dy < ry: wn = l2_ell_next(), inner = min(wn + 1, w)
--     (connect to the next row: from just outside its half width to ours)
--     spans x + inner .. x + w and x - w .. x - inner on rows y + dy, y - dy
--     w = wn
function ellipse(sword x, sword y, byte rx, byte ry, byte c)
    if ry == 0 then                 -- one row: the spans x .. x + rx and
        l2_span(x - sword(rx), x + sword(rx), y, c)    -- x - rx .. x meet
        return
    end
    l2_ell_start(x, y, rx, ry)
    l2_e_w = l2_ell_next()
    asm
            ld a,(g_l2_e_fast)
            or a
            jr z,.start
            ld a,(rt_l2mode)
            or a
            jr nz,.start
            ld a,2                  ; on the 256x192 screen: direct writes
            ld (g_l2_e_fast),a
.start:
            xor a
            ld (g_l2_e_dy),a
.row:
            ld a,(g_l2_e_dy)
            ld hl,p_ellipse_ry
            cp (hl)
            jr nc,.last
            call f_l2_ell_next
            ld a,(g_l2_e_out)
            ld (g_l2_e_wn),a
            ld b,a
            ld a,(g_l2_e_w)
            cp b                    ; wn <= w; inner = min(wn + 1, w)
            jr z,.inner
            ld a,b
            inc a
.inner:
            ld (g_l2_e_inner),a
            jr .spans
.last:
            xor a
            ld (g_l2_e_wn),a
            ld (g_l2_e_inner),a
.spans:
            ld bc,(p_ellipse_y)
            ld a,(g_l2_e_dy)
            add bc,a
            call .pair              ; row y + dy
            ld a,(g_l2_e_dy)
            or a
            jr z,.next
            ld e,a
            ld d,0
            ld hl,(p_ellipse_y)
            or a
            sbc hl,de
            ld b,h
            ld c,l
            call .pair              ; row y - dy
.next:
            ld a,(g_l2_e_wn)
            ld (g_l2_e_w),a
            ld hl,g_l2_e_dy
            ld a,(p_ellipse_ry)
            cp (hl)
            jp z,.done
            inc (hl)
            jp .row
.pair:                              ; BC = row
            push bc
            ld hl,(p_ellipse_x)
            ld a,(g_l2_e_inner)
            add hl,a
            call .span              ; x + inner .. x + w
            pop bc
            ld hl,(p_ellipse_x)
            ld a,(g_l2_e_w)
            ld e,a
            ld d,0
            or a
            sbc hl,de               ; x - w .. x - inner
.span:                              ; HL = x0, BC = row, length w - inner + 1
            ld a,(g_l2_e_inner)
            ld e,a
            ld a,(g_l2_e_w)
            sub e
            ld e,a                  ; E = length - 1
            ld a,(g_l2_e_fast)
            or a
            jr z,.clip
            dec a
            jr z,.hline
            ld b,e
            inc b                   ; B = length (0 = 256)
            ld e,l
            ld d,c
            call rt_l2_map          ; HL = address of (E, D)
            ld a,(p_ellipse_c)
.plot:
            ld (hl),a
            inc l
            djnz .plot
            ret
.hline:
            ld d,c                  ; D = y
            ld c,e
            ld b,0
            inc bc                  ; BC = length
            ld a,(p_ellipse_c)
            jp rt_hline
.clip:
            ld (p_l2_span_x0),hl
            ld d,0
            add hl,de
            ld (p_l2_span_x1),hl
            ld (p_l2_span_y),bc
            ld a,(p_ellipse_c)
            ld (p_l2_span_c),a
            jp f_l2_span
.done:
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

-- two triangle edges stepped row by row: x = a + (b - a) * t / n, rounded
-- towards a, without a multiplication or division per row
sword tr_a[2]
sword tr_m[2]
word tr_q[2]
word tr_r[2]
word tr_acc[2]
word tr_n[2]
bool tr_neg[2]

function tr_edge(byte k, sword a, sword b, sword t, sword n)
    tr_a[k] = a
    tr_m[k] = 0
    tr_q[k] = 0
    tr_r[k] = 0
    tr_acc[k] = 0
    tr_n[k] = word(n)
    tr_neg[k] = b < a
    if n == 0 then return end
    word d = word(abs(b - a))
    tr_q[k] = d / word(n)
    tr_r[k] = d % word(n)
    long p = long(d) * long(t)
    tr_m[k] = sword(p / long(n))
    tr_acc[k] = word(p % long(n))
end

function tr_x(byte k) -> sword
    if tr_neg[k] then return tr_a[k] - tr_m[k] end
    return tr_a[k] + tr_m[k]
end

function tr_step(byte k)
    tr_m[k] += sword(tr_q[k])
    tr_acc[k] += tr_r[k]
    if tr_acc[k] >= tr_n[k] and tr_n[k] ~= 0 then
        tr_acc[k] -= tr_n[k]
        tr_m[k] += 1
    end
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
    if ya > yb then return end
    tr_edge(0, x0, x2, ya - y0, y2 - y0)
    bool top_half = ya < y1
    if top_half then
        tr_edge(1, x0, x1, ya - y0, y1 - y0)
    else
        tr_edge(1, x1, x2, ya - y1, y2 - y1)
    end
    sword y = ya
    while y <= yb do
        if top_half and y == y1 then
            tr_edge(1, x1, x2, 0, y2 - y1)
            top_half = false
        end
        sword xa = tr_x(0)
        sword xb = tr_x(1)
        if xa <= xb then l2_span(xa, xb, y, c) else l2_span(xb, xa, y, c) end
        tr_step(0)
        tr_step(1)
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
            ld hl,g_snd_mode        ; nothing sounding: nothing to do
            ld b,9
            xor a
.any:
            or (hl)
            inc hl
            djnz .any
            or a
            jp z,.out
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
    ("sprite", &["Sprite", "sprite", "sprites_visible", "sprite_collision"]),
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
