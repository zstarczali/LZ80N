# LZ80N Programmer's Manual

> The simplicity of Lua. The power of Z80N.

LZ80N is a statically typed, Lua-inspired programming language for the
**ZX Spectrum Next**. The compiler `lz80nc` translates LZ80N source code into
native Z80N machine code and packs it into a `.nex` file that runs directly
on the Next or in an emulator. There is no interpreter, no virtual machine and
no garbage collector: what you write becomes Z80N instructions.

This manual describes the language and its libraries as implemented in
version 1.4.0 (see [`CHANGELOG.md`](../CHANGELOG.md)). A Hungarian translation is in [`manual-hu.md`](manual-hu.md).
The compact technical specification is [`spec.md`](spec.md).

---

## Contents

1. [Getting started](#1-getting-started)
2. [A first program](#2-a-first-program)
3. [Source code basics](#3-source-code-basics)
4. [Types, variables and constants](#4-types-variables-and-constants)
5. [Expressions and operators](#5-expressions-and-operators)
6. [Control flow](#6-control-flow)
7. [Functions](#7-functions)
8. [Structs and classes](#8-structs-and-classes)
9. [Strings](#9-strings)
10. [Fixed point numbers and math](#10-fixed-point-numbers-and-math)
11. [Text output](#11-text-output)
12. [Input: keyboard and joystick](#12-input-keyboard-and-joystick)
13. [Time, random numbers, CPU speed](#13-time-random-numbers-cpu-speed)
14. [Layer 2 graphics](#14-layer-2-graphics)
15. [Palettes](#15-palettes)
16. [Hardware sprites and assets](#16-hardware-sprites-and-assets)
17. [Sound and music](#17-sound-and-music)
18. [Interrupts and the copper](#18-interrupts-and-the-copper)
19. [Memory, ports, Next registers and DMA](#19-memory-ports-next-registers-and-dma)
20. [Inline assembly](#20-inline-assembly)
21. [Writing fast code](#21-writing-fast-code)
22. [How a program runs](#22-how-a-program-runs)
23. [Limitations and roadmap](#23-limitations-and-roadmap)
24. [Reference tables](#24-reference-tables)
25. [The compiler and the IDE on the Next](#25-the-compiler-and-the-ide-on-the-next)

---

## 1. Getting started

### 1.1 What you need

* **lz80nc**, the compiler. It assembles the generated code itself and
  writes the `.nex` file. The release packages for Windows, macOS and Linux
  also contain **SjASMPlus 1.24.0**, which `--sjasmplus` uses instead of
  the built-in assembler. (Building from source: see `README.md`.)
* Or the compiler and the IDE on the Next itself (chapter 25).
* A **ZX Spectrum Next** (core 3.0 or later) or the **CSpect** emulator.

LZ80N targets the Next only. There is no 48K or 128K compatibility mode. The
program switches the CPU to 28 MHz and uses Next hardware (tilemap, Layer 2,
sprites, zxnDMA, copper) directly.

### 1.2 Compiling a program

```powershell
lz80nc examples/hello.lz80n            # writes hello.asm and hello.nex
lz80nc game.lz80n -o build/game.nex    # choose the output file
lz80nc game.lz80n -S                   # stop after the assembly (.asm)
lz80nc game.lz80n -O0                  # no peephole optimisation
lz80nc game.lz80n --bank-all           # every function in its own code page (22.5)
lz80nc game.lz80n --sjasmplus C:\tools\sjasmplus.exe
lz80nc --export-selfhost D:\lz80n      # the compiler and the IDE for the Next (25)
```

Errors are reported as `file:line:column: error: message`. The compiler
reports as many errors as it can find in one run.

### 1.3 Running

* **CSpect**: `CSpect.exe -zxnext -w3 hello.nex`
* **Next**: copy the `.nex` file to the SD card and open it in the Browser.

### 1.4 Project layout

```
compiler/      the Rust compiler (lexer, parser, code generator, runtime, tests)
docs/          this manual, the specification, the concept document
examples/      runnable example programs (.lz80n) and their assets
benchmarks/    the same tasks in LZ80N and NextBASIC
tools/         SjASMPlus
```

---

## 2. A first program

```lua
-- hello.lz80n
function square(byte x) -> word
    return x * x
end

ink(BRIGHT_YELLOW)
print("Hello, Next!")
ink(WHITE)
for i = 1, 10 do
    print(i, square(i))
end
```

Some things to notice:

* There is no `main` function. The statements at the top level, in source
  order, form the main program. Functions can be declared anywhere in the
  file, before or after their use.
* Every variable has a type (`byte`, `word` …). The loop variable `i` gets
  its type from the bounds.
* `x * x` is calculated as a `word` because the function returns a `word`.
  The result is not cut to 8 bits.
* `print` writes to the Next's text layer (40 × 32 characters).

When the main program ends, the computer idles and the screen stays as it is.

---

## 3. Source code basics

### 3.1 Comments

```lua
-- a line comment
--[[ a block comment,
     several lines long ]]
```

### 3.2 Layout

Newlines and indentation do not matter. Blocks end with `end`. Statements
need no separators:

```lua
if x > 10 then x = 0 end
```

### 3.3 Literals

| Literal | Examples | Type |
|---------|----------|------|
| decimal integer | `42`, `65_535` | integer constant |
| hexadecimal | `0x7F`, `$7F` | integer constant |
| binary | `0b1010_0001` | integer constant |
| decimal fraction | `1.5`, `0.25`, `-0.1` | `fixed` |
| character | `'A'`, `'\n'`, `'\''` | integer constant (ASCII code) |
| string | `"Hello\n"` | string |
| boolean | `true`, `false` | `bool` |

The `_` character can separate digits. String escapes are `\n \t \\ \" \0`.
Character literals support `\n \t \\ \' \0`. A string literal can be up to 255
characters long.

### 3.4 Names

Names consist of letters, digits and `_`, and do not start with a digit. Type
names (structs and classes) start with an upper-case letter. Upper and lower
case are different: `score` and `Score` are two names.

### 3.5 Reserved words

The following names cannot be used for variables, constants, functions,
parameters, loop variables, types or assets. The compiler reports
`` `name` is a reserved name ``.

**Keywords**

`and` `or` `not` `if` `then` `elseif` `else` `end` `while` `do` `for`
`function` `return` `break` `const` `var` `true` `false` `asm` `struct`
`class` `asset` `interrupt` `far` `include`

**Type names and `self`**

`bool` `byte` `sbyte` `word` `sword` `fixed` `long` `fixed16` `string`, and `self` (the object
inside a method).

**Built-in functions** (implemented by the compiler)

`print` `write` `at` `ink` `cls` `border` `vsync` `pause` `frames` `key` `kempston` `any_key` `wait_key` `cpu_speed` `random` `seed` `memory` `nextreg` `port` `addr` `dma_copy` `dma_fill` `plot` `point` `hline` `layer2_clear` `layer2_flip` `sprite_load` `ay_write` `note_period` `abs` `min` `max` `clamp` `sqrt` `sin` `cos` `atan2` `round` `len` `sub` `find` `val` `asc` `chr` `str` `upper` `lower` `draw_image` `image_width` `image_height` `layer2_mode` `layer2_width` `layer2_height` `sample_play` `sample_loop` `sample_stop` `sample_playing` `music_play` `music_once` `music_stop` `music_playing`

**Standard library names** (functions, types and constants written in LZ80N).
They are reserved in **every** program, even one that does not use the
module, so a name never changes meaning when a program starts to use it:

| Module | Names |
|--------|-------|
| sprite | `Sprite` `sprite` `sprites_visible` `sprite_collision` |
| Layer 2 | `layer2_visible` `rgb` `vline` `line` `rect` `fill_rect` `circle` `fill_circle` `ellipse` `fill_ellipse` `triangle` `fill_triangle` `TRANSPARENT` `L2_256` `L2_320` `L2_640` |
| palette | `palette` `palette9` `palette_select` `rgb9` `PAL_ULA` `PAL_LAYER2` `PAL_SPRITES` `PAL_TEXT` `PAL_ULA_2` `PAL_LAYER2_2` `PAL_SPRITES_2` `PAL_TEXT_2` |
| copper | `copper_begin` `copper_wait` `copper_move` `copper_start` `copper_stop` |
| sound | `sound_reset` `sound_mix` `tone` `noise` `volume` `sound_off` `beep` `play` `play_loop` `playing` `sound_tick` |

Names starting with **`snd_`** are reserved for the sound module's internal
variables and functions, names starting with **`l2_`** for the Layer 2
module's helpers.

**Built-in constants** (they already exist, so declaring them again is an
error): `BLACK` … `WHITE`, `BRIGHT_BLACK` … `BRIGHT_WHITE`, `KEY_…`, `JOY_…`,
`MHZ_3` `MHZ_7` `MHZ_14` `MHZ_28`, `NOT_FOUND`.

**One namespace.** Functions, types and global variables share one set of
names: a variable cannot have the name of a function or a type.

**Free inside a struct or class.** Field and method names belong to their
type, so they may be any name except keywords: a class can have a field
`line` or a method `update()`.

### 3.6 Several source files: `include`

A program can be split into several files. `include "file"` pastes another
source file in, at the top level of the including file:

```lua
-- game.lz80n
include "lib/screen.lz80n"     -- title(), status()
include "lib/enemies.lz80n"    -- the Enemy class

title("My game")
```

* The path is relative to the file that contains the `include` (so a file
  in `lib/` includes its neighbours as `include "other.lz80n"`).
* Every file is included **once**: a second `include` of the same file is
  skipped. Files can therefore include what they need themselves, and two
  files may even include each other.
* `include` is allowed only at the top level of a file, not inside a
  function, class or block. The included file may contain anything a
  program can: functions, classes, globals, constants, assets and top-level
  statements (these run where the `include` stands).
* Asset paths (`asset x = "..."`) are relative to the file that declares
  the asset.
* Everything ends up in one program with one namespace: a name declared in
  one file is visible in all files after it, and the same name declared in
  two files is an error. Errors name the file they are in:
  `lib/enemies.lz80n:12:5: error: ...`
* The example `modules.lz80n` uses `lib/screen.lz80n` and `lib/ball.lz80n`.

---

## 4. Types, variables and constants

### 4.1 Basic types

| Type | Size | Range | Notes |
|------|------|-------|-------|
| `bool` | 1 byte | `false` / `true` | not interchangeable with numbers |
| `byte` | 1 | 0 … 255 | unsigned |
| `sbyte` | 1 | -128 … 127 | signed |
| `word` | 2 | 0 … 65535 | unsigned |
| `sword` | 2 | -32768 … 32767 | signed |
| `fixed` | 2 | -128 … 127.996 | 8.8 fixed point, step 1/256 |
| `long` | 4 | -2 147 483 648 … 2 147 483 647 | signed 32-bit (10.4) |
| `fixed16` | 4 | -32768 … 32767.99998 | 16.16 fixed point, step 1/65536 (10.4) |
| `string` | 65 | 0 … 64 characters | text value |

Choose the smallest type that is big enough. On the Z80, 8-bit operations
are the fastest.

### 4.2 Variables

```lua
byte lives = 3
word score            -- no initial value: 0
sword dx = -2
bool alive = true
var x = 1000          -- type inferred from the value: word
```

* Variables declared outside functions are **global** and visible
  everywhere.
* Variables declared inside a block (function, `if`, loop …) are **local**
  to that block.
* Scalar variables without an initial value start at 0.

### 4.3 Constants

```lua
const SPEED = 2
const GRAVITY = 0.25        -- fixed constant
const TITLE = "SPACE RACE"  -- string constant
```

Constants are compile-time values. They take no memory, and expressions made
of constants are calculated by the compiler. An integer constant takes the
type of the place where it is used, and must fit into it.

### 4.4 Arrays

```lua
byte map[256]
word scores[10]
string names[4]
Enemy enemies[8]      -- array of structs (see chapter 8)

map[5] = 1
scores[i] += 10
```

* The size is a constant expression. Indexes start at 0.
* Arrays without an initialiser start filled with zeros (strings empty).
* Indexes are **not checked at run time**, which keeps the code fast.
  Constant indexes are checked at compile time.
* A whole array cannot be assigned at once. Use a loop or `dma_copy`.

**Multi-dimensional arrays** have one size per dimension (at most 4):

```lua
byte grid[24][32]                 -- 24 rows of 32
grid[y][x] = 1
grid[y][x + 1] += grid[y][x]
word table[4][3] = {{1, 2, 3}, {4, 5, 6}}   -- rows as nested lists
byte flat[2][3] = {1, 2, 3, 4, 5, 6}        -- or one flat list, row by row
string names[][2] = {{"Ann", "Bob"}, {"Cid"}}   -- [] : number of rows from the list
Enemy wave[3][8]                  -- structs too
struct Map
    byte cells[16][16]            -- and in struct fields
end
```

* The elements are stored row by row (the last index changes fastest), so
  `grid[y][x]` is at `y * 32 + x`.
* `grid[y]` alone is a row, which can only be indexed further.
* Only the first size may be `[]`. Short rows in a nested list are padded
  with zeros.

### 4.5 Array initialisers

```lua
byte heights[] = {20, 45, 80, 120}        -- [] : the size comes from the list
word scores[10] = {500, 400, 300}         -- the remaining elements are 0
sbyte dx[] = {-1, 0, 1}
fixed speed[] = {0.5, 1, -1.25}
bool solid[] = {true, false, true}
byte keys[] = {'Q', 'A', 'O', 'P'}
string menu[] = {"Start", "Options", "Quit"}
const N = 3
word table[] = {N * 100, N * 200}         -- constant expressions are fine
```

* The values must be **compile-time constants** of the element type
  (numbers, characters, named constants, constant expressions; string
  literals and string constants for `string` arrays).
* Fewer values than elements: the rest is 0 (or `""`). More values than
  elements is an error.
* **Global** arrays: the values are part of the program file. Initialising
  costs no code and no time.
* **Local** arrays (inside a function): the values are copied in (one
  `LDIR`) every time the declaration runs, so each call starts with the
  original table.
* Arrays of structs cannot have an initialiser yet.

### 4.6 Conversions (casts)

A value can always be **widened** implicitly: `byte` → `word`, `sbyte` →
`sword`, any integer → `long`, `fixed` → `fixed16`. **Narrowing** must be written explicitly:

```lua
word w = 1000
byte b = byte(w)        -- 232: keeps the low 8 bits
sword s = sword(w)
fixed f = fixed(b)      -- integer -> fixed
sword n = sword(f)      -- fixed -> integer (rounds down)
```

Signedness changes of the same width (`word` ↔ `sword`, `byte` ↔ `sbyte`)
are implicit.

---

## 5. Expressions and operators

### 5.1 Operators

| Precedence (low → high) | Operators |
|---|---|
| 1 | `or` |
| 2 | `and` |
| 3 | `==  ~=  !=  <  <=  >  >=` |
| 4 | `\|` (bitwise or) |
| 5 | `~` (bitwise xor) |
| 6 | `&` (bitwise and) |
| 7 | `<<  >>` |
| 8 | `..` (string concatenation) |
| 9 | `+  -` |
| 10 | `*  /  %` |
| 11 | unary `-  not  ~` |

`~=` and `!=` both mean "not equal". Unary `~` is bitwise NOT, binary `~`
is XOR (as in Lua 5.3).

Compound assignments: `+=  -=  *=  /=  %=  &=  |=  <<=  >>=`.
`a[i] /= 2` means `a[i] = a[i] / 2`.

### 5.2 How arithmetic works

* **Integer arithmetic wraps around.** `byte b = 250  b += 10` gives 4.
  There are no overflow errors.
* **The destination decides the width.** An expression is calculated at the
  width of the variable, parameter or return value that receives it:

  ```lua
  byte a = 200
  byte b = 100
  word sum = a + b        -- 300: calculated in 16 bits
  byte low = a + b        -- 44: calculated in 8 bits
  ```

* **Signedness**: an operation is signed if any operand or the destination
  is signed.
* **Division** truncates toward zero; `%` has the sign of the dividend.
  Division by a constant 0 is a compile error. At run time, `x / 0` gives
  65535.
* Shifts: `>>` is arithmetic (keeps the sign) for signed types and logical
  for unsigned types.

### 5.3 Conditions are `bool`

Conditions must be `bool` values. Lua's and C's "0 is false" rule is
deliberately not used:

```lua
if count ~= 0 then ... end      -- correct
if count then ... end           -- error: condition must be bool
```

`and` and `or` short-circuit: the right side is only evaluated when needed.

---

## 6. Control flow

```lua
if hp == 0 then
    game_over()
elseif hp < 10 then
    ink(RED)
else
    ink(GREEN)
end

while not key(KEY_SPACE) do
    vsync()
end

for i = 0, 9 do ... end          -- 0, 1, ..., 9 (both ends included)
for i = 10, 0, -2 do ... end     -- 10, 8, ..., 0
for x = left, right do ... end   -- variable bounds

while true do
    if done then break end
end
```

`for` loops:

* The loop variable is a new local variable. Its type comes from the bounds.
* The end value is evaluated **once**, before the loop starts.
* The step must be a non-zero constant.
* A loop never "runs over": `for i = 0, 255` on a `byte` runs exactly 256
  times, and `for i = 3, 2` does not run at all.
* `break` leaves the innermost loop.

---

## 7. Functions

```lua
function add(byte a, byte b) -> word
    return a + b
end

function clear_line(byte row)
    at(row, 0)
    write("                                        ")
end

word total = add(200, 100)       -- 300
clear_line(5)
```

* Parameters are written with their types. The return type follows `->`.
  A function without `->` returns nothing.
* Numbers and `bool` values are passed **by value**.
* **Structs, classes and strings are passed by reference**: the function
  works on the caller's variable and can change it (like tables in Lua).
* A function can return a `string` (`-> string`) or a struct (`-> Vec`, 8.3).

### 7.1 Recursion

A function may call itself, directly or through other functions:

```lua
function factorial(word n) -> word
    if n <= 1 then return 1 end
    return n * factorial(n - 1)
end

function hanoi(byte n, byte from, byte to, byte via)
    if n == 0 then return end
    hanoi(n - 1, from, via, to)
    hanoi(n - 1, via, to, from)
end
```

How it works, and what it costs:

* To make calls fast, every function's parameters and local variables live
  at **fixed memory addresses** (static frames). Before a call that can come
  back to the caller (the callee is the caller itself, or can call it
  again), the caller copies its frame onto the stack, and restores it after
  the call. Ordinary calls do not pay anything for this.
* The cost of a recursive call grows with the caller's frame: about
  10 T-states per byte of parameters and locals. Keep big local arrays out
  of recursive functions (make them global).
* The stack has room for about 1–2 KB below the program, depending on the
  program's size. Recursion that goes too deep stops the program with
  "stack overflow (recursion too deep)" instead of corrupting memory.
* A function cannot be used both by the main program and by an interrupt
  handler (chapter 18): an interrupt could arrive while the main program is
  inside it. The compiler reports this as an error.

The example `recursion.lz80n` draws a fractal tree and solves the Towers of
Hanoi.

---

## 8. Structs and classes

### 8.1 Structs

```lua
struct Vec2
    sbyte x
    sbyte y
end

struct Player
    word x
    word y
    Vec2 speed           -- nested struct
    byte health = 100    -- default value
    byte items[8]        -- array field
end

Player p
p.x = 120
p.speed.x = -1
```

* Fields are stored **packed**, in declaration order, without padding.
* A struct variable owns its memory. `a = b` copies the whole struct.
* Default values (`= 100`) must be constants. They are stored each time the
  declaration runs: globals at that point in the main program, locals every
  time the block is entered.

### 8.2 Classes

A class is a struct with functions (methods):

```lua
class Ball
    byte x = 16
    byte y = 12
    Vec2 vel

    function move() -> bool
        self.x += self.vel.x
        self.y += self.vel.y
        return self.x > 38
    end

    function draw()
        at(self.y, self.x)
        write("o")
    end
end

Ball balls[3]
balls[0].vel.x = 1
if balls[0].move() then balls[0].vel.x = -1 end
balls[0].draw()
```

* Inside a method, `self` is the object. Fields are always accessed as
  `self.field`.
* A method call `obj.method(...)` is a direct `CALL`: there are no virtual
  functions and no extra cost.

### 8.3 Struct values: lists, constructors, results

A struct value can be written as a `{...}` list of its fields, in
declaration order, or made by a **constructor**, the type name called like
a function:

```lua
struct Vec
    sword x
    sword y
end

Vec a = {3, 4}                  -- fields in order; missing ones: default / 0
Vec b = Vec(10, 20)             -- constructor
a = {a.y, a.x}                  -- values may be any expressions
Vec pts[] = {{1, 2}, {3, 4}, {5}}   -- arrays of structs: one list per element

function add(Vec p, Vec q) -> Vec    -- functions return struct values
    return Vec(p.x + q.x, p.y + q.y)
end

Vec c = add(a, {1, 1})          -- lists and constructors work as arguments
var d = add(c, c)               -- `var` takes the type from the value
```

* A class with an `init` method is built by it: `Ship(10, 20)` first stores
  the field defaults, then calls `init(10, 20)` on the new object. Without
  `init`, the arguments go to the fields in order.
* Nested structs and array fields take nested lists:
  `Pix p = {{1, 2}, rgb(7, 0, 0)}`.
* A list made of constants costs nothing at load time (globals) or one
  `LDIR` (locals); other lists and constructors are built field by field.
* A function returning a struct fills a static buffer and the caller copies
  it at once, so the result is a value of its own.
* A struct value can be stored, returned and passed; using one directly
  in an expression (`add(a, b).x`) needs a variable first.

### 8.4 Inheritance

```lua
class Shape
    Vec pos
    byte colour = 255
    function init(sword x, sword y)
        self.pos = Vec(x, y)
    end
    function move() ... end
    function draw() ... end
end

class Ball extends Shape
    byte radius = 6                 -- new fields come after the parent's
    function init(sword x, sword y, byte r)
        super.init(x, y)            -- the parent's version
        self.radius = r
    end
    function draw()                 -- overrides Shape.draw
        fill_circle(self.pos.x, self.pos.y, self.radius, self.colour)
    end
end

Ball b = Ball(100, 50, 8)
b.move()                            -- inherited
b.draw()                            -- Ball's own
```

* A derived class has all the fields of its parent first, at the same
  offsets, then its own; methods are inherited unless redefined.
* `super.m(...)` calls the parent's method on `self`; `Shape.m(obj, ...)`
  calls a given class's method explicitly.
* A `Ball` can be passed where a `Shape` is expected and assigned to a
  `Shape` variable (only the `Shape` part is copied).
* Methods are bound when compiling (**no virtual dispatch**): a function
  taking a `Shape` calls `Shape`'s methods even when it is given a `Ball`.
  Inside `Ball`'s own methods, `self.draw()` is `Ball.draw`. This keeps
  every call a direct `CALL`.
* The parent must be declared before the derived class.

The example `inherit.lz80n` shows all of these.

---

## 9. Strings

```lua
string name = "Next"
string s = "Hello, " .. name .. "! Score: " .. 42
print(s)                          -- Hello, Next! Score: 42
if find(s, "Next") ~= NOT_FOUND then print(upper(name)) end
```

### 9.1 How strings are stored

* A `string` variable holds **up to 64 characters**: a length byte followed
  by the text (65 bytes in total).
* There is no heap and no garbage collector. Every string variable has its
  own fixed storage.
* A longer result is **cut at 64 characters**; this is not an error.
* Assignment copies the text. `s = "x" .. s` is safe.
* String **literals and constants** can be up to 255 characters, so long
  texts and tunes can be printed or played directly. When one is stored in
  a variable, its first 64 characters are kept.

### 9.2 Operations

* `..` joins strings. Numbers, `fixed` values and `bool` values are
  converted to text automatically: `"x=" .. x`.
* `==  ~=  <  <=  >  >=` compare strings in dictionary (byte) order.
* Positions start at **0**.

| Function | Result | Description |
|----------|--------|-------------|
| `len(s)` | byte | number of characters |
| `sub(s, from, count)` | string | part of `s`; cut at the end of `s` |
| `find(s, t)` | byte | first position of `t` in `s`, or `NOT_FOUND` (255) |
| `val(s)` | sword | the number at the start of `s`: `"-45"` → -45, `"7up"` → 7, `""` → 0 |
| `str(x)` | string | a number, `fixed` or `bool` value as text |
| `asc(s, i)` | byte | character code at position `i` (0 outside the string) |
| `chr(c)` | string | a one-character string |
| `upper(s)`, `lower(s)` | string | upper / lower case (ASCII letters) |

### 9.3 String functions

```lua
function greet(string who) -> string
    return "Hi " .. who .. "!"
end

function shout(string s)       -- by reference: changes the caller's string
    s = upper(s)
end

string n = "bob"
shout(n)
print(greet(n))                -- Hi BOB!
```

---

## 10. Fixed point numbers and math

### 10.1 Why `fixed` and not floating point

The Z80N has no floating point unit. A software float operation costs
hundreds or thousands of clock cycles. LZ80N therefore has a 16-bit
**8.8 fixed point** type: 8 bits of whole number and 8 bits of fraction. This
is what Next games use for smooth movement, speeds and gravity.

```lua
fixed y = 10
fixed vy = 0
const G = 0.2

vy = vy + G        -- falling speed grows
y = y + vy
plot(100, byte(y), 255)
```

### 10.2 Rules

* A number with a decimal point is `fixed`: `1.5`, `-0.25`. Values are
  rounded to 1/256: `0.1` is stored as 26/256 = 0.1016.
* Integer **constants** fit in automatically: `f + 1`, `f > 2`,
  `fixed v = 0`.
* Integer **variables** must be converted: `f + fixed(n)`. Exceptions, because
  they are exact and cheap: `fixed * integer`, `integer * fixed` and
  `fixed / integer`.
* `fixed * fixed` and `fixed / fixed` call fast runtime routines. Results are
  truncated toward zero.
* Conversions: `fixed(n)`, `sword(f)` / `byte(f)` (round down),
  `round(f)` → `sword` (halves round up).
* `print` shows up to two decimals: `1.5`, `-0.38`, `3`.

### 10.3 Math functions

| Function | Types | Description |
|----------|-------|-------------|
| `abs(x)` | any number | absolute value |
| `min(a, b)`, `max(a, b)` | any numbers | smaller / larger |
| `clamp(x, lo, hi)` | any numbers | limit `x` to `lo … hi` |
| `sqrt(x)` | integer → `byte`, `fixed` → `fixed` | square root (rounded down) |
| `sin(a)`, `cos(a)` | `byte` angle → `fixed` | sine / cosine from a table |
| `atan2(y, x)` | `sword` or `fixed` → `byte` angle | direction of the vector (x, y) |
| `round(f)` | `fixed` → `sword` | rounding |

**Angles are bytes**: a full turn is 256 units (64 = 90°, 128 = 180°). An
angle therefore wraps around for free: `angle += 3`. `atan2` returns 0 for the
+x direction and 64 for +y.

```lua
byte a = 0
while true do
    vsync()
    byte x = 128 + byte(sword(cos(a) * 60))
    byte y = 96 + byte(sword(sin(a) * 60))
    plot(x, y, rgb(7, 7, 0))
    a += 1
end
```

The sine and arctangent tables (about 0.6 KB) are only included in the
program when `sin`, `cos` or `atan2` is used.

### 10.4 32-bit numbers: `long` and `fixed16`

When 16 bits are not enough:

| Type | Size | Range | Use |
|------|------|-------|-----|
| `long` | 4 bytes | -2 147 483 648 … 2 147 483 647 | big scores, counters, products |
| `fixed16` | 4 bytes | -32768 … 32767.99998, step 1/65536 | precise positions and physics |

```lua
long score = 0
score += long(level) * 25000         -- widen before multiplying
long f = 1
for i = 2, 12 do
    f *= i                           -- 479001600
end

fixed16 x = 4
fixed16 vy = -3.5
vy += 0.05                           -- small steps add up precisely
plot(byte(x), byte(y), c)
```

* Integers (`byte` … `sword`) widen to `long` automatically; `fixed`
  widens to `fixed16`. The other directions need a conversion: `long(f)`,
  `fixed16(n)`, `word(l)`, `byte(l)`, `fixed(f16)`. Fixed point to integer
  rounds down.
* All operators work on `long` (`+ - * / % & | ~ << >>`, comparisons;
  `>>` keeps the sign). `fixed16` has `+ - * / %` and comparisons;
  `fixed16 * integer` and `fixed16 / integer` are exact.
* `print`, `write`, `str()` and `..` show `long` in full and `fixed16` with
  up to four decimals. `abs`, `min`, `max` and `clamp` work too.
* `long` and `fixed16` can be variables, array elements (with
  initialisers), struct fields, parameters and return values. Not yet:
  `far` arrays of them, and `for` loops counting in `long` (use `while`).
* Speed: addition and comparison take a few dozen T-states, a
  multiplication about 700 and a division about 3000 (runtime routines, the
  multiplication uses the Z80N `MUL`). Prefer 16-bit types where they fit.
* A number with a decimal point is `fixed` when it fits in 8.8, otherwise
  `fixed16`. Assigned to a `fixed16`, it keeps all its precision.

The example `bignum.lz80n` uses both types.

---

## 11. Text output

The text layer uses the Next's **tilemap**: 40 columns × 32 rows of 8 × 8
pixel characters cover the whole 320 × 256 display, border included. The
256 × 192 "paper" area is columns 4–35 and rows 4–27. The font is built into
the compiler (characters 32–127).

| Call | Description |
|------|-------------|
| `print(a, b, …)` | print values separated by spaces, then a new line |
| `write(a, b, …)` | print values without separators and without a new line |
| `at(row, col)` | set the print position (row 0–31, column 0–39) |
| `ink(c)` | text colour 0–15 |
| `cls()` | clear the text layer, cursor to 0,0 |

```lua
cls()
at(2, 4)
ink(BRIGHT_CYAN)
write("SCORE ")
ink(BRIGHT_WHITE)
write(score)
print("")                 -- new line
print("x =", x, "fixed:", 1.5, "ok:", true)
```

* `print` and `write` accept numbers, `fixed` values, `bool` values and
  strings.
* A line longer than 40 characters continues on the next line. At the
  bottom of the screen the text scrolls up.
* Colours: `BLACK BLUE RED MAGENTA GREEN CYAN YELLOW WHITE` (0–7) and
  `BRIGHT_BLACK` … `BRIGHT_WHITE` (8–15).
* The text background is transparent: Layer 2, sprites and the background
  colour show through it.
* `border(c)` sets the background colour behind every layer. This is the
  border, and also the paper wherever nothing else is drawn.
* `print` evaluates each argument at its own type: with `byte m = 20`,
  `print(m * 13)` prints 4 (wrap-around in 8 bits). Use `word(m) * 13` for
  260.

---

## 12. Input: keyboard and joystick

```lua
if key(KEY_O) then x -= 1 end
if key(KEY_P) then x += 1 end
if key(KEY_SPACE) then fire() end

byte j = kempston()
if j & JOY_FIRE ~= 0 then fire() end
if j & JOY_LEFT ~= 0 then x -= 1 end
```

| Call | Description |
|------|-------------|
| `key(KEY_x)` → bool | is the key held down |
| `kempston()` → byte | Kempston joystick bits (`JOY_RIGHT`, `JOY_LEFT`, `JOY_DOWN`, `JOY_UP`, `JOY_FIRE`) |
| `any_key()` → bool | is any key held down |
| `wait_key()` | wait until all keys are released, then wait for a key press |

Key constants: `KEY_A` … `KEY_Z`, `KEY_0` … `KEY_9`, `KEY_SPACE`,
`KEY_ENTER`, `KEY_CAPS` (Caps Shift), `KEY_SYM` (Symbol Shift).

---

## 13. Time, random numbers, CPU speed

| Call | Description |
|------|-------------|
| `vsync()` | wait for the next frame (50 per second) |
| `pause(n)` | wait `n` frames |
| `frames()` → word | frame counter since the program started |
| `random(n)` → word | random number 0 … n-1; `random(0)` gives a full 16-bit value |
| `seed(w)` | start the random sequence from `w` (same seed → same sequence) |
| `cpu_speed(s)` | `MHZ_3`, `MHZ_7`, `MHZ_14`, `MHZ_28` (the program starts at 28 MHz) |

A typical game loop:

```lua
while true do
    vsync()          -- once per frame, at the bottom of the display
    read_input()
    move_things()
    draw()
end
```

Measuring time:

```lua
word t = frames()
do_work()
print("took", (frames() - t) * 20, "ms")
```

---

## 14. Layer 2 graphics

Layer 2 is the Next's bitmap layer: 256 × 192 with one byte per pixel by
default, 320 × 256 or 640 × 256 on request (14.1). A colour
is an 8-bit `RRRGGGBB` value. `TRANSPARENT` (`$E3`) lets lower layers show
through.

| Call | Description |
|------|-------------|
| `layer2_visible(on)` | show / hide Layer 2 |
| `layer2_clear(c)` | fill the screen with colour `c` (zxnDMA, very fast) |
| `plot(x, y, c)` | draw a pixel |
| `point(x, y)` → byte | read a pixel |
| `hline(x, y, len, c)` | horizontal line |
| `vline(x, y, len, c)` | vertical line |
| `line(x0, y0, x1, y1, c)` | line between two points |
| `rect(x, y, w, h, c)` | rectangle outline |
| `fill_rect(x, y, w, h, c)` | filled rectangle |
| `rgb(r, g, b)` → byte | build a colour: r, g 0–7, b 0–3 |
| `circle(x, y, r, c)` | circle outline |
| `fill_circle(x, y, r, c)` | filled circle |
| `ellipse(x, y, rx, ry, c)` | ellipse outline, radii `rx` and `ry` |
| `fill_ellipse(x, y, rx, ry, c)` | filled ellipse |
| `triangle(x0, y0, x1, y1, x2, y2, c)` | triangle outline |
| `fill_triangle(x0, y0, x1, y1, x2, y2, c)` | filled triangle (corners in any order) |
| `draw_image(img, x, y)` | draw a picture (see 16.1) |
| `layer2_flip()` | double buffering: show the image drawn so far, draw into the other buffer |
| `layer2_mode(m)` | `L2_256`, `L2_320` or `L2_640` (14.1) |
| `layer2_width()` / `layer2_height()` → word | size of the current mode |

Circles, ellipses and filled triangles take `sword` coordinates and are
**clipped**: a shape may hang off any edge of the screen. Radii are 0–255.

```lua
layer2_visible(true)
layer2_clear(rgb(0, 0, 1))
for x = 0, 255 do
    vline(byte(x), 0, 192, byte(x))     -- colour bars
end
line(0, 0, 255, 191, rgb(7, 7, 3))
fill_rect(100, 80, 56, 32, rgb(7, 0, 0))
```

**Double buffering without flicker:** when drawing a frame takes longer than
one frame, draw into the hidden buffer, then swap:

```lua
while true do
    layer2_clear(0)
    draw_scene()
    layer2_flip()       -- show it at once, continue in the other buffer
end
```

Layer order is **S-U-L**: sprites on top, then text, then Layer 2.

Pixels outside the screen (in 256 × 192: rows 192 and below) are ignored by `plot`. Pixels are accessed through MMU
slot 0 (`$0000`–`$1FFF`); `draw_image` also uses slot 1 (`$2000`–`$3FFF`)
while it copies and puts the ROM back there afterwards. Once Layer 2 drawing has been used, `memory[0]` …
`memory[$1FFF]` show the last Layer 2 page used, not the ROM.

### 14.1 320 × 256 and 640 × 256

```lua
layer2_mode(L2_320)        -- 320 x 256, 256 colours: the whole display, border included
layer2_mode(L2_640)        -- 640 x 256, 16 colours (palette indices 0..15)
layer2_mode(L2_256)        -- back to 256 x 192 (the default)
word w = layer2_width()    -- 256, 320 or 640
word h = layer2_height()   -- 192 or 256
```

* Every drawing call works in every mode: x goes up to 639, y up to 255.
  Shapes are clipped to the current size; `layer2_clear`, `layer2_flip`
  and `draw_image` follow the mode too.
* In 640 × 256 a pixel is a palette index 0–15 and is half as wide as it is
  tall: a round circle needs twice the radius across
  (`fill_ellipse(x, y, 2 * r, r, c)`). Load the 16 colours with
  `palette(PAL_LAYER2, i, colour)`.
* The wide modes store pixels by columns: vertical lines and
  `hline`/`fill_rect` in 320 × 256 are fast, 640 × 256 draws pixel by pixel.
* The two buffers then take 16K banks 8–17 (instead of 8–13), and pictures
  and `far` arrays start above them. The compiler arranges this when the
  program calls `layer2_mode`.

The example `hires.lz80n` switches between the two modes.

---

## 15. Palettes

Every layer has its own palette, so colours can be changed without
redrawing anything. This is useful for colour cycling, fades and flashes.

| Call | Description |
|------|-------------|
| `palette(layer, index, colour)` | 8-bit `RRRGGGBB` colour |
| `palette9(layer, index, colour)` | 9-bit `RRRGGGBBB` colour |
| `rgb9(r, g, b)` → word | 9-bit colour: r, g, b 0–7 |
| `palette_select(layer, second)` | show the first (`false`) or second (`true`) palette |

`layer`: `PAL_ULA`, `PAL_LAYER2`, `PAL_SPRITES`, `PAL_TEXT`.
Text colour `k` is entry `2k+1` of `PAL_TEXT`.

Every layer has a **second palette** too: `PAL_ULA_2`, `PAL_LAYER2_2`,
`PAL_SPRITES_2`, `PAL_TEXT_2`. `palette_select(layer, second)` chooses which
of the two a layer shows, so a whole prepared set of colours (a night
version, a flash, a fade step) appears at once:

```lua
for i = 0, 255 do
    palette(PAL_LAYER2_2, byte(i), dim(byte(i)))   -- prepare the second palette
end
palette_select(PAL_LAYER2, true)                   -- show it
palette_select(PAL_LAYER2, false)                  -- and back
```

```lua
-- colour cycling: rewrite 16 Layer 2 palette entries every frame
byte phase = 0
while true do
    vsync()
    for i = 0, 15 do
        palette(PAL_LAYER2, 16 + i, rgb((i + phase) & 7, 0, 3))
    end
    phase += 1
end
```

See `examples/palette.lz80n`.

---

## 16. Hardware sprites and assets

### 16.1 Assets

```lua
asset sheet = "assets/sprites.png"
```

An `asset` is converted at compile time and built into the program. The path
is relative to the source file.

* `.png`: the image (width and height multiples of 16) is cut into 16 × 16
  sprite patterns, left to right and top to bottom, at most 64 patterns.
  Colours are converted to the Next's `RRRGGGBB` format. Transparent pixels
  (alpha < 128) become the sprite transparency colour `$E3`.
* `.spr`: raw 8-bit sprite patterns (256 bytes each).
* `.wav`: a sound sample (17.3).
* `.pt3`: music (17.4).

**Pictures for Layer 2**

```lua
asset title = image("assets/title.png")       -- up to 256 x 255 pixels
asset logo = image("assets/logo.png")

draw_image(title, 0, 0)                       -- a full-screen picture
draw_image(logo, x, y)                        -- x, y: sword, may be off screen
const LOGO_W = image_width(logo)              -- size, known when compiling
const LOGO_H = image_height(logo)
```

* The PNG is converted to `RRRGGGBB` when compiling. Its pixels are stored
  in their **own memory pages** (from 16K bank 14 up), not in the 31 KB
  program area, so a full 256 × 192 picture (48 KB) costs the program
  only 6 bytes. Pictures and `far` arrays (19.1) together can use about
  780 KB.
* `draw_image` copies the picture into Layer 2 (the buffer being drawn),
  clipped at every edge. Rows are copied with `LDIR`: a full screen takes
  about 2 frames at 28 MHz.
* Transparent PNG pixels (alpha < 128) are **skipped**, so the screen shows
  through, like with sprites. A picture without transparent pixels is
  copied with the faster plain copy.
* `image_width` and `image_height` are constants.

### 16.2 Sprites

The Next has 128 hardware sprites: `sprite[0]` … `sprite[127]`.

```lua
sprite_load(0, sheet)              -- upload the patterns from slot 0
sprites_visible(true, false)       -- sprites on, not over the border

sprite[0].position(152, 200)
sprite[0].show(0)                  -- pattern 0, visible

sprite[1].x = 60
sprite[1].y = 80
sprite[1].pattern = 2
sprite[1].mirror_x = true
sprite[1].scale = 1                -- 0..3: x1, x2, x4, x8
sprite[1].visible = true
sprite[1].update()                 -- send the changed fields to the hardware
```

| Field | Description |
|-------|-------------|
| `x`, `y` | position (word). The 256 × 192 area starts at 32, 32 |
| `pattern` | pattern number |
| `palette` | palette offset 0–15 |
| `visible`, `mirror_x`, `mirror_y`, `rotate` | bool |
| `scale` | 0–3 |
| `id` | the sprite's number (read only) |

| Method | Description |
|--------|-------------|
| `position(x, y)` | move and update |
| `show(pattern)` | set the pattern and show it |
| `hide()` | hide it |
| `update()` | send all fields to the hardware |
| `overlaps(other)` → bool | do the two sprites' squares overlap? |

### 16.3 Collision detection

There are two ways, and they work well together:

```lua
if sprite_collision() then            -- pixel-exact hardware flag
    for e = 1, 8 do
        if sprite[0].overlaps(sprite[e]) then
            -- sprite 0 hit sprite e
        end
    end
end
```

* `sprite[a].overlaps(sprite[b])` → bool: do the two sprites' squares
  (16 << `scale` pixels wide) overlap? It works from the shadow fields
  (`x`, `y`, `scale`, `visible`), so it sees values set without
  `update()`. `false` when either sprite is hidden. Transparent pixels
  count, and there is no wrapping at the screen edges. For a tighter
  hitbox, compare rectangles in your own code, as `examples/shooter.lz80n`
  does.
* `sprite_collision()` → bool: the hardware collision flag (port `$303B`,
  bit 0). True when the non-transparent pixels of two visible sprites
  overlapped on the displayed area since the last call. Reading clears the
  flag. The hardware does not say which two sprites collided: find them
  with `overlaps`. The hardware sets the flag while drawing, so after a
  move it reports at the earliest in the next frame (after `vsync()`).

See `examples/sprites.lz80n`: a class that controls sprites.

---

## 17. Sound and music

The Next has three AY-3-8912 sound chips (TurboSound), which give
**9 channels**: channel = chip × 3 + voice, so channels 0–2 are on chip 0, 3–5
on chip 1 and 6–8 on chip 2.

### 17.1 Sound effects

```lua
beep(3, 60, 12)            -- note 60 (C5) on channel 3 for 12 frames; does not wait
tone(6, 300)               -- square wave, AY period 300
volume(6, 12)              -- 0..15
noise(7, 18)               -- noise on channel 7
sound_off(6)
```

| Call | Description |
|------|-------------|
| `beep(ch, note, frames)` | play a note for a number of frames, in the background |
| `tone(ch, period)` | square wave; frequency = 109375 / period Hz (period 1–4095) |
| `noise(ch, period)` | noise, period 0–31 (one noise generator per chip) |
| `volume(ch, v)` | volume 0–15 |
| `sound_off(ch)` | silence a channel and stop its tune |
| `sound_reset()` | silence everything |
| `note_period(n)` → word | AY period of note `n` |
| `playing(ch)` → bool | is a beep or tune still playing |
| `ay_write(chip, reg, value)` | write an AY register directly |

Notes are numbered `n = octave × 12 + semitone`: C4 = 48, A4 = 57 (440 Hz),
C5 = 60.

A falling "laser" sound, updated every frame:

```lua
byte laser = 15
word period = 40
while laser > 0 do
    vsync()
    tone(6, period)
    volume(6, laser)
    period += 25
    laser -= 1
end
sound_off(6)
```

### 17.2 Music

```lua
play_loop(0, "o5 l12 e d c d e e e24 d d d24 e g g24")
play_loop(1, "o3 l24 c g c g c g < g > c")
play(2, "o4 l8 c e g >c")              -- once
```

Tunes use a notation similar to the 128K / NextBASIC `PLAY` command:

| Symbol | Meaning |
|--------|---------|
| `c d e f g a b` | note. `#` or `+` = sharp, `-` = flat. A number after it = length in frames: `c8`, `f#16` |
| `r` | rest (same length rules) |
| `o4` | octave 0–8 (default 4) |
| `>` `<` | one octave up / down |
| `l10` | default note length in frames (default 10; 50 frames = 1 second) |
| `v12` | volume 0–15 (default 12) |

Spaces and other characters are ignored. The last frame of each note is
silent, so repeated notes are heard separately.

`play` and `beep` **do not stop the program**. A sound driver written in LZ80N
runs in the frame interrupt and steps all channels. The game keeps running
while the music plays.

See `examples/sound.lz80n`.

### 17.3 Samples (digital sound)

```lua
asset boom = "assets/boom.wav"
asset engine = "assets/engine.wav"

sample_play(boom)            -- once
sample_loop(engine)          -- over and over
if sample_playing() then ... end
sample_stop()
```

* `.wav` files with 8 or 16 bit PCM, mono or stereo, are converted to 8-bit
  mono when compiling. Rates from 4 kHz to 31.25 kHz are kept; others are
  converted to 15.6 kHz. Like pictures, samples live in their own memory
  pages, not in the 31 KB program area: one second at 15.6 kHz is about
  15 KB.
* Playback runs **in the background**: the zxnDMA sends the bytes to the
  DAC (port `$DF`) at the sample's rate, through a 4 KB buffer at
  `$7000`–`$7FFF` that the frame interrupt refills. The program goes on,
  and the AY sound (17.1, 17.2) keeps working.
* One sample sounds at a time; a new `sample_play` replaces the current one.
* While a sample plays, the DMA is busy: `dma_copy`, `dma_fill` and
  `layer2_clear` then do their work with the CPU (correct, but slower:
  clearing Layer 2 takes about 2 frames).

See `examples/samples.lz80n`.

### 17.4 ProTracker 3 music

```lua
asset tune = "assets/tune.pt3"

music_play(tune)             -- loops from the module's loop position
music_once(tune)             -- once, then silence
music_play(tune, 2)          -- on AY chip 2 (0-2, default 0)
if music_playing() then ... end
music_stop()
```

* `.pt3` modules from ProTracker 3 or Vortex Tracker II (one AY; of a
  TurboSound module the first part). All five note tables, the 3.3-3.4 and
  3.5+ volume tables, samples, ornaments, envelopes and the effects
  (glissando, portamento, sample and ornament offsets, vibrato, envelope
  slide, tempo) are supported.
* The compiler plays the module through **when compiling** and stores the
  13 AY registers of every frame (only those that change, about 4-8 bytes
  per frame) in their own memory pages: a 3-minute song is about 60 KB of
  banks, a few bytes of the program area. In the game, the frame interrupt
  only copies the next frame to the chip: almost no CPU time.
* The music owns one AY chip; `beep`, `play` and `tone` work on the
  channels of the other two (17.1: channels 3-8 for chips 1 and 2).
* Samples (17.3) sound at the same time.

See `examples/music.lz80n` (its tune is made by `assets/make_music.py`).

---

## 18. Interrupts and the copper

### 18.1 Interrupt handlers

```lua
word ticks = 0

interrupt frame             -- once every frame (50 times a second)
    ticks += 1
end

interrupt raster(96)        -- when the display reaches line 96
    nextreg[0x4A] = 0x1C    -- change the background colour mid-screen
end

interrupt raster(160)
    nextreg[0x4A] = 0
end
```

* Line numbering: 0–191 is the display, 192 and above is the bottom border
  and vertical blank, and the top border is about 280–311 (50 Hz).
* Each line can have one handler. The line must be a constant 0–311.
* Handlers should be **short**. A raster line is about 1800 clock cycles at
  28 MHz.
* Use `nextreg[…] = …` with constant registers.
* Do not print text or draw on Layer 2 from a handler: the cursor and the
  memory mapping are shared with the main program.
* Functions called from a handler cannot also be called from the main
  program (static frames, see 7.1).

With raster handlers present, `vsync()` returns at the bottom of the display
(line 192), the best moment to update the picture.

### 18.2 Copper

The copper is a small co-processor that changes Next registers at chosen
raster positions, without using the CPU.

| Call | Description |
|------|-------------|
| `copper_begin()` | stop the copper and start writing a new list |
| `copper_wait(line)` | wait until the raster reaches `line` |
| `copper_move(reg, value)` | write Next register `reg` (0–127) |
| `copper_start()` | finish the list and run it every frame |
| `copper_stop()` | stop the copper |

```lua
-- rainbow background: a new colour every 8 lines
copper_begin()
for i = 0, 23 do
    copper_wait(word(i) * 8)
    copper_move(0x4A, rgb(byte(i) & 7, byte(i >> 1) & 7, 2))
end
copper_start()
```

Rebuild the list right after `vsync()` to avoid tearing. See
`examples/raster.lz80n`.

---

## 19. Memory, ports, Next registers and DMA

| Construct | Description |
|-----------|-------------|
| `memory[addr]` | read / write a byte at a 16-bit address |
| `port[n]` | read / write a 16-bit I/O port |
| `nextreg[n]` | read / write a Next register |
| `addr(x)` → word | address of a variable, array, element or field |
| `dma_copy(src, dst, len)` | copy memory with the zxnDMA |
| `dma_fill(dst, len, value)` | fill memory with the zxnDMA |

```lua
nextreg[0x07] = 3                     -- direct NEXTREG instruction
byte machine = nextreg[0x00]
memory[0x5800] = 0x47
dma_copy(addr(level), addr(backup[0]), 256)
dma_fill(addr(map[0]), 256, 0)
```

DMA transfers are much faster than copying with a loop. The CPU waits until
a transfer is finished. Addresses are CPU addresses, so the current memory
mapping applies.

---

### 19.1 `far` arrays: data in memory banks

The program area (code and ordinary variables) is about 31 KB. Large data
can live in the Next's other memory instead:

```lua
far byte level[22][720]                 -- 15840 bytes, outside the program area
far word scores[10000]
far fixed curve[] = {0.5, 1, 1.5, 2}    -- initialisers work too
far byte map[64][64] = {{1, 1, 1}, {1, 0, 1}}

level[y][x] = 3
level[y][x + 1] += level[y][x]
scores[i] = scores[i - 1] + 10
```

* Element types: `byte`, `sbyte`, `word`, `sword`, `fixed`, `bool`; one or
  more dimensions; at most 64 KB per array. Declared at the top level.
* The data is stored in 8K pages from 16K bank 14 up, shared with the
  pictures (`image(...)`): about 780 KB together. The values of an
  initialiser are part of the `.nex` file; arrays without one are cleared
  when the program starts.
* An element is read or assigned like any array element. A page is mapped
  into MMU slot 1 (`$2000`–`$3FFF`) on access, so it costs a little more
  than an ordinary array (about 60–80 T-states when the page is already
  mapped). Interrupt handlers may use `far` arrays: the mapping is saved
  and restored around them.
* A `far` element cannot be passed by reference or used with `addr()`; a
  whole row (`level[y]`) cannot be used on its own.

### 19.2 Files on the SD card

The file module reads and writes files on the SD card through the esxDOS
API of NextZXOS. It is part of a program when it uses a name starting with
`file_` or `FILE_`.

| Function | Description |
|----------|-------------|
| `file_open(name, mode)` → byte | open a file: `FILE_READ`, `FILE_WRITE` (create or empty) or `FILE_APPEND`; a handle or `FILE_ERROR` |
| `file_close(h)` | close a file (or a folder) |
| `file_read(h, address, count)` → word | read up to `count` bytes to `address` (at `$4000` or above); the number read, 0 at the end |
| `file_write(h, address, count)` → word | write `count` bytes from `address`; the number written |
| `file_getc(h)` → word | the next byte, or `FILE_EOF` at the end (buffered) |
| `file_seek(h, pos)`, `file_pos(h)` → long, `file_size(h)` → long | the position and the size |
| `file_exists(name)` → bool, `file_delete(name)` → bool | |
| `file_opendir(path)` → byte | open a folder (`"."`: the current one) for `file_readdir`; a handle or `FILE_ERROR` |
| `file_readdir(h, address)` → bool | the next entry into a 280-byte buffer: attributes (bit 4: a folder), the name ending with 0, time, date (2 + 2 bytes), size (4 bytes); false at the end |
| `file_exec(command)` → bool | run a dot command, e.g. `file_exec("nexload game.nex")` (does not come back when it starts another program) |
| `file_status` | the esxDOS error code of the last call (0: no error) |

```lua
byte h = file_open("scores.txt", FILE_WRITE)
if h ~= FILE_ERROR then
    string line = "hiscore " .. str(score) .. chr(10)
    file_write(h, addr(line) + 1, len(line))   -- a string's bytes follow its length
    file_close(h)
end

byte buf[280]
byte d = file_opendir(".")
while d ~= FILE_ERROR and file_readdir(d, addr(buf)) do
    -- buf[0]: attributes, buf[1..]: the name
end
if d ~= FILE_ERROR then file_close(d) end
```

File names may be long names and may contain folders (`levels/1.dat`).

## 20. Inline assembly

```lua
function fast_border(byte c)
    asm
        ld a,(l_fast_border_c_1)   ; parameter label: see the .asm file
        out ($FE),a
    end
end
```

* The lines between `asm` and `end` are copied into the output unchanged
  (SjASMPlus syntax).
* The compiler keeps no values in registers between statements, so an
  `asm` block may use `AF BC DE HL IX IY` and the shadow registers.
* It must keep `SP` and the stack balanced, and must not change the
  interrupt mode or the `I` register.
* Use local labels (`.loop`) or unique names.
* The peephole optimiser never changes `asm` blocks.

To find the label of a variable, compile with `-S` and look at the `.asm`
file. Globals are named `g_<name>`, locals `l_<function>_<name>_<n>`.

---

## 21. Writing fast code

LZ80N compiles to native code, so most programs are fast without special
care. These rules help when every cycle counts:

1. **Use the smallest type.** `byte` operations are often half the cost of
   `word` operations.
2. **Constant multipliers and divisors are cheap.** `x * 10` becomes shifts
   and additions. `x / 10` and `x % 1000` (unsigned) become a multiplication
   by a reciprocal, with no division loop. Powers of two become shifts and
   `&`.
3. **Division by a variable is the most expensive basic operation** (a
   16-step loop). Avoid it in inner loops.
4. **`for` loops with step ±1** are the fastest loops. An empty iteration
   with a `word` counter takes 59 clock cycles.
5. **Prefer `fixed` to emulated floating point.** `fixed + fixed` is a single
   16-bit addition.
6. **Use the hardware**: `layer2_clear`, `dma_copy` and `dma_fill` use the
   DMA; `hline` uses `LDIR`; sprites and the copper cost no CPU time.
7. **Avoid long strings in inner loops.** Every string assignment copies the
   text.
8. **Check the generated code.** `lz80nc -S` writes the `.asm` file with the
   source lines as comments.

### Benchmarks

`benchmarks/` contains the same seven tasks in LZ80N and NextBASIC (loop,
arithmetic, prime sieve, printing, strings, square root, pixel drawing).
`benchmark_int.bas` repeats the loop and arithmetic tasks with NextBASIC
integer variables. Details and results are in `benchmarks/README.md`.

---

## 22. How a program runs

### 22.1 Startup

1. Interrupts off, stack set up, **CPU at 28 MHz**.
2. The ULA screen is switched off. Layer order S-U-L. Layer 2 is prepared
   (256 × 192).
3. The tilemap text layer and the font are set up.
4. Own **IM2 interrupt handler**: frame counter, `interrupt` handlers, sound
   driver.
5. The library modules used by the program are initialised (sound,
   sprites …).
6. Interrupts on, then the main program runs.

The program does not use the ROM or its system variables. When the main
program ends, the processor idles in an `EI : HALT` loop.

### 22.2 Memory map

```
$0000..$1FFF  MMU slot 0: Layer 2 drawing window (after drawing)
$4000..$5AFF  ULA screen (unused, cleared)
$6000..$69FF  tilemap (text layer)
$6C00..$6FFF  text font
$2000..$3FFF  MMU slot 1: far data / picture pages, mapped on access
$4000..$5FFF  MMU slot 2: banked code pages (while a banked function runs)
banks 8-10    Layer 2 buffer 1, banks 11-13: buffer 2 (set at startup)
banks 14..    pictures and far arrays
$8000..$FCFF  program code and data (checked at compile time)
$FDFC..       stack (grows downwards)
$FDFD..$FDFF  JP to the interrupt handler
$FE00..$FF00  IM2 vector table
```

### 22.3 Standard modules

Some libraries are written in LZ80N itself (sprites, Layer 2 shapes,
palettes, copper, sound). They are compiled in only when the program uses
them, so an unused library costs no memory.

### 22.4 The compiler

`lexer → parser → AST → type checks and Z80N code → peephole optimiser →
SjASMPlus → .nex`.

The test suite (`cargo test`) compiles small programs, runs them in a
built-in Z80N emulator (with clock cycle counting, raster timing, IM2
interrupts, copper, DMA and AY registers) and checks the output, the screen
and the hardware state.

### 22.5 Code banking

Programs can be larger than the 31 KB program area. When the resident part
would not fit, the compiler moves functions into 8K **code pages**,
automatically, largest first. Nothing changes in the source.

* A banked function runs in MMU slot 2 (`$4000`–`$5FFF`, unused because
  the ULA screen is off). A call goes through an 8-byte stub that maps the
  function's page, calls it and maps the caller's page back, so banked
  functions can call each other across pages, return values and use
  everything else as usual. A call costs about 150 T-states extra.
* Code pages come after the pictures and `far` arrays (from 16K bank 14
  up). Interrupt handlers stay resident; functions they call may be banked
  (the mapping is saved and restored).
* The main program can be large too. If it does not fit even with all its
  functions banked, the compiler moves its big statements (loops, `if`
  blocks, long statements, innermost first) into hidden functions, which
  are banked the same way. A `break` that leaves such a statement, or
  inline `asm`, keeps it in place.
* What always stays resident: global variables, strings and other data, the
  runtime, and the main program's small statements. If those alone are too
  large, the assembler reports it; move big tables into `far` arrays (19.1).
* `lz80nc --bank-all game.lz80n` puts every function, and the bigger
  statements of the main program, into code pages, to try banking with a
  small program (for example `examples/banks.lz80n`).

---

## 23. Limitations and roadmap

Not supported yet:

* more than about 31 KB of global variables and data in the program area
  (big tables belong in `far` arrays, 19.1)

Planned order: benchmarks against NextBASIC on real hardware →
`long`/`fixed16`/Layer 2 modes → constructors and inheritance → memory banks
→ sample playback and music import → an intermediate representation and
register allocation.

---

## 24. Reference tables

### 24.1 Built-in functions

| Function | Chapter |
|----------|---------|
| `print`, `write`, `at`, `ink`, `cls`, `border` | 11 |
| `key`, `kempston`, `any_key`, `wait_key` | 12 |
| `vsync`, `pause`, `frames`, `random`, `seed`, `cpu_speed` | 13 |
| `layer2_visible`, `layer2_clear`, `layer2_flip`, `plot`, `point`, `hline`, `vline`, `line`, `rect`, `fill_rect`, `circle`, `fill_circle`, `ellipse`, `fill_ellipse`, `triangle`, `fill_triangle`, `rgb` | 14 |
| `draw_image`, `image_width`, `image_height` | 14, 16 |
| `palette`, `palette9`, `rgb9` | 15 |
| `sprite_load`, `sprites_visible`, `sprite[n]`, `sprite_collision` | 16 |
| `beep`, `tone`, `noise`, `volume`, `sound_off`, `sound_reset`, `play`, `play_loop`, `playing`, `note_period`, `ay_write` | 17 |
| `copper_begin`, `copper_wait`, `copper_move`, `copper_start`, `copper_stop` | 18 |
| `memory[]`, `port[]`, `nextreg[]`, `addr`, `dma_copy`, `dma_fill`, `far` arrays | 19 |
| `file_open`, `file_close`, `file_read`, `file_write`, `file_getc`, `file_seek`, `file_pos`, `file_size`, `file_exists`, `file_delete`, `file_opendir`, `file_readdir`, `file_exec` | 19.2 |
| `len`, `sub`, `find`, `val`, `str`, `asc`, `chr`, `upper`, `lower` | 9 |
| `abs`, `min`, `max`, `clamp`, `sqrt`, `sin`, `cos`, `atan2`, `round` | 10 |

### 24.2 Built-in constants

| Constants | Values |
|-----------|--------|
| `BLACK BLUE RED MAGENTA GREEN CYAN YELLOW WHITE` | 0–7 |
| `BRIGHT_BLACK` … `BRIGHT_WHITE` | 8–15 |
| `KEY_A` … `KEY_Z`, `KEY_0` … `KEY_9`, `KEY_SPACE`, `KEY_ENTER`, `KEY_CAPS`, `KEY_SYM` | key codes |
| `JOY_RIGHT JOY_LEFT JOY_DOWN JOY_UP JOY_FIRE` | Kempston bits |
| `MHZ_3 MHZ_7 MHZ_14 MHZ_28` | 0–3 |
| `NOT_FOUND` | 255 |
| `TRANSPARENT` | `$E3` |
| `PAL_ULA PAL_LAYER2 PAL_SPRITES PAL_TEXT` | palette selectors (first palettes) |
| `PAL_ULA_2 PAL_LAYER2_2 PAL_SPRITES_2 PAL_TEXT_2` | second palettes |
| `L2_256 L2_320 L2_640` | Layer 2 modes |

### 24.3 Example programs

| File | Shows |
|------|-------|
| `hello.lz80n` | numbers, functions, `for`, colours |
| `move.lz80n` | keyboard and Kempston joystick, `at` |
| `classes.lz80n` | structs, classes, methods, arrays of objects |
| `inherit.lz80n` | constructors, inheritance, struct results, struct lists |
| `text.lz80n` | the text layer, 16 colours |
| `layer2.lz80n` | Layer 2 drawing, layering with text and sprites |
| `math.lz80n` | `fixed`, `sin`/`cos`/`atan2`/`sqrt`, double buffering |
| `palette.lz80n` | colour cycling, `dma_copy`, `addr` |
| `hires.lz80n` | Layer 2 320 × 256 and 640 × 256, second palettes |
| `raster.lz80n` | copper, raster and frame interrupts, `random` |
| `strings.lz80n` | strings and string functions, a scrolling ticker |
| `tables.lz80n` | array initialisers: menu, bar chart and colour tables, `/=` `%=` |
| `life.lz80n` | Game of Life on 2-D arrays, 2-D initialisers |
| `shapes.lz80n` | circles, ellipses, clipping, double buffering |
| `images.lz80n` | PNG pictures on Layer 2, transparency |
| `banks.lz80n` | `far` arrays: a 16 KB cave map in memory banks |
| `bignum.lz80n` | `long` and `fixed16`: big scores, 12!, a cannonball's path |
| `recursion.lz80n` | recursion: a fractal tree, the Towers of Hanoi, mutual recursion |
| `modules.lz80n` | a program in three files: `include`, `lib/screen.lz80n`, `lib/ball.lz80n` |
| `shooter.lz80n` | a little space shooter: player cut from a picture, five kinds of 16x16 pixel-art enemies, stars, sound |
| `sound.lz80n` | background music, `beep`, sound effects |
| `samples.lz80n` | `.wav` samples on the DAC in the background |
| `music.lz80n` | ProTracker 3 music, level meters |
| `sprites.lz80n` | hardware sprites, PNG assets |
| `benchmarks/benchmark.lz80n` | speed measurement |

---

## 25. The compiler and the IDE on the Next

LZ80N also runs on the ZX Spectrum Next itself (a 2 MB machine): the
compiler is written in LZ80N as well, and `lz80nc` builds it for the Next.

```powershell
lz80nc --export-selfhost D:\lz80n      # the .nex files, the runtime files, the manual
```

The release packages contain the result in `next/lz80n` (and as
`lz80n-VERSION-zxnext-sdcard.zip`). Copy the `lz80n` folder to the SD card,
then in NextBASIC:

```
.cd /lz80n
.nexload ide.nex
```

The folder holds the five `.nex` programs; `lib/` the runtime, the modules
and the manuals; `examples/` the example programs with their pictures,
sounds and include files (open `examples/hello.lz80n` to start). A
program's includes and assets are relative to its own folder.

### 25.1 The LZ80N IDE

`ide.nex` is a full-screen editor in the 80-column text mode: menus (File,
Edit, Build, Help), syntax highlighting, line numbers, a scroll bar, a
status line, the Kempston mouse with a pointer (click, drag to select,
double click a word, the right button opens a menu, the wheel scrolls).

| Keys | |
|------|---|
| CAPS+5 6 7 8 | cursor left, down, up, right |
| CAPS+3 / CAPS+4 | page up / page down |
| CAPS+0 / CAPS+9 | delete back / forward |
| CAPS+1 (EDIT) | the menus |
| CAPS+SPACE (BREAK) | cancel |
| SYM+Q W E A S D F G X | `[` `]` `^` `~` `\|` `\` `{` `}` and the backquote |
| EXTEND (CAPS+SYM), then S O N | save, open (the file browser), new |
| EXTEND, then B / R | compile / compile and run |
| EXTEND, then F G | find, go to a line |
| EXTEND, then M A C X V K | mark, select all, copy, cut, paste, delete the line |
| EXTEND, then L / H | the build output / the manual at the word under the cursor |

**Compile** (Build -> Compile, EXTEND B) saves the program and starts the
compiler; it comes back to the IDE with the result. The compiler's messages
appear on the screen like in a command line session and are shown in the
IDE afterwards (Build -> Build output); an error puts the cursor on its
line (in an included file it opens that file). **Compile & run** starts the
program when it compiled.

**File -> Open** shows the folders and the program files of a folder.
**Help -> Language manual** shows this manual (Hungarian:
**Kezikonyv (magyar)**); left / right go to the chapters, ENTER on a line of
the contents opens its chapter, F finds a text.

### 25.2 The stages

The compiler runs as stages, each a `.nex` program that starts the next one:
`lexer.nex` (tokens, includes and modules) -> `assets.nex` (pictures,
samples, music; only when the program has assets) -> `codegen.nex` (the
assembly, `out.asm`) -> `asm.nex` (`out.nex`). By hand, without the IDE:

```
COPY "myprog.lz80n" TO "in.lz80n"
.nexload lexer.nex
```

Errors go to `codegen.err`, `asm.err` or `assets.err`; `build.log` has the
messages of the stages, which also appear on the screen in the 80-column
text mode. A small program takes a few seconds at 28 MHz (plus loading the
stages from the SD card); one that uses the Layer 2 module about 20 seconds.
The language is the same as with `lz80nc` on a PC (`selfhost/README.md`
lists the details).
