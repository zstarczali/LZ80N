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
| 4b. Code generation (`codegen.lz80n`) | the subset the compiler itself is written in |
| 5. The compiler compiles itself | done: `selfhost_bootstrap` reaches a fixed point |

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
comes from SD card files, the texts of `compiler/src/runtime.rs`, `font.rs`
and `prelude.rs`: `rt_start.asm`, `rt_lib.asm`, `rt_long.asm`,
`rt_banked.asm`, `rt_font.asm`, `lib_file.lz80n` (the tests make them).

It does not copy `compiler/src/codegen.rs` instruction for instruction: it
makes simpler code (no optimisation) with the same labels (`g_ p_ l_ f_ s_`),
calling convention and runtime, so `asm` blocks and the standard modules work
unchanged.

Supported: `bool byte sbyte word sword long string`, constants, globals,
locals, parameters, one-dimensional arrays (initialiser lists, string
arrays), `far` arrays, every integer operator with the width / signedness
rules of the spec, `..` and string comparisons, casts, `if while for break
return`, functions, recursion (a call that can come back to the caller saves
the caller's frame `fr_NAME..fr_NAME_end` on the stack), `print write len asc
chr str addr memory[] port[] nextreg[] asm`, the Layer 2 built-ins (`plot
point hline layer2_clear layer2_flip layer2_mode layer2_width
layer2_height`), `abs min max` (folded when constant), `vsync pause cls at
ink border key kempston any_key wait_key frames random seed`, the built-in
constants (`KEY_* JOY_* BLACK.. MHZ_* NOT_FOUND`, from `lib_consts.lz80n`),
structs and classes (fields with defaults, array and struct fields, `{...}`
values, constructors `Type(...)` with or without `init`, methods and `self`,
`extends`, `super.m()`, `Type.m(obj)`, struct arrays, struct and string
parameters by reference, functions and methods returning structs or strings
through `r_NAME` buffers), `include`, the standard modules `sprites` (the
`Sprite` class and `sprite[]`), `file`, `Layer 2`, `palette` and `copper`
(pulled in by the same words as in `compiler/src/prelude.rs`), and code
banking (functions move into 8K pages at $4000 when the resident area would
overflow; code sizes are estimated per emitted instruction, the data of
locals, temporaries and literals is reserved from the whole tree first).

The self-hosted sources must not contain the words that pull in a module
(`plot`, `rect`, `sprite` ...), not even in comments, or the module is
compiled into them; where they need such a word they write it with a `|`
(`"p|lot"`) and drop the bar at run time.

Not yet: `fixed`, `fixed16`, multi-dimensional arrays, assets (so
`sprite_load` and the examples that load images or music), interrupts,
string constants, the other built-in functions (`dma_*`, `sin`, `upper` ...)
and the sound module, outlining of very large main programs, and diagnostics of type errors (programs are
expected to be correct: check them with lz80nc first).

## Tests

* `selfhost_codegen_runs_like_rust`, `selfhost_codegen_strings_long_far_files`:
  programs compiled both ways must print the same in the emulator (includes,
  files on the virtual SD card and a banked program among them);
* `selfhost_codegen_layer2`: drawings (all the shapes of the Layer 2 module,
  the 320x256 mode), palettes, NextRegs, ports, the copper and the border must
  be the same as with lz80nc, page by page;
* `selfhost_codegen_structs`: structs, classes, inheritance and the sprite
  module: the same output and sprite attributes as with lz80nc;
* `SELFHOST=1 cargo test --release`: every `expect(...)` program of the test
  suite is also compiled by the self-hosted compiler when it supports it
  (40 of 62 now);
* `selfhost_codegen_examples` (ignored, a report): which examples the
  self-hosted compiler compiles: now classes, hello, hires, inherit, move,
  tables and text;
* `selfhost_compiles_lexer`, `selfhost_compiles_parser_and_assembler`: the
  self-hosted compiler compiles the lexer, the parser and the assembler; they
  give byte-identical results to their lz80nc-compiled versions;
* `selfhost_bootstrap`: the code generator compiled by lz80nc (stage 1)
  compiles its own source into stage 2; stage 2 makes the same assembly as
  stage 1 for test programs, and for its own source (stage 3 = stage 2).

## The bootstrap in numbers (emulated, 28 MHz)

Compiling `codegen.lz80n` (with `parser_core.lz80n` and the file module,
about 4600 lines): lexer 11 s, code generator 138 s, giving 39033 lines of
assembly; assembling that with `asm.lz80n` takes about 3 more minutes. The
compiled code generator is a 115K NEX (9 code pages). It uses 8K pages up to
191, so it needs a 2 MB Next: the token and tree arrays are the bulk and can
shrink later (positions only for errors, tokens freed after parsing).

## Running it on the Next

```
lz80nc --export-selfhost DIR
```

writes everything into DIR: `lexer.nex`, `codegen.nex`, `asm.nex` (built from
these sources), the runtime and module files, the sources themselves, and
`chain.on`. Copy DIR to the SD card (a 2 MB Next), then in NextBASIC:

```
.cd /lz80n                     (the folder you copied)
COPY "myprog.lz80n" TO "in.lz80n"
.nexload lexer.nex
```

`lexer.nex` writes `tokens.bin` and starts `codegen.nex` (esxDOS
`M_EXECCMD`, `nexload`), which writes `out.asm` and starts `asm.nex`, which
writes `out.nex`. With a file named `run.on` in the folder `out.nex` is started
too. Errors appear on the screen (and in `codegen.err` / `asm.err`). Without
`chain.on` every stage stops after its work. `file_exec` / `file_exists` are
new in the file module for this.

`selfhost_export_and_chain` runs exactly this in the emulator (its virtual
SD card implements `M_EXECCMD` as "start that NEX") and checks `out.nex`
byte for byte and by its output. **Not yet tried on a real Next or in
CSpect**: whether `.nexload` started from a running NEX through
`M_EXECCMD` behaves there as expected is the first thing to check.
