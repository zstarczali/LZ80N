# Self-hosting: LZ80N written in LZ80N

The goal is an `lz80nc` that runs on the ZX Spectrum Next itself. The Rust
compiler stays the stage-0 compiler that builds it.

| Step | State |
|---|---|
| 1. Built-in assembler + NEX writer (`compiler/src/asm/`) | done: no SjASMPlus needed |
| 2. Files on the SD card (`file_*` module, esxDOS API) | done |
| 3a. Lexer (`lexer.lz80n`) | done: same tokens as `compiler/src/lexer.rs` |
| 3b. Parser (`parser.lz80n`) | done: same tree as `compiler/src/parser.rs` |
| 4. Code generation | next |
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
