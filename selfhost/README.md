# Self-hosting: LZ80N written in LZ80N

The goal is an `lz80nc` that runs on the ZX Spectrum Next itself. The Rust
compiler stays the stage-0 compiler that builds it.

| Step | State |
|---|---|
| 1. Built-in assembler + NEX writer (`compiler/src/asm/`) | done: no SjASMPlus needed |
| 2. Files on the SD card (`file_*` module, esxDOS API) | done |
| 3a. Lexer (`lexer.lz80n`) | done: same tokens as `compiler/src/lexer.rs` |
| 3b. Parser (`parser.lz80n`) | done: same tree as `compiler/src/parser.rs` |
| 4a. Assembler (`asm.lz80n`) | done: same NEX as `compiler/src/asm/` |
| 4b. Code generation (`codegen.lz80n`) | first part: integer programs, functions, recursion |
| 5. The compiler compiles itself | |

## lexer.lz80n

Reads `in.lz80n`, writes the tokens to `tokens.bin` (record format at the top
of the file). `cargo test selfhost_lexer_matches_rust` runs it in the test
emulator on every example, the standard modules, itself and a set of edge
cases (including every lexer error) and compares the output byte for byte
with the Rust lexer. Speed: about 10 KB of source per second at 28 MHz.

Memory: `far` arrays (8K pages) take the place of a heap; `file_getc` buffers
the input, so the source is never held in memory whole.

## parser.lz80n

Reads `tokens.bin`, builds the syntax tree in `far` arrays (node numbers
instead of pointers; lists are chained through `nx`) and writes `ast.txt`, a
canonical text form of the tree with every position. On an error the file
holds only `error LINE:COL message`, with the message the Rust parser gives.
`cargo test selfhost_parser_matches_rust` runs lexer + parser in the emulator
on the examples, the standard modules, both self-hosted sources and 35 edge
and error cases, and compares `ast.txt` with the Rust tree printed the same
way.

Limits: 16000 tokens, 12000 tree nodes, 60000 bytes of names/strings/asm.
Speed (28 MHz): about 90 lines per second; `shooter.lz80n` (471 lines) takes
5.3 s. Most of the time goes into `far` array accesses (each one maps a page)
and the token file round trip; to be tuned once code generation exists.

## asm.lz80n

Assembles `in.asm` into the NEX named by its `SAVENEX OPEN` (errors:
`asm.err`, `LINE: message`). The same subset as the built-in Rust assembler:
all Z80 and Z80N instructions, labels and `.local` labels, `equ`, `ORG`,
`MMU` (with `e` / `n`), `DB DW DD DS ALIGN ASSERT`, `SAVENEX OPEN CORE CFG
AUTO CLOSE` (no `INCBIN`, no `SAVENEX BANK`). Symbols live in a hash table in
`far` arrays, the output image in up to 35 8K pages.

Tests:
* `selfhost_asm_matches_rust`: two examples, NEX byte for byte;
* `selfhost_asm_all_instruction_forms`: every instruction of the encoder's
  test table and the data directives / expression operators;
* `selfhost_asm_errors`: the error messages;
* `selfhost_asm_all_examples` (ignored, ~80 s): all 24 examples, NEX byte
  for byte: `cargo test --release selfhost_asm_all -- --ignored`.

Two passes (layout, then output; more only if a value changes). Speed at
28 MHz: about 220 lines per second, e.g. 9.5 s for `hello` (2132 lines),
54 s for `shooter` (9134 lines); `images` takes 99 s (large data, many pages).

## codegen.lz80n

`include`s `parser_core.lz80n` (the parser of `parser.lz80n` without its
output), reads `tokens.bin` and writes `out.asm` for `asm.lz80n`. The runtime
comes from SD card files: `rt_start.asm`, `rt_lib.asm`, `rt_font.asm` (the texts
of `compiler/src/runtime.rs` and `font.rs`).

It does not copy `compiler/src/codegen.rs` instruction for instruction: it
makes simpler code (no optimisation) with the same labels (`g_ p_ l_ f_ s_`),
calling convention and runtime, so `asm` blocks and the standard modules work
unchanged. The checks are behavioural:

* `selfhost_codegen_runs_like_rust`: nine programs, compiled both ways, must
  print the same in the emulator;
* `SELFHOST=1 cargo test --release`: every `expect(...)` program of the test
  suite is also compiled by the self-hosted compiler when it supports it, and
  must print the same (now 17 of 62; the rest use what is not there yet).

Supported: `bool byte sbyte word sword`, constants, globals, locals,
parameters, one-dimensional arrays (with initialiser lists), every integer
operator with the width / signedness rules of the spec, casts, `if`,
`while`, `for` (steps, no overflow), `break`, `return`, functions, recursion
(a call that can come back to the caller saves the caller's frame
`fr_NAME..fr_NAME_end` on the stack), `print`, `write`, `memory[]`, `asm`.

Not yet: strings variables and `..`, `fixed`, `long`, `fixed16`, structs and
classes, multi-dimensional and `far` arrays, assets, interrupts, the built-in
functions beyond print/write/casts and the standard modules (Layer 2,
sprites, sound ...), and diagnostics of type errors (programs are expected
to be correct: check them with lz80nc first).
