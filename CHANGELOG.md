# Changelog

## Unreleased

**Language**

* Sprite collision detection: `sprite[a].overlaps(sprite[b])` compares the
  two sprites' squares, `sprite_collision()` reads the pixel-exact hardware
  flag (port `$303B`). The built-in emulator models the flag.

**Fixes**

* Circles and ellipses (`circle`, `fill_circle`, `ellipse`, `fill_ellipse`)
  no longer have single-pixel spikes at the top, bottom and sides: the
  half widths use the radius r + 1/2.
* Examples: `banks` and `samples` clear Layer 2 (garbage on real hardware),
  the moon in `shapes` goes behind the planet, and the planet's ring is
  drawn in one piece.

## 1.2.1 — 2026-11-20

**Tools**

* Program icon: `lz80nc.exe` carries the LZ80N icon on Windows, and the
  macOS packages set it as the Finder icon of `lz80nc`.
* VS Code extension 1.2.1: right-click menus in the editor and the Explorer
  (Build, Build and Run, Build without optimisation `-O0`, Build with code
  banking `--bank-all`, Show Generated Assembly, Check File, Reveal .nex).
* Release archives are published to the c64va.pro download area by the
  `publish` workflow.

No language or compiler changes.

## 1.2.0 — 2026-11-20

**Speed** (measurements: `docs/performance-review.md`, section 6)

* In assembly: `Sprite.update`, the sound driver, `line`, `vline`.
* `vline` 7.8x, `hline` 3x (DMA from 32 pixels), 640x256 `hline` 29x,
  circles and triangles 1.6-2x, number output 2.7x, `sqrt(word)` 2.3x,
  string appending 6.4x, `long`/`fixed16` += constant inline.
* Shorter branches: `and`/`or`/`not` jump directly, comparisons with
  constants, rotated `while` loops; `x op= y` on elements and fields works
  in memory.

**Fixes**

* `fill_triangle`: big triangles in 320x256 / 640x256 were drawn wrong
  (a 16-bit overflow).
* Calling `sound_tick()` from the main program is reported as an error.

**Benchmarks**: LZ80N vs NextBASIC measured in CSpect: LZ80N is 20-435x
faster (`benchmarks/README.md`).

## 1.1.0 — 2026-11-19

**Language**

* `long` (32-bit) and `fixed16` (16.16 fixed point).
* Struct values: `{...}` lists (also for arrays of structs), constructors
  `Type(args)` with an optional `init` method, functions returning structs,
  `class B extends A` with `super`.
* Layer 2 320x256 and 640x256 (`layer2_mode`, `layer2_width`,
  `layer2_height`); second palettes (`PAL_LAYER2_2` ..., `palette_select`).
* Main programs larger than the 31 KB area: big statements of the
  top-level code move into hidden, banked functions automatically.
* Recursion, direct and mutual (the caller's frame is saved on the stack
  only around calls that can come back; stack overflow is caught).
* `include "file.lz80n"`: programs in several source files (paths relative
  to the including file, every file included once).

**Sound**

* Samples: `.wav` assets played on the DAC in the background by the zxnDMA
  (`sample_play`, `sample_loop`, `sample_stop`, `sample_playing`).
* ProTracker 3 music: `.pt3` assets rendered into AY register frames when
  compiling, played by the frame interrupt (`music_play`, `music_once`,
  `music_stop`, `music_playing`).

**Tools**

* VS Code extension 1.1.0: the new keywords, types, functions and
  constants.
* The test emulator plays zxnDMA transfers to the DAC and reads AY
  registers back.

**Examples**: 24 programs (new: `modules`, `recursion`, `bignum`,
`inherit`, `hires`, `samples`, `music`).

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

**Examples**: 17 programs in `examples/`, from `hello.lz80n` to the
`shooter.lz80n` space shooter.
