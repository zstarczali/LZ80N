===============================================================================
  LZ80N @VERSION@  --  The simplicity of Lua. The power of Z80N.
  Package: @PLATFORM@
===============================================================================

LZ80N is a statically typed, Lua-inspired programming language for the
ZX Spectrum Next. The compiler, lz80nc, turns LZ80N source code into native
Z80N machine code and packs it into a .nex file that runs on the Next or in
an emulator. No interpreter, no virtual machine, no garbage collector.

Home page and source code:  https://github.com/zstarczali/LZ80N


CONTENTS
--------
  1. What is in this package
  2. Installation (Windows, macOS, Linux; x64 and ARM64)
  3. Your first program
  4. Running a program (real Next, CSpect, ZEsarUX)
  5. Compiler command line
  6. The language in one page
  7. Examples
  8. Troubleshooting
  9. Licences


1. WHAT IS IN THIS PACKAGE
--------------------------

  lz80nc / lz80nc.exe        the LZ80N compiler
  sjasmplus / sjasmplus.exe  SjASMPlus @SJASM@, the Z80N assembler the compiler
                             uses (it is found automatically in this folder)
  docs/manual-en.pdf         Programmer's manual (English)
  docs/manual-hu.pdf         Programozói kézikönyv (magyar)
  docs/*.md                  the same manuals and the technical specification
                             as Markdown text
  examples/                  17 example programs and their pictures
  benchmarks/                speed tests: LZ80N and NextBASIC
  README.txt                 this file
  CHANGELOG.md               what is new in each version
  LICENSE                    LZ80N licence (MIT)
  licenses/                  SjASMPlus licence

The packages:

  lz80n-@VERSION@-windows-x64.zip      Windows 10/11, Intel/AMD 64-bit
  lz80n-@VERSION@-windows-arm64.zip    Windows 11 on ARM (Snapdragon etc.)
  lz80n-@VERSION@-macos-x64.tar.gz     macOS on Intel Macs
  lz80n-@VERSION@-macos-arm64.tar.gz   macOS on Apple Silicon (M1/M2/M3/M4)
  lz80n-@VERSION@-linux-x64.tar.gz     Linux, Intel/AMD 64-bit (glibc 2.35+)
  lz80n-@VERSION@-linux-arm64.tar.gz   Linux on ARM64 (Raspberry Pi 4/5 with a
                                       64-bit OS, ARM servers; glibc 2.35+)


2. INSTALLATION
---------------

Unpack the archive anywhere. Nothing else is needed: the compiler and the
assembler are in the same folder. To call lz80nc from any folder, add the
folder to your PATH (optional).

Windows (x64 and ARM64)
  1. Right-click the .zip -> Extract All..., e.g. to C:\LZ80N
  2. Open PowerShell or Command Prompt in that folder:
         cd C:\LZ80N
         .\lz80nc --version
  3. Optional, PATH: Start -> "Edit environment variables for your account"
     -> Path -> New -> C:\LZ80N
  * If SmartScreen warns about an unknown publisher: "More info" ->
    "Run anyway" (the programs are not code-signed).
  * Windows on ARM: lz80nc is a native ARM64 program; sjasmplus.exe is the
    official x64 build, which Windows 11 runs through its built-in x64
    emulation.

macOS (Intel and Apple Silicon)
  1. Unpack:
         tar xzf lz80n-@VERSION@-macos-arm64.tar.gz      (or -macos-x64)
         cd lz80n-@VERSION@-macos-arm64
  2. The programs are not notarised, so remove the download quarantine once:
         xattr -dr com.apple.quarantine .
  3. Check:
         ./lz80nc --version
  4. Optional, PATH (zsh):
         echo 'export PATH="$PATH:'"$PWD"'"' >> ~/.zshrc
  * Pick the package that matches your Mac: Apple menu -> About This Mac ->
    "Chip: Apple ..." = arm64, "Processor: Intel ..." = x64.

Linux (x64 and ARM64)
  1. Unpack:
         tar xzf lz80n-@VERSION@-linux-x64.tar.gz        (or -linux-arm64)
         cd lz80n-@VERSION@-linux-x64
  2. Check:
         ./lz80nc --version
  3. Optional, PATH (bash):
         echo 'export PATH="$PATH:'"$PWD"'"' >> ~/.bashrc
  * `uname -m` tells your platform: x86_64 = x64, aarch64 = arm64.
  * The programs need glibc 2.35 or newer (Ubuntu 22.04, Debian 12,
    Fedora 36 and later). If the bundled sjasmplus does not start, build it
    from source (https://github.com/z00m128/sjasmplus) and point lz80nc at it
    with --sjasmplus PATH or the SJASMPLUS environment variable.


3. YOUR FIRST PROGRAM
---------------------

Save this as hello.lz80n:

    -- my first LZ80N program
    ink(BRIGHT_YELLOW)
    print("Hello, Next!")

    for i = 1, 10 do
        ink(i)
        print("line ", i, ": ", i * i)
    end

Compile it:

    lz80nc hello.lz80n          (Windows: lz80nc.exe hello.lz80n)

This writes hello.asm (the generated assembly) and hello.nex (the program).
Or try an example from the package:

    lz80nc examples/shooter.lz80n


4. RUNNING A PROGRAM
--------------------

ZX Spectrum Next (real hardware or a clone)
  Copy the .nex file to the SD card, start the Next, and choose the file in
  the Browser (or type .nexload hello.nex in NextBASIC).

CSpect emulator (Windows; macOS/Linux with Mono)  -- https://mdf200.itch.io/cspect
  Run the .nex directly:
      CSpect.exe -w3 -zxnext -mmc=./ hello.nex
  With NextZXOS installed on an SD image you can also start it from the
  Browser like on the real machine (see CSpect's readme).

ZEsarUX  -- https://github.com/chernandezba/zesarux
  Machine: ZX Spectrum Next, then Smartload the .nex file.

The program takes over the machine: it runs until you reset.


5. COMPILER COMMAND LINE
------------------------

    lz80nc <input.lz80n> [options]

    -o FILE          output .nex file (default: next to the source)
    -S               stop after writing the assembly (.asm)
    -O0              no peephole optimisation
    --bank-all       put every function into its own code page (to try
                     code banking; normally automatic)
    --sjasmplus PATH use this SjASMPlus
    -V, --version    print the version
    -h, --help       short help

Errors look like  file.lz80n:12:5: error: message  and the compiler reports
as many as it can find in one run. Relative asset paths (PNG files) are
relative to the source file.

The SjASMPlus used is the first of: --sjasmplus PATH, the SJASMPLUS
environment variable, sjasmplus next to lz80nc, sjasmplus on the PATH.


6. THE LANGUAGE IN ONE PAGE
---------------------------

(The manuals in docs/ explain everything in detail, with examples.)

    -- comment
    byte lives = 3                 -- 0..255
    sbyte dx = -1                  -- -128..127
    word score = 0                 -- 0..65535
    sword speed = -300             -- -32768..32767
    fixed angle = 1.5              -- 8.8 fixed point, -128..127.99
    bool alive = true
    string name = "Ann"            -- up to 64 characters
    const MAX = 10                 -- compile-time constant

    byte map[24][32]               -- arrays, up to 4 dimensions
    byte table[] = {1, 2, 4, 8}    -- with an initialiser list
    far byte level[40000]          -- big data in the Next's memory banks

    if score > 100 and alive then ... elseif ... else ... end
    while lives > 0 do ... end
    for i = 0, 9 do ... end        -- also: for i = 10, 0, -2 do
    score += 10                    -- also -= *= /= %= &= |= <<= >>=

    function add(word a, word b) -> word
        return a + b
    end

    class Enemy                    -- structs and classes with methods
        word x
        word y
        function move()
            self.y += 1
        end
    end
    Enemy enemies[8]
    enemies[0].move()

    interrupt frame                -- runs 50 times a second
        ...
    end

Built-in and library functions (selection):
    text      print write at ink cls border
    input     key(KEY_SPACE) kempston() any_key() wait_key()
    time      vsync() pause(n) frames() random(n) seed(n) cpu_speed(MHZ_28)
    math      abs min max clamp sqrt sin cos atan2 round
    strings   len sub find val str asc chr upper lower  ..  (concatenation)
    Layer 2   layer2_visible layer2_clear layer2_flip plot point hline vline
              line rect fill_rect circle fill_circle ellipse fill_ellipse
              triangle fill_triangle rgb draw_image
    sprites   asset sheet = "ships.png"   sprite_load(0, sheet)
              sprite[n].position(x, y)  sprite[n].show(pattern)
    pictures  asset pic = image("title.png")   draw_image(pic, x, y)
    palette   palette palette9 rgb9
    sound     beep tone noise volume sound_off play play_loop playing
    hardware  memory[a] port[p] nextreg[r] dma_copy dma_fill addr  asm ... end

Programs larger than the 31 KB program area are fine: functions move into
8 KB code pages automatically, and far arrays hold big data.


7. EXAMPLES
-----------

In examples/ (compile any of them with lz80nc examples/NAME.lz80n):

    hello      numbers, functions, for loops, colours
    move       keyboard and joystick
    classes    structs, classes, methods, arrays of objects
    text       the text layer and its 16 colours
    strings    string functions, a scrolling ticker
    math       fixed point, sin/cos/atan2/sqrt, double buffering
    layer2     Layer 2 drawing, layers
    shapes     circles and ellipses, clipping
    palette    colour cycling, DMA
    raster     copper, raster interrupts
    sound      background music, sound effects
    sprites    hardware sprites from a PNG
    images     PNG pictures on Layer 2 with transparency
    tables     array initialisers
    life       Game of Life on a 2-D array
    banks      far arrays: a 16 KB cave map in memory banks
    shooter    a little space shooter (O/P/Q/A or joystick, SPACE to fire)


8. TROUBLESHOOTING
------------------

  "sjasmplus failed" / "cannot run sjasmplus"
      Keep sjasmplus next to lz80nc, or use --sjasmplus PATH.
  macOS: "cannot be opened because the developer cannot be verified"
      Run  xattr -dr com.apple.quarantine .  in the package folder.
  Linux: "Permission denied"
      chmod +x lz80nc sjasmplus
  "main program and data too large"
      The top-level code and the global data must fit in about 31 KB. Move
      top-level code into functions (they are banked automatically) and
      big tables into far arrays.
  The program shows nothing in the emulator
      Make sure the emulator runs as a ZX Spectrum Next (CSpect: -zxnext).


9. LICENCES
-----------

LZ80N is free software under the MIT licence (see LICENSE).
SjASMPlus is (c) its authors, BSD licence (see licenses/).
https://github.com/z00m128/sjasmplus
