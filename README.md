<p align="center">
  <img src="assets/LZ80N Neon Rainbow Logo.png" alt="LZ80N logo" width="500">
</p>

<h1 align="center">LZ80N - The Next Language</h1>

<p align="center"><em>The simplicity of Lua. The power of Z80N.</em></p>

<p align="center">
  <a href="https://github.com/zstarczali/LZ80N"><img alt="version: 1.0.0" src="https://img.shields.io/badge/version-1.0.0-brightgreen"></a>
  <a href="https://github.com/zstarczali/LZ80N/actions/workflows/ci.yml"><img alt="build" src="https://img.shields.io/github/actions/workflow/status/zstarczali/LZ80N/ci.yml?label=build"></a>
  <a href="https://github.com/zstarczali/LZ80N/blob/main/LICENSE"><img alt="license" src="https://img.shields.io/github/license/zstarczali/LZ80N?color=blue"></a>
</p>

<p align="center">
  <a href="compiler/"><img alt="compiler: Rust" src="https://img.shields.io/badge/compiler-Rust-b7410e?logo=rust&logoColor=white"></a>
  <a href="docs/manual-en.md#22-how-a-program-runs"><img alt="target: Z80N / ZX Spectrum Next" src="https://img.shields.io/badge/target-Z80N%20%2F%20ZX%20Spectrum%20Next-6f42c1"></a>
  <a href="docs/spec.md#toolchain"><img alt="output: .nex" src="https://img.shields.io/badge/output-.nex-6f42c1"></a>
  <a href="https://github.com/z00m128/sjasmplus"><img alt="assembler: SjASMPlus 1.24" src="https://img.shields.io/badge/assembler-SjASMPlus%201.24-555"></a>
</p>

<p align="center">
  <a href="https://github.com/zstarczali/LZ80N/commits"><img alt="last commit" src="https://img.shields.io/github/last-commit/zstarczali/LZ80N"></a>
  <a href="https://github.com/zstarczali/LZ80N/commits"><img alt="commit activity" src="https://img.shields.io/github/commit-activity/m/zstarczali/LZ80N"></a>
  <a href="https://github.com/zstarczali/LZ80N/issues"><img alt="issues" src="https://img.shields.io/github/issues/zstarczali/LZ80N"></a>
  <a href="https://github.com/zstarczali/LZ80N/stargazers"><img alt="stars" src="https://img.shields.io/github/stars/zstarczali/LZ80N?style=flat"></a>
</p>

<p align="center">
  <a href="docs/manual-en.md"><img alt="manual: English" src="https://img.shields.io/badge/manual-English-0a7bbb"></a>
  <a href="docs/manual-hu.md"><img alt="kézikönyv: Magyar" src="https://img.shields.io/badge/k%C3%A9zik%C3%B6nyv-Magyar-0a7bbb"></a>
  <a href="docs/spec.md"><img alt="spec: 1.0" src="https://img.shields.io/badge/spec-1.0-0a7bbb"></a>
  <a href="examples/"><img alt="examples: 19 programs" src="https://img.shields.io/badge/examples-19%20programs-2ea44f"></a>
  <a href="benchmarks/"><img alt="benchmark: LZ80N vs NextBASIC" src="https://img.shields.io/badge/benchmark-LZ80N%20vs%20NextBASIC-2ea44f"></a>
</p>

A statically typed, Lua-inspired language that compiles to native Z80N
machine code for the ZX Spectrum Next. No VM, no garbage collector.

Status: **1.0.0** — a complete command-line compiler: typed variables,
multi-dimensional arrays with initialisers, strings, `fixed` point math,
structs and classes, hardware sprites, Layer 2 graphics (lines, circles,
ellipses, triangles, PNG pictures), palettes, AY sound and music, zxnDMA,
copper and raster interrupts, tilemap text, inline `asm`, data and code
banking for programs beyond the 31 KB program area, a peephole optimizer,
and a test suite that runs the compiled `.nex` files in a built-in Z80N
emulator. See the [changelog](CHANGELOG.md).

## Layout

```
compiler/        Rust compiler (lz80nc): lexer → parser → AST → checks + Z80N codegen
docs/spec.md     what 1.0 implements and the decisions behind it
examples/        sample .lz80n programs
benchmarks/      the same tasks in LZ80N and NextBASIC
editors/vscode   VS Code extension: highlighting, errors on save, build + run in CSpect
tools/sjasmplus  SjASMPlus 1.24.0 (not in the repository, see below)
```

## Documentation

* [Programmer's manual (English)](docs/manual-en.md)
* [Programozói kézikönyv (magyar)](docs/manual-hu.md)
* [Specification](docs/spec.md)
* [Changelog](CHANGELOG.md)
* [VS Code extension](editors/vscode/README.md)

## Build and run

The compiler needs [SjASMPlus 1.24.0](https://github.com/z00m128/sjasmplus/releases/tag/v1.24.0)
(BSD licence). Put the Windows release into `tools/sjasmplus/sjasmplus-1.24.0.win/`
or pass `--sjasmplus PATH`.

```powershell
cd compiler
cargo build --release
cargo test
.\target\release\lz80nc.exe ..\examples\hello.lz80n      # -> hello.asm + hello.nex
```

Examples:

| File | Shows |
|------|-------|
| `hello.lz80n` | numbers, functions, `for`, border colours |
| `move.lz80n` | keyboard + Kempston input, `at` |
| `classes.lz80n` | structs, classes, methods, arrays of objects, by-reference params |
| `text.lz80n` | tilemap text layer: 40x32, built-in font, 16 inks |
| `layer2.lz80n` | Layer 2 bitmap: gradient, lines, rectangles, `point()`, layering with text and sprites |
| `math.lz80n` | `fixed` point, `sin`/`cos`/`atan2`/`sqrt`, gravity, Layer 2 double buffering |
| `palette.lz80n` | palette cycling animation, `dma_copy`, `addr()` |
| `raster.lz80n` | copper rainbow, `interrupt raster` / `interrupt frame`, `random()`, `cpu_speed()` |
| `tables.lz80n` | array initializers: menu, bar chart and colour tables, `/=` `%=` |
| `life.lz80n` | Game of Life on 2-D arrays |
| `shapes.lz80n` | circles, ellipses, clipping, double buffering |
| `images.lz80n` | PNG pictures on Layer 2 with transparency |
| `banks.lz80n` | `far` arrays: a 16 KB cave map in memory banks |
| `recursion.lz80n` | recursion: a fractal tree, the Towers of Hanoi |
| `modules.lz80n` | a program in several files with `include` |
| `shooter.lz80n` | a little space shooter with sprites cut from a picture |
| `strings.lz80n` | string values, `..`, `sub`/`find`/`upper`/`str`, user string functions, a ticker |
| `sound.lz80n` | 3 × AY: background music (`play_loop`), `beep` piano, laser and noise effects |
| `sprites.lz80n` | hardware sprites, PNG `asset` conversion, a class driving sprites |

`EXAMPLE=sprites FRAMES=120 cargo test render_example -- --ignored` renders
an example's display to `compiler/target/sprites.png` using the test emulator.

Run `examples\hello.nex` in an emulator (CSpect: `CSpect.exe -zxnext -w3 hello.nex`)
or copy it to the Next's SD card.

## Example

```lua
function square(byte x) -> word
    return x * x            -- byte * byte -> word, one MUL D,E
end

for i = 1, 10 do
    print(i, square(i))
end
```

See `docs/LZ80N.md` (concept document) for the full vision and roadmap.

## Benchmarks

`benchmarks/` holds the same seven tasks in LZ80N and NextBASIC
(`benchmark.lz80n`, `benchmark.bas`); see `benchmarks/README.md`.

## License

LZ80N is released under the [MIT License](LICENSE).
SjASMPlus, which the compiler calls, is a separate project under the BSD licence.
