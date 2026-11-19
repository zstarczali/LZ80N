# LZ80N for Visual Studio Code

Language support for [LZ80N](https://github.com/zstarczali/LZ80N), the
Lua-inspired compiled language for the ZX Spectrum Next.

## Features

* **Syntax highlighting** for `.lz80n` files: keywords, types, numbers
  (`$FF`, `0x1F`, `0b1010`, `1.25` fixed point), strings and `'c'` literals,
  `--` and `--[[ ]]` comments, built-in functions and constants, and
  **Z80N assembly inside `asm … end` blocks** (mnemonics, registers, labels,
  `;` comments).
* **Errors as you save**: the file is compiled (assembly only, into a
  temporary folder) on open and save; compiler errors appear in the
  *Problems* panel and as squiggles.
* **Build** (`LZ80N: Build (.nex)`) and **Build and Run** (`F5` or the ▶
  button in the editor title): compiles to `.nex` next to the source and
  starts the emulator (CSpect by default arguments `-zxnext -w3`).
* **Show Generated Assembly**: compiles with `-S` and opens the `.asm`
  beside the source.
* **Outline / breadcrumbs / Go to Symbol**: functions, classes and structs
  with their fields and methods, constants, assets, interrupts, globals.
* **Hover** documentation for built-in functions, types and constants,
  and for your own functions (with the comment lines above them).
* **Completion**: keywords, built-ins, your declarations, locals and
  parameters, and members after `.` (`self.`, `ball.vel.`, `sprite[0].`).
* **Signature help** for built-in and user functions.
* **Go to Definition** for functions, types, constants, globals and fields.
* **Snippets**: `function`, `if`, `ifelse`, `for`, `while`, `mainloop`,
  `class`, `struct`, `irq`, `irqframe`, `asm`, `assetimg`, `assetspr`,
  `layer2`, `sprites`, `copper`.
* **Build task** with the `$lz80nc` problem matcher (`Ctrl+Shift+B` →
  *lz80n: build current file*).

## Setup

1. Build the compiler: `cd compiler && cargo build --release`.
   Inside the LZ80N repository the extension finds
   `compiler/target/release/lz80nc.exe` automatically; elsewhere set
   `lz80n.compilerPath` or put `lz80nc` on `PATH`.
2. The compiler needs SjASMPlus (see the main README). Set
   `lz80n.sjasmplusPath` if the compiler does not find it.
3. For *Build and Run* set `lz80n.emulatorPath` to `CSpect.exe` (you are
   asked for it the first time).

## Settings

| Setting | Default | Meaning |
|---------|---------|---------|
| `lz80n.compilerPath` | *(auto)* | `lz80nc` executable |
| `lz80n.sjasmplusPath` | *(auto)* | passed as `--sjasmplus` |
| `lz80n.compilerArgs` | `[]` | extra build arguments, e.g. `-O0`, `--bank-all` |
| `lz80n.emulatorPath` | | emulator executable |
| `lz80n.emulatorArgs` | `["-zxnext", "-w3", "${file}"]` | `${file}` = the `.nex` path |
| `lz80n.checkOnSave` | `true` | compile on save and show errors |
| `lz80n.checkOnOpen` | `true` | also check when a file is opened |
