# LZ80N 1.0 — implemented language and decisions

This is the *working* specification for what `lz80nc` 1.0 implements. It turns
the open questions of the concept document into concrete (and revisable)
decisions. Anything not listed here is not implemented yet.

## Toolchain

* Host cross-compiler written in Rust (`compiler/`, binary `lz80nc`).
* Output: Z80N assembly in sjasmplus syntax (`.asm`), assembled with
  SjASMPlus 1.24.0 (`tools/`) into a `.nex` file (`SAVENEX`, core 3.0.0+).
* `lz80nc prog.lz80n [-o out.nex] [-S]` — `-S` stops after the `.asm`.

## Lexical

* Line comments `-- ...`, block comments `--[[ ... ]]`.
* Integers: `123`, `0x7F`, `$7F`, `0b1010`, `_` separators allowed.
* Decimal literals `1.5` are `fixed` (see below).
* Strings `"..."` with `\n \t \\ \" \0` escapes, up to 255 characters.
* Character literals `'a'`, `'\n'` are integer constants (the ASCII code).
* Newlines and indentation are insignificant; blocks end with `end`.

## Types

| Type    | Size | Notes |
|---------|------|-------|
| `bool`  | 1    | 0 / 1; not interchangeable with integers |
| `byte`  | 1    | unsigned |
| `sbyte` | 1    | signed (two's complement) |
| `word`  | 2    | unsigned, little endian |
| `sword` | 2    | signed |
| `fixed` | 2    | signed 8.8 fixed point: -128 .. 127.996, step 1/256 |
| `string` | 65  | up to 64 characters (length byte + text), copied by value |

Later phases: `long`, `fixed16` (16.16), `color`, `rgb333`, `pointer`, `farptr`.

### Fixed point (`fixed`)

There is no floating point: the Z80N has no FPU and a software float costs
hundreds to thousands of T-states per operation. `fixed` is a 16-bit
8.8 number (raw value / 256), which is what Next games use for smooth
movement, speeds and gravity.

* Literals with a decimal point are `fixed`: `1.5`, `-0.25`, `0.1` (stored
  rounded: 0.1 is 26/256 = 0.1016, so ten times 0.1 is 1.02).
  `const G = 0.2` declares a fixed constant.
* Integer constants adapt: `f + 1`, `f > 2`, `fixed v = 0`.
* Integer *variables* never mix implicitly with `fixed` in `+ - %` and
  comparisons: write `fixed(n)`. Exceptions, because they are exact and
  cheap: `fixed * integer`, `integer * fixed` and `fixed / integer` are
  plain 16-bit operations (`v * 2` is one `ADD HL,HL`).
* `fixed * fixed` and `fixed / fixed` use runtime routines (`MUL D,E`
  based; results truncated toward zero, overflow wraps, `/ 0` gives 127.99).
* Conversions: `fixed(n)` from an integer; `sword(f)`, `byte(f)` … floor;
  `round(f)` → `sword` rounds half up.
* `print`/`write` show up to two decimals: `1.5`, `-0.38`, `3`.
* Bitwise operators and `~` are not allowed on `fixed`; `<< >>` scale by
  powers of two.

### Math functions

| Call | Types | |
|------|-------|---|
| `abs(x)` | any number | type of `x` |
| `min(a, b)`, `max(a, b)` | any numbers | common type |
| `clamp(x, lo, hi)` | any numbers | `min(max(x, lo), hi)` |
| `sqrt(x)` | integer → `byte`; `fixed` → `fixed` | bit-by-bit square root (negative fixed → 0) |
| `sin(a)`, `cos(a)` | `byte` angle → `fixed` | 256 units per turn (64 = 90°), table lookup |
| `atan2(y, x)` | `sword` or `fixed` → `byte` angle | 0 = +x, 64 = +y; ~1 unit precision |
| `round(f)` | `fixed` → `sword` | |
| `random(n)` | | see hardware intrinsics |

With constant arguments `abs`, `min`, `max` and `clamp` are folded at
compile time. The sine and arctangent tables (≈ 0.6 KB) are only included
when `sin`, `cos` or `atan2` is used. Angles fit in a `byte`, so they wrap
around for free: `angle += 1`.

### Arithmetic rules (decided)

* All integer arithmetic **wraps** (modulo 2^8 / 2^16). No overflow traps.
* An operation is evaluated at the width of the **destination** (the
  variable, parameter or return type it flows into), never narrower than its
  operands. So `function add(byte a, byte b) -> word  return a + b  end`
  widens `a` and `b` to 16 bits first and `add(200, 100)` is `300`.
* Signedness: an operation is signed if any operand or the destination is
  signed. Byte→word widening sign-extends signed values, zero-extends
  unsigned ones.
* Implicit **widening** is allowed; implicit **narrowing** is an error.
  Use a cast: `byte(w)`, `sbyte(x)`, `word(x)`, `sword(x)` (truncates).
  Same-width signedness changes (`word` ↔ `sword`) are implicit.
* Integer constants are untyped and take the type of the context; they
  must fit (8-bit: -128..255, 16-bit: -32768..65535).
* Division truncates toward zero; `%` takes the sign of the dividend.
  Division by a constant zero is a compile error; at run time `x / 0`
  gives `$FFFF` (remainder = dividend).
* Shift counts use the low 5 bits (Z80N barrel shifter); `>>` is
  arithmetic for signed and logical for unsigned types.
* Conditions must be `bool` — Lua's "0 is true" rule is deliberately **not**
  adopted; write `x ~= 0`.

Operator precedence (low → high): `or`, `and`, comparisons
(`== ~= != < <= > >=`), `|`, `~` (xor), `&`, `<< >>`, `+ -`, `* / %`,
unary (`- not ~`).

## Declarations and statements

```lua
byte lives = 3          -- typed variable
var x = 1000            -- inferred (word)
const SPEED = 2         -- compile-time constant (folded)
byte buf[64]            -- fixed-size array
byte t[] = {1, 2, 3}    -- initialised array (size from the list)
byte m[4][8]            -- multi-dimensional (up to 4), m[y][x], row-major
far byte big[20000]     -- banked array (8K pages via MMU slot 1)

x = x + 1               -- also += -= *= /= %= &= |= <<= >>=
if c then ... elseif d then ... else ... end
while c do ... end
for i = a, b [, step] do ... end   -- inclusive; step is a nonzero constant
break
return [expr]
asm ... end             -- raw Z80N assembly lines
```

* Top-level declarations are globals, visible to all functions.
  Declarations inside blocks are local to the block.
* Uninitialized scalar variables are zero-initialized; arrays start at zero
  when the program is loaded.
* Array initializers `{...}`: compile-time constants of the element type
  (strings: literals / string constants). Missing values are 0 / `""`.
  Globals: the values are emitted as data (no code). Locals: copied from a
  constant table with `LDIR` each time the declaration runs. `name[]`
  takes the size from the list. Arrays of structs: one `{...}` per
  element (fields in order, nested lists for nested structs / array fields).
  Multi-dimensional: nested lists are rows (`{{1,2},{3,4}}`) or one flat
  list; only the first size may be `[]`.
* `asset name = image("file.png")`: a Layer 2 picture (<= 256 x 255) stored
  in 8K pages from page 28 (no row crosses a page), drawn with
  `draw_image(name, x, y)` (clipped, TRANSPARENT pixels skipped);
  `image_width` / `image_height` are compile-time constants.
* `far` arrays (top level, numeric/bool elements, <= 64 KB): pages from the
  same pool as pictures (page 28 up). Element access = byte offset in HL,
  `rt_far_map` (A = first page) maps page + (offset >> 13) into slot 1 and
  returns $2000 + (offset & $1FFF). `rt_farpage` mirrors NR $51; the ISR
  saves/restores it when any banked page exists. Values are evaluated
  before the target is mapped. All-zero pages are not stored in the .nex
  (SAVENEX AUTO skips empty banks) and are cleared at startup
  (`rt_far_zero`). No place()/by-ref/addr() for far elements.
* Code banking: functions are compiled into separate chunks. If the
  estimated resident size (asm_size, ~exact) exceeds 30500 bytes, the
  largest non-interrupt functions move into 8K code pages (first fit,
  <= 7800 est. bytes per page) assembled at $4000 via `MMU 2 e`. Each gets
  a resident stub `f: ld hl,f_bank / ld a,page / jp rt_bcall`; rt_bcall
  saves rt_codepage, maps NR $52, calls, restores (keeps A via rt_bret, HL;
  flags not kept). The ISR saves/restores rt_codepage and rt_bret.
  `--bank-all` (tests: compile_opts) = one function per page.
* Reserved names: keywords, type names, `self`, built-in functions, built-in
  constants, and the public names of the standard modules (always, even
  when the module is not used) plus the `snd_` prefix
  (`prelude::LIBRARY_NAMES`). Field and method names are not restricted.
  See the manual, section 3.5.
* `for`: the loop variable is a new local typed from the bounds; the end
  value is evaluated once; the loop never overflows (`for i = 0, 255` on a
  `byte` runs exactly 256 times).
* Array indexes are not bounds-checked at run time; constant indexes are
  checked at compile time.
* Compound assignment to `a[expr]` evaluates `expr` twice.

## Structs and classes

```lua
struct Vec2
    sbyte x
    sbyte y
end

class Ball                 -- a class is a struct with methods
    byte x = 16            -- field defaults: compile-time constants
    Vec2 vel               -- nested struct
    byte trail[8]          -- array field (size: any constant expression)

    function move() -> bool
        self.x += self.vel.x
        return self.x > 31
    end
end

Ball balls[3]              -- arrays of structs
balls[i].vel.x = -1
if balls[i].move() then ... end
```

* Type names start with an upper-case letter. A struct may contain
  primitives, arrays, and previously declared structs (not itself).
* Layout is **packed** in declaration order, no padding.
  `struct PlayerState  word x  word y  byte health  byte lives  end` is 6 bytes.
* **Value semantics for storage**: every struct variable owns its storage;
  `a = b` copies the whole struct (`LDIR`).
* **Reference semantics for parameters**: struct/class parameters, and
  `self`, are passed by address (2 bytes), so a function can modify the
  caller's object (like Lua tables). Struct values are returned through a
  static buffer per function (`r_<fn>`, HL = its address); the caller copies it.
* **Struct values**: `{...}` lists (constant: an image in the data, else
  field by field) and constructors `T(args)`: the default image `sd_T` is
  copied in, then `init(args)` is called if `T` has one, else the args go
  to the fields in order. Temporaries are hidden static (frame) variables.
* Field defaults are stored each time the declaration executes (globals at
  their place in the main program, locals on every entry to the block).
  Without defaults, globals start at zero.
* Methods are ordinary functions with a hidden first parameter `self`;
  `obj.m(args)` is a direct `CALL`. `class B extends A`: A's fields come
  first at the same offsets, methods are inherited unless redefined;
  `super.m()` / `A.m(obj)` call a given version. A `B` is accepted where an
  `A` is expected. Binding is static: no virtual dispatch.
  Inside a method, fields are always accessed via `self.`.
* Address computation picks the cheapest form: constant offsets fold
  into the address at assembly time; a `byte` index into an array of
  non-power-of-two-sized elements uses one `MUL D,E`.

## Strings

```lua
string name = "Next"
string s = "Hello, " .. name .. "! " .. 42     -- numbers are formatted like print
if s ~= "" and find(s, "Next") ~= NOT_FOUND then print(upper(s)) end
```

* A `string` variable, field or array element holds up to **64**
  characters: a length byte followed by the text (65 bytes). There is no
  heap and no garbage collector; longer results are **truncated** at 64.
* Assignment copies the text (`a = b`, `a = "x" .. a` are safe: the
  right side is built in a temporary first). Globals start empty, locals
  are emptied on each entry to their declaration.
* `..` concatenates (precedence between shifts and `+ -`, as in Lua).
  Numbers, `fixed` values and bools on either side are converted to text.
* `==`, `~=`, `<`, `<=`, `>`, `>=` compare in dictionary (byte) order.
* String parameters are passed **by reference** (like structs): a function
  that assigns to its parameter changes the caller's variable.
  Functions can return strings (`-> string`); the result lives in a
  per-function buffer and is copied by the caller when needed.
* Positions are 0-based.
* `const TITLE = "text"` declares a string constant.
* String **literals** (and constants) may be up to 255 characters, so long
  texts and tunes can be printed or played directly; storing one in a
  `string` variable keeps the first 64 characters.

| Function | Result | |
|----------|--------|---|
| `len(s)` | byte | number of characters |
| `sub(s, from, count)` | string | substring; clipped to the end of `s` |
| `find(s, t)` | byte | first position of `t` in `s`, `NOT_FOUND` (255) if absent |
| `val(s)` | sword | leading decimal number (`"-45"` → -45, `"7up"` → 7, `""` → 0) |
| `str(x)` | string | number / fixed / bool as text, like `print` |
| `asc(s, i)` | byte | character code at `i` (0 outside the string) |
| `chr(c)` | string | one-character string |
| `upper(s)`, `lower(s)` | string | ASCII letters converted |

`print` and `write` accept string expressions. Internally the print
routines write through a redirectable output (`rt_emit`), so formatting a
number into a string and printing it use the same code.

## Sound (standard module)

The Next has three AY-3-8912 chips (TurboSound, enabled at startup):
**9 channels**, channel = chip × 3 + voice (A/B/C).

| Call | |
|------|---|
| `tone(ch, period)` | square wave; frequency = 109375 / period Hz (period 1..4095) |
| `noise(ch, period)` | noise, period 0..31 (one noise generator per chip) |
| `volume(ch, v)` | 0..15 |
| `sound_off(ch)`, `sound_reset()` | silence one channel / everything (also stops tunes) |
| `note_period(n)` → word | period of note `n` = octave × 12 + semitone (C4 = 48, A4 = 57 = 440 Hz) |
| `beep(ch, note, frames)` | play a note for a number of frames, **without waiting** |
| `play(ch, tune)` / `play_loop(ch, tune)` | play a tune in the background (once / repeating) |
| `playing(ch)` → bool | a beep or tune is still running |
| `ay_write(chip, reg, value)` | raw AY register write |

Tunes use a notation close to the 128K / NextBASIC `PLAY` command:

| | |
|---|---|
| `c d e f g a b` | note; `#` or `+` sharp, `-` flat; an optional length in frames: `c8` |
| `r` | rest (same length rules) |
| `o4` | octave 0..8 (default 4) |
| `>` `<` | octave up / down |
| `l10` | default note length in frames (default 10; 50 frames = 1 s) |
| `v12` | volume 0..15 (default 12) |

Other characters (spaces, `|`) are ignored. The last frame of each note is
silent so repeated notes stay separate. A tune may be up to 255 characters;
`play` copies it (with the DMA), so it can be a temporary string.

`beep` and tunes run in the background: the sound driver `sound_tick`
(written in LZ80N, `compiler/src/prelude.rs`) is called from the frame
interrupt. `ay_write` holds interrupts off for its select + write pair and
restores the previous interrupt state, so the main program, interrupt
handlers and the driver can all use the chips. The driver's functions
must not be called from the main program (compile-time check).

## Assets

```lua
asset sheet = "assets/sprites.png"   -- path relative to the source file
```

Converted at compile time and embedded in the program:

* `.png` → 8-bit sprite patterns. The image (width and height multiples of
  16) is cut into 16×16 cells left-to-right, top-to-bottom. Each pixel is
  mapped to RRRGGGBB (the Next's default sprite palette). Alpha < 128 →
  transparent index `$E3`; opaque pixels that would map to `$E3` become `$E2`.
  At most 64 patterns.
* `.spr` → raw 8-bit patterns (multiple of 256 bytes).
* anything else → raw bytes (reserved for future loaders).

## Hardware sprites (standard module)

The sprite module is written in LZ80N (`compiler/src/prelude.rs`) and is
compiled in automatically when the program mentions `sprite`.

```lua
sprite_load(0, sheet)                 -- upload patterns 0.. (port $303B / $5B, OTIR)
sprites_visible(true, false)          -- NextReg $15: sprites on, not over border
sprite[0].position(152, 200)          -- 128 hardware sprites: sprite[0]..sprite[127]
sprite[0].show(0)                     -- pattern 0, visible
sprite[1].mirror_x = true             -- change shadow fields ...
sprite[1].update()                    -- ... then upload them
```

`Sprite` fields: `x`, `y` (word, hardware coordinates — the 256×192 area
starts at 32,32), `pattern`, `palette` (offset 0–15), `visible`, `mirror_x`,
`mirror_y`, `rotate`, `scale` (0–3: ×1, ×2, ×4, ×8), `id` (read-only).
Methods: `position(x, y)`, `show(pattern)`, `hide()`, `update()`.
Attributes go through NextReg `$34`–`$39` (5-byte form: X8 in attr 2,
Y8 in attr 4).

## Layer 2 (standard module)

256×192, one byte per pixel, colours RRRGGGBB (default palette);
`TRANSPARENT` (`$E3`) lets lower layers show through.

| Call | |
|------|---|
| `layer2_visible(on)` | port `$123B` bit 1 |
| `layer2_clear(c)` | fill the screen (6 × 8K pages, zxnDMA) |
| `layer2_flip()` | double buffering: show the buffer drawn so far, draw into the other (NextReg `$12` / `$13` banks) |
| `plot(x, y, c)` / `point(x, y)` → byte | write / read one pixel; rows ≥ 192 ignored |
| `hline(x, y, len, c)` | horizontal line, `LDIR`, clipped at x = 255 |
| `vline(x, y, len, c)` | vertical line |
| `line(x0, y0, x1, y1, c)` | Bresenham line |
| `rect(x, y, w, h, c)` / `fill_rect(x, y, w, h, c)` | outline / filled rectangle |
| `rgb(r, g, b)` → byte | r, g 0–7, b 0–3 → RRRGGGBB |

`plot`, `point`, `hline`, `layer2_clear` are assembly built-ins; the rest
is written in LZ80N (`compiler/src/prelude.rs`) and compiled in when used.
Pixels are accessed through **MMU slot 0** (`$0000`–`$1FFF`, NextReg `$50`):
page = Layer 2 base (NextReg `$12` × 2) + y / 32, offset = (y & 31) × 256 + x;
the current page is cached. Consequently `memory[$0000..$1FFF]` refers to
the last mapped Layer 2 page, not the ROM, once Layer 2 drawing was used.

Layer order is set to **S-U-L** (NextReg `$15` = `%010`): sprites above
the text layer above Layer 2.

## Palettes (standard module)

| Call | |
|------|---|
| `palette(layer, index, colour)` | 8-bit RRRGGGBB colour (NextReg `$43`, `$40`, `$41`) |
| `palette9(layer, index, colour)` | 9-bit RRRGGGBBB colour (NextReg `$44`, two writes) |
| `rgb9(r, g, b)` → word | r, g, b 0–7 → RRRGGGBBB |

`layer`: `PAL_ULA`, `PAL_LAYER2`, `PAL_SPRITES`, `PAL_TEXT` (the first
palette of each layer; the second palettes are not used yet). Text ink `k`
is entry `2k+1` of `PAL_TEXT`, its paper is entry `2k`.
Palette cycling: draw once with palette indices, then rewrite the entries
every frame (`examples/palette.lz80n`).

## DMA

| Call | |
|------|---|
| `dma_copy(src, dst, len)` | memory → memory copy with the zxnDMA (port `$6B`) |
| `dma_fill(dst, len, value)` | fill memory (port A fixed on a byte holding `value`) |
| `addr(x)` → word | address of a variable, array, element or field |

Transfers run in continuous mode and the CPU waits until they finish.
Addresses are CPU (logical) addresses, so the current MMU mapping
applies. `len = 0` does nothing. `layer2_clear` uses `dma_fill` per 8K page.

## Target machine

LZ80N targets the **ZX Spectrum Next only**. The startup code sets the
CPU to 28 MHz (NextReg `$07` = 3). There is no 48K compatibility mode.
Text output uses the Next **tilemap** (see below), not the ULA screen
and not the ROM font.

## Text layer

* Tilemap in **text mode**: 40×32 characters of 8×8 pixels covering the
  whole 320×256 display (the 256×192 "paper" area is columns 4–35,
  rows 4–27 — the same area as sprite coordinates 32..287 / 32..223).
* Built-in original 8×8 font (`compiler/src/font.rs`, characters 32–127;
  127 is a solid block), uploaded at startup to `$6C00`. Tilemap at `$6000`.
* Each character has its own ink (palette offset); 16 inks set up in
  tilemap palette 0. Paper is `$E3` (the global transparency colour), so
  sprites, Layer 2 and the ULA border/paper show through. **To verify on
  hardware:** text-mode transparency against NextReg `$14`.
* Default layer order (NextReg `$15` = SLU): sprites above the text.
* The ULA is switched off at startup (NextReg `$68` bit 7). Where every
  layer is transparent the Next shows the fallback colour (NextReg `$4A`),
  which `border(c)` sets — so it colours the border *and* the background.

## Functions and ABI

```lua
function name(byte a, word b) -> word
    ...
end
```

* **Static frames**: every parameter and local has a fixed address.
  The caller stores arguments directly into the callee's parameter slots
  (via the stack when an argument itself contains a call).
* Return value: `A` (8-bit/bool) or `HL` (16-bit).
* Recursion: `call_fn` leaves `;@rec_save caller|callee` /
  `;@rec_restore` markers around every call. After code generation
  (`expand_recursion`), a call where the callee can reach the caller in the
  call graph becomes: stack check (`SP - frame - 64 < __program_end` ->
  `rt_stack_overflow`), push the caller's whole frame (params, locals,
  string temporaries; 1/2-byte slots with push, larger ones copied with
  `ldir`), the call, then restore with the return value (`A`, `HL`) parked
  in `af'` / `exx`. Other calls drop the markers. A self-call with more than
  one argument evaluates its arguments onto the stack first
  (`gcd(b, a % b)`).
* Functions are not reentrant from interrupts: a function reachable both
  from the main program and from an `interrupt` handler is a compile-time
  error.
* All registers except `SP` are caller-clobbered.

## Inline assembly contract

* Lines between `asm` and `end` are copied verbatim into the output.
* The compiler holds no values in registers across statements, so an `asm`
  block may clobber `AF BC DE HL IX IY` and the shadow registers.
  It must preserve `SP`, must leave the stack balanced, and must not
  change the interrupt mode or `I`.
* Labels should be sjasmplus local labels (`.loop`) or unique names.

## Interrupts

```lua
interrupt raster(96)        -- runs when the raster reaches line 96
    nextreg[0x4A] = 0x1C
end

interrupt frame             -- runs once per frame
    ticks += 1
end
```

* Lines use the Next raster numbering: 0..191 = display, 192.. bottom
  border and vertical blank, the top border is roughly 280..311 (50 Hz).
  The line must be a constant 0..311; each line can have one handler.
* Without `interrupt raster` the ULA frame interrupt drives the frame
  counter and `interrupt frame`. With raster handlers the ULA interrupt
  is switched off and one **line interrupt** is chained through all
  handler lines plus line 192, where the frame counter is incremented
  and `interrupt frame` runs (so `vsync()` returns at the bottom of the
  display, the best moment to update the screen).
* The ISR saves AF, BC, DE, HL; interrupts stay disabled while a handler
  runs. Handlers should be short (a raster line is ~1800 T-states at
  28 MHz). Use `nextreg[...] = ...` with constant registers (the `NEXTREG`
  instruction does not disturb the main program's `$243B` selection).
  Do not draw text or Layer 2 from a handler (shared cursor and MMU state).
* Global variables start as 0 and handlers may run before the main
  program's initialisers.

## Copper (standard module)

| Call | |
|------|---|
| `copper_begin()` | stop the copper, start writing at instruction 0 |
| `copper_wait(line)` | WAIT until the raster reaches `line` |
| `copper_move(reg, value)` | MOVE: write NextReg `reg` (0..127) |
| `copper_start()` | append HALT and run the list from the top of every frame (NextReg `$62` = `$C0`) |
| `copper_stop()` | stop the copper |

Rebuilding the list right after `vsync()` (in the bottom border) avoids
tearing. See `examples/raster.lz80n`.

## Hardware intrinsics

| Construct           | Code |
|---------------------|------|
| `nextreg[n] = v`    | `NEXTREG n,v` / `NEXTREG n,A` (constant register), port `$243B/$253B` otherwise |
| `v = nextreg[n]`    | read via ports `$243B/$253B` |
| `memory[addr]`      | byte read / write at a 16-bit address |
| `border(c)`         | background/border colour 0–15 → fallback colour NextReg `$4A` |
| `port[n]`           | 16-bit I/O port read / write (`IN A,(C)` / `OUT (C),A`) |
| `vsync()`           | wait until the frame counter changes (raster interrupts in between do not end the wait) |
| `pause(n)`          | wait `n` frames |
| `ay_write(chip, reg, v)` | AY register write (see Sound) |
| `cpu_speed(s)`      | NextReg `$07`: `MHZ_3`, `MHZ_7`, `MHZ_14`, `MHZ_28` (startup: 28 MHz) |
| `random(n)` → word  | pseudo-random 0..n-1 (xorshift16); `random(0)` = full 16-bit value |
| `seed(w)`           | set the random seed (same seed → same sequence) |
| `any_key()` → bool  | any key held |
| `wait_key()`        | wait for all keys released, then a key press |
| `print(a, b, ...)`  | prints numbers, fixed values, bools and strings on the text layer, space-separated, then newline; scrolls at the bottom |
| `write(a, b, ...)`  | like `print` but without separators and without newline |
| `at(row, col)`      | set the print position (row 0–31, col 0–39; clamped) |
| `ink(c)`            | text colour 0–15 (`BLACK`..`WHITE`, `BRIGHT_BLACK`..`BRIGHT_WHITE`) |
| `cls()`             | clear the text layer, cursor to 0,0 |
| `key(KEY_x)` → bool | keyboard matrix read; constant key → inline `IN A,($FE)` |
| `kempston()` → byte | `IN A,($1F)`; test bits with `JOY_RIGHT/LEFT/DOWN/UP/FIRE` |
| `frames()` → word   | frame counter incremented by the IM2 handler |

Built-in constants: `KEY_A`..`KEY_Z`, `KEY_0`..`KEY_9`, `KEY_SPACE`,
`KEY_ENTER`, `KEY_CAPS`, `KEY_SYM`, `JOY_*`, and the colours `BLACK BLUE
RED MAGENTA GREEN CYAN YELLOW WHITE`.

`print` evaluates each argument at its **own** type (no widening context):
`byte m = 20  print(m * 13)` prints `4`, like assigning to a `byte` would.
A line longer than 40 characters wraps.

Z80N instructions currently selected by the backend: `MUL D,E` (8×8→16,
16-bit multiply), `NEXTREG`, `ADD HL,nn`, `BSLA/BSRA/BSRL DE,B`
(variable shifts).

## Optimizer

A peephole pass (`compiler/src/peephole.rs`, disable with `-O0`) runs over
the generated code: redundant reloads after stores, jumps to a label
that directly follows, dead code after `jp`/`ret`, unused labels, `add a,1` → `inc a`.
Finally `jp`/`jp z|nz|c|nc` are relaxed to `jr` when the target is
provably within range (size upper bounds; never across inline `asm`).
This favours size (a taken `jr` is 2 T-states slower than `jp`); a speed
mode may keep `jp` in hot loops later. Inline `asm` blocks are never modified.

## Testing

`cargo test` compiles programs, assembles them with SjASMPlus, loads the
resulting `.nex` into a small Z80N interpreter (`compiler/src/emu.rs`,
test-only) and compares the printed output, NextReg/border state, sprite
attributes/patterns and the tilemap. `EXAMPLE=name FRAMES=n cargo test
render_example -- --ignored` renders an example's display (background,
Layer 2, text layer, sprites) to `compiler/target/<name>.png`.
The interpreter counts T-states (Z80 timings, no memory wait states;
writes to NextReg `$7F` are recorded as timing marks for benchmarks) and
derives raster timing from them (224 T-states per line at 3.5 MHz, scaled
by the CPU speed; 312 lines, frame start at line 248), delivers IM2 interrupts (ULA frame
and line interrupts), runs the copper per line, and records the display
state of every line so raster effects show up in the rendered image.
It is not cycle exact and does not replace testing in CSpect / on hardware.

## Runtime / memory layout

```
$4000..$5AFF  ULA screen (cleared to black)
$6000..$69FF  tilemap (text layer)
$6C00..$6FFF  text font (tile definitions)
$8000..$FCFF  program code + data       (ASSERT checked)
$FDFD..$FDFF  JP __isr                  (IM2 jump target)
$FE00..$FF00  IM2 vector table (257 × $FD)
$FDFC..       stack (grows down)
```

* Startup: `DI`, set `SP`, CPU to 28 MHz, ULA off, fallback colour black,
  layer order S-U-L, Layer 2 256×192, build the IM2 table, `IM 2`, clear the ULA
  `EI`. The ISR (generated per program) increments the frame counter
  (`rt_frames`) and dispatches `interrupt` handlers.
  The program does not depend on the ROM's interrupt handler or system
  variables.
* After the main program ends, the CPU idles in an `EI : HALT` loop.

## Not yet implemented (planned order)

1. Benchmarks: measure `benchmarks/benchmark.bas` and `benchmark_int.bas`
   on the Next and fill in `benchmarks/README.md`; faster call arguments.
2. `long`, `fixed16` (16.16), Layer 2 320×256 / 640×256 modes, second palettes.
3. Constructors / struct literals, returning structs, inheritance.
4. `bank` blocks, far pointers, banked calls, larger programs (MMU).
5. Sound: 8-bit DAC sample playback, music import (PT3 / Vortex Tracker).
6. An IR between the AST and the backend, register allocation.

## 32-bit types

* `long` (signed 32) and `fixed16` (16.16) live in DE:HL (DE = high word).
  Binary operations: left operand pushed, right evaluated, `exx`, left
  popped: X = DE:HL, Y = DE':HL' (constant right operands are loaded
  straight into the shadow set). Runtime (`runtime::long()`, emitted when
  used): add/sub/and/or/xor/neg/cpl, `rt_cmp32` (Z equal, C less, signed),
  `rt_mul32` (low 32 bits from 10 `MUL D,E` partial products, IX frame),
  `rt_divu32`/`rt_divs32`/`rt_mods32` (restoring division, remainder in the
  shadow set), `rt_fmul32` (bytes 2..5 of the product), `rt_fdiv32`
  ((X << 16) / Y with X >> 16 as the initial remainder), shifts, min/max,
  decimal output. The ISR saves the shadow registers when 32-bit code or
  recursion is used.
* Fixed point literals are lexed as raw 16.16; `fixed` rounds to 8.8.
  `FConst` holds raw 16.16. A literal's natural type is `fixed` if it fits
  8.8, else `fixed16`.

## Source files

* `include "path"` (top level only) is expanded on the token stream before
  parsing (`include.rs`). Paths are relative to the including file; each
  canonical path is included once (the main file counts). Tokens carry
  `Pos::file` (0 = main); diagnostics in other files get `Diag::file`.
  Asset paths resolve against the declaring file's directory
  (`Options::file_dirs`). The standard library detection scans the text of
  all files.

