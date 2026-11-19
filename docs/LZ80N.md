# LZ80N — The Next Language

> The simplicity of Lua. The power of Z80N.

## 1. Project overview

LZ80N is a proposed modern, statically typed, Lua-inspired programming language designed exclusively for the ZX Spectrum Next. Its compiler is intended to generate optimized native Z80N machine code, without a virtual machine or garbage collector.

The goal is to combine the accessibility of BASIC and Lua with the performance and hardware control of assembly, making game and application development on the Next easier without hiding its unique capabilities.

**Status:** design/concept phase. This document records the decisions and ideas discussed so far; it is not a finalized language specification or an implementation report.

## 2. Identity

- **Final chosen name:** LZ80N (L = Lua, Z80N = Z80 Next).
- Previous names: L80, LZ80 (superseded).
- Target: ZX Spectrum Next only; no compatibility requirement for other Z80 platforms.
- Proposed product names: LZ80N Compiler, LZ80N Studio, LZ80N SDK, LZ80N Debugger.
- Proposed source extension: `.lz80n`.
- Proposed project extension: `.lz80nproj`.
- Intended executable output: `.nex`.
- Taglines: **The Next Language**; **The simplicity of Lua. The power of Z80N.**
- Branding: clean, minimal, geometric wordmark, with the classic Spectrum-inspired red/yellow/green/cyan stripes integrated into the lettering. Dark and light background variants have been explored.

A preliminary name search in the conversation found no matching LZ80N repository or programming language. This does not establish trademark clearance or domain availability. The similar name `lz80` has been used for an assembler. Official trademark and domain checks remain open.

## 3. Design goals

- Lua-inspired syntax with explicit block endings (`end`), no mandatory semicolons or significant indentation.
- Static typing with compile-time type inference where safe.
- Native machine code; no Lua runtime, bytecode VM or garbage collector.
- Structures, classes, methods, constructors, optional inheritance and polymorphism.
- Fixed-size arrays, enums, modules, constants, pointers and references.
- Inline Z80N assembly and direct hardware access.
- Predictable memory usage and runtime performance.
- Hardware-aware optimizing compiler, with explicit escape hatches for expert programmers.
- Bank-aware code and data placement, with diagnostics for unsafe bank switches.
- Beginner-friendly SDK for graphics, sound, input and Next hardware.
- Prefer zero-cost abstractions where feasible; optional runtime features should have explicit costs.

LZ80N is an independent language, not a compatible implementation or port of Lua. Lua's dynamic tables, metatables and garbage collection are not intended as the default object model.

## 4. Proposed syntax

All examples are illustrative and have not been compiled. Syntax and semantics remain subject to design decisions.

### Variables, conditions and loops

```lua
byte lives = 3
word score = 0
var name = "Player"
const speed = 2

if score > 100 then
    lives += 1
end

for i = 0, 9 do
    print(i)
end

while lives > 0 do
    lives -= 1
end
```

### Functions

```lua
function add(byte a, byte b) -> word
    return a + b
end
```

### Structures

```lua
struct PlayerState
    word x
    word y
    byte health
    byte lives
end

PlayerState player
player.x = 100
player.lives = 3
```

With packed layout this example occupies 6 bytes. Alignment, initialization and value/reference semantics need specification.

### Classes and methods

```lua
class Player
    word x = 128
    word y = 96
    byte health = 100

    function move(sword dx, sword dy)
        self.x += dx
        self.y += dy
    end

    function draw()
        sprite[0].position(self.x, self.y)
    end
end

Player player
player.move(2, 0)
player.draw()
```

Ordinary methods should compile to direct calls or inline code. Virtual dispatch, if supported, should be opt-in. Object storage and lifetime rules must be defined before implementing classes.

### Inline assembly and hardware access

```lua
asm
    ld d, 10
    ld e, 20
    mul d,e
end

nextreg[0x07] = 3
memory[0x4000] = 255
```

Inline assembly needs a defined register-clobber contract and safe interaction with the compiler's calling convention. The register and memory writes above are low-level examples, not universally safe operations.

### Hardware-oriented constructs

```lua
bank 32
    byte levelData[8192]
end

asset playerImage = "player.png"
asset music = "music.pt3"
asset level = "level.tmx"

interrupt raster(128)
    screen.border(RED)
end
```

These are proposed constructs for bank placement, compile-time asset conversion and interrupt handling. The hardware and linking semantics still need specification.

## 5. Proposed primitive types

| Type | Proposed representation |
|---|---|
| `bool` | 1 byte in ordinary storage |
| `byte` | unsigned 8-bit |
| `sbyte` | signed 8-bit |
| `word` | unsigned 16-bit |
| `sword` | signed 16-bit |
| `long` | 32-bit; signedness to be specified |
| `fixed8` | 16-bit fixed point (8.8) |
| `fixed16` | 32-bit fixed point (16.16) |
| `color` | 8-bit palette index |
| `rgb333` | 9-bit color in 16-bit storage |
| `pointer` | 16-bit near address |
| `farptr` | bank-aware pointer; representation TBD |

Define overflow, signed conversions, integer promotion, literal inference, array indexing, string encoding/storage and fixed-point rounding explicitly. In particular, an expression returning a 16-bit result from two 8-bit operands must have well-defined widening behavior.

## 6. ZX Spectrum Next target architecture

The Next uses an FPGA implementation of the Z80-compatible Z80N processor, not a software-emulated Z80. Its CPU supports 3.5, 7, 14 and 28 MHz modes, a 64 KiB logical address space and banked physical RAM via the MMU.

Hardware areas relevant to the language and SDK:

- Z80N CPU and its enhanced instruction set.
- MMU, RAM banking and NextReg registers.
- ULA, Layer 2, tilemap, hardware sprites, palettes and video composition.
- Copper, DMA, interrupt/raster timing and CTC.
- Three AY-compatible sound generators (TurboSound), other audio features as applicable.
- Keyboard, joystick, mouse, SD card / DivMMC and NextZXOS services.
- Additional peripherals and core-version-specific capabilities as required.

Z80N instructions to consider during backend instruction selection include `MUL D,E`, `SWAPNIB`, `MIRROR A`, `NEXTREG`, `BSLA`, `BSRA`, `ADD HL,A`, `PIXELAD`, `PIXELDN` and `LDIRX`. Exact encodings, flag effects, timing and core version compatibility must be verified against documentation and implementation.

## 7. Compiler architecture

Proposed host implementation: Rust, with a possible Tauri + React IDE later.

```text
.lz80n source
    -> Lexer / Parser
    -> Abstract Syntax Tree (AST)
    -> Name resolution / type checking
    -> Intermediate Representation (IR)
    -> Target-independent optimizations
    -> Z80N instruction selection / register allocation
    -> Assembly output or direct machine code
    -> Assembler / linker / bank placement
    -> .nex output + symbol/debug information
```

For the first version, emitting Z80N assembly and using SjASMPlus for assembling and NEX packaging is a proposed shortcut. A direct machine-code backend could follow. The compiler should not depend on a software emulator to generate valid code.

### Optimization goals

- Constant folding and propagation, dead code elimination.
- Peephole optimization, efficient register allocation and calling conventions.
- Inline functions and methods when beneficial.
- Selection of Z80N-specific instructions where semantics match.
- Compact object and structure layout.
- Code/data bank placement and safe bank-aware calls.
- Optional speed, size and debug optimization modes.
- Compile-time asset conversion and optional compile-time evaluation.
- Cycle estimates and profiling support.

DMA substitution and bank switching are not universally safe automatic optimizations. They must respect side effects, interrupts, mapping, timing, overlap and memory visibility. Prefer explicit intrinsics initially.

## 8. Proposed standard library / SDK

| Module | Purpose |
|---|---|
| `cpu`, `nextreg`, `memory`, `mmu` | CPU, registers, RAM and banking |
| `screen`, `layer2`, `palette` | Display and palettes |
| `sprite`, `tilemap` | Hardware-assisted game graphics |
| `copper`, `dma` | Raster effects and data movement |
| `sound` | AY and other supported audio features |
| `keyboard`, `joystick`, `mouse` | Input |
| `file` | SD card / NextZXOS file access |
| `interrupt` | Interrupt and raster handlers |

### Example game loop

```lua
import next

class Player
    word x = 128
    word y = 96

    function update()
        if keyboard.left then
            self.x -= 2
        end
        if keyboard.right then
            self.x += 2
        end
    end

    function draw()
        sprite[0].position(self.x, self.y)
    end
end

screen.mode(LAYER2)
screen.clear(BLACK)
sprite[0].load("player.spr")
sprite[0].visible = true

Player player

while true do
    player.update()
    player.draw()
    vsync()
end
```

The example is a design sketch; input APIs, coordinates, loading, clipping and synchronization behavior are not finalized.

## 9. Hardware reference and development resources

The official FPGA VHDL source is the primary hardware implementation reference. It is particularly useful for answering questions about register behavior and timing that higher-level documentation may omit. It is not a substitute for the CPU instruction documentation, NEX format, NextZXOS API, board/peripheral documentation or real hardware testing.

| Resource | URL | Intended use |
|---|---|---|
| Official FPGA core | https://gitlab.com/SpectrumNext/ZX_Spectrum_Next_FPGA | Hardware implementation reference |
| Spectrum Next Wiki | https://wiki.specnext.dev/ | Hardware, registers, CPU and programming documentation |
| SjASMPlus | https://github.com/z00m128/sjasmplus | Assembler and NEX output |
| Klive IDE | https://github.com/Dotneteer/kliveide | Emulator and debugger candidate |
| Klive documentation | https://kliveide.net/ | Emulator architecture and documentation |
| JNEXT | https://github.com/jorgegv/jnext | Alternative emulator/debugger candidate |
| MAME | https://github.com/mamedev/mame | Independent emulation reference |
| GHDL | https://github.com/ghdl/ghdl | VHDL simulation for selected hardware modules |

These resources were discussed earlier; their current revisions, capabilities, license details and links should be verified before dependency adoption. The preceding discussion identified the official VHDL as GPLv3 and Klive as MIT, but individual dependencies and ROMs may have separate terms. Reading hardware source as a reference and incorporating its code are distinct activities with potentially different licensing implications.

## 10. Emulation and debugging strategy

A dedicated emulator is not required for the initial language and compiler implementation.

1. Use official hardware documentation and VHDL for the backend and SDK.
2. Test generated programs in existing Next emulators and on a physical Next.
3. Use independent tests for Z80N instructions, flags, MMU, graphics, DMA and interrupts.
4. Add bank-aware source maps, variable locations and debug symbols.
5. If useful later, integrate an emulator core such as Klive into LZ80N Studio; inspect its API and licensing first.
6. Consider VHDL simulation for selected timing-sensitive hardware behaviors.

No emulator was benchmarked or verified cycle-accurate in the discussion. Select the target FPGA core version(s), pin reference commits and validate behavior against actual hardware.

## 11. Future LZ80N Studio

Potential features: code editor, syntax highlighting, autocomplete, building and launching `.nex` files, integrated emulator, source-level debugger, breakpoints and watches, structure/object inspection, CPU registers, memory-bank visualization, profiling, sprite/palette/tilemap editors, Copper/DMA inspectors and music tools.

This is a later project phase, not a prerequisite for the compiler. A command-line compiler and a small SDK should be usable independently of the IDE.

## 12. Proposed development roadmap

### Phase 0 — Specification and reference material

- Pin target hardware/core versions and establish the official documentation/reference set.
- Define grammar, primitive types, expressions, scope, error behavior and ABI.
- Define stack usage, memory layout, bank mapping and NEX output requirements.
- Decide compiler dependency and SDK licensing.

### Phase 1 — Minimal working compiler

- Lexer/parser, AST, semantic analysis, typed variables and expressions.
- Conditions, loops, functions, fixed-size arrays, basic inline assembly.
- Z80N assembly output, SjASMPlus integration, minimal startup code and NEX generation.
- Run a minimal program in an emulator and on physical hardware.
- Add automated compiler and runtime regression tests.

### Phase 2 — Hardware SDK

- CPU/NextReg/MMU, input, screen, palettes and sprites.
- Layer 2, tilemap, audio and filesystem functionality.
- DMA, Copper and interrupts, along with hardware compatibility tests.

### Phase 3 — Language and optimization

- Structures, methods, classes and explicit object lifetime semantics.
- Bank-aware linking, far pointers and assets.
- IR optimizations, register allocation, Z80N instruction selection and profiling.

### Phase 4 — Tools and distribution

- Source-level debugger, optional integrated emulator and LZ80N Studio.
- Documentation, tutorials, example games, SDK reference and release packaging.
- Trademark/domain verification and public project branding.

## 13. Open design questions

- Is the compiler primarily a host cross-compiler, or should a native Next-hosted compiler also exist later?
- What are the exact syntax rules, type inference rules and arithmetic overflow semantics?
- Are classes value types, reference types, or both? How are objects allocated and initialized?
- What calling convention is used for arguments, returns, methods and interrupts?
- How are near/far pointers, banked calls and mutable hardware state represented?
- What is the default runtime and NextZXOS coexistence policy?
- Which FPGA core versions and physical Next revisions are supported?
- Which functions are intrinsics, libraries or language constructs?
- What license will the compiler, SDK and generated runtime use?
- What source-level debug format is best for banked addresses and optimized code?

---

*Project memory snapshot — 2026-11-18. Chosen name and exclusive target are established; other technical details and roadmap are proposals synthesized from the project discussion, not accepted or tested specifications.*
