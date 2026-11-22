//! Startup code and runtime library (Z80N assembly, sjasmplus syntax).
//!
//! Memory layout:
//!   $8000..$FCFF  code + data (banks 2/0 as mapped by the NEX loader)
//!   $FDFD..$FDFF  `jp __isr` (IM2 target)
//!   $FE00..$FF00  IM2 vector table (257 x $FD)
//!   stack grows down from $FDFD
//!   bank 5: $6000 tilemap (text), $6C00 font

pub const STARTUP: &str = r#"__start:
    di
    ld sp,$FDFD
    nextreg $07,3           ; CPU at 28 MHz
    nextreg $68,$80         ; ULA output off (text = tilemap, graphics = Layer 2)
    nextreg $4A,0           ; fallback ("border"/background) colour: black
    nextreg $15,%00001000   ; layer order S-U-L: sprites, text, Layer 2; sprites off
    nextreg $70,0           ; Layer 2: 256x192, palette offset 0
    nextreg $16,0           ; Layer 2 scroll X/Y
    nextreg $17,0
    call rt_l2_init
    ; IM2 table: every vector points to $FDFD, which jumps to __isr
    ld hl,$FE00
    ld de,$FE01
    ld bc,256
    ld (hl),$FD
    ldir
    ld a,$C3                ; jp nn
    ld ($FDFD),a
    ld hl,__isr
    ld ($FDFE),hl
    ld a,$FE
    ld i,a
    im 2
    call rt_text_init
    ei
"#;

pub const RUNTIME: &str = r#"
__main_end:
    ei
    halt
    jr __main_end

; ==== runtime library ====

; esxDOS call (RST $08, the function byte at rt_esx_fn; A, BC, DE, HL, IX
; as the call takes them; carry and registers as it leaves them), with the
; system variables (bank 5) at $4000: banked code has its page there, and
; NextZXOS (dot commands) needs them. Resident, as it changes slot 2.
rt_esx:
    ld (rt_esx_a),a
    push bc
    ld bc,$243B
    ld a,$52
    out (c),a
    inc b
    in a,(c)
    ld (rt_esx_mmu2),a
    pop bc
    nextreg $52,10
    ld a,(rt_esx_a)
    rst $08
rt_esx_fn:
    db 0
    push af
    ld a,(rt_esx_mmu2)
    nextreg $52,a
    pop af
    ret
rt_esx_a:    db 0
rt_esx_mmu2: db 10

; wait for the next frame (rt_frames changes; raster interrupts in
; between do not end the wait)
rt_vsync:
    ld hl,rt_frames
    ld a,(hl)
.w:
    ei
    halt
    cp (hl)
    jr z,.w
    ret

; HL = number of frames
rt_pause:
    ld a,h
    or l
    ret z
    push hl
    call rt_vsync
    pop hl
    dec hl
    jr rt_pause

; xorshift16 (7, 9, 8). HL = n -> HL = 0..n-1 (n = 0: 0..65535)
rt_random:
    push hl
    ld hl,(rt_rnd)
    ld a,h
    rra
    ld a,l
    rra
    xor h
    ld h,a
    ld a,l
    rra
    ld a,h
    rra
    xor l
    ld l,a
    xor h
    ld h,a
    ld (rt_rnd),hl
    pop de
    ld a,d
    or e
    ret z
    call rt_divu16
    ex de,hl                ; remainder
    ret

; HL = seed (0 is replaced, xorshift must not be 0)
rt_seed:
    ld a,h
    or l
    jr nz,.ok
    inc hl
.ok:
    ld (rt_rnd),hl
    ret

; A = 1 if any key is held
rt_any_key:
    xor a
    in a,($fe)
    cpl
    and $1F
    ret z
    ld a,1
    ret

; wait until all keys are released, then for a key press
rt_wait_key:
    call rt_vsync
    call rt_any_key
    or a
    jr nz,rt_wait_key
.p:
    call rt_vsync
    call rt_any_key
    or a
    jr z,.p
    ret

; HL = HL * DE (low 16 bits), uses Z80N MUL D,E
rt_mul16:
    ld b,d
    ld c,e
    ld d,h
    mul d,e                 ; xh*yl
    ld a,e
    ld d,l
    ld e,b
    mul d,e                 ; xl*yh
    add a,e
    ld d,l
    ld e,c
    mul d,e                 ; xl*yl
    add a,d
    ld h,a
    ld l,e
    ret

; HL = (HL * DE) >> 16 (high word of the unsigned 32-bit product)
rt_mulhi16:
    push de                 ; y
    ld b,h
    ld c,l                  ; BC = x
    ld d,l
    mul d,e                 ; xl*yl
    ld a,d                  ; its high byte
    pop de
    push de
    ld d,b
    mul d,e                 ; xh*yl
    add a,e
    ld l,a
    ld a,d
    adc a,0
    ld h,a                  ; HL = xh*yl + (xl*yl >> 8)
    pop de
    push de
    ld e,c
    mul d,e                 ; xl*yh
    add hl,de
    ld a,0
    adc a,a                 ; bit 16
    ld l,h
    ld h,a                  ; HL = middle >> 8
    pop de
    ld e,b
    mul d,e                 ; xh*yh
    add hl,de
    ret

; HL / DE -> HL = quotient, DE = remainder (unsigned)
; division by zero returns HL = $FFFF, DE = dividend
rt_divu16:
    ld a,d
    or e
    jr z,.zero
    bit 7,d
    jr nz,.big
    ld c,l                  ; low byte of the dividend
    ld a,h
    ld hl,0                 ; remainder
    or a
    jr z,.hzero             ; dividend < 256: high quotient byte is 0
    ld b,8
.l1:
    rla                     ; next dividend bit; inverted quotient bit in
    adc hl,hl
    sbc hl,de
    jr nc,.n1
    add hl,de               ; restore (sets carry: quotient bit 0)
.n1:
    djnz .l1
    rla
    cpl
.hzero:
    push af                 ; high quotient byte
    ld a,c
    ld b,8
.l2:
    rla
    adc hl,hl
    sbc hl,de
    jr nc,.n2
    add hl,de
.n2:
    djnz .l2
    rla
    cpl
    ex de,hl                ; DE = remainder
    ld l,a
    pop af
    ld h,a
    ret
.big:                       ; divisor >= $8000: quotient 0 or 1
    or a
    sbc hl,de
    jr nc,.one
    add hl,de
    ex de,hl
    ld hl,0
    ret
.one:
    ex de,hl
    ld hl,1
    ret
.zero:
    ex de,hl
    ld hl,$FFFF
    ret

; signed HL / DE -> HL = quotient (truncated), DE = remainder (sign of dividend)
rt_divs16:
    ld a,h
    push af                 ; dividend sign
    xor d
    push af                 ; quotient sign
    bit 7,h
    call nz,rt_neghl
    bit 7,d
    jr z,.p
    ex de,hl
    call rt_neghl
    ex de,hl
.p:
    call rt_divu16
    pop af
    bit 7,a
    call nz,rt_neghl
    pop af
    bit 7,a
    ret z
    ex de,hl
    call rt_neghl
    ex de,hl
    ret

rt_neghl:
    xor a
    sub l
    ld l,a
    sbc a,a
    sub h
    ld h,a
    ret

; ---- AY sound chips (3 x AY-3-8912, TurboSound) ----
; D = chip 0..2, A = register, E = value. Interrupts are held off during
; the select + write pair and restored afterwards (safe in handlers too).
rt_ay_write:
    ld c,a
    ld a,i                  ; P/V = IFF2
    push af
    di
    ld a,d
    cpl                     ; chip 0/1/2 -> %11111111 / %11111110 / %11111101
    ld b,$FF
    ld l,c
    ld c,$FD
    out (c),a               ; select chip (port $FFFD)
    out (c),l               ; select register
    ld b,$BF
    out (c),e               ; data (port $BFFD)
    pop af
    ret po
    ei
    ret

; ---- strings: length byte + up to STR_CAP characters ----
STR_CAP equ 64

; the print routines output through rt_emit: normally the screen, while
; formatting (str(), "text" .. number) the end of the string at rt_sdest
rt_emit:
    jp rt_putc
; DE = destination string; preserves A, HL
rt_fmt_on:
    ld (rt_sdest),de
    push hl
    ld hl,rt_semit
    ld (rt_emit+1),hl
    pop hl
    ret
rt_fmt_off:
    ld hl,rt_putc
    ld (rt_emit+1),hl
    ret
rt_semit:
    push bc
    push de
    ld de,(rt_sdest)
    call rt_sapp_chr
    pop de
    pop bc
    ret

; A = character, DE = string (preserved): append if there is room
rt_sapp_chr:
    push hl
    ld c,a
    ld a,(de)
    cp STR_CAP
    jr nc,.full
    inc a
    ld (de),a
    ld l,a
    ld h,0
    add hl,de
    ld (hl),c
.full:
    pop hl
    ret

; HL = source, DE = destination: copy (at most STR_CAP characters)
rt_scopy:
    ld a,(hl)
    cp STR_CAP+1
    jr c,.ok
    ld a,STR_CAP
.ok:
    ld (de),a
    inc hl
    inc de
    or a
    ret z
    ld c,a
    ld b,0
    ldir
    ret

; HL = source, DE = destination: append (truncated at STR_CAP)
rt_scat:
    ld a,(de)
    ld c,a                  ; old length
    add a,(hl)
    jr c,.cap               ; a long literal can exceed 255
    cp STR_CAP+1
    jr c,.fits
.cap:
    ld a,STR_CAP
.fits:
    ld (de),a
    sub c                   ; characters to copy
    ret z
    push af
    ex de,hl
    inc hl
    ld b,0
    add hl,bc               ; end of the destination
    ex de,hl
    inc hl                  ; first source character
    pop af
    ld c,a
    ld b,0
    ldir
    ret

; HL = a, DE = b: Z if equal, C if a < b (dictionary order)
rt_scmp:
    ld b,(hl)
    ld a,(de)
    ld c,a
    inc hl
    inc de
.lp:
    ld a,b
    or a
    jr z,.aend
    ld a,c
    or a
    jr z,.bend
    ld a,(hl)
    ex de,hl
    cp (hl)
    ex de,hl
    ret nz
    inc hl
    inc de
    dec b
    dec c
    jr .lp
.aend:
    ld a,c
    or a
    ret z                   ; same length: equal
    scf                     ; a is a prefix of b
    ret
.bend:
    ld a,1
    or a                    ; b is a prefix of a: greater
    ret

; HL = source, DE = destination, B = first (0-based), C = count
rt_ssub:
    ld a,(hl)
    sub b
    jr nc,.av
    xor a
.av:
    cp c
    jr nc,.cnt
    ld c,a
.cnt:
    ld a,c
    ld (de),a
    or a
    ret z
    inc de
    inc hl
    ld a,b
    add hl,a
    ld b,0
    ldir
    ret

; HL = s, DE = t -> A = first index of t in s, or 255
rt_sfind:
    ld a,(de)
    ld c,a                  ; length of t
    ld a,(hl)
    sub c
    jr c,.no
    inc a
    ld b,a                  ; positions to try
    inc hl
    inc de
    push hl                 ; start of s
.lp:
    push hl
    push de
    push bc
    ld a,c
    or a
    jr z,.found
.cmp:
    ld a,(de)
    cp (hl)
    jr nz,.ne
    inc hl
    inc de
    dec c
    jr nz,.cmp
.found:
    pop bc
    pop de
    pop hl
    pop de
    or a
    sbc hl,de
    ld a,l
    ret
.ne:
    pop bc
    pop de
    pop hl
    inc hl
    djnz .lp
    pop hl
.no:
    ld a,255
    ret

; HL = s -> HL = value of a leading (optionally negative) decimal number
rt_sval:
    ld b,(hl)
    inc hl
    ld de,0
    ld c,0
    ld a,b
    or a
    jr z,.done
    ld a,(hl)
    cp '-'
    jr nz,.dig
    inc c
    inc hl
    dec b
    jr z,.done
.dig:
    ld a,(hl)
    sub '0'
    jr c,.done
    cp 10
    jr nc,.done
    push hl
    ld h,d
    ld l,e
    add hl,hl
    add hl,hl
    add hl,de
    add hl,hl               ; * 10
    add hl,a
    ex de,hl
    pop hl
    inc hl
    djnz .dig
.done:
    ex de,hl
    ld a,c
    or a
    ret z
    jp rt_neghl

; HL = s, A = index -> A = character code (0 outside the string)
rt_sasc:
    cp (hl)
    jr nc,.out
    inc hl
    add hl,a
    ld a,(hl)
    ret
.out:
    xor a
    ret

; HL = s: letters to upper / lower case, in place
rt_supper:
    ld bc,'a' << 8 | ('z' + 1)
    ld d,-32
    jr rt_scase
rt_slower:
    ld bc,'A' << 8 | ('Z' + 1)
    ld d,32
rt_scase:
    ld e,(hl)
    inc e
.l:
    dec e
    ret z
    inc hl
    ld a,(hl)
    cp b
    jr c,.l
    cp c
    jr nc,.l
    add a,d
    ld (hl),a
    jr .l

; HL = string: print it
rt_print_pstr:
    ld a,(hl)
    or a
    ret z
    ld b,a
    inc hl
; HL = text, B = length (1..255)
rt_print_n:
    ; formatting into a string: one character at a time through rt_emit
    ld de,(rt_emit+1)
    ld a,e
    cp low rt_putc
    jr nz,rt_pp_slow
    ld a,d
    cp high rt_putc
    jr nz,rt_pp_slow
    ; screen: write straight into the tilemap
    call rt_pp_addr
rt_pp_fast:
    ld a,(hl)
    cp 32
    jr c,rt_pp_special
    cp 128
    jr nc,rt_pp_special
rt_pp_chr:                  ; (the test emulator reads A here)
    ld (de),a
    inc de
    ld a,(rt_attr)
    ld (de),a
    inc de
    inc hl
    dec c
    jr z,rt_pp_wrap
    djnz rt_pp_fast
    ld a,40
    sub c
    ld (rt_col),a
    ret
rt_pp_wrap:                 ; end of the line
    push hl
    push bc
    call rt_newline
    pop bc
    pop hl
    call rt_pp_addr
    djnz rt_pp_fast
    ret
rt_pp_special:              ; newline / unprintable: the slow path
    push hl
    push bc
    ld d,a
    ld a,40
    sub c
    ld (rt_col),a
    ld a,d
    call rt_putc
    pop bc
    pop hl
    inc hl
    call rt_pp_addr
    djnz rt_pp_fast
    ret
; DE = tilemap address of the cursor, C = columns left on the line
rt_pp_addr:
    ld a,(rt_row)
    ld d,a
    ld e,40
    mul d,e
    ld a,(rt_col)
    add de,a
    ex de,hl
    add hl,hl
    add hl,TEXT_MAP
    ex de,hl
    ld a,(rt_col)
    neg
    add a,40
    ld c,a
    ret
rt_pp_slow:
    ld a,(hl)
    push hl
    push bc
    call rt_emit
    pop bc
    pop hl
    inc hl
    djnz rt_pp_slow
    ret

; ---- fixed point (8.8) and math ----
; HL = HL * DE (signed 8.8), magnitude truncated
rt_fmul:
    ld a,h
    xor d
    push af                 ; result sign in bit 7
    bit 7,h
    call nz,rt_neghl
    ex de,hl                ; DE = |a|
    bit 7,h
    call nz,rt_neghl
    ld b,h
    ld c,l                  ; BC = |b|
    push de
    ld d,e
    ld e,c
    mul d,e                 ; al*bl
    ld l,d
    ld h,0                  ; HL = al*bl >> 8
    pop de
    push de
    ld d,b
    mul d,e                 ; al*bh
    add hl,de
    pop de
    push de
    ld e,c
    mul d,e                 ; ah*bl
    add hl,de
    pop de
    ld e,b
    mul d,e                 ; ah*bh (only the low byte fits)
    ld a,h
    add a,e
    ld h,a
    pop af
    bit 7,a
    ret z
    jp rt_neghl

; HL = HL / DE (signed 8.8), magnitude truncated; /0 -> $7FFF
rt_fdiv:
    ld a,d
    or e
    jr nz,.ok
    ld hl,$7FFF
    ret
.ok:
    ld a,h
    xor d
    push af
    bit 7,h
    call nz,rt_neghl
    ex de,hl
    bit 7,h
    call nz,rt_neghl
    ex de,hl                ; HL = |a|, DE = |b|
    push de
    call rt_divu16          ; HL = integer part, DE = remainder
    ld b,l
    ex de,hl
    pop de
    ld c,0
    ld a,8                  ; 8 fraction bits: (remainder << 8) / |b|
.lp:
    add hl,hl
    jr c,.over
    sbc hl,de               ; carry is clear here
    jr nc,.one
    add hl,de
    sla c
    jr .next
.over:
    ccf
    sbc hl,de
.one:
    scf
    rl c
.next:
    dec a
    jr nz,.lp
    ld h,b
    ld l,c
    pop af
    bit 7,a
    ret z
    jp rt_neghl

; print HL (signed 8.8) with up to two decimals: 1.5, -0.25, 3.14
rt_print_fixed:
    bit 7,h
    jr z,.pos
    call rt_neghl
    push hl
    ld a,'-'
    call rt_emit
    pop hl
.pos:
    ld d,l
    ld e,100
    mul d,e
    ld a,e
    add a,128
    ld a,d
    adc a,0                 ; A = round(fraction * 100)
    cp 100
    jr c,.nc
    inc h
    xor a
.nc:
    push af
    ld l,h
    ld h,0
    call rt_print_u16
    pop af
    or a
    ret z
    push af
    ld a,'.'
    call rt_emit
    pop af
    ld c,0
.d10:
    sub 10
    jr c,.dd
    inc c
    jr .d10
.dd:
    add a,10                ; A = units, C = tens
    push af
    ld a,c
    add a,'0'
    call rt_emit
    pop af
    or a
    ret z
    add a,'0'
    jp rt_emit

; D = floor(sqrt(HL)), 16-bit value (bit pairs, after M. Bazelides)
rt_isqrt16:
    ld de,$0040             ; $40 appends binary 01 to D
    ld a,l
    ld l,h
    ld h,d
    or a
    ld b,8
.lp:
    sbc hl,de
    jr nc,.keep
    add hl,de
.keep:
    ccf
    rl d
    add a,a
    adc hl,hl
    add a,a
    adc hl,hl
    djnz .lp
    ret

; DE = floor(sqrt(A:H:L)) (24-bit value, bit-pair method)
rt_isqrt24:
    ld c,12                 ; bit pairs
    or a
    jr nz,.full
    ld a,h                  ; 16-bit value: skip the zero top byte
    ld h,l
    ld c,8
.full:
    push hl                 ; the lower byte(s), fetched later
    ld b,a                  ; current byte
    ld hl,0                 ; remainder
    ld de,0                 ; root
.lp:
    add hl,hl
    sla b
    jr nc,.a
    inc l
.a:
    add hl,hl
    sla b
    jr nc,.b
    inc l
.b:
    ex de,hl
    add hl,hl
    ex de,hl                ; root <<= 1
    push hl
    or a
    sbc hl,de
    jr c,.rest
    or a
    sbc hl,de
    jr c,.rest
    ld a,h
    or l
    jr z,.rest
    dec hl                  ; remainder -= 2*root + 1
    inc sp
    inc sp
    inc e                   ; root |= 1
    jr .next
.rest:
    pop hl
.next:
    dec c
    jr z,.done
    ld a,c
    and 3
    jr nz,.lp
    ex (sp),hl              ; next byte of the value
    ld b,h
    ld h,l
    ex (sp),hl
    jr .lp
.done:
    pop hl
    ret

; carry set if HL < DE (signed); HL, DE preserved
rt_cmp_s16:
    ld a,h
    xor d
    jp m,.diff
    push hl
    or a
    sbc hl,de
    pop hl
    ret
.diff:
    ld a,h
    rla
    ret

rt_min_s16:
    call rt_cmp_s16
    ret c
    ex de,hl
    ret

rt_max_s16:
    call rt_cmp_s16
    ret nc
    ex de,hl
    ret

rt_min_u16:
    push hl
    or a
    sbc hl,de
    pop hl
    ret c
    ex de,hl
    ret

rt_max_u16:
    push hl
    or a
    sbc hl,de
    pop hl
    ret nc
    ex de,hl
    ret

; ---- hardware sprites ----
; A = first pattern slot, HL = pattern data, D = number of 256-byte patterns
rt_sprite_upload:
    ld bc,$303B
    out (c),a               ; select pattern slot
    ld c,$5B
.p:
    ld b,0
    otir                    ; 256 bytes to the pattern port
    dec d
    jr nz,.p
    ret

; ---- text output: Next tilemap in text mode (40x32, 1-bit font) ----
; tilemap  $6000..$69FF (40*32 entries: char, attribute)
; font     $6C00 + 8*char (chars 32..127)
; attribute = palette offset << 1; offset k uses palette entries 2k (paper)
; and 2k+1 (ink). Paper is $E3 = the global transparency colour, so the
; layers below (ULA border/paper, sprites, Layer 2) show through.
TEXT_MAP   equ $6000
TEXT_TILES equ $6C00

rt_text_init:
    nextreg $6B,%10001001   ; tilemap on, 40x32, attributes, text mode, over ULA
    nextreg $6C,0
    nextreg $6E,high(TEXT_MAP) & $3F
    nextreg $6F,high(TEXT_TILES) & $3F
    nextreg $2F,0           ; no scroll offset
    nextreg $30,0
    nextreg $31,0
    ld hl,rt_font
    ld de,TEXT_TILES+32*8
    ld bc,96*8
    ldir
    nextreg $43,%00110000   ; edit tilemap palette 0, auto-increment
    nextreg $40,0
    ld hl,rt_text_palette
    ld b,32
.pal:
    ld a,(hl)
    nextreg $41,a
    inc hl
    djnz .pal
    ld a,7*2                ; white ink
    ld (rt_attr),a
rt_cls:
    ld hl,TEXT_MAP
    ld bc,40*32
    ld a,(rt_attr)
    ld e,a
.l:
    ld (hl),' '
    inc hl
    ld (hl),e
    inc hl
    dec bc
    ld a,b
    or c
    jr nz,.l
    xor a
    ld (rt_row),a
    ld (rt_col),a
    ret

rt_space:
    ld a,' '
; A = character
rt_putc:
    cp 10
    jr z,rt_newline
    cp 32
    jr c,.bad
    cp 128
    jr c,.ok
.bad:
    ld a,'?'
.ok:
    push af
    ld a,(rt_row)
    ld d,a
    ld e,40
    mul d,e                 ; DE = row * 40
    ld a,(rt_col)
    add de,a                ; Z80N
    ex de,hl
    add hl,hl
    add hl,TEXT_MAP
    pop af
    ld (hl),a
    inc hl
    ld a,(rt_attr)
    ld (hl),a
    ld a,(rt_col)
    inc a
    ld (rt_col),a
    cp 40
    ret c
rt_newline:
    xor a
    ld (rt_col),a
    ld a,(rt_row)
    inc a
    cp 32
    jr c,.ok
    call rt_scroll
    ld a,31
.ok:
    ld (rt_row),a
    ret

; scroll the text up one line
rt_scroll:
    ld hl,TEXT_MAP+80
    ld de,TEXT_MAP
    ld bc,31*80
    ldir
    ex de,hl                ; HL = start of the last line
    ld a,(rt_attr)
    ld e,a
    ld b,40
.c:
    ld (hl),' '
    inc hl
    ld (hl),e
    inc hl
    djnz .c
    ret

; A = row; values >= 32 are clamped to 31
rt_set_row:
    cp 32
    jr c,.ok
    ld a,31
.ok:
    ld (rt_row),a
    ret

; A = column; values >= 40 are clamped to 39
rt_set_col:
    cp 40
    jr c,.ok
    ld a,39
.ok:
    ld (rt_col),a
    ret

; A = ink colour 0..15
rt_set_ink:
    and 15
    add a,a
    ld (rt_attr),a
    ret

; paper (transparent), ink pairs for palette offsets 0..15
rt_text_palette:
    db $E3,$00, $E3,$02, $E3,$A0, $E3,$A2, $E3,$14, $E3,$16, $E3,$B4, $E3,$B6
    db $E3,$00, $E3,$03, $E3,$E0, $E3,$E7, $E3,$1C, $E3,$1F, $E3,$FC, $E3,$FF

; ---- Layer 2 (256x192, 8 bpp) ----
; Pixels are written through MMU slot 0 ($0000-$1FFF): 8K page = base + y/32,
; offset = (y & 31) * 256 + x. The mapped page is cached in rt_l2page.
rt_l2_init:
    ; fixed buffers: banks 8-10 shown, 11-13 drawn into. They are set, not
    ; read: NextZXOS leaves NR $12 = 9, which overlaps the default shadow
    ; bank 11 (the two buffers would share a bank). Pictures start at bank 14.
    nextreg $12,8
    nextreg $13,11
    ld a,11
    ld (rt_l2alt),a
    ld a,8
    ld (rt_l2bank),a
    ld a,16
    ld (rt_l2base),a
    ld a,$FF
    ld (rt_l2page),a
    ret

; ---- far memory: 8K pages through MMU slot 1 ($2000-$3FFF) ----
; rt_farpage always holds the page in slot 1 ($FF = ROM); interrupts save
; and restore it. A = first page of the data, HL = byte offset (< 64K) ->
; the right page mapped, HL = address. Clobbers A, C.
rt_far_map:
    ld c,a
    ld a,h
    rlca
    rlca
    rlca
    and 7                   ; offset / 8K
    add a,c
    ld c,a
    ld a,(rt_farpage)
    cp c
    jr z,.same
    ld a,c
    ld (rt_farpage),a
    nextreg $51,a
.same:
    ld a,h
    and $1F
    or $20
    ld h,a
    ret

rt_farpage: db $FF

; double buffering: show the bank drawn so far, draw into the other one
rt_l2_flip:
    ld a,(rt_l2bank)
    nextreg $12,a
    ld hl,rt_l2alt
    ld c,(hl)
    ld (hl),a
    ld a,c
    ld (rt_l2bank),a
    add a,a
    ld (rt_l2base),a
    ld a,$FF
    ld (rt_l2page),a
    ret

; ---- Layer 2 modes ----
; rt_plot, rt_point and rt_hline jump to the routines of the current mode
; (rt_l2_setmode patches them): D = y, HL = x (0..width-1), A = colour.
; Pixels outside the screen are ignored.
rt_plot:    jp rt_plot256
rt_point:   jp rt_point256
rt_hline:   jp rt_hline256

; D = y, HL = x, A = colour (256x192)
rt_plot256:
    inc h
    dec h
    ret nz
    ld e,l
; D = y, E = x, A = colour. Rows >= 192 are ignored. Leaves HL = address.
rt_plot_de:
    ld c,a
    ld a,d
    cp 192
    ret nc
    call rt_l2_map
    ld (hl),c
    ret

; D = y, HL = x -> A = colour (0 outside the screen)
rt_point256:
    xor a
    or h
    ld a,0
    ret nz
    ld e,l
    ld a,d
    cp 192
    ld a,0
    ret nc
    call rt_l2_map
    ld a,(hl)
    ret

; 320x256 and 640x256 are stored by columns: 8K page = base + column / 32,
; offset = (column & 31) * 256 + y. D = y, HL = byte column (< 320) ->
; page mapped into slot 0, HL = address. Clobbers A, E.
rt_l2_maphi:
    ld a,l
    and 31
    ld e,a
    ld a,l
    rlca
    rlca
    rlca
    and 7
    bit 0,h
    jr z,.lo
    or 8
.lo:
    ld hl,rt_l2base
    add a,(hl)
    inc hl                  ; rt_l2page
    cp (hl)
    jr z,.same
    ld (hl),a
    nextreg $50,a
.same:
    ld h,e
    ld l,d
    ret

; HL < 320? (NC = outside)
rt_l2_x320:
    ld a,h
    or a
    jr z,.in
    dec a
    jr z,.hi
    or a                    ; >= 512: NC
    ret
.hi:
    ld a,l
    cp 64
    ret
.in:
    scf
    ret

; D = y, HL = x, A = colour (320x256)
rt_plot320:
    ld c,a
    call rt_l2_x320
    ret nc
    call rt_l2_maphi
    ld (hl),c
    ret

rt_point320:
    call rt_l2_x320
    ld a,0
    ret nc
    call rt_l2_maphi
    ld a,(hl)
    ret

; HL < 640? (NC = outside); B = 0 for the left (high) nibble, 1 for the
; right one; HL = byte column
rt_l2_x640:
    ld a,h
    cp 2
    jr c,.in
    ret nz
    ld a,l
    cp 128
    ret nc
.in:
    ld a,l
    and 1
    ld b,a
    srl h
    rr l
    scf
    ret

; D = y, HL = x, A = colour 0..15 (640x256, 4 bits per pixel)
rt_plot640:
    and 15
    ld c,a
    call rt_l2_x640
    ret nc
    call rt_l2_maphi
    bit 0,b
    jr nz,.right
    ld a,c
    add a,a
    add a,a
    add a,a
    add a,a
    ld c,a
    ld a,(hl)
    and $0F
    or c
    ld (hl),a
    ret
.right:
    ld a,(hl)
    and $F0
    or c
    ld (hl),a
    ret

rt_point640:
    call rt_l2_x640
    ld a,0
    ret nc
    call rt_l2_maphi
    ld a,(hl)
    bit 0,b
    jr nz,.right
    rrca
    rrca
    rrca
    rrca
.right:
    and 15
    ret

; D = y, HL = x, BC = length, A = colour (320x256): along the columns,
; one page per 32 pixels
rt_hline320:
    ld (rt_l2colour),a
    ld a,b
    or c
    ret z
    call rt_l2_x320
    ret nc
    push hl
    push de
    ex de,hl
    ld hl,320
    or a
    sbc hl,de               ; pixels left on the row
    or a
    sbc hl,bc
    jr nc,.fits
    add hl,bc
    ld b,h
    ld c,l
.fits:
    pop de
    pop hl
; HL = byte column, D = y, BC = bytes (> 0, on the screen), the value in
; rt_l2colour: one tight loop per 8K page (32 columns)
rt_l2_hrun:
.page:
    push hl
    push bc
    call rt_l2_maphi        ; H = column & 31, L = y
    pop bc
    ld a,32
    sub h
    ld e,a                  ; columns left in this page
    ld a,b
    or a
    jr nz,.chunk
    ld a,c
    cp e
    jr nc,.chunk
    ld e,a                  ; the rest of the line is shorter
.chunk:
    ld a,c
    sub e
    ld c,a
    jr nc,.nb
    dec b
.nb:
    ld a,(rt_l2colour)
    push bc
    ld b,e
.px:
    ld (hl),a
    inc h
    djnz .px
    pop bc
    pop hl
    ld a,b
    or c
    ret z
    ld a,l                  ; the next page: column = (column | 31) + 1
    or 31
    ld l,a
    inc hl
    jr .page

; D = y, HL = x, BC = length, A = colour (640x256): an odd first and last
; pixel one by one, the byte pairs between them along the columns
rt_hline_hi:
    and 15
    ld (rt_l2colour),a
    ld a,b
    or c
    ret z
    push de
    ex de,hl
    ld hl,640
    or a
    sbc hl,de               ; pixels left on the row
    jr c,.out
    jr z,.out
    or a
    sbc hl,bc
    jr nc,.fits
    add hl,bc
    ld b,h
    ld c,l
.fits:
    ex de,hl                ; HL = x
    pop de
    bit 0,l
    jr z,.even
    call .one
    inc hl
    dec bc
.even:
    ld a,b
    or c
    ret z
    push hl
    push bc
    srl b
    rr c                    ; whole bytes
    ld a,b
    or c
    jr z,.nobytes
    srl h
    rr l                    ; byte column
    ld a,(rt_l2colour)
    ld e,a
    add a,a
    add a,a
    add a,a
    add a,a
    or e
    ld (rt_l2colour),a      ; both nibbles
    push de
    call rt_l2_hrun
    pop de
.nobytes:
    pop bc
    pop hl
    bit 0,c
    ret z
    add hl,bc               ; the last pixel: x + length - 1
    dec hl
.one:
    push hl
    push bc
    push de
    ld a,(rt_l2colour)
    and 15
    call rt_plot
    pop de
    pop bc
    pop hl
    ret
.out:
    pop de
    ret

; A = mode: 0 = 256x192 (6 pages per buffer), 1 = 320x256, 2 = 640x256
; (10 pages each). Buffers: bank 8 (shown and drawn into until the first
; flip) and bank 11 or 13.
rt_l2_setmode:
    cp 3
    ret nc
    ld (rt_l2mode),a
    ld e,a
    ld d,14                 ; bytes per table row
    mul d,e
    ld hl,rt_l2_modes
    add hl,de
    ld e,(hl)
    inc hl
    ld d,(hl)
    inc hl
    ld (rt_plot+1),de
    ld e,(hl)
    inc hl
    ld d,(hl)
    inc hl
    ld (rt_point+1),de
    ld e,(hl)
    inc hl
    ld d,(hl)
    inc hl
    ld (rt_hline+1),de
    ld e,(hl)
    inc hl
    ld d,(hl)
    inc hl
    ld (rt_l2wid),de
    ld a,(hl)
    inc hl
    ld (rt_l2hmax),a
    ld a,(hl)               ; 8K pages per buffer
    inc hl
    ld (rt_l2np),a
    ld a,(hl)
    inc hl
    nextreg $70,a           ; resolution
    ld a,(hl)               ; second buffer (16K bank)
    inc hl
    ld (rt_l2alt),a
    nextreg $13,a
    ld c,(hl)               ; clip window: right edge (in 2/4 pixel steps)
    inc hl
    ld b,(hl)               ; bottom edge
    nextreg $1C,1           ; reset the Layer 2 clip index
    nextreg $18,0
    ld a,c
    nextreg $18,a
    nextreg $18,0
    ld a,b
    nextreg $18,a
    ld a,8
    ld (rt_l2bank),a
    nextreg $12,a
    ld a,16
    ld (rt_l2base),a
    ld a,$FF
    ld (rt_l2page),a
    ret

; plot, point, hline, width, max y, pages, NR $70, second bank, clip x2, clip y2
rt_l2_modes:
    dw rt_plot256, rt_point256, rt_hline256, 256
    db 191, 6, $00, 11, 255, 191
    dw rt_plot320, rt_point320, rt_hline320, 320
    db 255, 10, $10, 13, 159, 255
    dw rt_plot640, rt_point640, rt_hline_hi, 640
    db 255, 10, $20, 13, 159, 255

; D = y (< 192), E = x -> page mapped into slot 0, HL = address
rt_l2_map:
    ld a,d
    rlca
    rlca
    rlca
    and 7                   ; y / 32
    ld hl,rt_l2base
    add a,(hl)
    inc hl                  ; rt_l2page
    cp (hl)
    jr z,.same
    ld (hl),a
    nextreg $50,a
.same:
    ld a,d
    and 31
    ld h,a
    ld l,e
    ret

; D = y, HL = x, BC = length, A = colour; clipped at the right edge
rt_hline256:
    inc h
    dec h
    ret nz
    ld e,l
    push af
    ld a,b
    or c
    jr z,.none
    xor a
    sub e
    ld l,a
    ld h,0
    jr nz,.nz
    inc h                   ; x = 0: 256 pixels available
.nz:
    or a
    sbc hl,bc
    jr nc,.fits
    add hl,bc
    ld b,h
    ld c,l
.fits:
    ld a,d
    cp 192
    jr nc,.none
    push bc
    call rt_l2_map          ; HL = address (a row never crosses an 8K page)
    pop bc
    pop af
    ex de,hl
    ld hl,-32
    add hl,bc
    jp c,rt_dma_fill        ; 32 pixels or more: the DMA fills them
    ex de,hl
    ld (hl),a
    dec bc
    ld a,b
    or c
    ret z
    ld d,h
    ld e,l
    inc de
    ldir
    ret
.none:
    pop af
    ret

; D = y, HL = x, BC = length, A = colour: vertical line, clipped
rt_vline:
    push af
    ld a,b
    or c
    jr z,.none
    ld a,(rt_l2hmax)
    sub d
    jr c,.none              ; below the screen
    ld e,a                  ; rows available - 1
    dec bc
    ld a,b
    or a
    jr nz,.clip
    ld a,c
    cp e
    jr c,.ok
.clip:
    ld c,e
.ok:
    ld b,c
    inc b                   ; B = rows (0 = 256)
    ld a,(rt_l2mode)
    or a
    jr z,.m256
    dec a
    jr z,.m320
    pop af                  ; 640x256: pixel by pixel
.p640:
    push af
    push bc
    push de
    push hl
    call rt_plot
    pop hl
    pop de
    pop bc
    pop af
    inc d
    djnz .p640
    ret
.m320:
    call rt_l2_x320
    jr nc,.none
    call rt_l2_maphi        ; a column is one page, y = the low byte
    pop af
.l320:
    ld (hl),a
    inc l
    djnz .l320
    ret
.m256:
    ld a,h
    or a
    jr nz,.none
    ld e,l
    pop af
    ld c,a
    call rt_l2_map
.l256:
    ld (hl),c
    inc d
    dec b
    ret z
    inc h
    ld a,h
    cp 32
    jr c,.l256
    call rt_l2_map          ; the next 8K page
    jr .l256
.none:
    pop af
    ret

; A = colour: fill the whole Layer 2 screen (6 pages, DMA)
rt_l2_clear:
    ld c,a
    ld a,(rt_l2mode)
    cp 2
    ld a,c
    jr nz,.full
    and 15                  ; 640x256: the colour in both nibbles
    ld c,a
    add a,a
    add a,a
    add a,a
    add a,a
    or c
.full:
    ld (rt_dma_value),a
    ld a,(rt_l2np)
    ld b,a
    ld a,(rt_l2base)
.p:
    nextreg $50,a
    ld (rt_l2page),a
    push af
    push bc
    ld a,(rt_dma_value)
    ld de,0
    ld bc,8192
    call rt_dma_fill
    pop bc
    pop af
    inc a
    djnz .p
    ret

; ---- zxnDMA (port $6B), memory to memory, continuous mode ----
; HL = source, DE = destination, BC = length (0 = nothing)
rt_dma_copy:
    ld a,b
    or c
    ret z
    ld (rt_dma_src),hl
    ld a,%01010100          ; WR1: port A = memory, incrementing, timing follows
    jr rt_dma_go

; DE = destination, BC = length, A = value
rt_dma_fill:
    ld (rt_dma_value),a
    ld a,b
    or c
    ret z
    ld hl,rt_dma_value
    ld (rt_dma_src),hl
    ld a,%01100100          ; WR1: port A = memory, fixed address
rt_dma_go:
    ld (rt_dma_wr1),a
    ld (rt_dma_len),bc
    ld (rt_dma_dst),de
    ld a,(rt_smp_on)
    or a
    jr nz,.cpu
    ld hl,rt_dma_prog
    ld bc,(rt_dma_prog_len << 8) | $6B
    otir
    ret

; the DMA is playing a sample: copy / fill with the CPU instead
.cpu:
    ld hl,(rt_dma_src)
    ld a,(rt_dma_wr1)
    cp %01010100            ; incrementing source: a copy
    jr z,.copy
    ld a,(hl)               ; fill: the value
    ex de,hl
    ld (hl),a
    dec bc
    ld a,b
    or c
    ret z
    ld d,h
    ld e,l
    inc de
.copy:
    ldir
    ret

rt_smp_on:  db 0            ; a sample is playing (the DMA is busy)

rt_dma_prog:
    db %10000011            ; WR6: disable DMA
    db %01111101            ; WR0: A->B transfer, port A address + length follow
rt_dma_src:
    dw 0
rt_dma_len:
    dw 0
rt_dma_wr1:
    db %01010100            ; WR1 (patched: increment / fixed)
    db %00000010            ;   timing: 2 cycles
    db %01010000            ; WR2: port B = memory, incrementing, timing follows
    db %00000010            ;   timing: 2 cycles
    db %10101101            ; WR4: continuous mode, port B address follows
rt_dma_dst:
    dw 0
    db %10000010            ; WR5: stop at end of block
    db %11001111            ; WR6: load
    db %10000111            ; WR6: enable DMA
rt_dma_prog_len equ $ - rt_dma_prog
rt_dma_value:
    db 0

; A = colour 0..15 -> fallback colour (background / border area)
rt_border:
    and 15
    add a,a
    inc a                   ; ink entry of the text palette pair
    ld hl,rt_text_palette
    add hl,a
    ld a,(hl)
    nextreg $4A,a
    ret

; ---- input ----
; A = key code (row*8 + bit) -> A = 1 if pressed, else 0
rt_key:
    ld c,a
    rrca
    rrca
    rrca
    and 7
    ld hl,rt_rowmask
    add hl,a                ; Z80N ADD HL,A
    ld b,(hl)
    ld a,c
    and 7
    ld hl,rt_bitmask
    add hl,a
    ld e,(hl)
    ld a,b
    in a,($fe)
    cpl
    and e
    ret z
    ld a,1
    ret
rt_rowmask: db $FE,$FD,$FB,$F7,$EF,$DF,$BF,$7F
rt_bitmask: db 1,2,4,8,16,32,64,128

; HL = zero-terminated string
rt_print_str:               ; zero-terminated: count, then the common path
    push hl
    xor a
    ld b,a
.n:
    cp (hl)
    jr z,.e
    inc hl
    inc b
    jr .n
.e:
    pop hl
    ld a,b
    or a
    ret z
    jp rt_print_n

; A = bool
rt_print_bool:
    or a
    ld hl,rt_s_true
    jr nz,rt_print_str
    ld hl,rt_s_false
    jr rt_print_str

; HL = signed 16-bit
rt_print_s16:
    bit 7,h
    jr z,rt_print_u16
    push hl
    ld a,'-'
    call rt_emit
    pop hl
    call rt_neghl
; HL = unsigned 16-bit, printed in decimal
rt_print_u16:
    ld e,0                  ; 1 once a digit has been printed
    ld bc,-10000
    call .digit
    ld bc,-1000
    call .digit
    ld bc,-100
    call .digit
    ld bc,-10
    call .digit
    ld a,l
    add a,'0'
    jp rt_emit
; one decimal digit by repeated subtraction (BC = -power of ten)
.digit:
    ld a,'0'-1
.sub:
    inc a
    add hl,bc
    jr c,.sub
    sbc hl,bc               ; one subtraction too many (carry is clear)
    cp '0'
    jr nz,.emit
    bit 0,e
    ret z                   ; no leading zeros
.emit:
    ld e,1
    push de
    push hl
    call rt_emit
    pop hl
    pop de
    ret

rt_s_true:  db "true",0
rt_s_false: db "false",0
rt_frames:  dw 0
rt_sdest:   dw 0
rt_irq_idx: db 0
rt_rnd:     dw $ACE1
rt_row:     db 0
rt_col:     db 0
rt_attr:    db 14
rt_l2bank:  db 8           ; 16K bank drawn into
rt_l2alt:   db 11          ; the other buffer
rt_l2base:  db 16
rt_l2page:  db $FF          ; must follow rt_l2base
rt_l2mode:  db 0            ; 0 = 256x192, 1 = 320x256, 2 = 640x256
rt_l2wid:   dw 256
rt_l2hmax:  db 191
rt_l2np:    db 6            ; 8K pages per buffer
rt_l2colour: db 0
"#;

/// atan2 (emitted together with the sin/atan tables when used).
pub const TRIG: &str = r#"
; HL = y, DE = x (signed) -> A = angle, 256 units per turn (0 = +x, 64 = +y)
rt_atan2:
    ld c,0                  ; bit 0: x < 0, bit 1: y < 0, bit 2: swapped
    bit 7,d
    jr z,.xp
    ex de,hl
    call rt_neghl
    ex de,hl
    set 0,c
.xp:
    bit 7,h
    jr z,.yp
    call rt_neghl
    set 1,c
.yp:
    push hl
    or a
    sbc hl,de
    pop hl
    jr c,.ns                ; |y| < |x|
    ex de,hl
    set 2,c
.ns:
    ld a,d                  ; HL = smaller, DE = larger
    or e
    ret z                   ; (0, 0) -> 0
.scale:
    ld a,d
    cp 4
    jr c,.go
    srl d
    rr e
    srl h
    rr l
    jr .scale
.go:
    add hl,hl
    add hl,hl
    add hl,hl
    add hl,hl
    add hl,hl
    add hl,hl               ; smaller * 64
    push bc
    call rt_divu16          ; 0..64
    pop bc
    ld de,rt_atan_tab
    add hl,de
    ld a,(hl)               ; 0..32 (0..45 degrees)
    bit 2,c
    jr z,.s1
    neg
    add a,64
.s1:
    bit 0,c
    jr z,.s2
    neg
    add a,128
.s2:
    bit 1,c
    ret z
    neg
    ret

"#;

/// Note period lookup (emitted with the note table when used).
pub const NOTES: &str = r#"
; A = note (0..107) -> HL = AY tone period
rt_note_period:
    cp 108
    jr c,.ok
    ld a,107
.ok:
    ld l,a
    ld h,0
    add hl,hl
    ld de,rt_note_tab
    add hl,de
    ld a,(hl)
    inc hl
    ld h,(hl)
    ld l,a
    ret

"#;

/// Layer 2 pictures (`asset name = image("file.png")`, `draw_image`).
pub const IMAGES: &str = r#"
; ---- images ----
; HL = descriptor: db first page, dw width, db height, db rows per page,
; db masked (1 = the picture has TRANSPARENT pixels; they are skipped).
; Position (signed words) in rt_img_x / rt_img_y. The picture is clipped to
; the screen; its 8K pages are mapped into MMU slot 1 ($2000) one at a time
; and every visible row is copied with LDIR into the Layer 2 page in slot 0.
rt_draw_image:
    ld a,(hl)
    ld (rt_img_page),a
    inc hl
    ld e,(hl)
    inc hl
    ld d,(hl)
    inc hl
    ld (rt_img_w),de
    ld a,(hl)
    ld (rt_img_h),a
    inc hl
    ld a,(hl)
    ld (rt_img_rpp),a
    inc hl
    ld a,(hl)
    ld (rt_img_mask),a
    ld a,(rt_l2mode)
    or a
    jp nz,rt_img_hi
    ; horizontal: BC = source columns skipped, HL = screen x
    ld hl,(rt_img_x)
    ld bc,0
    bit 7,h
    jr z,.xpos
    ld a,l
    cpl
    ld c,a
    ld a,h
    cpl
    ld b,a
    inc bc                  ; skip = -x
    ld hl,0
.xpos:
    ld a,h
    or a
    ret nz                  ; x >= 256
    ld (rt_img_sx),bc
    ld a,l
    ld (rt_img_dx),a
    ex de,hl                ; DE = x
    ld hl,(rt_img_w)
    or a
    sbc hl,bc               ; visible width
    ret c
    ret z
    push hl
    add hl,de
    ld bc,257
    or a
    sbc hl,bc
    pop hl
    jr c,.wok               ; x + width <= 256
    ld hl,256
    or a
    sbc hl,de
.wok:
    ld (rt_img_cw),hl
    ; vertical: B = source rows skipped, A = screen y
    ld hl,(rt_img_y)
    ld b,0
    bit 7,h
    jr z,.ypos
    ld a,h
    inc a
    ret nz                  ; y < -256
    ld a,l
    neg
    ret z                   ; y = -256
    ld b,a
    xor a
    jr .ytop
.ypos:
    ld a,h
    or a
    ret nz
    ld a,l
    cp 192
    ret nc
.ytop:
    ld (rt_img_dy),a
    ld c,a
    ld a,(rt_img_h)
    sub b
    ret c
    ret z
    ld e,a                  ; rows left in the picture
    ld a,192
    sub c                   ; rows left on the screen
    cp e
    jr c,.hok
    ld a,e
.hok:
    ld (rt_img_ch),a
    ; source start: first page, row 0, column skip
    xor a
    ld (rt_img_rip),a
    ld hl,(rt_img_sx)
    ld de,$2000
    add hl,de
    ld (rt_img_ptr),hl
    ld a,b
    or a
    jr z,.row
.skip:
    push bc
    call rt_img_adv
    pop bc
    djnz .skip
.row:
    ld a,(rt_img_dy)
    ld d,a
    ld a,(rt_img_dx)
    ld e,a
    call rt_l2_map          ; slot 0 = destination page, HL = address
    ex de,hl
    ld a,(rt_img_page)
    ld (rt_farpage),a
    nextreg $51,a
    ld hl,(rt_img_ptr)
    ld bc,(rt_img_cw)
    ld a,(rt_img_mask)
    or a
    jr nz,.masked
    ldir
.next:
    ld hl,rt_img_dy
    inc (hl)
    call rt_img_adv
    ld hl,rt_img_ch
    dec (hl)
    jr nz,.row
    ld a,$FF
    ld (rt_farpage),a
    nextreg $51,a           ; ROM back in slot 1
    ret
; copy BC pixels, leaving the screen where the picture is TRANSPARENT
.masked:
    ld a,(hl)
    cp $E3
    jr z,.clear
    ld (de),a
.clear:
    inc hl
    inc de
    dec bc
    ld a,b
    or c
    jr nz,.masked
    jr .next
; next source row
rt_img_adv:
    ld hl,(rt_img_ptr)
    ld de,(rt_img_w)
    add hl,de
    ld (rt_img_ptr),hl
    ld hl,rt_img_rip
    inc (hl)
    ld a,(rt_img_rpp)
    cp (hl)
    ret nz
    ld (hl),0
    ld hl,rt_img_page
    inc (hl)
    ld hl,(rt_img_sx)
    ld de,$2000
    add hl,de
    ld (rt_img_ptr),hl
    ret

; 320x256 / 640x256: every pixel through rt_plot (which clips)
rt_img_hi:
    ld hl,0
    ld (rt_img_sx),hl
    xor a
    ld (rt_img_rip),a
    ld hl,$2000
    ld (rt_img_ptr),hl
    ld hl,(rt_img_y)
    ld (rt_img_cy),hl
    ld a,(rt_img_h)
    or a
    ret z
    ld (rt_img_ch),a
.row:
    ld hl,(rt_img_cy)
    ld a,h
    or a
    jr nz,.next             ; above or below the screen
    ld a,(rt_l2hmax)
    cp l
    jr c,.next
    ld a,(rt_img_page)
    ld (rt_farpage),a
    nextreg $51,a
    ld hl,(rt_img_x)
    ld (rt_img_cx),hl
    ld hl,(rt_img_ptr)
    ld bc,(rt_img_w)
.col:
    ld a,(hl)
    inc hl
    push hl
    push bc
    ld c,a
    ld a,(rt_img_mask)
    or a
    jr z,.draw
    ld a,c
    cp $E3
    jr z,.skip
.draw:
    ld a,(rt_img_cy)
    ld d,a
    ld hl,(rt_img_cx)
    ld a,c
    call rt_plot
.skip:
    ld hl,(rt_img_cx)
    inc hl
    ld (rt_img_cx),hl
    pop bc
    pop hl
    dec bc
    ld a,b
    or c
    jr nz,.col
.next:
    ld hl,(rt_img_cy)
    inc hl
    ld (rt_img_cy),hl
    call rt_img_adv
    ld hl,rt_img_ch
    dec (hl)
    jr nz,.row
    ld a,$FF
    ld (rt_farpage),a
    nextreg $51,a
    ret

rt_img_cx:   dw 0
rt_img_cy:   dw 0
rt_img_x:    dw 0
rt_img_y:    dw 0
rt_img_w:    dw 0
rt_img_sx:   dw 0
rt_img_cw:   dw 0
rt_img_ptr:  dw 0
rt_img_page: db 0
rt_img_h:    db 0
rt_img_rpp:  db 0
rt_img_rip:  db 0
rt_img_dx:   db 0
rt_img_dy:   db 0
rt_img_ch:   db 0
rt_img_mask: db 0
"#;

/// Banked code: functions in 8K pages, called through `rt_bcall`.
pub const BANKED: &str = r#"
; ---- banked code: functions in 8K pages mapped into MMU slot 2 ($4000) ----
; Every banked function has a stub in the resident area:
;     f: ld hl,f_bank / ld a,page / jp rt_bcall
; A = page, HL = address. The caller's page is mapped back afterwards; the
; return value in A or HL is kept. rt_codepage mirrors NR $52 (interrupts
; save and restore it, and rt_bret).
rt_bcall:
    ld c,a
    ld a,(rt_codepage)
    push af
    ld a,c
    ld (rt_codepage),a
    nextreg $52,a
    ld de,.back
    push de
    jp (hl)
.back:
    ld (rt_bret),a
    pop af
    ld (rt_codepage),a
    nextreg $52,a
    ld a,(rt_bret)
    ret

rt_codepage: db 10           ; bank 5, the normal contents of $4000
rt_bret:     db 0
"#;

/// Recursive calls: the handler when a saved frame would not fit.
pub const RECURSION: &str = r#"
; recursion too deep: the stack would run into the program
rt_stack_overflow:
    di
    ld sp,$FDFD
    ld hl,.msg
.l:
    ld a,(hl)
    or a
    jr z,.h
    push hl
    call rt_putc
    pop hl
    inc hl
    jr .l
.h:
    halt
    jr .h
.msg:
    db 10,"stack overflow (recursion too deep)",0
"#;

/// 32-bit runtime for `long` and `fixed16`.
/// X = DE:HL (DE = high word), Y = DE':HL' (the shadow registers).
pub fn long() -> String {
    let mut s = String::from(LONG_BASE);
    for (name, op) in [("and", "and"), ("or", "or"), ("xor", "xor")] {
        s.push_str(&format!(
            "rt_{name}32:\n    exx\n    push de\n    push hl\n    exx\n    pop bc\n    ld a,l\n    {op} c\n    ld l,a\n    ld a,h\n    {op} b\n    ld h,a\n    pop bc\n    ld a,e\n    {op} c\n    ld e,a\n    ld a,d\n    {op} b\n    ld d,a\n    ret\n"
        ));
    }
    s.push_str(&mul_routine("rt_mul32", 0, 3));
    s.push_str(&mul_routine("rt_umulhi32", 2, 5));
    s
}

/// Unsigned X * Y with the Z80N MUL D,E: product bytes `first..=last`
/// (0..3 = low 32 bits, 2..5 = bits 16..47) into DE:HL.
fn mul_routine(name: &str, first: usize, last: usize) -> String {
    let mut s = format!(
        "; X = bytes {first}..{last} of the unsigned product X * Y\n{name}:\n    push ix\n    ld bc,0\n    push bc\n    push bc\n    exx\n    push de\n    push hl\n    exx\n    push de\n    push hl\n    ld ix,0\n    add ix,sp               ; x at ix+0..3, y at ix+4..7, result at ix+8..11\n    ld hl,0\n    ld c,0\n"
    );
    for k in 0..=last {
        let pairs: Vec<(usize, usize)> = (0..4).filter(|i| k >= *i && k - i < 4).map(|i| (i, k - i)).collect();
        if k == last {
            s.push_str("    ld a,l\n");
            for (i, j) in &pairs {
                s.push_str(&format!("    ld d,(ix+{i})\n    ld e,(ix+{})\n    mul d,e\n    add a,e\n", 4 + j));
            }
            s.push_str(&format!("    ld (ix+{}),a\n", 8 + k - first));
        } else {
            for (i, j) in &pairs {
                s.push_str(&format!(
                    "    ld d,(ix+{i})\n    ld e,(ix+{})\n    mul d,e\n    add hl,de\n    jr nc,$+3\n    inc c\n",
                    4 + j
                ));
            }
            if k >= first {
                s.push_str(&format!("    ld (ix+{}),l\n", 8 + k - first));
            }
            s.push_str("    ld l,h\n    ld h,c\n    ld c,0\n");
        }
    }
    s.push_str("    ld l,(ix+8)\n    ld h,(ix+9)\n    ld e,(ix+10)\n    ld d,(ix+11)\n    ld ix,12\n    add ix,sp\n    ld sp,ix\n    pop ix\n    ret\n");
    s
}

const LONG_BASE: &str = r#"
; ---- 32-bit arithmetic: X = DE:HL, Y = DE':HL' ----
rt_add32:
    exx
    push de
    push hl
    exx
    pop bc
    add hl,bc
    ex de,hl
    pop bc
    adc hl,bc
    ex de,hl
    ret

rt_sub32:
    exx
    push de
    push hl
    exx
    pop bc
    or a
    sbc hl,bc
    ex de,hl
    pop bc
    sbc hl,bc
    ex de,hl
    ret

rt_neg32:
    xor a
    sub l
    ld l,a
    ld a,0
    sbc a,h
    ld h,a
    ld a,0
    sbc a,e
    ld e,a
    ld a,0
    sbc a,d
    ld d,a
    ret

rt_cpl32:
    ld a,l
    cpl
    ld l,a
    ld a,h
    cpl
    ld h,a
    ld a,e
    cpl
    ld e,a
    ld a,d
    cpl
    ld d,a
    ret

; signed compare X with Y: Z = equal, C = X < Y
rt_cmp32:
    ld a,d
    xor $80
    ld d,a
    exx
    ld a,d
    xor $80
    ld d,a
    push de
    push hl
    exx
    pop bc
    or a
    sbc hl,bc
    ex de,hl
    pop bc
    sbc hl,bc
    ret nz
    push af
    ld a,d
    or e
    jr nz,.ne
    pop af
    ret
.ne:
    pop af
    ld a,1
    jr c,.lt
    or a
    ret
.lt:
    or a
    scf
    ret

rt_min32:
    exx
    push de
    push hl
    exx
    push de
    push hl
    call rt_cmp32
    pop hl
    pop de
    jr c,.left
    pop hl
    pop de
    ret
.left:
    pop bc
    pop bc
    ret

rt_max32:
    exx
    push de
    push hl
    exx
    push de
    push hl
    call rt_cmp32
    pop hl
    pop de
    jr nc,.left
    pop hl
    pop de
    ret
.left:
    pop bc
    pop bc
    ret

; X <<= B
rt_shl32:
    ld a,b
    or a
    ret z
.l:
    add hl,hl
    rl e
    rl d
    djnz .l
    ret

; X >>= B (arithmetic)
rt_sar32:
    ld a,b
    or a
    ret z
.l:
    sra d
    rr e
    rr h
    rr l
    djnz .l
    ret

; unsigned X / Y: X = quotient, Y = remainder (X, Y < 2^31 + 1)
rt_divu32:
    ld bc,0
; the same with BC = the high part of the dividend (initial remainder)
rt_div_core:
    push ix
    exx
    push de
    push hl                 ; divisor at ix+0..3
    exx
    push bc
    exx
    pop hl
    ld de,0                 ; remainder
    exx
    ld ix,0
    add ix,sp
    ld b,32
.loop:
    add hl,hl
    rl e
    rl d                    ; next dividend bit
    exx
    adc hl,hl
    ex de,hl
    adc hl,hl
    ex de,hl                ; remainder = remainder * 2 + bit
    ld c,(ix+0)
    ld b,(ix+1)
    or a
    sbc hl,bc
    ex de,hl
    ld c,(ix+2)
    ld b,(ix+3)
    sbc hl,bc
    ex de,hl
    jr nc,.fits
    ld c,(ix+0)
    ld b,(ix+1)
    add hl,bc
    ex de,hl
    ld c,(ix+2)
    ld b,(ix+3)
    adc hl,bc
    ex de,hl
    exx
    djnz .loop
    jr .done
.fits:
    exx
    inc l                   ; quotient bit
    djnz .loop
.done:
    pop bc
    pop bc
    pop ix
    ret

; signed X / Y: X = quotient (truncated), Y = remainder (sign of X)
rt_divs32:
    ld a,d
    push af                 ; dividend sign
    exx
    xor d
    exx
    push af                 ; quotient sign
    bit 7,d
    call nz,rt_neg32
    exx
    bit 7,d
    call nz,rt_neg32
    exx
    call rt_divu32
    pop af
    bit 7,a
    call nz,rt_neg32
    pop af
    bit 7,a
    ret z
    exx
    call rt_neg32
    exx
    ret

rt_mods32:
    call rt_divs32
    exx
    ret

; 16.16: X = X * Y
rt_fmul32:
    ld a,d
    exx
    xor d
    exx
    push af
    bit 7,d
    call nz,rt_neg32
    exx
    bit 7,d
    call nz,rt_neg32
    exx
    call rt_umulhi32
    pop af
    bit 7,a
    ret z
    jp rt_neg32

; 16.16: X = X / Y
rt_fdiv32:
    ld a,d
    exx
    xor d
    exx
    push af
    bit 7,d
    call nz,rt_neg32
    exx
    bit 7,d
    call nz,rt_neg32
    exx
    ld b,d
    ld c,e                  ; the dividend is X << 16: X >> 16 starts the remainder
    ex de,hl
    ld hl,0
    call rt_div_core
    pop af
    bit 7,a
    ret z
    jp rt_neg32

; print X (signed 32-bit) in decimal
rt_print_s32:
    bit 7,d
    jr z,rt_print_u32
    call rt_neg32
    push de
    push hl
    ld a,'-'
    call rt_emit
    pop hl
    pop de
rt_print_u32:
    ld a,d
    or e
    jp z,rt_print_u16
    ld a,$FF
    push af                 ; sentinel
.div:
    exx
    ld hl,10
    ld de,0
    exx
    call rt_divu32
    exx
    ld a,l
    exx
    add a,'0'
    push af
    ld a,d
    or e
    or h
    or l
    jr nz,.div
.out:
    pop af
    cp $FF
    ret z
    call rt_emit
    jr .out

; print X (signed 16.16) with up to four decimals: 3.1416, -0.5
rt_print_fixed16:
    bit 7,d
    jr z,.pos
    call rt_neg32
    push de
    push hl
    ld a,'-'
    call rt_emit
    pop hl
    pop de
.pos:
    push hl                 ; fraction
    ex de,hl
    call rt_print_u16       ; integer part
    pop hl
    ld de,10000
    call rt_mulhi16         ; HL = fraction in 1/10000
    ld a,h
    or l
    ret z
    ld b,4
.trim:
    push hl
    push bc
    ld de,10
    call rt_divu16
    pop bc
    ld a,e
    or a
    jr nz,.keep
    pop af                  ; trailing zero: drop it
    dec b
    jr .trim
.keep:
    pop hl
    push hl
    push bc
    ld a,'.'
    call rt_emit
    pop bc
    pop hl
    ld c,b
.d1:
    push bc
    ld de,10
    call rt_divu16
    pop bc
    ld a,e
    add a,'0'
    push af
    djnz .d1
.d2:
    pop af
    push bc
    call rt_emit
    pop bc
    dec c
    jr nz,.d2
    ret
"#;

/// Sample playback: the zxnDMA plays a 4K ring buffer at $7000 (two 2K
/// halves, bank 5) to the Specdrum DAC (port $DF) at 875 kHz / prescaler,
/// restarting at its end; the frame interrupt refills the half that has
/// just been played from the sample's 8K pages.
pub const SAMPLES: &str = r#"
; ---- samples ----
SMP_BUF equ $7000

; HL = descriptor (db first page, prescaler; dw length bits 0-15;
; db length bits 16-23), A = 1: loop
rt_smp_play:
    push af
    push hl
    call rt_smp_stop
    pop hl
    pop af
    ld (rt_smp_loop),a
    ld (rt_smp_desc),hl
    inc hl
    ld a,(hl)
    ld (rt_smp_presc),a
    call rt_smp_load
    xor a
    ld (rt_smp_silent),a
    ld (rt_smp_next),a
    call rt_smp_fill        ; half 0
    ld a,1
    call rt_smp_fill        ; half 1
    ; NR $08 bit 3: the 8-bit DACs on
    ld bc,$243B
    ld a,8
    out (c),a
    inc b
    in a,(c)
    and $7F                 ; (bit 7 would unlock port $7FFD)
    or 8
    nextreg $08,a
    ld hl,rt_smp_prog
    ld bc,(rt_smp_prog_len << 8) | $6B
    otir
    ld a,1
    ld (rt_smp_on),a
    ret

rt_smp_stop:
    xor a
    ld (rt_smp_on),a
    ld a,%10000011          ; WR6: disable DMA
    out ($6B),a
    ret

; source position = start of the sample
rt_smp_load:
    ld hl,(rt_smp_desc)
    ld a,(hl)
    ld (rt_smp_page),a
    inc hl
    inc hl
    ld e,(hl)
    inc hl
    ld d,(hl)
    inc hl
    ld a,(hl)
    ld (rt_smp_left),de
    ld (rt_smp_left+2),a
    ld hl,0
    ld (rt_smp_off),hl
    ret

; HL = min(HL, DE), unsigned
rt_smp_min:
    or a
    sbc hl,de
    add hl,de
    ret c
    ex de,hl
    ret

; A = half (0 / 1): fill it with the next 2K of the sample (the rest of
; it with silence once the sample has ended and does not loop)
rt_smp_fill:
    ld de,SMP_BUF
    or a
    jr z,.h0
    ld de,SMP_BUF+2048
.h0:
    ld bc,2048
    ld a,(rt_farpage)
    push af
.loop:
    ld a,b
    or c
    jp z,.done
    ld hl,(rt_smp_left)
    ld a,(rt_smp_left+2)
    or h
    or l
    jr nz,.data
    ld a,(rt_smp_loop)
    or a
    jr z,.silence
    push bc
    push de
    call rt_smp_load
    pop de
    pop bc
    jp .loop
.silence:
    ld h,d
    ld l,e
    ld (hl),$80
    dec bc
    ld a,b
    or c
    jr z,.quiet
    inc de
    ldir
.quiet:
    ld hl,rt_smp_silent
    inc (hl)
    jp .done
.data:
    ld (rt_smp_need),bc
    ld (rt_smp_dst),de
    ld hl,8192
    ld de,(rt_smp_off)
    or a
    sbc hl,de               ; room left in the page
    ld de,(rt_smp_need)
    call rt_smp_min
    ld a,(rt_smp_left+2)
    or a
    jr nz,.nok
    ld de,(rt_smp_left)
    call rt_smp_min
.nok:
    ld b,h
    ld c,l                  ; BC = bytes in this step
    ld a,(rt_smp_page)
    ld (rt_farpage),a       ; (an interrupt restores this page)
    nextreg $51,a
    ld hl,(rt_smp_off)
    ld de,$2000
    add hl,de
    ld de,(rt_smp_dst)
    push bc
    ldir
    pop bc
    ld (rt_smp_dst),de
    ld hl,(rt_smp_off)
    add hl,bc
    ld a,h
    cp $20
    jr nz,.same
    ld hl,0
    ld a,(rt_smp_page)
    inc a
    ld (rt_smp_page),a
.same:
    ld (rt_smp_off),hl
    ld hl,(rt_smp_left)     ; left -= BC (24 bits)
    or a
    sbc hl,bc
    ld (rt_smp_left),hl
    jr nc,.nb
    ld hl,rt_smp_left+2
    dec (hl)
.nb:
    ld hl,(rt_smp_need)
    or a
    sbc hl,bc
    ld b,h
    ld c,l
    ld de,(rt_smp_dst)
    xor a
    ld (rt_smp_silent),a
    jp .loop
.done:
    pop af
    ld (rt_farpage),a
    nextreg $51,a
    ret

; frame interrupt: refill the half the DMA has left
rt_smp_tick:
    ld a,(rt_smp_on)
    or a
    ret z
    ld a,$BB                ; WR6: read mask follows
    out ($6B),a
    ld a,%00011000          ;   port A address
    out ($6B),a
    ld a,$A7                ; WR6: initiate read sequence
    out ($6B),a
    in a,($6B)              ; low byte
    in a,($6B)              ; high byte
    cp high(SMP_BUF+2048)
    ld a,0
    jr c,.h
    inc a
.h:
    ld hl,rt_smp_next
    cp (hl)
    ret z                   ; the half to refill is still playing
    ld a,(rt_smp_silent)
    cp 2
    jp nc,rt_smp_stop       ; the end has been played
    ld a,(hl)
    xor 1
    ld (hl),a
    xor 1
    jp rt_smp_fill

rt_smp_prog:
    db %10000011            ; WR6: disable DMA
    db %01111101            ; WR0: A->B, port A address + length follow
    dw SMP_BUF
    dw 4096
    db %01010100            ; WR1: port A = memory, incrementing, timing follows
    db %00000010            ;   2 cycles
    db %01101000            ; WR2: port B = I/O, fixed, timing follows
    db %00100010            ;   2 cycles, prescaler follows
rt_smp_presc:
    db 56                   ;   875 kHz / prescaler
    db %11001101            ; WR4: burst mode, port B address follows
    dw $00DF                ;   Specdrum DAC
    db %10100010            ; WR5: restart at the end of the block
    db %11001111            ; WR6: load
    db %10000111            ; WR6: enable DMA
rt_smp_prog_len equ $ - rt_smp_prog

rt_smp_desc:   dw 0
rt_smp_loop:   db 0
rt_smp_page:   db 0
rt_smp_off:    dw 0
rt_smp_left:   ds 3
rt_smp_need:   dw 0
rt_smp_dst:    dw 0
rt_smp_next:   db 0
rt_smp_silent: db 0
"#;

/// .pt3 music, rendered when compiling into AY registers per frame (see
/// pt3.rs): the frame interrupt writes one frame to the chosen AY chip.
/// The stream lives in 8K pages, read through slot 1 ($2000-$3FFF).
pub const MUSIC: &str = r#"
; ---- music ----
; HL = descriptor (db first page; dw loop address; db loop page),
; A = 1: loop, D = AY chip 0..2
rt_mus_play:
    push af
    push de
    push hl
    call rt_mus_stop
    pop hl
    pop de
    pop af
    ld (rt_mus_loop),a
    ld (rt_mus_desc),hl
    ld a,d
    ld (rt_mus_chip),a
    ld a,(hl)
    ld (rt_mus_page),a
    ld hl,$2000
    ld (rt_mus_ptr),hl
    ; NR $08 bit 1: TurboSound (all three AY chips)
    ld bc,$243B
    ld a,8
    out (c),a
    inc b
    in a,(c)
    and $7F
    or 2
    nextreg $08,a
    ld a,1
    ld (rt_mus_on),a
    ret

; stop and silence the chip
rt_mus_stop:
    ld a,(rt_mus_on)
    or a
    ret z
    xor a
    ld (rt_mus_on),a
rt_mus_quiet:
    ld a,(rt_mus_chip)
    ld d,a
    ld e,$3F
    ld a,7
    call rt_ay_write
    ld a,(rt_mus_chip)
    ld d,a
    ld e,0
    ld a,8
    call rt_ay_write
    ld a,(rt_mus_chip)
    ld d,a
    ld e,0
    ld a,9
    call rt_ay_write
    ld a,(rt_mus_chip)
    ld d,a
    ld e,0
    ld a,10
    jp rt_ay_write

; next byte of the stream (HL in slot 1, the next page after $3FFF)
rt_mus_byte:
    ld a,(hl)
    inc hl
    bit 6,h
    ret z
    push af
    ld h,$20
    ld a,(rt_mus_page)
    inc a
    ld (rt_mus_page),a
    nextreg $51,a
    pop af
    ret

; frame interrupt: one frame of registers
rt_mus_tick:
    ld a,(rt_mus_on)
    or a
    ret z
    ld a,(rt_farpage)
    push af
    ld a,(rt_mus_page)
    nextreg $51,a
    ld hl,(rt_mus_ptr)
    ld a,(rt_mus_chip)
    cpl                     ; chip 0/1/2 -> $FF / $FE / $FD
    ld bc,$FFFD
    out (c),a
.frame:
    call rt_mus_byte
    ld e,a
    call rt_mus_byte
    ld d,a
    bit 7,d
    jr z,.regs
    ; the end of the song
    ld a,(rt_mus_loop)
    or a
    jr z,.end
    ld hl,(rt_mus_desc)
    inc hl
    ld e,(hl)
    inc hl
    ld d,(hl)
    inc hl
    ld a,(hl)
    ld (rt_mus_page),a
    nextreg $51,a
    ex de,hl
    jr .frame
.end:
    xor a
    ld (rt_mus_on),a
    pop af
    nextreg $51,a
    jp rt_mus_quiet
.regs:
    xor a                   ; register number
.r:
    srl d
    rr e
    jr nc,.n
    ld bc,$FFFD
    out (c),a               ; register
    push af
    call rt_mus_byte
    ld b,$BF
    out (c),a               ; value (port $BFFD)
    pop af
.n:
    inc a
    cp 14
    jr nz,.r
    ld (rt_mus_ptr),hl
    pop af
    nextreg $51,a
    ret

rt_mus_on:    db 0
rt_mus_loop:  db 0
rt_mus_chip:  db 0
rt_mus_desc:  dw 0
rt_mus_page:  db 0
rt_mus_ptr:   dw 0
"#;
