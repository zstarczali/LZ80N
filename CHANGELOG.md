# Changelog

## 1.0.0 — 2026-11-19

The first release of LZ80N, a statically typed, Lua-inspired language that
compiles to native Z80N code for the ZX Spectrum Next (`.nex` files).

**Language**

* Types `bool`, `byte`, `sbyte`, `word`, `sword`, `fixed` (8.8), `string`
  (up to 64 characters); constants, casts, character literals.
* Arrays with up to 4 dimensions, initialiser lists (`byte t[] = {1, 2, 3}`,
  nested lists for rows), `far` arrays in memory banks (up to 64 KB each).
* Operators including `/= %= <<= >>=` and the other compound assignments.
* `if / elseif / else`, `while`, `for` with step, `break`; functions with
  return values (also strings); structs and classes with methods.
* `interrupt frame` and `interrupt raster(n)` handlers; inline `asm`.
* Recursion, direct and mutual (the caller's frame is saved on the stack
  only around calls that can come back; stack overflow is caught).
* `include "file.lz80n"`: programs in several source files (paths relative
  to the including file, every file included once).
* Reserved words and standard library names are reserved in every program.

**Libraries**

* Text on the tilemap (`print`, `write`, `at`, `ink`, 16 colours).
* Keyboard and Kempston joystick; `vsync`, `frames`, `random`, `cpu_speed`.
* Math: `abs min max clamp sqrt sin cos atan2 round`; string functions.
* Layer 2: pixels, lines, rectangles, circles, ellipses, triangles (filled
  and clipped), double buffering, PNG pictures (`asset x = image(...)`,
  `draw_image`).
* Hardware sprites from PNG sprite sheets; palettes; copper; zxnDMA.
* AY sound on all three chips: tones, noise, `beep`, background music.

**Compiler**

* `lz80nc` (Rust) → SjASMPlus → `.nex`; peephole optimiser, constant
  multiplication and division by multiplication, a fast text path.
* Programs larger than the 31 KB program area: functions move into 8 KB
  code pages automatically; big data goes into `far` arrays.
* Test suite running the compiled programs in a built-in Z80N emulator.

**Examples**: 19 programs in `examples/`, from `hello.lz80n` to the
`shooter.lz80n` space shooter.
