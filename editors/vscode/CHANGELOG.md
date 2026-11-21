# Changelog

## 1.3.0

* Follows LZ80N 1.3.0 (sprite collision detection, faster lines and
  ellipses).
* `sprite_collision()` in completion, hover help and highlighting;
  `overlaps(other)` in the hover help of `sprite[n]`.

## 1.2.1

* Follows LZ80N 1.2.1 (program icon; no language changes).
* New extension icon: the neon LZ80N emblem; `.lz80n` files get the pixel
  icon of the compiler in the Explorer.
* Right-click menus, as in the Ultimate Basic extension: in the editor
  Build, Build and Run, Build without Optimisation (`-O0`), Build with Code
  Banking (`--bank-all`), Show Generated Assembly, Check File and Reveal .nex
  in File Explorer; on `.lz80n` files in the Explorer Build, Build and Run,
  Show Generated Assembly and Reveal .nex.
* The commands work on the right-clicked file, and are grouped under
  *LZ80N* in the Command Palette.

## 1.2.0

* Follows LZ80N 1.2.0 (a faster compiler; no language changes).

## 1.1.0

* Follows LZ80N 1.1.0: keywords `extends`, `super`; types `long`,
  `fixed16`; functions `layer2_mode`, `layer2_width`, `layer2_height`,
  `palette_select`, `sample_*`, `music_*`; constants `L2_256`, `L2_320`,
  `L2_640`, `PAL_*_2`. Hover, completion and signature help for all of them.

## 0.1.0

* First version: syntax highlighting (with embedded Z80N `asm` blocks),
  diagnostics on save, build / run in emulator / show assembly commands,
  build task and problem matcher, outline, hover, completion, signature
  help, go to definition, snippets.
