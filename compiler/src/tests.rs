use super::compile_source;
use crate::emu::{load_nex, Cpu};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;

fn ok(src: &str) -> String {
    match compile_source(src, "t.nex") {
        Ok(a) => a,
        Err(e) => panic!("unexpected errors: {:?}", e.iter().map(|d| d.to_string()).collect::<Vec<_>>()),
    }
}

fn err(src: &str) -> String {
    match compile_source(src, "t.nex") {
        Ok(_) => panic!("expected an error"),
        Err(e) => e.iter().map(|d| d.to_string()).collect::<Vec<_>>().join("\n"),
    }
}

fn sjasmplus() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("SJASMPLUS") {
        return Some(p.into());
    }
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tools/sjasmplus/sjasmplus-1.24.0.win/sjasmplus.exe");
    p.exists().then_some(p)
}

/// Compile and assemble; checks the assembler accepts the generated code.
fn assembles(name: &str, src: &str) {
    build(name, src);
}

/// Compile + assemble, returning (nex bytes, symbols). Panics on failure.
fn build(name: &str, src: &str) -> Option<(Vec<u8>, HashMap<String, u16>)> {
    build_opts(name, src, false)
}

/// Compile + assemble a program whose `include`s are relative to `dir`.
fn build_in(dir: &std::path::Path, src: &str) -> (Vec<u8>, HashMap<String, u16>) {
    let asm = match crate::compile_opts(src, "t.nex", true, dir, false) {
        Ok(a) => a,
        Err(e) => panic!("unexpected errors: {:?}", e.iter().map(|d| d.to_string()).collect::<Vec<_>>()),
    };
    let out = crate::asm::assemble(&asm, dir).unwrap_or_else(|e| panic!("assembler: {}", e[0]));
    let syms = out.symbols16();
    (out.nex.unwrap().1, syms)
}

/// `bank_all`: every function in its own code page.
fn build_opts(name: &str, src: &str, bank_all: bool) -> Option<(Vec<u8>, HashMap<String, u16>)> {
    // relative asset paths resolve against examples/
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let asm = match crate::compile_opts(src, "t.nex", true, &base, bank_all) {
        Ok(a) => a,
        Err(e) => panic!("unexpected errors: {:?}", e.iter().map(|d| d.to_string()).collect::<Vec<_>>()),
    };
    let _ = name;
    let out = match crate::asm::assemble(&asm, &base) {
        Ok(o) => o,
        Err(e) => panic!(
            "assembler failed:\n{}\n--- asm ---\n{asm}",
            e.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n")
        ),
    };
    let syms = out.symbols16();
    let (_, nex) = out.nex.expect("no SAVENEX in the generated code");
    Some((nex, syms))
}

/// Run a program in the test emulator and return everything it printed.
fn run_with(name: &str, src: &str, setup: impl FnOnce(&mut Cpu)) -> Option<(String, Cpu)> {
    run_opts(name, src, false, setup)
}

fn run_opts(name: &str, src: &str, bank_all: bool, setup: impl FnOnce(&mut Cpu)) -> Option<(String, Cpu)> {
    let (nex, syms) = build_opts(name, src, bank_all)?;
    let mut cpu = load_nex(&nex).unwrap();
    setup(&mut cpu);
    let putc = syms["rt_putc"];
    let pchr = syms.get("rt_pp_chr").copied().unwrap_or(0xFFFF);
    let newline = syms["rt_newline"];
    let end = syms["__main_end"];
    let mut out = String::new();
    for _ in 0..50_000_000u64 {
        if cpu.pc == end {
            return Some((out, cpu));
        }
        if (cpu.pc == putc || cpu.pc == pchr) && cpu.a != 10 {
            out.push(cpu.a as char);
        } else if cpu.pc == newline {
            out.push('\n');
        }
        if let Err(e) = cpu.tick() {
            panic!("emulator: {e}\noutput so far:\n{out}");
        }
    }
    panic!("program did not terminate; output so far:\n{out}");
}

fn run(name: &str, src: &str) -> Option<String> {
    run_with(name, src, |_| {}).map(|r| r.0)
}

/// Assert the printed output. With SELFHOST=1 the program is also compiled
/// by the self-hosted compiler (selfhost/codegen.lz80n) when it supports it,
/// and must print the same.
fn expect(name: &str, src: &str, want: &str) {
    if let Some(out) = run(name, src) {
        assert_eq!(out, want, "program:\n{src}");
    }
    if std::env::var("SELFHOST").is_ok() {
        static SH: std::sync::OnceLock<SelfHost> = std::sync::OnceLock::new();
        let sh = SH.get_or_init(SelfHost::new);
        match sh.compile(src) {
            Ok(asm) => {
                assert_eq!(run_asm(&asm), want, "self-hosted compiler, program:\n{src}");
                eprintln!("SELFHOST ok {name}");
            }
            Err(e) if e.contains("not supported by the self-hosted compiler") => eprintln!("SELFHOST skip {name}: {e}"),
            Err(e) => panic!("self-hosted compiler: {e}\nprogram:\n{src}"),
        }
    }
}

#[test]
fn spec_variables_and_loops() {
    assembles(
        "spec_vars",
        r#"
byte lives = 3
word score = 0
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
"#,
    );
}

#[test]
fn spec_function_widening() {
    let asm = ok(r#"
function add(byte a, byte b) -> word
    return a + b
end
word r = add(200, 100)
print(r)
"#);
    // operands must be widened to 16 bits before the addition
    assert!(asm.contains("add hl,de"), "{asm}");
    assembles("widen", "function add(byte a, byte b) -> word\n return a + b\nend\nprint(add(200,100))\n");
}

#[test]
fn hardware_access_and_asm() {
    let asm = ok(r#"
asm
    ld d, 10
    ld e, 20
    mul d,e
end
nextreg[0x07] = 3
memory[0x4000] = 255
byte r = 7
nextreg[r] = 1
byte v = nextreg[0x07]
"#);
    assert!(asm.contains("nextreg 7,3"));
    assert!(asm.contains("    mul d,e"));
    assert!(asm.contains("ld (16384),a"));
    assembles("hw", "nextreg[7] = 3\nmemory[$4000] = 255\nasm\n  nop\nend\n");
}

#[test]
fn all_operators_assemble() {
    assembles(
        "ops",
        r#"
byte a = 10
byte b = 3
sbyte sa = -5
word w = 1000
sword sw = -1000
bool f = a > b and not (b == 0) or false
byte buf[16]
word wb[8]

a = a + b - 1
a = a * b
a = a / b
a = a % b
a = a & b | 1 ~ 2
a = a << 2
a = a >> b
sa = sa >> 1
sa = -sa
w = w * 3 + word(a) * 7
w = w / 10
w = w % 7
w = w << b
w = w >> 5
w = a * b
sw = sw / -3
sw = sw >> b
buf[a] = 5
buf[3] = buf[a] + 1
wb[a] = w
wb[2] = wb[a]
memory[w] = a
a = byte(w)
if sa < -2 then print("neg", sa) elseif sw >= 0 then print(sw) else print(f, w) end
while true do
    if a == 0 then break end
    a -= 1
end
for i = 10, 0, -2 do print(i) end
for j = 0, 1000, 100 do print(j) end
vsync()
border(2)
"#,
    );
}

#[test]
fn local_and_nested_calls() {
    assembles(
        "calls",
        r#"
function sq(byte x) -> word
    word r = x * x
    return r
end
function sum(word a, word b) -> word
    return a + b
end
print(sum(sq(3), sq(4)))
"#,
    );
}

#[test]
fn errors() {
    assert!(err("byte x = 300").contains("does not fit"));
    assert!(err("word w = 5\nbyte b = w").contains("narrow"));
    assert!(err("print(y)").contains("undefined variable"));
    assert!(err("if 1 then end").contains("bool"));
    assert!(err("break").contains("outside"));
    assert!(err("byte a[4]\na[4] = 1").contains("out of bounds"));
    assert!(err("byte x = 1\nbyte x = 2").contains("already declared"));
    assert!(err("function f(byte a)\nend\nf()").contains("expects 1"));
    assert!(err("byte x = 1 / 0").is_empty() == false);
}

// ------------------------------------------------------------------
// Execution tests: programs run in the Z80N test emulator.

#[test]
fn run_hello_and_numbers() {
    expect("r_hello", r#"print("Hi", 42, true, -7)"#, "Hi 42 true -7\n");
}

#[test]
fn run_widening_add() {
    expect(
        "r_widen",
        "function add(byte a, byte b) -> word\n return a + b\nend\nprint(add(200, 100))\nbyte c = 200 + 100 - 256\nprint(c)",
        "300\n44\n",
    );
}

#[test]
fn run_byte_arithmetic_wraps() {
    expect(
        "r_wrap",
        r#"
byte a = 250
a += 10
print(a)
sbyte s = 127
s += 1
print(s)
byte m = 20
print(m * 13)
"#,
        "4\n-128\n4\n",
    );
}

#[test]
fn run_mul_div_mod() {
    expect(
        "r_muldiv",
        r#"
word w = 1234
word v = 56
print(w * v, w / v, w % v)
sword s = -1000
sword t = 7
print(s / t, s % t, s * t)
byte a = 200
byte b = 7
print(a / b, a % b)
sbyte x = -100
sbyte y = 9
print(x / y, x % y)
word q = 60000
word zero = 0
print(q / 3, q / zero)
byte i = 15
byte j = 17
word big = i * j
print(big)
"#,
        "3568 22 2\n-142 -6 -7000\n28 4\n-11 -1\n20000 65535\n255\n",
    );
}

#[test]
fn run_shifts_and_bits() {
    expect(
        "r_shift",
        r#"
byte a = 0b1011
byte n = 3
print(a << 2, a << n, a >> 1, a >> n)
sbyte s = -64
print(s >> 2, s >> n)
word w = 0x1234
print(w << 4, w >> 4, w << n, w >> 12)
sword sw = -4096
print(sw >> 4, sw >> n)
print(a & 6, a | 0x30, a ~ 0xFF, ~a)
word m = 0xF0F0
print(m & 0x0FF0, m | 0x000F, m ~ 0xFFFF)
"#,
        "44 88 5 1\n-16 -8\n9024 291 37280 1\n-256 -512\n2 59 244 244\n240 61695 3855\n",
    );
}

#[test]
fn run_comparisons() {
    expect(
        "r_cmp",
        r#"
byte a = 5
byte b = 200
sbyte sa = -3
sbyte sb = 4
word w = 1000
sword sw = -1000
print(a < b, a > b, a <= 5, a >= 6, a == 5)
print(sa < sb, sa > sb, sa < -2, sa >= -3, sb <= -4)
print(w > 999, w < 999, sw < 0, sw > -1001, sw >= w)
print(not (a < b) or a == 5 and b == 200)
"#,
        "true false true false true\ntrue false true true false\ntrue false true true false\ntrue\n",
    );
}

#[test]
fn run_control_flow() {
    expect(
        "r_flow",
        r#"
for i = 1, 3 do print(i) end
for j = 10, 0, -4 do print(j) end
word count = 0
for k = 0, 255 do count += 1 end
print(count)
for e = 5, 1 do print("never") end
byte n = 0
while true do
    n += 1
    if n == 2 then
        print("two")
    elseif n == 3 then
        print("three")
    else
        print("other")
    end
    if n >= 4 then break end
end
"#,
        "1\n2\n3\n10\n6\n2\n256\nother\ntwo\nthree\nother\n",
    );
}

#[test]
fn run_functions_and_arrays() {
    expect(
        "r_fn",
        r#"
word fib[20]
fib[0] = 0
fib[1] = 1
for i = 2, 19 do
    fib[i] = fib[i - 1] + fib[i - 2]
end
print(fib[10], fib[19])

function sq(byte x) -> word
    return x * x
end
function sum(word a, word b) -> word
    return a + b
end
print(sum(sq(3), sq(4)))

function fact(byte n) -> word
    word r = 1
    for i = 2, n do r = r * i end
    return r
end
print(fact(8))

byte buf[4]
byte idx = 2
buf[idx] = 77
buf[idx] += 3
print(buf[2], memory[0x4000 + 0])
"#,
        "55 4181\n25\n40320\n80 0\n",
    );
}

#[test]
fn run_nextreg_and_asm() {
    if let Some((out, cpu)) = run_with(
        "r_hw",
        r#"
nextreg[0x07] = 3
byte r = 0x14
nextreg[r] = 0xE3
print(nextreg[0x14])
border(5)
byte result = 0
asm
    ld d,6
    ld e,7
    mul d,e
    ld a,e
    ld (g_result),a
end
print(result)
"#,
        |_| {},
    ) {
        assert_eq!(out, "227\n42\n");
        assert_eq!(cpu.nextreg[7], 3);
        assert_eq!(cpu.nextreg[0x4A], 0x16, "border(5) = cyan fallback colour");
    }
}

#[test]
fn run_keyboard_and_joystick() {
    let src = r#"
print(key(KEY_Q), key(KEY_SPACE), key(KEY_A))
byte k = KEY_SPACE
print(key(k))
byte j = kempston()
if j & JOY_FIRE ~= 0 then print("fire") end
"#;
    if let Some((out, _)) = run_with("r_keys", src, |cpu| {
        cpu.keys[2] &= !1; // Q pressed  (row $FB, bit 0)
        cpu.keys[7] &= !1; // SPACE pressed (row $7F, bit 0)
        cpu.kempston = 16;
    }) {
        assert_eq!(out, "true true false\ntrue\nfire\n");
    }
}

#[test]
fn run_scroll_and_at() {
    let mut src = String::from("for i = 1, 30 do print(i) end\nat(0, 5)\nprint(\"x\")\ncls()\nprint(RED, frames())\n");
    src.push_str("");
    if let Some(out) = run("r_scroll", &src) {
        let expect: String = (1..=30).map(|i| format!("{i}\n")).collect::<String>() + "x\n2 0\n";
        assert_eq!(out, expect);
    }
}

#[test]
fn run_text_layer() {
    let src = "for i = 1, 40 do print(i) end
at(0, 5)
write(\"x\")
ink(BRIGHT_RED)
at(2, 38)
write(\"abc\")";
    if let Some((_, cpu)) = run_with("r_text", src, |_| {}) {
        // 40 lines on a 32-line screen: the last newline scrolled 9 times
        assert_eq!(screen_line(&cpu, 0), "10   x");
        assert_eq!(screen_line(&cpu, 30), "40");
        assert_eq!(screen_line(&cpu, 31), "");
        // "abc" at column 38 wraps after two characters
        assert_eq!(&screen_text(&cpu).lines().nth(2).unwrap()[38..], "ab");
        assert_eq!(screen_line(&cpu, 3), "c3"); // overwrote the "1" of "13"
        assert_eq!(screen_attr(&cpu, 2, 38), 10 * 2);
        assert_eq!(screen_attr(&cpu, 0, 0), 7 * 2);
        // tilemap configuration and font
        assert_eq!(cpu.nextreg[0x6B], 0b1000_1001);
        assert_eq!(cpu.nextreg[0x6E], 0x20);
        assert_eq!(cpu.nextreg[0x6F], 0x2C);
        assert_eq!(cpu.mem[0x6C00 + 65 * 8], 0x38, "'A' glyph row 0");
    }
}

#[test]
fn run_structs() {
    expect(
        "r_struct",
        r#"
struct Point
    sword x
    sword y
end

struct PlayerState
    word x
    word y
    byte health = 100
    byte lives = 3
    Point vel
    byte inv[4]
end

PlayerState player
player.x = 100
player.vel.x = -2
player.inv[2] = 9
player.x += 5
print(player.x, player.y, player.health, player.lives)
print(player.vel.x, player.vel.y, player.inv[2])

Point pts[5]
for i = 0, 4 do
    pts[i].x = i * 10
    pts[i].y = -i
end
byte k = 3
print(pts[k].x, pts[k].y, pts[4].x)

PlayerState copy
copy = player
copy.lives -= 1
print(copy.x, copy.lives, player.lives, copy.vel.x)
"#,
        "105 0 100 3\n-2 0 9\n30 -3 40\n105 2 3 -2\n",
    );
}

#[test]
fn run_struct_params_by_reference() {
    expect(
        "r_struct_ref",
        r#"
struct Enemy
    byte hp = 10
    word score
end

function hit(Enemy e, byte dmg)
    if e.hp > dmg then
        e.hp -= dmg
    else
        e.hp = 0
        e.score += 50
    end
end

Enemy enemies[3]
hit(enemies[1], 4)
hit(enemies[1], 4)
hit(enemies[1], 4)
byte i = 2
hit(enemies[i], 1)
print(enemies[0].hp, enemies[1].hp, enemies[1].score, enemies[2].hp)
"#,
        "10 0 50 9\n",
    );
}

#[test]
fn run_classes() {
    expect(
        "r_class",
        r#"
class Player
    word x = 128
    word y = 96
    byte health = 100

    function move(sword dx, sword dy)
        self.x += dx
        self.y += dy
    end

    function hurt(byte n) -> bool
        if self.health <= n then
            self.health = 0
            return true
        end
        self.health -= n
        return false
    end

    function report()
        print(self.x, self.y, self.health)
    end
end

Player player
player.move(2, 0)
player.move(-10, 4)
player.report()
print(player.hurt(30), player.hurt(80))

Player team[2]
team[1].move(1, 1)
byte i = 1
team[i].move(1, 1)
team[i].report()
team[0].report()
"#,
        "120 100 100\nfalse true\n130 98 100\n128 96 100\n",
    );
}

#[test]
fn struct_errors() {
    assert!(err("struct P\n byte x\nend\nP p\nprint(p.z)").contains("no field"));
    assert!(err("struct P\n byte x\nend\nP p\nbyte b = p").contains("whole"));
    assert!(err("struct P\n P inner\nend").contains("itself"));
    assert!(err("Foo f").contains("unknown type"));
    assert!(err("class C\n function f()\n end\nend\nC c\nc.g()").contains("no method"));
    assert!(err("struct A\n byte x\nend\nstruct B\n byte x\nend\nA a\nB b\na = b").contains("not a `A`"));
}

/// Every example must compile, assemble and run (they loop forever, so we
/// only execute a few million instructions and check nothing faults).
#[test]
fn examples_smoke() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let mut n = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("lz80n") {
            continue;
        }
        n += 1;
        let src = std::fs::read_to_string(&path).unwrap();
        let name = format!("ex_{}", path.file_stem().unwrap().to_string_lossy());
        let Some((nex, _)) = build(&name, &src) else { return };
        let mut cpu = load_nex(&nex).unwrap();
        for _ in 0..3_000_000 {
            cpu.halted = false;
            if let Err(e) = cpu.step() {
                panic!("{}: {e}", path.display());
            }
            assert!(cpu.sp > 0xF000 && cpu.sp <= 0xFDFD, "{}: stack out of range (sp={:04X})", path.display(), cpu.sp);
        }
    }
    assert!(n >= 3);
}

#[test]
fn const_sized_arrays() {
    expect(
        "r_constarr",
        "const N = 4\nstruct S\n byte a[N * 2]\nend\nS s\nword w[N]\ns.a[7] = 5\nw[N - 1] = 1000\nprint(s.a[7], w[3])",
        "5 1000\n",
    );
}

/// The tilemap text screen (40x32) as text.
fn screen_text(cpu: &Cpu) -> String {
    let mut s = String::new();
    for row in 0..32usize {
        for col in 0..40usize {
            let c = cpu.mem[0x6000 + (row * 40 + col) * 2];
            s.push(if (32..127).contains(&c) { c as char } else { '?' });
        }
        s.push('\n');
    }
    s
}

fn screen_line(cpu: &Cpu, row: usize) -> String {
    screen_text(cpu).lines().nth(row).unwrap().trim_end().to_string()
}

fn screen_attr(cpu: &Cpu, row: usize, col: usize) -> u8 {
    cpu.mem[0x6000 + (row * 40 + col) * 2 + 1]
}


#[test]
fn run_write() {
    expect("r_write", "write(\"a\", 1, true)\nwrite(-2)\nprint()\nprint(\"b\")", "a1true-2\nb\n");
}

// ------------------------------------------------------------------
// Hardware sprites

fn write_test_png(name: &str) -> PathBuf {
    // 32x16 sheet = 2 patterns: left red square with transparent corner,
    // right solid colour that maps to $E3 (must be nudged to $E2)
    let (w, h) = (32u32, 16u32);
    let mut rgba = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let px = if x < 16 {
                if x == 0 && y == 0 { [0, 0, 0, 0] } else { [255, 0, 0, 255] }
            } else {
                [0xE0, 0x00, 0xC0, 255]
            };
            rgba.extend_from_slice(&px);
        }
    }
    let dir = std::env::temp_dir().join("lz80nc_assets");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, crate::assets::encode_png(w, h, &rgba)).unwrap();
    path
}

#[test]
fn png_conversion() {
    let path = write_test_png("conv.png");
    let a = crate::assets::load(&path).unwrap();
    assert_eq!(a.patterns, Some(2));
    assert_eq!(a.bytes[0], 0xE3); // transparent pixel
    assert_eq!(a.bytes[1], 0xE0); // red = 111 000 00
    assert_eq!(a.bytes[256], 0xE2); // would be $E3: nudged
}

#[test]
fn run_sprites() {
    let png = write_test_png("run.png");
    let src = format!(
        r#"
asset ship = "{}"
sprite_load(10, ship)
sprites_visible(true, false)
sprite[3].mirror_x = true
sprite[3].scale = 1
sprite[3].position(300, 260)
sprite[3].show(11)
sprite[5].palette = 2
sprite[5].mirror_y = true
sprite[5].rotate = true
sprite[5].scale = 2
sprite[5].position(40, 50)
sprite[5].hide()
print(sprite[3].x, sprite[3].id)
"#,
        png.display().to_string().replace(std::path::MAIN_SEPARATOR, "/")
    );
    if let Some((out, cpu)) = run_with("r_sprites", &src, |_| {}) {
        assert_eq!(out, "300 3\n");
        assert_eq!(cpu.nextreg[0x07], 3, "28 MHz at startup");
        assert_eq!(cpu.nextreg[0x15] & 3, 1, "sprites visible, not over border");
        assert_eq!(cpu.patterns[10 * 256], 0xE3);
        assert_eq!(cpu.patterns[10 * 256 + 1], 0xE0);
        assert_eq!(cpu.patterns[11 * 256], 0xE2);
        // x = 300 = $12C, y = 260 = $104
        assert_eq!(cpu.sprite_attr[3], [0x2C, 0x04, 0x08 | 0x01, 0x80 | 0x40 | 11, (1 << 3) | (1 << 1) | 1]);
        assert_eq!(cpu.sprite_attr[5], [40, 50, 0x20 | 4 | 2, 0x40, (2 << 3) | (2 << 1)]);
    }
}

/// `overlaps` compares the sprites' squares; `sprite_collision` reads the
/// pixel-exact hardware flag (pattern 0 has a transparent top-left pixel).
#[test]
fn sprite_collisions() {
    let png = write_test_png("coll.png");
    let src = format!(
        r#"
asset s = "{}"
sprite_load(0, s)
sprites_visible(true, false)
sprite[0].position(100, 100)
sprite[0].show(0)
sprite[1].position(85, 85)
sprite[1].show(0)
-- only B's bottom-right pixel touches A's transparent corner
print(sprite[0].overlaps(sprite[1]), sprite_collision())
sprite[1].position(86, 85)
print(sprite[0].overlaps(sprite[1]), sprite_collision())
sprite[1].position(84, 100)
print(sprite[1].overlaps(sprite[0]), sprite_collision())
sprite[1].scale = 1
sprite[1].update()
print(sprite[1].overlaps(sprite[0]), sprite_collision())
sprite[1].hide()
print(sprite[0].overlaps(sprite[1]), sprite_collision())
sprite[1].position(300, 200)
sprite[1].show(0)
sprite[2].position(300, 200)
sprite[2].show(1)
-- off the display: the squares overlap, the hardware clips them
print(sprite[1].overlaps(sprite[2]), sprite_collision())
sprites_visible(true, true)
print(sprite_collision())
"#,
        png.display().to_string().replace(std::path::MAIN_SEPARATOR, "/")
    );
    if let Some(out) = run("r_sprite_coll", &src) {
        assert_eq!(
            out,
            "true false\ntrue true\nfalse false\ntrue true\nfalse false\ntrue false\ntrue\n"
        );
    }
}

#[test]
fn sprite_errors() {
    let png = write_test_png("err.png");
    let p = png.display().to_string().replace(std::path::MAIN_SEPARATOR, "/");
    assert!(err(&format!("asset s = \"{p}\"\nsprite_load(63, s)")).contains("do not fit"));
    assert!(err("asset s = \"does_not_exist.png\"").contains("cannot read"));
    assert!(err("byte b = 1\nsprite_load(0, b)").contains("sprite asset"));
}

/// Run the sprites example for a number of frames and check the sprite
/// engine state: ship + 8 balls visible, balls stay inside the display.
#[test]
fn example_sprites_behaviour() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/sprites.lz80n");
    let src = std::fs::read_to_string(&path).unwrap();
    let asm = crate::compile_with(&src, "t.nex", true, path.parent().unwrap()).unwrap();
    let nex = crate::asm::assemble(&asm, path.parent().unwrap()).unwrap().nex.unwrap().1;
    let mut cpu = load_nex(&nex).unwrap();
    let pos = |a: &[u8; 5]| ((a[0] as u16) | ((a[2] as u16 & 1) << 8), (a[1] as u16) | ((a[4] as u16 & 1) << 8));
    let mut frames = 0;
    let mut first = None;
    let mut moved = false;
    for _ in 0..30_000_000 {
        cpu.tick().unwrap();
        if cpu.frame_count != frames {
            frames = cpu.frame_count;
            if frames > 2 {
                for n in 1..=8 {
                    let (x, y) = pos(&cpu.sprite_attr[n]);
                    assert!((32..=272 + 3).contains(&x) && (32..=208 + 4).contains(&y), "ball {n} at {x},{y}");
                }
                let p1 = pos(&cpu.sprite_attr[1]);
                match first {
                    None => first = Some(p1),
                    Some(f) if f != p1 => moved = true,
                    _ => {}
                }
            }
        }
    }
    assert!(frames > 100, "only {frames} frames");
    assert!(moved, "balls do not move");
    assert_eq!(cpu.sprite_attr[0][3], 0x80 | 0x40, "ship: visible, pattern 0");
    for n in 1..=8 {
        assert_eq!(cpu.sprite_attr[n][3], 0x80 | 0x40 | 1, "ball {n}: visible, pattern 1");
    }
    assert_eq!(cpu.patterns[256 + 5], 0xE3, "ball: transparent corner");
    assert_eq!(cpu.patterns[256 + 16 + 5], 0xB6, "ball: grey outline (160,160,160) -> RRRGGGBB");
}

/// Render the Next display (320x256): black background, tilemap text
/// layer (palette 0 of the tilemap, $E3 = transparent), sprites on top
/// (8-bit patterns, default palette = identity, X mirror and scale x1/x2).
fn render(cpu: &Cpu) -> Vec<u8> {
    let (w, h) = (320usize, 256usize);
    let mut img = vec![0u8; w * h * 4];
    let rgb = |c: u8| -> [u8; 3] {
        let r = ((c >> 5) & 7) as u16;
        let g = ((c >> 2) & 7) as u16;
        let b = (c & 3) as u16;
        [(r * 255 / 7) as u8, (g * 255 / 7) as u8, (b * 255 / 3) as u8]
    };
    let put = |img: &mut Vec<u8>, x: usize, y: usize, c: u8| {
        if x < w && y < h {
            let o = (y * w + x) * 4;
            img[o..o + 3].copy_from_slice(&rgb(c));
            img[o + 3] = 255;
        }
    };
    // display state per image row (raster effects); row 32 = line 0
    let state = |row: usize| -> (&[u8; 256], &[[u8; 256]; 8]) {
        let line = (row + crate::emu::LINES as usize - 32) % crate::emu::LINES as usize;
        match cpu.line_state.get(line) {
            Some(s) => (&s.0, &s.1),
            None => (&cpu.nextreg, &cpu.palettes),
        }
    };
    for (i, p) in img.chunks_mut(4).enumerate() {
        p[..3].copy_from_slice(&rgb(state(i / w).0[0x4A]));
        p[3] = 255;
    }
    if cpu.l2_port & 2 != 0 {
        // NR $70: 256x192 by rows, 320x256 / 640x256 by columns (640: the
        // left pixel of each pair is shown); NR $43 bit 2: second palette
        let l2pal = |row: usize| if state(row).0[0x43] & 4 != 0 { 5 } else { 1 };
        let base = cpu.nextreg[0x12] as usize * 2;
        let byte = |page: usize, off: usize| cpu.pages.get(&(page as u8)).map_or(0, |p| p[off]);
        match (cpu.nextreg[0x70] >> 4) & 3 {
            0 => {
                for y in 0..192 {
                    let pal = state(32 + y).1;
                    for x in 0..256 {
                        let c = pal[l2pal(32 + y)][cpu.l2_pixel(x, y) as usize];
                        if c != 0xE3 {
                            put(&mut img, 32 + x, 32 + y, c);
                        }
                    }
                }
            }
            mode => {
                for y in 0..256 {
                    let pal = state(y).1;
                    for x in 0..320 {
                        let mut v = byte(base + x / 32, (x % 32) * 256 + y);
                        if mode == 2 {
                            v >>= 4;
                        }
                        let c = pal[l2pal(y)][v as usize];
                        if c != 0xE3 {
                            put(&mut img, x, y, c);
                        }
                    }
                }
            }
        }
    }
    let map = 0x4000 + (cpu.nextreg[0x6E] as usize & 0x3F) * 256;
    let tiles = 0x4000 + (cpu.nextreg[0x6F] as usize & 0x3F) * 256;
    if cpu.nextreg[0x6B] & 0x80 != 0 {
        for row in 0..32 {
            for col in 0..40 {
                let ch = cpu.mem[map + (row * 40 + col) * 2] as usize;
                let attr = cpu.mem[map + (row * 40 + col) * 2 + 1];
                for py in 0..8 {
                    let bits = cpu.mem[tiles + ch * 8 + py];
                    for px in 0..8 {
                        let bit = (bits >> (7 - px)) & 1;
                        let idx = (attr & 0xFE) | bit;
                        let tpal = if cpu.nextreg[0x6B] & 0x10 != 0 { 7 } else { 3 };
                        let c = state(row * 8 + py).1[tpal][idx as usize];
                        if c != 0xE3 {
                            put(&mut img, col * 8 + px, row * 8 + py, c);
                        }
                    }
                }
            }
        }
    }
    if cpu.nextreg[0x15] & 1 != 0 {
        for n in 0..128 {
            let a = cpu.sprite_attr[n];
            if a[3] & 0x80 == 0 {
                continue;
            }
            let x = a[0] as usize | ((a[2] as usize & 1) << 8);
            let y = a[1] as usize | ((a[4] as usize & 1) << 8);
            let pat = (a[3] & 63) as usize;
            let scale = if a[4] & 0x18 != 0 { 2 } else { 1 };
            let mirror = a[2] & 8 != 0;
            for sy in 0..16 * scale {
                for sx in 0..16 * scale {
                    let (px, py) = (sx / scale, sy / scale);
                    let px = if mirror { 15 - px } else { px };
                    let c = cpu.patterns[pat * 256 + py * 16 + px];
                    if c != 0xE3 {
                        let spal = if state(y + sy).0[0x43] & 8 != 0 { 6 } else { 2 };
                        put(&mut img, x + sx, y + sy, state(y + sy).1[spal][c as usize]);
                    }
                }
            }
        }
    }
    crate::assets::encode_png(w as u32, h as u32, &img)
}

/// `EXAMPLE=name FRAMES=n cargo test render_example -- --ignored --nocapture`
/// writes target/<name>.png with the emulated display.
#[test]
#[ignore]
fn render_example() {
    let name = std::env::var("EXAMPLE").unwrap_or("text".into());
    let frames_wanted: u32 = std::env::var("FRAMES").ok().and_then(|f| f.parse().ok()).unwrap_or(50);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../examples/{name}.lz80n"));
    let src = std::fs::read_to_string(&path).unwrap();
    let (nex, syms) = build(&format!("render_{name}"), &src).unwrap();
    let mut cpu = load_nex(&nex).unwrap();
    let end = syms["__main_end"];
    // stop at the start of a frame, so line_state holds one complete frame
    for _ in 0..1_000_000_000u64 {
        cpu.tick().unwrap();
        if cpu.frame_count >= frames_wanted.max(2) || (cpu.pc == end + 1 && cpu.frame_count >= 2) {
            if cpu.line == crate::emu::FRAME_START_LINE {
                break;
            }
        }
    }
    let frames = cpu.frame_count;
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("target/{name}.png"));
    std::fs::write(&out, render(&cpu)).unwrap();
    println!("{frames} frames -> {}", out.display());
}

#[test]
fn run_widening_compound_mul() {
    // (6*40 + 16) overflows a byte; the outer *2 must be computed at word width
    expect(
        "r_widen_mul",
        "byte r = 6\nbyte c = 16\nword a = (r * 40 + c) * 2\nprint(a)\nbyte x = 200\nword b = (x + x) * 3\nprint(b)\nword d = x * 3\nprint(d)",
        "512\n1200\n600\n",
    );
}

// ------------------------------------------------------------------
// Layer 2

#[test]
fn run_layer2() {
    let src = r#"
layer2_visible(true)
layer2_clear(TRANSPARENT)
plot(0, 0, 0xFF)
plot(255, 191, rgb(7, 0, 0))
plot(10, 200, 0x1C)            -- off screen: ignored
hline(250, 40, 20, 0x03)       -- clipped at x = 255
fill_rect(100, 60, 10, 5, 0x1C)
rect(20, 100, 30, 20, 0xFC)
line(0, 150, 63, 181, 0xE0)    -- crosses an 8K page boundary (y 159/160)
print(point(0, 0), point(255, 191), point(1, 0), point(10, 200))
border(BLUE)
"#;
    if let Some((out, cpu)) = run_with("r_l2", src, |_| {}) {
        assert_eq!(out, "255 224 227 0\n");
        assert_eq!(cpu.l2_port, 2);
        assert_eq!(cpu.l2_pixel(255, 191), 0xE0);
        assert_eq!(cpu.l2_pixel(249, 40), 0xE3);
        assert!((250..256).all(|x| cpu.l2_pixel(x, 40) == 0x03));
        assert_eq!(cpu.l2_pixel(0, 41), 0xE3, "hline must not wrap to the next row");
        assert!((100..110).all(|x| (60..65).all(|y| cpu.l2_pixel(x, y) == 0x1C)));
        assert_eq!(cpu.l2_pixel(110, 60), 0xE3);
        assert_eq!(cpu.l2_pixel(20, 100), 0xFC);
        assert_eq!(cpu.l2_pixel(49, 119), 0xFC);
        assert_eq!(cpu.l2_pixel(30, 110), 0xE3, "rect is an outline");
        assert_eq!(cpu.l2_pixel(0, 150), 0xE0);
        assert_eq!(cpu.l2_pixel(63, 181), 0xE0);
        let on_line = (0..64).filter(|&x| (150..182).any(|y| cpu.l2_pixel(x, y) == 0xE0)).count();
        assert_eq!(on_line, 64, "one pixel per column for a shallow line");
        assert_eq!(cpu.nextreg[0x4A], 0x02, "border(BLUE) -> fallback colour");
        assert_eq!(cpu.nextreg[0x68], 0x80, "ULA off");
    }
}

// ------------------------------------------------------------------
// DMA and palettes

#[test]
fn run_dma_and_addr() {
    let src = r#"
byte src[8]
byte dst[8]
for i = 0, 7 do src[i] = i * 10 end
dma_copy(addr(src), addr(dst), 8)
print(dst[0], dst[3], dst[7])
dma_fill(addr(dst[2]), 4, 99)
print(dst[1], dst[2], dst[5], dst[6])
struct P
    word a
    byte b
end
P p
P q
p.a = 1234
p.b = 7
dma_copy(addr(p), addr(q), 3)
print(q.a, q.b, addr(q) - addr(p))
dma_copy(addr(src), addr(dst), 0)
"#;
    if let Some((out, cpu)) = run_with("r_dma", src, |_| {}) {
        assert_eq!(out, "0 30 70\n10 99 99 60\n1234 7 3\n");
        assert_eq!(cpu.dma_transfers, 3, "zero-length copy must not start the DMA");
    }
}

#[test]
fn run_palette_and_l2_clear() {
    let src = r#"
palette(PAL_LAYER2, 5, rgb(7, 0, 0))
palette9(PAL_SPRITES, 200, rgb9(7, 7, 7))
palette(PAL_TEXT, 2 * WHITE + 1, 0x1C)
layer2_clear(5)
print(point(0, 0), point(255, 191), rgb9(1, 2, 3))
"#;
    if let Some((out, cpu)) = run_with("r_pal", src, |_| {}) {
        assert_eq!(out, "5 5 83\n");
        assert_eq!(cpu.palettes[1][5], 0xE0);
        assert_eq!(cpu.palettes[2][200], 0xFF);
        assert_eq!(cpu.palettes[3][15], 0x1C);
        assert_eq!(cpu.l2_pixel(100, 100), 5);
        assert!(cpu.dma_transfers >= 6, "layer2_clear uses the DMA");
    }
}

// ------------------------------------------------------------------
// Interrupts, copper, practical built-ins

#[test]
fn run_raster_interrupts() {
    let src = r#"
byte hits = 0
byte ticks = 0
interrupt raster(0)
    nextreg[0x4A] = 0xE0
end
interrupt raster(100)
    nextreg[0x4A] = 0x1C
    hits += 1
end
interrupt frame
    ticks += 1
end
word f0 = frames()
pause(10)
print(frames() - f0, hits >= 9 and hits <= 11, ticks >= 9 and ticks <= 11)
"#;
    if let Some((out, cpu)) = run_with("r_raster", src, |_| {}) {
        assert_eq!(out, "10 true true\n");
        assert_eq!(cpu.nextreg[0x22] & 0b110, 0b110, "ULA interrupt off, line interrupt on");
        assert_eq!(cpu.line_state[50].0[0x4A], 0xE0);
        assert_eq!(cpu.line_state[150].0[0x4A], 0x1C);
    }
}

#[test]
fn run_frame_interrupt_without_raster() {
    let src = "byte n = 0\ninterrupt frame\n    n += 1\nend\npause(5)\nprint(n, frames())";
    if let Some((out, cpu)) = run_with("r_frame_irq", src, |_| {}) {
        assert_eq!(out, "5 5\n");
        assert_eq!(cpu.nextreg[0x22] & 2, 0, "no line interrupt needed");
    }
}

#[test]
fn interrupt_errors() {
    assert!(err("function f()\nend\nf()\ninterrupt frame\n    f()\nend").contains("called both"));
    assert!(err("interrupt raster(400)\nend").contains("0..311"));
    assert!(err("byte l = 3\ninterrupt raster(l)\nend").contains("constant"));
    assert!(err("interrupt frame\nend\ninterrupt frame\nend").contains("twice"));
    assert!(err("interrupt vblank\nend").contains("interrupt raster"));
    assert!(err("function f()\n    interrupt frame\n    end\nend").contains("top level"));
    // separate functions are fine; constants work as line numbers
    ok("const SPLIT = 96\nfunction g()\nend\ninterrupt raster(SPLIT)\n    g()\nend\nfunction h()\nend\nh()");
}

#[test]
fn run_copper() {
    let src = r#"
copper_begin()
copper_wait(0)
copper_move(0x4A, 0xE0)
copper_wait(100)
copper_move(0x4A, 0x03)
copper_start()
pause(3)
"#;
    if let Some((_, cpu)) = run_with("r_copper", src, |_| {}) {
        assert_eq!(cpu.nextreg[0x62] >> 6, 3);
        assert_eq!(&cpu.copper[..10], &[0x80, 0, 0x4A, 0xE0, 0x80, 100, 0x4A, 0x03, 0xFF, 0xFF]);
        assert_eq!(cpu.line_state[50].0[0x4A], 0xE0);
        assert_eq!(cpu.line_state[150].0[0x4A], 0x03);
    }
}

#[test]
fn run_practical_builtins() {
    let src = r#"
cpu_speed(MHZ_7)
seed(1234)
byte bucket[10]
byte bad = 0
for i = 1, 500 do
    word r = random(10)
    if r >= 10 then bad += 1 else bucket[r] += 1 end
end
byte empty = 0
for i = 0, 9 do
    if bucket[i] < 20 then empty += 1 end
end
print(bad, empty)
seed(1234)
word a = random(0)
seed(1234)
print(a == random(0), any_key())
"#;
    if let Some((out, cpu)) = run_with("r_practical", src, |c| c.keys[0] = 0x1E) {
        assert_eq!(out, "0 0\ntrue true\n");
        assert_eq!(cpu.nextreg[0x07], 1);
    }
    expect("r_nokey", "print(any_key())", "false\n");
}

// ------------------------------------------------------------------
// fixed point and math

#[test]
fn run_fixed_arith() {
    expect(
        "r_fixed",
        r#"
fixed a = 1.5
fixed b = -0.25
const G = 0.125
print(a, b, G, a + b, a - b, a * b, a / 4, a * 3)
print(a * a, a / b, fixed(7) / 2.0, -a, 2 * a)
byte n = 3
sword s = -2
print(a * n, a / n, fixed(n) + a, fixed(s), sword(b), sword(a), round(a), round(b), byte(fixed(200 / 2)))
print(a > b, a == 1.5, b < 0, a >= 2, 0.1, 100.0 + 27.5)
fixed v = 0
for i = 1, 10 do v += 0.1 end
print(v)
v *= 2
v -= 1
print(v, 1.0 / 3.0)
"#,
        "1.5 -0.25 0.13 1.25 1.75 -0.38 0.38 4.5\n2.25 -6 3.5 -1.5 3\n4.5 0.5 4.5 -2 -1 1 2 0 100\ntrue true true false 0.1 127.5\n1.02\n1.03 0.33\n",
    );
}

#[test]
fn run_math_functions() {
    expect(
        "r_math",
        r#"
sbyte sb = -5
sword sw = -300
fixed f = -2.5
print(abs(sb), abs(sw), abs(f), abs(-7), abs(byte(9)))
print(min(3, 9), max(sw, 10), min(f, 1.0), max(f, -3), min(sb, sb), clamp(sw, -100, 100), clamp(f, -1, 1))
word big = 50000
word w2 = 60000
print(max(big, w2), min(big, w2), max(sw, sb))
print(sqrt(word(10000)), sqrt(word(2)), sqrt(word(65535)), sqrt(byte(0)), sqrt(2.0), sqrt(100.0), sqrt(0.25), sqrt(f))
print(sin(0), sin(64), sin(128), sin(192), cos(0), cos(128), sin(32))
print(atan2(0, 10), atan2(10, 0), atan2(0, -10), atan2(-10, 0), atan2(10, 10), atan2(-5, -5), atan2(0, 0), atan2(1.0, 1.0))
print(atan2(100, 3000), atan2(-30000, 20000))
"#,
        "5 300 2.5 7 9\n3 10 -2.5 -2.5 -5 -100 -1\n60000 50000 -5\n100 1 255 0 1.41 10 0.5 0\n0 1 0 -1 1 -1 0.71\n0 64 128 192 32 160 0 32\n1 216\n",
    );
}

#[test]
fn fixed_errors() {
    assert!(err("fixed a = 1.5\nbyte b = 2\nprint(a + b)").contains("fixed(x)"));
    assert!(err("fixed a = 1.5\nsword s = a").contains("found `fixed`"));
    assert!(err("fixed a = 200.0").contains("does not fit"));
    assert!(err("fixed a = 1.5\nprint(a & 1)").contains("bitwise"));
    assert!(err("fixed a = 1.5\nbyte n = 2\nprint(n / a)").contains("fixed(x)"));
    assert!(err("fixed a = 1.5 / 0").contains("division by zero"));
    assert!(err("function abs(byte x) -> byte\n    return x\nend").contains("reserved"));
    ok("struct P\n    fixed x\n    fixed y\nend\nP p\np.x = 1.25\nfixed arr[4]\narr[2] = p.x * 2\nfunction half(fixed v) -> fixed\n    return v / 2\nend\nprint(half(arr[2]))");
}

#[test]
fn run_layer2_double_buffer() {
    let src = r#"
layer2_visible(true)
layer2_clear(1)          -- visible buffer (bank 8)
layer2_flip()            -- show bank 8, draw into bank 11
layer2_clear(2)
plot(5, 5, 9)
print(point(5, 5), point(0, 0))
layer2_flip()            -- show bank 11, draw into bank 8
print(point(0, 0))
"#;
    if let Some((out, cpu)) = run_with("r_l2_flip", src, |_| {}) {
        assert_eq!(out, "9 2
1
");
        assert_eq!(cpu.nextreg[0x12], 11);
        assert_eq!(cpu.l2_pixel(5, 5), 9, "the drawn buffer is now visible");
        assert_eq!(cpu.l2_pixel(0, 0), 2);
    }
}

// ------------------------------------------------------------------
// strings

#[test]
fn run_strings_basic() {
    expect(
        "r_str_basic",
        r#"
string name = "Next"
string s
print(len(s), len(name), name)
s = "Hello, " .. name .. "!"
print(s, len(s))
s = s .. " " .. 42 .. " " .. -7 .. " " .. 1.5 .. " " .. (3 > 2)
print(s)
byte n = 200
sword m = -1234
print("n=" .. n .. " m=" .. m)
print(str(word(n) * 2), str(m), str(name), chr(65) .. chr(66))
print(sub("abcdef", 1, 3), sub("abcdef", 4, 10), sub("abc", 5, 2) == "", len(sub("abc", 0, 0)))
print(find("hello world", "world"), find("hello", "x"), find("abc", ""), find("ab", "abc"))
print(val("123"), val("-45"), val("7up"), val(""), val(str(m)))
print(asc("ABC", 0), asc("ABC", 2), asc("ABC", 3))
print(upper("Hello, World 1"), lower("MiXeD"))
"#,
        "0 4 Next\nHello, Next! 12\nHello, Next! 42 -7 1.5 true\nn=200 m=-1234\n400 -1234 Next AB\nbcd ef true 0\n6 255 0 255\n123 -45 7 0 -1234\n65 67 0\nHELLO, WORLD 1 mixed\n",
    );
}

#[test]
fn run_strings_compare_and_self_reference() {
    expect(
        "r_str_cmp",
        r#"
string a = "apple"
string b = "banana"
print(a == "apple", a ~= b, a < b, b < a)
print("ab" < "abc", "abc" < "ab", a <= "apple", b >= a)
a = "x" .. a                -- the right side is built before a is overwritten
print(a)
a = a .. a
print(a)
string s = ""
for i = 1, 30 do s = s .. "ab" end
print(len(s))               -- capped at 64
s = s .. "more"
print(len(s), sub(s, 60, 10))
"#,
        "true true true false\ntrue false true true\nxapple\nxapplexapple\n60\n64 more\n",
    );
}

#[test]
fn run_strings_functions_structs_arrays() {
    expect(
        "r_str_fn",
        r#"
function greet(string who, byte times) -> string
    string r = ""
    for i = 1, times do
        r = r .. "hi " .. who .. " "
    end
    return r
end

function shout(string s)
    s = upper(s)            -- string parameters are references
end

struct Player
    string name
    word score
end

class Board
    string title
    function caption() -> string
        return "[" .. self.title .. "]"
    end
end

print(greet("Bob", 2))
print(greet("A", 1) .. greet("B", 1))
string t = "quiet"
shout(t)
print(t)
Player p
p.name = "Ann"
p.score = 1200
Player q
q = p
q.name = q.name .. "ie"
print(p.name, q.name, q.score)
string list[3]
list[0] = "zero"
list[2] = "two"
byte k = 2
print(list[0], len(list[1]), list[k], list[k] .. list[0])
Board bd
bd.title = "Top"
print(bd.caption(), len(bd.caption()))
print(find(greet("x", 1), "x"))
"#,
        "hi Bob hi Bob \nhi A hi B \nQUIET\nAnn Annie 1200\nzero 0 two twozero\n[Top] 5\n3\n",
    );
}

#[test]
fn string_errors() {
    assert!(err("string s = \"a\"\nbyte b = s").contains("string cannot be used as a number"));
    assert!(err("string s = \"a\"\nprint(s == 1)").contains("cannot compare a string"));
    assert!(err("string s = 5").contains("expected a string"));
    assert!(err("byte b = len(5)").contains("expected a string"));
    assert!(err("str(5)").contains("not used"));
    expect("r_notfound", "print(find(\"abc\", \"z\") == NOT_FOUND)", "true
");
    let long = format!("string s = \"{}\"", "x".repeat(256));
    assert!(err(&long).contains("longer than 255"));
    // a long literal is truncated to 64 characters when stored
    let src = format!("const L = \"{}\"\nstring s = L\nprint(len(L), len(s), len(s .. L), find(L, \"y\"))", "x".repeat(100) + "y");
    expect("r_long_lit", &src, "101 64 64 100\n");
    assert!(err("function f() -> string\n    return\nend").contains("missing return value"));
}

// ------------------------------------------------------------------
// sound

#[test]
fn char_literals() {
    expect("r_chars", "print('A', 'a' - 'A', '\\n', '0' + 7)\nbyte c = 'x'\nprint(chr(c) .. chr('!'))", "65 32 10 55\nx!\n");
    assert!(err("byte c = 'ab'").contains("one character"));
}

#[test]
fn run_sound_direct() {
    let src = r#"
tone(0, 500)
volume(0, 15)
noise(4, 20)
volume(4, 9)
tone(8, note_period(57))    -- A4
print(note_period(57), note_period(69), note_period(0), playing(0))
"#;
    if let Some((out, cpu)) = run_with("r_snd_direct", src, |_| {}) {
        assert_eq!(out, "249 124 4095 false\n");
        assert_eq!(cpu.nextreg[0x08] & 2, 2, "TurboSound enabled");
        assert_eq!((cpu.ay[0][0], cpu.ay[0][1], cpu.ay[0][8]), (244u8, 1u8, 15u8));
        assert_eq!(cpu.ay[0][7], 0x3F & !1, "chip 0: tone A on");
        assert_eq!(cpu.ay[1][6], 20);
        assert_eq!(cpu.ay[1][9], 9);
        assert_eq!(cpu.ay[1][7], 0x3F & !(8 << 1), "chip 1: noise B on");
        assert_eq!((cpu.ay[2][4], cpu.ay[2][5]), (249, 0));
        assert_eq!(cpu.ay[2][7], 0x3F & !(1 << 2));
    }
}

/// Run frame by frame and record AY register snapshots after each frame.
fn sound_frames(name: &str, src: &str, frames: u32) -> Vec<[[u8; 16]; 3]> {
    let Some((nex, _)) = build(name, src) else { return vec![] };
    let mut cpu = load_nex(&nex).unwrap();
    let mut shots = Vec::new();
    let mut last = 0;
    for _ in 0..200_000_000u64 {
        cpu.tick().unwrap();
        if cpu.frame_count != last {
            last = cpu.frame_count;
            shots.push(cpu.ay);
            if last >= frames {
                break;
            }
        }
    }
    shots
}

#[test]
fn run_sound_play_and_beep() {
    let src = r#"
play(1, "o4 l4 c e8 r2 >c#")
beep(3, 69, 3)             -- A5 on chip 1, voice A
while true do vsync() end
"#;
    let shots = sound_frames("r_snd_play", src, 30);
    if shots.is_empty() {
        return;
    }
    let period = |s: &[[u8; 16]; 3], chip: usize, v: usize| s[chip][v * 2] as u16 | (s[chip][v * 2 + 1] as u16) << 8;
    let c4 = 1_750_000.0 / (16.0 * 440.0 * 2f64.powf((48.0 - 57.0) / 12.0));
    let e4 = 1_750_000.0 / (16.0 * 440.0 * 2f64.powf((52.0 - 57.0) / 12.0));
    let cs5 = 1_750_000.0 / (16.0 * 440.0 * 2f64.powf((61.0 - 57.0) / 12.0));
    // the note sequence on channel 1 (chip 0, voice B), one entry per frame
    let seq: Vec<(u16, u8)> = shots.iter().map(|s| (period(s, 0, 1), s[0][9])).collect();
    let first_c = seq.iter().position(|&(p, v)| p == c4.round() as u16 && v == 12).expect("c4 plays");
    // c: 4 frames (last one silent), e: 8 frames, rest: 2 frames, c#5
    let at = |k: usize| seq[first_c + k];
    assert_eq!(at(2).1, 12);
    assert_eq!(at(3).1, 0, "gap before the next note");
    assert_eq!(at(4).0, e4.round() as u16);
    assert_eq!(at(4).1, 12);
    assert_eq!(at(12).1, 0, "rest");
    assert_eq!(at(14).0, cs5.round() as u16);
    assert_eq!(at(14).1, 12);
    let end = seq.iter().rposition(|&(_, v)| v != 0).unwrap();
    assert!(seq[end + 1..].iter().all(|&(_, v)| v == 0), "silent after the tune");
    // beep: chip 1 voice A, A5 for 3 frames, then silent
    let beep: Vec<u8> = shots.iter().map(|s| s[1][8]).collect();
    assert_eq!(beep.iter().filter(|&&v| v == 12).count(), 3, "beep lasts 3 frames: {beep:?}");
    assert_eq!(*beep.last().unwrap(), 0);
}

#[test]
fn sound_errors() {
    // the driver's helpers must not be called from the main program
    assert!(err("play(0, \"c\")\nsound_tick()").contains("called both"));
}

/// The sound example: background music on channels 0 and 1, key 1 plays a
/// beep on channel 3 (chip 1), SPACE sweeps a laser on channel 6 (chip 2).
#[test]
fn example_sound_behaviour() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/sound.lz80n");
    let src = std::fs::read_to_string(&path).unwrap();
    let Some((nex, _)) = build("ex_sound", &src) else { return };
    let mut cpu = load_nex(&nex).unwrap();
    let mut last = 0;
    let (mut melody_notes, mut bass_on, mut beep_on, mut laser_periods) = (HashSet::new(), false, false, Vec::new());
    for _ in 0..300_000_000u64 {
        cpu.tick().unwrap();
        if cpu.frame_count == last {
            continue;
        }
        last = cpu.frame_count;
        match last {
            30 => cpu.keys[3] = 0x1E,  // key 1 down
            36 => cpu.keys[3] = 0x1F,
            60 => cpu.keys[7] = 0x1E,  // SPACE down
            64 => cpu.keys[7] = 0x1F,
            _ => {}
        }
        if cpu.ay[0][8] != 0 {
            melody_notes.insert(cpu.ay[0][0] as u16 | (cpu.ay[0][1] as u16) << 8);
        }
        bass_on |= cpu.ay[0][9] != 0;
        beep_on |= (30..50).contains(&last) && cpu.ay[1][8] == 12;
        if (60..80).contains(&last) && cpu.ay[2][8] != 0 {
            laser_periods.push(cpu.ay[2][0] as u16 | (cpu.ay[2][1] as u16) << 8);
        }
        if last >= 200 {
            break;
        }
    }
    assert!(melody_notes.len() >= 4, "melody plays several notes: {melody_notes:?}");
    assert!(bass_on, "bass plays");
    assert!(beep_on, "key 1 beeps on channel 3");
    assert!(laser_periods.len() >= 10 && laser_periods.windows(2).all(|w| w[1] > w[0]), "laser sweeps: {laser_periods:?}");
}

/// benchmarks/benchmark.lz80n: correct results, and the per-run time of each
/// task from the emulator's T-state count (28 MHz, no wait states).
/// `cargo test benchmark -- --nocapture` prints the table.
#[test]
fn benchmark_lz80n() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../benchmarks/benchmark.lz80n");
    let src = std::fs::read_to_string(&path).unwrap();
    let Some((nex, syms)) = build("bench", &src) else { return };
    let mut cpu = load_nex(&nex).unwrap();
    let end = syms["__main_end"];
    for _ in 0..2_000_000_000u64 {
        if cpu.pc == end {
            break;
        }
        cpu.tick().unwrap();
    }
    assert_eq!(cpu.pc, end, "benchmark finished");
    let names = ["loop", "arith", "sieve", "print", "string", "sqrt", "plot"];
    let mut start = 0;
    let mut times = Vec::new();
    for &(v, t) in &cpu.marks {
        if v != 0 {
            start = t;
        } else {
            times.push((t - start) as f64 / 10.0 / 28_000.0); // ms per run
        }
    }
    assert_eq!(times.len(), 7);
    for (n, ms) in names.iter().zip(&times) {
        println!("{n:8} {ms:8.2} ms");
    }
    let text = screen_text(&cpu);
    assert!(text.contains("arith 500  primes 303  sqrt 20615"), "{text}");
}

#[test]
fn run_division_cases() {
    // byte dividends, large divisors (>= $8000), signed, by-constant multiply
    let src = r#"
word a = 200
word b = 7
print(a / b, a % b, word(65535) / 3, word(65535) % 1000, word(40000) / 40001, word(50000) / 40000, word(50000) % 40000)
print(word(12345) / 1, word(12345) / 256, word(100) / 300, word(9) % 10)
sword n = -1000
print(n / 7, n % 7, n / -7)
word m = 1234
print(m * 7, m * 10, m * 100, m * 255, m * 1000)
byte z = 0
print(m / z, m % z)
"#;
    expect("r_div", src, "28 4 21845 535 0 1 10000\n12345 48 0 9\n-142 -6 142\n8638 12340 57864 52526 54352\n65535 1234\n");
}

#[test]
fn run_for_loop_forms() {
    let src = r#"
word c = 0
for i = 0, 255 do c += 1 end
for i = 65530, 65535 do c += 1 end
for i = 5, 1, -1 do c += 1 end
for i = 3, 2 do c += 100 end
byte hi = 4
for i = 1, hi do c += 1 end
hi = 0
for i = 1, hi do c += 100 end
for i = 10, 0, -2 do c += 1 end
word w = 300
for i = 299, w do c += 1 end
print(c)
"#;
    expect("r_for_forms", src, &format!("{}\n", 256 + 6 + 5 + 4 + 6 + 2));
}

#[test]
fn run_division_by_constants() {
    // every unsigned constant divisor path: powers of two, 16- and 17-bit magic
    let mut src = String::from("word x = 0\nbyte b = 0\nword s = 0\n");
    let divs = [1u32, 2, 3, 7, 10, 16, 100, 255, 641, 1000, 4096, 32767, 40000, 65535];
    let xs = [0u32, 1, 9, 99, 255, 256, 999, 1000, 12345, 32768, 39999, 40000, 65534, 65535];
    let mut want = 0u32;
    for &x in &xs {
        src.push_str(&format!("x = {x}\n"));
        for &d in &divs {
            src.push_str(&format!("s += x / {d} + x % {d}\n"));
            want = (want + x / d + x % d) & 0xFFFF;
        }
    }
    let mut bwant = 0u32;
    for x in [0u32, 1, 7, 100, 200, 255] {
        src.push_str(&format!("b = {x}\n"));
        for d in [1u32, 3, 5, 8, 10, 100, 255] {
            src.push_str(&format!("s += b / {d} + b % {d}\n"));
            bwant = (bwant + x / d + x % d) & 0xFFFF;
        }
    }
    src.push_str("print(s)\n");
    expect("r_div_const", &src, &format!("{}\n", (want + bwant) & 0xFFFF));
}

#[test]
fn run_print_fast_path() {
    // wrapping at column 40, newlines inside a string, unprintable characters
    let src = r#"
at(0, 35)
write("abcdefgh")
write("x\ny")
print("")
print(str(5) .. "!")
at(5, 0)
write("q\tz")
"#;
    if let Some((_, cpu)) = run_with("r_print_fast", src, |_| {}) {
        let t = screen_text(&cpu);
        let lines: Vec<&str> = t.lines().collect();
        assert!(lines[0].ends_with("abcde"), "{t}");
        assert!(lines[1].starts_with("fghx"), "{t}");
        assert!(lines[2].starts_with('y'), "{t}");
        assert!(lines[3].starts_with("5!"), "{t}");
        assert!(lines[5].starts_with("q?z"), "{t}");
    }
}

// ------------------------------------------------------------------
// array initializers, compound assignment

#[test]
fn run_array_initializers() {
    let src = r#"
const N = 3
byte primes[] = {2, 3, 5, 7, 11}
word big[4] = {1000, 65535, N * 100}      -- the rest is 0
sbyte delta[] = {-1, 0, 1}
fixed speed[] = {0.5, 1, -1.25}
bool solid[] = {true, false, true}
byte letters[] = {'L', 'Z', 'X'}
string names[] = {"Ada", "Bob", "Cy"}
print(primes[4], big[0], big[1], big[2], big[3], delta[0], delta[2])
print(speed[0], speed[1], speed[2], solid[0], solid[1], letters[1], names[1], len(names[2]))

function local_table(byte i) -> byte
    byte t[] = {10, 20, 30}
    t[0] += 1                         -- changed, but restored on the next call
    return t[i]
end
print(local_table(0), local_table(0), local_table(2))

function local_names() -> string
    string n[2] = {"hi"}
    return n[0] .. "/" .. n[1] .. "/" .. str(len(n[1]))
end
print(local_names())
"#;
    expect(
        "r_arrinit",
        src,
        "11 1000 65535 300 0 -1 1\n0.5 1 -1.25 true false 90 Bob 2\n11 11 30\nhi//0\n",
    );
}

#[test]
fn run_compound_assignments() {
    let src = r#"
word w = 1000
w /= 7
print(w)
w %= 10
print(w)
byte b = 3
b <<= 4
print(b)
b >>= 2
print(b)
sword s = -100
s /= 3
s %= 7
print(s)
byte a[3] = {100, 50, 9}
a[1] /= 5
a[2] <<= 1
print(a[1], a[2])
"#;
    expect("r_compound", src, "142\n2\n48\n12\n-5\n10 18\n");
}

#[test]
fn array_initializer_errors() {
    assert!(err("byte t[2] = {1, 2, 3}").contains("3 values for an array of 2"));
    assert!(err("byte t[] = {1, 300}").contains("constants that fit"));
    assert!(err("byte x = 5\nbyte t[] = {x}").contains("constants that fit"));
    assert!(err("byte t[]").contains("needs an initializer list"));
    assert!(err("byte t = {1}").contains("needs an array"));
    assert!(err("byte t[3] = 5").contains("initialised with a list"));
    assert!(err("var t[] = {1}").len() > 0);
    assert!(err("struct P\n byte x\nend\nP ps[] = {1}").contains("`{...}` list"));
    assert!(err("string s[] = {5}").contains("string constants"));
    assert!(err("print({1, 2})").len() > 0);
}

#[test]
fn library_names_are_reserved() {
    // reserved even when the module is not used by the program
    assert!(err("byte tone = 3").contains("standard library: sound"));
    assert!(err("function line(byte a)\nend").contains("standard library: Layer 2"));
    assert!(err("byte snd_x = 1").contains("sound"));
    assert!(err("for rgb = 1, 2 do end").contains("Layer 2"));
    assert!(err("function f(byte volume)\nend").contains("sound"));
    assert!(err("struct Sprite\n byte x\nend").contains("sprite"));
    // field and method names are scoped to their class
    expect("r_lib_methods", "class Box\n byte line\n function update() -> byte\n return self.line + 1\n end\nend\nBox b\nb.line = 4\nprint(b.update())", "5\n");
}

#[test]
fn run_circles_and_ellipses() {
    let src = r#"
fill_circle(100, 100, 10, 7)
print(point(100, 100), point(110, 100), point(111, 100), point(100, 90), point(100, 89), point(90, 100))
circle(50, 50, 20, 9)
print(point(70, 50), point(30, 50), point(50, 30), point(50, 70), point(50, 50), point(64, 64) + point(65, 64) + point(64, 65))
ellipse(128, 150, 40, 20, 5)
print(point(168, 150), point(88, 150), point(128, 130), point(128, 170), point(128, 150), point(169, 150))
fill_ellipse(200, 40, 30, 8, 3)
print(point(230, 40), point(231, 40), point(200, 48), point(200, 49), point(215, 44))
fill_circle(-5, 5, 20, 4)              -- clipped at the left and top
fill_circle(250, 188, 12, 6)           -- clipped at the right and bottom
print(point(0, 5), point(14, 5), point(250, 191), point(255, 188))
"#;
    expect("r_circles", src, "7 7 0 7 0 7\n9 9 9 9 0 9\n5 5 5 5 0 0\n3 0 3 0 3\n4 4 6 6\n");
}

/// The outline of a circle has no gaps: every pixel has a neighbour.
#[test]
fn circle_outline_is_closed() {
    // one shape per run: where two outlines cross, one overwrites the other
    let shapes = [
        "circle(128, 96, 60, 1)",
        "ellipse(128, 96, 100, 30, 1)",
        "ellipse(128, 96, 20, 90, 1)",
        "circle(20, 20, 3, 1)",
    ];
    for (i, src) in shapes.iter().enumerate() {
        let Some((_, cpu)) = run_with(&format!("r_circle_closed{i}"), src, |_| {}) else { return };
        let px: Vec<(i32, i32)> = (0..192)
            .flat_map(|y| (0..256).map(move |x| (x, y)))
            .filter(|&(x, y)| cpu.l2_pixel(x as usize, y as usize) == 1)
            .collect();
        assert!(px.len() > 10, "{src}: {} pixels", px.len());
        let set: HashSet<_> = px.iter().copied().collect();
        for &(x, y) in &px {
            let n = (-1..=1)
                .flat_map(|dx| (-1..=1).map(move |dy| (dx, dy)))
                .filter(|&(dx, dy)| (dx, dy) != (0, 0) && set.contains(&(x + dx, y + dy)))
                .count();
            assert!(n >= 2, "{src}: pixel {x},{y} has {n} neighbours");
        }
    }
}

#[test]
fn run_multi_dim_arrays() {
    let src = r#"
byte grid[3][4]
for y = 0, 2 do
    for x = 0, 3 do
        grid[y][x] = y * 10 + x
    end
end
print(grid[0][0], grid[1][2], grid[2][3])
byte r = 2
byte c = 1
grid[r][c] += 100
print(grid[r][c], grid[2][0], grid[1][3])
word big[2][3] = {{1000, 2000, 3000}, {4000, 5000}}
print(big[0][2], big[1][1], big[1][2])
byte flat[2][3] = {1, 2, 3, 4, 5, 6}
print(flat[1][0], flat[0][2])
string names[][2] = {{"a", "b"}, {"c", "d"}, {"e"}}
print(names[2][0], names[1][1], len(names[2][1]))
sbyte cube[2][2][2] = {{{1, -2}, {3, 4}}, {{5, 6}, {7, -8}}}
print(cube[0][0][1], cube[1][1][1], cube[1][0][0])
struct P
    byte x
    byte y
end
P pts[2][3]
pts[1][2].x = 7
pts[1][2].y = 9
print(pts[1][2].x + pts[1][2].y, pts[0][0].x)
function local_t() -> byte
    byte t[2][2] = {{1, 2}, {3, 4}}
    t[1][1] += 1
    return t[1][1] * 10 + t[0][1]
end
print(local_t(), local_t())
struct Map
    byte cells[2][5]
    byte w
end
Map m
m.cells[1][4] = 42
m.w = 5
print(m.cells[1][4], m.w)
"#;
    expect("r_multidim", src, "0 12 23\n121 20 13\n3000 5000 0\n4 3\ne d 0\n-2 -8 5\n16 0\n52 52\n42 5\n");
}

#[test]
fn multi_dim_errors() {
    assert!(err("byte g[2][3]\ng[0][3] = 1").contains("out of bounds"));
    assert!(err("byte g[2][3]\ng[2][0] = 1").contains("out of bounds"));
    assert!(err("byte g[2][3] = {{1, 2, 3, 4}}").contains("row of 3"));
    assert!(err("byte g[2][] = {1}").contains("only the first size"));
    assert!(err("byte g[2][3]\nprint(g[1])").contains("array used without an index"));
}

/// A 100x90 test picture: pixel = (x % 8) << 5 | (y % 8) << 2; (1,1) transparent.
fn image_pixel(x: usize, y: usize) -> u8 {
    if (x, y) == (1, 1) { 0xE3 } else { ((x % 8) << 5 | (y % 8) << 2) as u8 }
}

#[test]
fn run_draw_image() {
    let (w, h) = (100usize, 90usize);
    let mut rgba = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let a = if (x, y) == (1, 1) { 0 } else { 255 };
            rgba.extend([((x % 8) << 5) as u8, ((y % 8) << 5) as u8, 0, a]);
        }
    }
    let dir = std::env::temp_dir().join("lz80nc_assets");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("pic.png");
    std::fs::write(&path, crate::assets::encode_png(w as u32, h as u32, &rgba)).unwrap();
    let src = format!(
        "asset pic = image(\"{}\")\nprint(image_width(pic), image_height(pic))\n\
         draw_image(pic, -10, 120)\ndraw_image(pic, 200, -5)\ndraw_image(pic, 300, 0)\ndraw_image(pic, 0, -200)",
        path.display().to_string().replace('\\', "/")
    );
    let Some((out, cpu)) = run_with("r_draw_image", &src, |_| {}) else { return };
    assert_eq!(out, "100 90\n");
    for sy in 0..192i32 {
        for sx in 0..256i32 {
            let mut want = 0;
            for (ox, oy) in [(-10, 120), (200, -5)] {
                let (ix, iy) = (sx - ox, sy - oy);
                if (0..w as i32).contains(&ix) && (0..h as i32).contains(&iy) {
                    let p = image_pixel(ix as usize, iy as usize);
                    if p != 0xE3 {
                        want = p; // transparent pixels leave the screen as it was
                    }
                }
            }
            assert_eq!(cpu.l2_pixel(sx as usize, sy as usize), want, "pixel {sx},{sy}");
        }
    }
    assert!(err("byte n = 1\ndraw_image(n, 0, 0)").contains("image asset"));
}

#[test]
fn run_triangles() {
    let src = r#"
fill_triangle(10, 10, 60, 10, 10, 60, 5)
print(point(10, 10), point(60, 10), point(10, 60), point(30, 30), point(40, 40), point(61, 10))
fill_triangle(200, 100, 150, 180, 250, 150, 6)    -- any corner order
print(point(200, 100), point(200, 150), point(150, 180), point(250, 150), point(150, 100))
fill_triangle(-20, 100, 40, -30, 40, 230, 7)      -- clipped
print(point(0, 100), point(39, 0), point(39, 191), point(30, 100))
triangle(100, 20, 140, 20, 120, 60, 8)
print(point(100, 20), point(120, 20), point(120, 60), point(110, 40), point(120, 30))
"#;
    expect("r_triangles", src, "5 5 5 5 0 0\n6 6 6 6 0\n7 7 7 7\n8 8 8 8 0\n");
}

/// NextZXOS leaves the Layer 2 bank at 9 (NR $12), overlapping the shadow
/// bank 11: the runtime must set both buffers itself.
#[test]
fn layer2_buffers_do_not_overlap_under_nextzxos() {
    // frame 1: only the bottom third; frame 2 (other buffer): only the middle.
    // With overlapping buffers frame 1's bottom lands in frame 2's top third.
    let src = "fill_rect(0, 128, 256, 64, 3)\nlayer2_flip()\nfill_rect(0, 64, 256, 64, 2)\nlayer2_flip()";
    let Some((_, cpu)) = run_with("r_l2_nextzxos", src, |c| {
        c.nextreg[0x12] = 9;
        c.nextreg[0x13] = 11;
    }) else { return };
    assert_eq!(cpu.l2_pixel(10, 100), 2);
    assert_eq!(cpu.l2_pixel(10, 10), 0, "top third shows another frame");
    assert_eq!(cpu.l2_pixel(10, 150), 0);
}

#[test]
fn run_far_arrays() {
    let src = r#"
far byte big[30000]
far word w[10000]                       -- 20000 bytes: 3 pages
far sbyte grid[100][100]
far fixed speeds[] = {0.5, -1.25, 3}
far byte table[4][3] = {{1, 2, 3}, {4, 5, 6}}
for i = 0, 29999 do big[i] = byte(i & 255) end
word sum = 0
for i = 0, 29999 do sum += big[i] end
print(sum, big[8191], big[8192], big[29999])
w[0] = 1234
w[4095] = 4095
w[4096] = 4096                          -- first word of page 2
w[9999] = 65535
print(w[0], w[4095], w[4096], w[9999], w[5])
grid[99][99] = -7
grid[50][3] = 12
grid[50][3] += grid[99][99]
print(grid[50][3], grid[99][99], grid[0][0])
print(speeds[1], speeds[2], table[1][2], table[3][0])
byte j = 2
table[j][j] = big[j * 100] + table[1][1]   -- one far element into another
print(table[2][2])
"#;
    expect("r_far", src, "18920 255 0 47\n1234 4095 4096 65535 0\n5 -7 0\n-1.25 3 6 0\n205\n");
}

#[test]
fn far_array_pages_survive_interrupts() {
    // the frame handler reads another far array while the main loop works
    let src = r#"
far byte a[20000]
far byte b[20000]
byte seen = 0
interrupt frame
    seen = b[19999]
end
b[19999] = 9
for i = 0, 19999 do a[i] = 1 end
word s = 0
for i = 0, 19999 do s += a[i] end
print(s, seen)
"#;
    expect("r_far_irq", src, "20000 9\n");
}

#[test]
fn far_array_errors() {
    assert!(err("far byte a[10]\nprint(addr(a[1]))").contains("only be read or assigned"));
    assert!(err("far string s[3]").contains("numbers or `bool`"));
    assert!(err("far byte a[70000]").contains("64 KB") || err("far byte a[70000]").contains("array"));
    assert!(err("far byte x = 3").contains("array declaration"));
    assert!(err("function f()\n far byte a[3]\nend").contains("top level"));
    assert!(err("far byte g[2][3]\nprint(g[1])").contains("without an index"));
    assert!(err("far byte a[10]\na[10] = 1").contains("out of bounds"));
}

/// Pages that are not in the .nex hold garbage on real hardware: the
/// zero pages of far arrays must be cleared at startup.
#[test]
fn far_zero_pages_are_cleared() {
    let src = "far byte z[20000]\nfar byte t[3] = {7, 8, 9}\nprint(z[0], z[12345], z[19999], t[1])";
    let out = run_with("r_far_zero", src, |c| {
        for p in 28..40u8 {
            c.pages.entry(p).or_insert_with(|| vec![0xAA; 8192]);
        }
    });
    if let Some((out, _)) = out {
        assert_eq!(out, "0 0 0 8\n");
    }
}


/// Every function in its own code page gives the same results.
#[test]
fn run_banked_code() {
    let src = r#"
struct P
    byte x
    word y
end
class Counter
    word n
    function add(word k) -> word
        self.n += k
        return self.n
    end
end
function twice(word v) -> word
    return v * 2
end
function is_big(word v) -> bool
    return v > 100
end
function name(byte k) -> string
    if k == 0 then return "zero" end
    return "n" .. str(k)
end
function fill(P p, byte v)
    p.x = v
    p.y = twice(v) + 1
end
function deep(byte k) -> word
    -- calls through three pages
    return twice(word(k)) + twice(twice(word(k)))
end
word ticks = 0
function on_frame()
    ticks += 1
end
interrupt frame
    on_frame()
end
Counter c
P p
fill(p, 21)
print(twice(21), is_big(50), is_big(500), name(0), name(7))
print(p.x, p.y, deep(5), c.add(3), c.add(4))
if is_big(twice(60)) then print("big") else print("small") end
byte i = 0
while not is_big(word(i) * 10) do i += 1 end
print(i)
vsync()
vsync()
print(ticks >= 2)
"#;
    let want = "42 false true zero n7
21 43 30 3 7
big
11
true
";
    expect("r_unbanked", src, want);
    let Some((out, cpu)) = run_opts("r_banked", src, true, |_| {}) else { return };
    assert_eq!(out, want);
    assert!(cpu.slot2 == 10, "the main program runs with bank 5 at $4000");
}

/// Banked versions of the examples assemble, and the size estimate is close.
#[test]
fn examples_assemble_banked_and_estimate_sizes() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("lz80n") {
            continue;
        }
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let src = std::fs::read_to_string(&path).unwrap();
        // the estimate never undercounts by much
        let asm = crate::compile_opts(&src, "t.nex", true, &dir, false).unwrap();
        if let Some((_, syms)) = build_opts(&format!("est_{name}"), &src, false) {
            let real = (syms["__program_end"] - 0x8000) as usize;
            let est = crate::codegen::resident_size(&asm);
            assert!(est + 64 >= real && est <= real + real / 8, "{name}: estimate {est}, real {real}");
        }
        build_opts(&format!("bank_{name}"), &src, true);
    }
}

/// A program far larger than the resident area: functions move into code
/// pages automatically and the program still runs.
#[test]
fn run_auto_banked_large_program() {
    let (nfun, nline) = (60u16, 60u16);
    let mut src = String::new();
    for f in 0..nfun {
        src += &format!("function f{f}(word v) -> word
");
        for j in 0..nline {
            src += &format!("    v = v * 3 + {}
", f * 7 + j);
        }
        src += "    return v
end
";
    }
    src += "word s = 1
";
    for f in 0..nfun {
        src += &format!("s = f{f}(s)
");
    }
    src += "print(s)
";
    let mut v: u16 = 1;
    for f in 0..nfun {
        for j in 0..nline {
            v = v.wrapping_mul(3).wrapping_add(f * 7 + j);
        }
    }
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let asm = crate::compile_with(&src, "t.nex", true, &base).unwrap();
    let pages = asm.matches("    MMU 2 e,").count();
    assert!(pages >= 2, "expected code pages, got {pages}");
    expect("r_auto_bank", &src, &format!("{v}
"));
}

/// Write `files` into a fresh temp folder and compile its `main.lz80n`.
fn include_compile(name: &str, files: &[(&str, &str)]) -> (PathBuf, Result<String, Vec<crate::diag::Diag>>) {
    let dir = std::env::temp_dir().join(format!("lz80nc_inc_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, text) in files {
        let p = dir.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
    }
    let main = dir.join("main.lz80n");
    let src = std::fs::read_to_string(&main).unwrap();
    let r = crate::compile_main(&src, Some(&main), "t.nex", true, &dir, false);
    (dir, r)
}

#[test]
fn include_files_once_relative_paths() {
    // lib/b includes "a.lz80n" relative to lib/; a is included only once;
    // including main.lz80n again is ignored
    let (_, r) = include_compile(
        "once",
        &[
            ("main.lz80n", "include \"lib/a.lz80n\"\ninclude \"lib/b.lz80n\"\ninclude \"main.lz80n\"\nprint(twice(add(2, 3)))\n"),
            ("lib/a.lz80n", "function add(byte x, byte y) -> byte\n    return x + y\nend\n"),
            ("lib/b.lz80n", "include \"a.lz80n\"\nfunction twice(byte x) -> byte\n    return add(x, x)\nend\n"),
        ],
    );
    let asm = r.unwrap_or_else(|e| panic!("{:?}", e.iter().map(|d| d.to_string()).collect::<Vec<_>>()));
    assert!(asm.contains("f_twice") || asm.contains("twice"));
    // the program runs: same code through the normal test path
    expect(
        "include_run",
        "function add(byte x, byte y) -> byte\n    return x + y\nend\nfunction twice(byte x) -> byte\n    return add(x, x)\nend\nprint(twice(add(2, 3)))\n",
        "10\n",
    );
}

#[test]
fn include_errors_name_the_file() {
    // an error inside an included file reports that file
    let (_, r) = include_compile(
        "err",
        &[("main.lz80n", "include \"part.lz80n\"\nprint(1)\n"), ("part.lz80n", "byte a = 1\n\nbyte a = 2\n")],
    );
    let e = r.unwrap_err();
    assert!(e[0].file.as_deref().unwrap().ends_with("part.lz80n"), "{:?}", e[0].file);
    assert_eq!(e[0].pos.line, 3);
    // a missing file
    let (_, r) = include_compile("missing", &[("main.lz80n", "include \"nope.lz80n\"\n")]);
    let e = r.unwrap_err();
    assert!(e[0].msg.contains("cannot open") && e[0].file.is_none() && e[0].pos.line == 1, "{}", e[0]);
    // only at the top level
    let (_, r) = include_compile(
        "nested",
        &[("main.lz80n", "function f()\n    include \"x.lz80n\"\nend\n"), ("x.lz80n", "print(1)\n")],
    );
    assert!(r.unwrap_err()[0].msg.contains("top level"));
    // a syntax error in an included file
    let (_, r) = include_compile("syntax", &[("main.lz80n", "include \"s.lz80n\"\n"), ("s.lz80n", "print(1\n")]);
    let e = r.unwrap_err();
    assert!(e[0].file.as_deref().unwrap().ends_with("s.lz80n"));
}

#[test]
fn include_assets_relative_to_their_file() {
    let png = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/assets/badge.png");
    let dir = std::env::temp_dir().join("lz80nc_inc_asset_src");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("gfx/pics")).unwrap();
    std::fs::copy(&png, dir.join("gfx/pics/badge.png")).unwrap();
    std::fs::write(dir.join("gfx/pictures.lz80n"), "asset badge = image(\"pics/badge.png\")\n").unwrap();
    std::fs::write(dir.join("main.lz80n"), "include \"gfx/pictures.lz80n\"\nlayer2_visible(true)\ndraw_image(badge, 10, 10)\n").unwrap();
    let main = dir.join("main.lz80n");
    let src = std::fs::read_to_string(&main).unwrap();
    let r = crate::compile_main(&src, Some(&main), "t.nex", true, &dir, false);
    assert!(r.is_ok(), "{:?}", r.err().map(|e| e.iter().map(|d| d.to_string()).collect::<Vec<_>>()));
}

#[test]
fn run_recursion() {
    expect(
        "rec_basic",
        "function fact(word n) -> word
    if n <= 1 then return 1 end
    return n * fact(n - 1)
end
function fib(byte n) -> word
    if n < 2 then return n end
    return fib(n - 1) + fib(n - 2)
end
-- the arguments read the caller's parameters, crossed over
function gcd(word a, word b) -> word
    if b == 0 then return a end
    return gcd(b, a % b)
end
-- mutual recursion
function is_even(byte n) -> bool
    if n == 0 then return true end
    return is_odd(n - 1)
end
function is_odd(byte n) -> bool
    if n == 0 then return false end
    return is_even(n - 1)
end
-- locals and a local array survive the inner calls
function sum_digits(word n) -> byte
    byte buf[6]
    byte d = byte(n % 10)
    buf[3] = d
    if n < 10 then return d end
    byte rest = sum_digits(n / 10)
    return buf[3] + rest
end
print(fact(8), fib(15), gcd(1071, 462))
print(is_even(10), is_odd(7), is_even(9))
print(sum_digits(59876))
",
        "40320 610 21\ntrue true false\n35\n",
    );
}

#[test]
fn run_recursion_methods_strings_and_ordinary_calls() {
    expect(
        "rec_more",
        "class Node
    byte value
    function count(byte n) -> byte
        if n == 0 then return self.value end
        return self.value + self.count(n - 1)
    end
end
Node a
a.value = 3
-- a string result built recursively
function stars(byte n) -> string
    if n == 0 then return \"\" end
    return \"*\" .. stars(n - 1)
end
-- Towers of Hanoi: moves counted in a global
word moves = 0
function hanoi(byte n, byte from, byte to, byte via)
    if n == 0 then return end
    hanoi(n - 1, from, via, to)
    moves += 1
    hanoi(n - 1, via, to, from)
end
hanoi(10, 1, 3, 2)
print(a.count(4), stars(5), moves)
",
        "15 ***** 1023\n",
    );
    // a non-recursive call gets no save/restore code
    let asm = crate::compile_source(
        "function f(byte x) -> byte\n return x + 1\nend\nfunction g() -> byte\n return f(1)\nend\nprint(g())",
        "t.nex",
    )
    .unwrap();
    assert!(!asm.contains("recursive call") && !asm.contains("rt_stack_overflow"));
}

#[test]
fn recursion_stack_overflow_is_caught() {
    let src = "function down(word n) -> word\n    byte pad[40]\n    pad[0] = 1\n    return down(n + 1) + 1\nend\nprint(down(0))\n";
    let Some((nex, syms)) = build("rec_overflow", src) else { return };
    let mut cpu = load_nex(&nex).unwrap();
    let handler = syms["rt_stack_overflow"];
    for _ in 0..5_000_000u64 {
        if cpu.pc == handler {
            return;
        }
        cpu.tick().unwrap();
    }
    panic!("the stack overflow handler was not reached");
}

#[test]
fn run_long() {
    expect(
        "long_basic",
        "long a = 100000
long b = -250000
long big = 2147483647
print(a, b, a + b, a - b)
print(a * 3, a * -7, 123456 * 789)
print(b / 7, b % 7, 1000000000 / 3)
print(big, -big - 1, big + 1)
word w = 60000
byte k = 200
long c = w * 1000 + k        -- word and byte widen to long
print(c, long(w) * 30)
print(a << 4, b >> 3, a & $FF00, a | 1)
print(a ~ -1, ~a)
print(a > b, a < b, a == 100000, b ~= -250000, a >= 100000, b <= -250001)
print(abs(b), min(a, b), max(a, b), clamp(5000000, 0, 999999))
print(word(a), byte(a), sword(b), long(-5))
-- a long array, a long field, a long parameter and result
long tab[] = {1, 70000, -3, 4000000000 - 4000000000}
struct Acc
    long total
    byte n
end
Acc acc
function add(Acc x, long v) -> long
    x.total += v
    x.n += 1
    return x.total
end
for i = 0, 3 do
    add(acc, tab[i])
end
print(acc.total, acc.n, \"total: \" .. str(acc.total * 2))
long i = 0
long sum = 0
while i < 100000 do
    sum += i
    i += 1000
end
print(sum)
",
        "100000 -250000 -150000 350000\n300000 -700000 97406784\n-35714 -2 333333333\n2147483647 -2147483648 -2147483648\n60000200 1800000\n1600000 -31250 34304 100001\n-100001 -100001\ntrue false true false true false\n250000 -250000 100000 999999\n34464 160 12144 -5\n69998 4 total: 139996\n4950000\n",
    );
}

#[test]
fn run_fixed16() {
    expect(
        "fixed16_basic",
        "fixed16 a = 1000.5
fixed16 b = -2.25
fixed16 tiny = 0.0001
print(a, b, a + b, a - b)
print(a * b, a / b)
print(a * 3, a / 4, b * -2, fixed16(7), fixed16(-300))
fixed f = 1.5
fixed16 g = f                  -- fixed widens to fixed16
print(g, g * g, g / 3, fixed(a / 100), long(a), word(a), sword(b))
print(a > b, b < -2, a == 1000.5, tiny > 0)
fixed16 pi = 3.14159
fixed16 r = 12.5
print(pi * r * r)
fixed16 x = 0
for k = 1, 10 do
    x += 0.1
end
print(x, abs(b), -a, min(a, b), max(a, b))
fixed16 v[] = {0.5, -1.25, 2}
print(v[0] + v[1] + v[2])
",
        "1000.5 -2.25 998.25 1002.75
-2251.125 -444.6666\n3001.5 250.125 4.5 7 -300\n1.5 2.25 0.5 10 1000 1000 -3\ntrue true true true\n490.8727\n1 2.25 -1000.5 -2.25 1000.5\n1.25\n",
    );
}

#[test]
fn long_errors() {
    assert!(err("long a = 5\nword w = a").contains("narrow"));
    assert!(err("fixed16 a = 1.5\nlong l = a").contains("long(x)"));
    assert!(err("word w = 5\nfixed16 f = w").contains("fixed16(x)"));
    assert!(err("fixed16 f = 1.5\nfixed16 g = f & 1").contains("bitwise"));
    assert!(err("long a = 5\nlong b = a / 0").contains("division by zero"));
    assert!(err("long a = 5000000000").contains("invalid number"));
    assert!(err("for i = 0, 100000 do\nend").contains("`while`"));
    assert!(err("far long x[10]").contains("not supported"));
    assert!(err("fixed f = 200.5").contains("does not fit"));
}


#[test]
fn run_struct_values() {
    expect(
        "struct_values",
        "struct Point
    sword x
    sword y
end
class Ship
    Point pos
    byte hp = 3
    string name
    function init(sword x, sword y)
        self.pos = Point(x, y)
        self.name = \"ship\"
    end
    function hit() -> byte
        self.hp -= 1
        return self.hp
    end
    function describe() -> string
        return self.name .. \" \" .. str(self.pos.x) .. \",\" .. str(self.pos.y) .. \" hp\" .. str(self.hp)
    end
end
class Boss extends Ship
    byte shield = 2
    function init(sword x, sword y)
        super.init(x, y)
        self.name = \"boss\"
        self.hp = 9
    end
    function hit() -> byte
        if self.shield > 0 then
            self.shield -= 1
            return self.hp
        end
        return super.hit()
    end
end
function add(Point a, Point b) -> Point
    return Point(a.x + b.x, a.y + b.y)
end
function mid(Point a, Point b) -> Point
    Point m = add(a, b)
    m.x /= 2
    m.y /= 2
    return m
end
function show(Ship s)
    print(s.describe())
end
Point p = Point(3, -4)
Point q = {10, 20}
Point r = add(p, q)
print(r.x, r.y)
r = mid(Point(0, 0), {8, 6})
print(r.x, r.y)
var z = add(r, r)
print(z.x, z.y)
p = Point(p.y, p.x)
print(p.x, p.y)
Point pts[] = {{1, 2}, {3, 4}, {5}}
print(pts[0].x, pts[1].y, pts[2].x, pts[2].y)
Ship s = Ship(1, 2)
Boss b = Boss(7, 8)
print(s.describe())
print(b.describe(), b.shield)
print(b.hit(), b.hit(), b.hit(), s.hit())
show(b)
Ship copy = b
print(copy.describe())
Ship fleet[] = {{{1, 1}, 5, \"a\"}, {{2, 2}}}
print(fleet[0].describe(), fleet[1].hp, fleet[1].name == \"\")
function local_test()
    Point a = {100, 200}
    Point c = Point(1)
    print(a.x + a.y, c.x, c.y)
    a.x = 0
end
local_test()
local_test()
",
        "13 16\n4 3\n8 6\n-4 3\n1 4 5 0\nship 1,2 hp3\nboss 7,8 hp9 2\n9 9 8 2\nboss 7,8 hp8\nboss 7,8 hp8\na 1,1 hp5 3 true\n300 1 0\n300 1 0\n",
    );
}

#[test]
fn struct_value_errors() {
    assert!(err("struct P\n byte x\nend\nbyte n = P(1)").contains("makes a `P`"));
    assert!(err("struct P\n byte x\nend\nP p = {1, 2}").contains("2 values"));
    assert!(err("struct P\n byte x\nend\nstruct Q\n byte x\nend\nQ q\nP p = q").contains("not a `P`"));
    assert!(err("class A extends B\nend").contains("unknown parent"));
    assert!(err("class A\n function f()\n  super.f()\n end\nend").contains("`super`"));
    assert!(err("struct P\n byte x\nend\nP p = P(1, 2)").contains("has 1 fields"));
}

#[test]
fn run_struct_lists_at_run_time() {
    expect(
        "struct_lists",
        "struct V
    sword x
    sword y
end
struct Pix
    V at
    byte c
end
V v = {3, 4}
v = {v.y, v.x}
print(v.x, v.y)
sword k = 7
V w = {k, k * 2}
Pix p = {{1, 2}, rgb(7, 0, 0)}
Pix q = {w, rgb(0, 7, 0)}
print(w.x, w.y, p.at.y, p.c, q.at.y, q.c)
",
        "4 3\n7 14 2 224 14 28\n",
    );
}

#[test]
fn run_struct_var_and_explicit_method() {
    expect(
        "struct_var",
        "struct Vec
    sword x
    sword y
end
function add(Vec p, Vec q) -> Vec
    return Vec(p.x + q.x, p.y + q.y)
end
class A
    byte n = 1
    function who() -> byte
        return self.n
    end
end
class B extends A
    function who() -> byte
        return 100 + A.who(self)
    end
end
var d = add({1, 2}, {3, 4})
B b
print(d.x, d.y, b.who(), A.who(b))
",
        "4 6 101 1\n",
    );
    assert!(err("struct V\n byte x\nend\nfunction f() -> V\n return {1}\nend\nbyte k = f().x").contains("in a variable first"));
}

#[test]
fn run_layer2_hires() {
    expect(
        "l2_hires",
        "layer2_mode(L2_320)
hline(250, 10, 20, 7)
print(point(249, 10), point(250, 10), point(256, 10), point(269, 10), point(270, 10))
hline(300, 11, 100, 8)
print(point(299, 11), point(300, 11), point(319, 11), point(320, 11))
fill_circle(224, 110, 36, 5)
print(point(224, 110), point(190, 110), point(258, 110), point(224, 80))
fill_circle(288, 110, 36, 6)
print(point(288, 110), point(300, 120))
plot(300, 255, 9)
print(point(300, 255), layer2_width(), layer2_height())
layer2_mode(L2_640)
layer2_clear(3)
plot(639, 255, 12)
plot(638, 255, 21)
hline(600, 3, 50, 9)
print(point(0, 0), point(639, 255), point(638, 255), point(599, 3), point(600, 3), point(639, 3))
print(layer2_width(), layer2_height())
layer2_mode(L2_256)
print(layer2_width(), layer2_height(), point(0, 192))
palette_select(PAL_LAYER2, true)
palette_select(PAL_SPRITES, true)
palette(PAL_LAYER2_2, 1, 2)
print(nextreg[0x43] & 0x0E, nextreg[0x43] >> 4)
palette_select(PAL_LAYER2, false)
print(nextreg[0x43] & 0x0E)
",
        "0 7 7 7 0
0 8 8 0
5 5 5 5
6 6
9 320 256
3 12 5 3 9 9
640 256
256 192 0
12 5
8
",
    );
}

/// A main program (no functions at all) far larger than the resident area:
/// its big statements become hidden functions, which are banked.
#[test]
fn run_large_main_program() {
    let (nblock, nline) = (60u16, 60u16);
    let mut src = String::from("word s = 1\nbyte pass = 0\nwhile true do\n    pass += 1\n    if pass > 2 then break end\n");
    for b in 0..nblock {
        src += &format!("    if s ~= {} or pass > 0 then\n", 70000 % 65536 + b as u32);
        for j in 0..nline {
            src += &format!("        s = s * 3 + {}\n", b * 7 + j);
        }
        src += "    end\n";
    }
    src += "end\nprint(s, pass)\n";
    let mut v: u16 = 1;
    for _ in 0..2 {
        for b in 0..nblock {
            for j in 0..nline {
                v = v.wrapping_mul(3).wrapping_add(b * 7 + j);
            }
        }
    }
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let asm = crate::compile_with(&src, "t.nex", true, &base).unwrap();
    let pages = asm.matches("    MMU 2 e,").count();
    assert!(pages >= 2, "expected code pages, got {pages}");
    assert!(asm.contains("mp_1:"), "parts of the main program moved out");
    expect("r_big_main", &src, &format!("{v} 3\n"));
}

/// A .wav file in a temp folder, for `asset` declarations in tests.
fn temp_wav(name: &str, rate: u32, samples: &[u8]) -> String {
    let dir = std::env::temp_dir().join("lz80nc_wav");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(format!("{name}.wav"));
    std::fs::write(&p, crate::assets::encode_wav(rate, samples)).unwrap();
    p.display().to_string().replace('\\', "/")
}

#[test]
fn run_sample_playback() {
    // 5000 bytes: more than the 4K ring buffer, so it is refilled
    let s: Vec<u8> = (0..5000u32).map(|i| (i * 7 % 251) as u8).collect();
    let path = temp_wav("ramp", 15625, &s);
    let src = format!(
        "asset boom = \"{path}\"
layer2_visible(true)
sample_play(boom)
print(sample_playing())
word n = 0
word t0 = frames()
while sample_playing() do
    layer2_clear(byte(n))      -- the DMA is busy: done by the CPU
    vsync()
    n += 1
end
word t = frames() - t0
print(t > 15, t < 40, point(10, 10) == byte(n - 1))
"
    );
    let Some((out, cpu)) = run_with("smp_play", &src, |_| {}) else { return };
    assert_eq!(out, "true\ntrue true true\n");
    assert!(cpu.dac.len() >= 5000 + 100, "{} bytes reached the DAC", cpu.dac.len());
    assert_eq!(&cpu.dac[..5000], &s[..], "the sample, in order");
    assert!(cpu.dac[5000..5100].iter().all(|&b| b == 0x80), "then silence");
    assert_eq!(cpu.nextreg[8] & 8, 8, "DACs enabled");
}

#[test]
fn run_sample_loop_and_stop() {
    let s: Vec<u8> = (0..300u32).map(|i| (i % 200 + 20) as u8).collect();
    let path = temp_wav("short", 8000, &s);
    let src = format!(
        "asset hum = \"{path}\"
sample_loop(hum)
for i = 1, 20 do vsync() end
print(sample_playing())
sample_stop()
print(sample_playing())
"
    );
    let Some((out, cpu)) = run_with("smp_loop", &src, |_| {}) else { return };
    assert_eq!(out, "true\nfalse\n");
    // 8 kHz for 20 frames: ~3200 bytes, the 300-byte sample over and over
    assert!(cpu.dac.len() > 2500, "{} bytes", cpu.dac.len());
    for (i, &b) in cpu.dac.iter().take(2400).enumerate() {
        assert_eq!(b, s[i % 300], "byte {i}");
    }
    assert!(err("asset x = \"ships.png\"\nsample_play(x)").contains("not a sample"));
}

fn tune_path() -> String {
    format!("{}/../examples/assets/tune.pt3", env!("CARGO_MANIFEST_DIR")).replace('\\', "/")
}

#[test]
fn pt3_render() {
    let d = std::fs::read(tune_path()).unwrap();
    let song = crate::pt3::render(&d).unwrap();
    // 5 positions x 32 lines x 4 frames, looping to position 1
    assert_eq!(song.frames.len(), 5 * 32 * 4);
    assert_eq!(song.loop_frame, 32 * 4);
    let f0 = song.frames[0];
    // channel A silent in the intro; B plays A-3 (note 33) with the minor
    // ornament: 0, 0, 3, 3 ... semitones
    assert_eq!(f0.regs[8], 0);
    let tone = |f: &crate::pt3::Frame, c: usize| f.regs[c * 2] as u16 | (f.regs[c * 2 + 1] as u16) << 8;
    assert_eq!(tone(&f0, 1), 0x01ce);
    assert_eq!(tone(&song.frames[2], 1), 0x0184); // + 3 semitones (C-4)
    // the lead starts with E-5 (note 52) at the loop
    let fl = song.frames[song.loop_frame];
    assert_eq!(tone(&fl, 0), 0x009a);
    assert!(fl.regs[8] > 0 && fl.regs[8] < 16);
    // the kick: noise on, then off; the snare: tone off on channel C
    assert_eq!(f0.regs[7] & 0x24, 0x00);
    assert!(song.frames[16..20].iter().any(|f| f.regs[7] & 4 != 0), "snare: no tone on C");
    // the buzzer bass in pattern 2: R13 written, envelope bit on channel C
    let f = song.frames.iter().position(|f| f.env).expect("envelope written");
    assert_eq!(song.frames[f].regs[13], 10);
    assert_eq!(song.frames[f].regs[10] & 16, 16);
    assert_eq!(song.frames[f].regs[11], 24);
    // the portamento in pattern 2 line 27: the tone moves between the notes
    let start = (2 * 32 + 27) * 4;
    let t: Vec<u16> = (start..start + 12).map(|i| tone(&song.frames[i], 0)).collect();
    assert!(t.windows(2).any(|w| w[0] != w[1]), "{t:?}");
    assert!(t[0] > t[11] && t[11] >= 0x0082, "{t:?}");

    let (bytes, loop_off) = crate::pt3::encode(&song);
    assert_eq!(&bytes[bytes.len() - 2..], &[0, 0x80]);
    assert_eq!(bytes[loop_off], 0xFF);
    assert_eq!(bytes[loop_off + 1] & 0x1F, 0x1F);
    assert!(crate::pt3::render(b"not a module").is_err());
}

#[test]
fn run_music_player() {
    let d = std::fs::read(tune_path()).unwrap();
    let song = crate::pt3::render(&d).unwrap();
    let src = format!(
        "asset tune = \"{}\"
music_play(tune, 1)
for i = 1, 150 do vsync() end
print(music_playing())
",
        tune_path()
    );
    let Some((out, cpu)) = run_with("mus_play", &src, |_| {}) else { return };
    assert_eq!(out, "true\n");
    // the chip holds one of the rendered frames from about frame 200
    let ay = cpu.ay[1];
    let hit = (140..160).find(|&k| song.frames[k].regs[..13] == ay[..13]);
    assert!(hit.is_some(), "AY 1 = {:?}", &ay[..14]);
    assert_eq!(cpu.ay[0][8], 0, "chip 0 untouched");

    // played once: silent at the end
    let src = format!(
        "asset tune = \"{}\"
cpu_speed(0)          -- 3.5 MHz: fewer emulated instructions
music_once(tune)
word t = frames()
while music_playing() do vsync() end
print(frames() - t)
",
        tune_path()
    );
    let Some((out, cpu)) = run_with("mus_once", &src, |_| {}) else { return };
    let n: usize = out.trim().parse().unwrap();
    assert!((640..650).contains(&n), "{n} frames");
    assert_eq!(&cpu.ay[0][8..11], &[0, 0, 0]);
    assert_eq!(cpu.ay[0][7] & 0x3F, 0x3F);
    assert!(err("asset x = \"ships.png\"\nmusic_play(x)").contains("not music"));
}

/// Where does a program spend its time? Runs PROFILE (an example name or a
/// .lz80n path) in the emulator for FRAMES frames (default 300, after 20
/// frames of start-up) and prints the T-states per routine: the routine is
/// the nearest function (`f_`), runtime routine (`rt_`), outlined
/// main-program chunk (`mp_`), the main program (`__start`) or the frame
/// interrupt (`__isr`) at or below the PC. `(vsync)` is time spent halted,
/// waiting for the next frame.
///
/// `PROFILE=shooter cargo test --release profile -- --ignored --nocapture`
#[test]
#[ignore]
fn profile() {
    let name = std::env::var("PROFILE").expect("PROFILE=example or file.lz80n");
    let frames: u32 = std::env::var("FRAMES").ok().and_then(|f| f.parse().ok()).unwrap_or(300);
    let path = if name.ends_with(".lz80n") {
        PathBuf::from(&name)
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../examples/{name}.lz80n"))
    };
    let src = std::fs::read_to_string(&path).unwrap();
    let (nex, syms) = build("profile", &src).unwrap();
    let mut labels: Vec<(u16, &str)> = syms
        .iter()
        .filter(|(k, _)| {
            !k.contains('.')
                && (["f_", "rt_", "mp_"].iter().any(|p| k.starts_with(p))
                    || ["__start", "__isr", "__main_end"].contains(&k.as_str()))
        })
        .map(|(k, v)| (*v, k.as_str()))
        .collect();
    labels.sort();
    let mut cpu = load_nex(&nex).unwrap();
    while cpu.frame_count < 20 {
        cpu.tick().unwrap();
    }
    let mut time: HashMap<&str, u64> = HashMap::new();
    let t0 = cpu.t_states;
    while cpu.frame_count < 20 + frames {
        let (pc, t, halted) = (cpu.pc, cpu.t_states, cpu.halted);
        cpu.tick().unwrap();
        let i = labels.partition_point(|&(a, _)| a <= pc);
        let routine = if halted { "(vsync)" } else if i == 0 { "(?)" } else { labels[i - 1].1 };
        *time.entry(routine).or_default() += cpu.t_states - t;
    }
    let total = cpu.t_states - t0;
    let mut rows: Vec<_> = time.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    println!("{name}: {frames} frames, {} T per frame", total / frames as u64);
    for (routine, t) in rows.iter().take(25) {
        println!("{:>6.2}% {:>9} T/frame  {routine}", *t as f64 * 100.0 / total as f64, t / frames as u64);
    }
}

/// T-states between calls of `bench_start()` and `bench_stop(n)` in the
/// program named by BENCH (run with --ignored --nocapture).
#[test]
#[ignore]
fn bench() {
    let path = std::env::var("BENCH").expect("BENCH=file.lz80n");
    let src = std::fs::read_to_string(&path).unwrap();
    let (nex, syms) = build("bench", &src).unwrap();
    let mut cpu = load_nex(&nex).unwrap();
    let (start, stop, end) = (syms["f_bench_start"], syms["f_bench_stop"], syms["__main_end"]);
    let mut t0 = 0;
    for _ in 0..2_000_000_000u64 {
        if cpu.pc == end {
            return;
        }
        if cpu.pc == start {
            t0 = cpu.t_states;
        } else if cpu.pc == stop {
            println!("BENCH {:>3} {:>10}", cpu.a, cpu.t_states - t0);
        }
        cpu.tick().unwrap();
    }
    panic!("did not finish");
}

#[test]
fn run_branches_match_bool_values() {
    // every condition as a jump (if) and as a value (bool v = ...) must agree
    let conds = [
        "x > 100 or not (x < 50 and x ~= 7)",
        "x > 254",
        "x <= 3",
        "not (x == 0)",
        "s < 0",
        "s >= 0",
        "s > -1",
        "s <= 126",
        "s > 126 or s < -127",
        "w == 0",
        "w ~= 0",
        "w > 65000",
        "w <= 1200",
        "sw < 0",
        "sw >= 0 and x ~= 3 or s < -100",
        "not (sw > -2) and not (w == 300)",
    ];
    let mut src = String::from("word bad = 0\nfor i = 0, 255 do\n    byte x = byte(i)\n    sbyte s = sbyte(x)\n    word w = word(x) * 300\n    sword sw = sword(w)\n");
    for (k, c) in conds.iter().enumerate() {
        src.push_str(&format!(
            "    bool v{k} = {c}\n    bool j{k} = false\n    if {c} then j{k} = true end\n    if v{k} ~= j{k} then bad += 1 end\n"
        ));
    }
    src.push_str("end\nword n = 0\nwhile n < 10 or n == 12 do n += 1 end\nprint(bad, n)\n");
    expect("r_branches", &src, "0 10\n");
}

#[test]
fn run_update_in_place() {
    // t op= x on struct array fields (address once) must match plain variables
    let src = r#"
struct T
    byte b
    sbyte s
    word w
    sword sw
    fixed f
end
T t[4]
byte b = 0
sbyte s = 0
word w = 0
sword sw = 0
fixed f = 0.0
word bad = 0
byte k = 2
for i = 0, 40 do
    byte v = byte(i * 37)
    sbyte sv = sbyte(v)
    t[k].b += v       b += v
    t[k].b -= 3       b -= 3
    t[k].b += 1       b += 1
    t[k].b -= 255     b -= 255
    t[k].b &= 0xF7    b &= 0xF7
    t[k].b |= v       b |= v
    t[k].s += sv      s += sv
    t[k].s -= 1       s -= 1
    t[k].w += v       w += v
    t[k].w += 1       w += 1
    t[k].w -= 700     w -= 700
    t[k].w = t[k].w ~ 0x0F0F   w = w ~ 0x0F0F
    t[k].w -= word(v) * 3   w -= word(v) * 3
    t[k].w += sv      w += sv
    t[k].sw += sv     sw += sv
    t[k].sw -= 1      sw -= 1
    t[k].sw -= sw / 4
    sw -= sw / 4
    t[k].f += 1.25    f += 1.25
    t[k].f -= f / 8.0
    f -= f / 8.0
    if t[k].b ~= b or t[k].s ~= s or t[k].w ~= w or t[k].sw ~= sw or t[k].f ~= f then bad += 1 end
end
print(bad, t[1].w, t[3].b)
"#;
    expect("r_inplace", src, "0 0 0\n");
}

#[test]
fn run_word_byte_loads() {
    let src = r#"
struct Q
    byte pad
    word w
    sword s
end
Q q[3]
word g = 0x1234
sword n = -2
q[2].w = 0xABCD
q[2].s = -300
word a[4] = {1, 0x0201, 0xFF80, 4}
byte i = 2
print(byte(g), byte(g >> 8), byte(n), byte(n >> 8))
print(byte(q[i].w), byte(q[i].w >> 8), byte(q[i].s), byte(q[i].s >> 8))
print(byte(a[i]), byte(a[i] >> 8), sbyte(a[i]))
"#;
    expect("r_wordbyte", src, "52 18 254 255\n205 171 212 254\n128 255 -128\n");
}

#[test]
fn run_field_by_field() {
    let src = r#"
struct M
    word x
    sword y
    byte bx
    sbyte dx
    word dw
    fixed f
    fixed df
end
M m[3]
byte j = 1
m[j].x = 1000  m[j].y = -5  m[j].bx = 250  m[j].dx = -3  m[j].dw = 0x0F0F
m[j].f = 1.5  m[j].df = 0.25
m[j].x += m[j].dx
m[j].y -= m[j].dx
m[j].bx += m[j].dx
m[j].dw = m[j].dw ~ m[j].x
m[j].dw |= m[j].bx
m[j].x &= m[j].dw
m[j].f -= m[j].df
m[j].dx = -m[j].dx
m[j].y = -m[j].y
print(m[j].x, m[j].y, m[j].bx, m[j].dw, m[j].f, m[j].dx)
print(m[0].x, m[2].y)
"#;
    // x = 997; y = -2; bx = 247; dw = $F0F ^ 997 = $CEA, | 247 = $CFF (3327);
    // x = 997 & 3327 = 229; f = 1.25; dx = 3; y = 2
    expect("r_fieldfield", src, "229 2 247 3327 1.25 3\n0 0\n");
}

#[test]
fn run_sound_loop_flat_volume() {
    let src = r#"
play_loop(2, "v9 o5 l3 d- <b")
while true do vsync() end
"#;
    let shots = sound_frames("r_snd_loop", src, 24);
    if shots.is_empty() {
        return;
    }
    let period = |s: &[[u8; 16]; 3]| s[0][4] as u16 | (s[0][5] as u16) << 8;
    let p = |n: f64| (1_750_000.0 / (16.0 * 440.0 * 2f64.powf((n - 57.0) / 12.0))).round() as u16;
    let (db5, b4) = (p(61.0), p(59.0));
    let seq: Vec<(u16, u8)> = shots.iter().map(|s| (period(s), s[0][10])).collect();
    let first = seq.iter().position(|&(q, v)| q == db5 && v == 9).expect("d-5 at volume 9");
    // d-5 3 frames (last silent), b4 3 frames, then again from the start
    assert_eq!(seq[first + 1], (db5, 9));
    assert_eq!(seq[first + 2].1, 0);
    assert_eq!(seq[first + 3], (b4, 9));
    // the loop restarts in octave 4 (after `<`) only if `o5` is read again: it is
    assert_eq!(seq[first + 6], (db5, 9), "{seq:?}");
    assert_eq!(seq[first + 9], (b4, 9));
}

#[test]
#[ignore]
fn bench_sound_tick() {
    let src = "play_loop(0, \"o4 l2 cdefgab\")\nplay_loop(1, \"o3 l3 cegc\")\nplay_loop(2, \"o5 l1 c\")\nfor j = 1, 20 do\nfor i = 1, 30000 do end\nend\n";
    let (nex, syms) = build("bench_snd", src).unwrap();
    let mut cpu = load_nex(&nex).unwrap();
    let (tick, end) = (syms["f_sound_tick"], syms["__main_end"]);
    let (mut inside, mut t0, mut sp0, mut total, mut calls) = (false, 0, 0, 0u64, 0);
    let mut hist: HashMap<u16, u64> = HashMap::new();
    while cpu.pc != end {
        if !inside && cpu.pc == tick {
            inside = true;
            t0 = cpu.t_states;
            sp0 = cpu.sp;
            calls += 1;
        }
        let (pc, t1) = (cpu.pc, cpu.t_states);
        cpu.tick().unwrap();
        if inside {
            *hist.entry(pc).or_insert(0u64) += cpu.t_states - t1;
        }
        if inside && cpu.sp > sp0 {
            inside = false;
            total += cpu.t_states - t0;
        }
    }
    println!("SOUND {calls} calls, {} T per call", total / calls.max(1));
    let mut v: Vec<_> = hist.into_iter().collect();
    v.sort_by_key(|x| std::cmp::Reverse(x.1));
    let mut names: Vec<(u16, &String)> = syms.iter().map(|(k, v)| (*v, k)).collect();
    names.sort();
    for (pc, t) in v.iter().take(25) {
        let n = names.iter().rev().find(|(a, _)| a <= pc).map(|(a, n)| format!("{n}+{}", pc - a)).unwrap_or_default();
        println!("  {pc:04X} {t:>8} {n}");
    }
}

#[test]
fn run_print_number_edges() {
    expect(
        "r_numedges",
        "word a[9] = {0, 9, 10, 99, 100, 1000, 10000, 60009, 65535}\nfor i = 0, 8 do write(a[i], \" \") end\nprint()\nsword s = -32768\nprint(s, sword(-1), sword(-10), str(word(40000)))",
        "0 9 10 99 100 1000 10000 60009 65535 \n-32768 -1 -10 40000\n",
    );
}

#[test]
fn run_sqrt16_all_values() {
    // floor(sqrt(w)) for every 16-bit w: s*s <= w < (s+1)*(s+1)
    let src = "word bad = 0\nword w = 0\nwhile true do\n    long s = long(sqrt(w))\n    long v = long(w)\n    if s * s > v or (s + 1) * (s + 1) <= v then bad += 1 end\n    if w == 65535 then break end\n    w += 1\nend\nprint(bad, sqrt(word(65535)), sqrt(word(0)), sqrt(word(48)), sqrt(word(49)))\n";
    expect("r_sqrt16", src, "0 255 0 6 7\n");
}

#[test]
fn run_long_add_constant() {
    expect(
        "r_longk",
        "long x = 65535\nx += 1\nprint(x)\nx -= 65537\nprint(x)\nx += 65536\nprint(x)\nx = x - 0\nx = x + 2147418112\nprint(x)\nfixed16 f = 1.5\nf += 2.75\nprint(f)\nf -= 10\nprint(f)",
        "65536\n-1\n65535\n2147483647\n4.25\n-5.75\n",
    );
}

/// The old LZ80N line, for comparing the assembly version pixel by pixel.
const LINE_REF: &str = r#"
function line_ref(word x0, byte y0, word x1, byte y1, byte c)
    sword dx = x1 - x0
    sword dy = y1 - y0
    sbyte sx = 1
    sbyte sy = 1
    if dx < 0 then
        dx = -dx
        sx = -1
    end
    if dy < 0 then
        dy = -dy
        sy = -1
    end
    sword err = dx - dy
    while true do
        plot(x0, y0, c)
        if x0 == x1 and y0 == y1 then break end
        sword e2 = err + err
        if e2 > -dy then
            err -= dy
            x0 += sx
        end
        if e2 < dx then
            err += dx
            y0 += sy
        end
    end
end
"#;

#[test]
fn run_line_matches_reference() {
    let src = format!(
        "{LINE_REF}
word bad = 0
function check(word x0, byte y0, word x1, byte y1, word w, byte h)
    layer2_clear(0)
    line(x0, y0, x1, y1, 2)
    line_ref(x0, y0, x1, y1, 1)       -- 2 left: drawn only by line
    line(x0, y0, x1, y1, 3)           -- 1 left: drawn only by line_ref
    for y = 0, h do
        for x = 0, w - 1 do
            byte p = point(x, byte(y))
            if p == 1 or p == 2 then bad += 1 end
        end
    end
end
layer2_visible(true)
check(10, 10, 200, 40, 256, 191)
check(200, 40, 10, 10, 256, 191)
check(30, 180, 35, 0, 256, 191)
check(250, 5, 290, 150, 256, 191)
check(0, 0, 255, 191, 256, 191)
check(100, 100, 100, 100, 256, 191)
check(120, 50, 20, 51, 256, 191)
check(255, 191, 0, 0, 256, 191)
check(5, 190, 40, 2, 256, 191)
layer2_mode(L2_320)
check(5, 250, 319, 3, 320, 255)
check(300, 10, 20, 200, 320, 255)
print(bad)
"
    );
    expect("r_line_ref", &src, "0\n");
}

/// The stepped half widths (`l2_ell_next`) equal the square-root formula
/// (`ref_halfw`) on every row, for small radii exhaustively and for a
/// spread of radii up to 255.
#[test]
fn run_ellipse_rows_match_reference() {
    let reference = include_str!("../tests/fixtures/shapes_ref.lz80n");
    let src = format!(
        "{reference}
word bad = 0
word rows = 0
function check(byte rx, byte ry)
    l2_ell_start(0, 0, rx, ry)
    for dy = 0, ry do
        if l2_ell_next() ~= ref_halfw(rx, ry, dy) then bad += 1 end
        rows += 1
    end
end
for rx = 0, 24 do
    for ry = 0, 24 do check(rx, ry) end
end
byte big[] = {{1, 2, 3, 7, 31, 32, 60, 99, 127, 128, 200, 254, 255}}
for i = 0, 12 do
    for j = 0, 12 do check(big[i], big[j]) end
end
print(bad, rows)
"
    );
    expect("r_ellipse_rows", &src, "0 23881\n");
}

#[test]
fn run_shapes_match_reference() {
    let reference = include_str!("../tests/fixtures/shapes_ref.lz80n");
    let src = format!(
        "{reference}
word bad = 0
function count(word w, byte h)
    for y = 0, h do
        for x = 0, w - 1 do
            byte p = point(x, byte(y))
            if p == 1 or p == 2 then bad += 1 end
        end
    end
    layer2_clear(0)
end
function tri(sword a, sword b, sword c, sword d, sword e, sword f, word w, byte h)
    fill_triangle(a, b, c, d, e, f, 2)
    fill_triangle_ref(a, b, c, d, e, f, 1)
    fill_triangle(a, b, c, d, e, f, 3)
    count(w, h)
end
function ell(sword x, sword y, byte rx, byte ry, word w, byte h)
    ellipse(x, y, rx, ry, 2)
    ellipse_ref(x, y, rx, ry, 1)
    ellipse(x, y, rx, ry, 3)
    count(w, h)
    fill_ellipse(x, y, rx, ry, 2)
    fill_ellipse_ref(x, y, rx, ry, 1)
    fill_ellipse(x, y, rx, ry, 3)
    count(w, h)
end
layer2_visible(true)
tri(10, 10, 200, 60, 50, 180, 256, 191)
tri(128, 5, 128, 5, 128, 5, 256, 191)
tri(0, 100, 255, 100, 128, 100, 256, 191)
tri(-40, -30, 300, 90, 20, 250, 256, 191)
tri(250, 0, 3, 7, 140, 190, 256, 191)
tri(5, 5, 6, 180, 7, 90, 256, 191)
ell(128, 96, 60, 30, 256, 191)
ell(20, 20, 40, 9, 256, 191)
ell(200, 150, 3, 70, 256, 191)
ell(100, 100, 0, 0, 256, 191)
layer2_mode(L2_320)
tri(-10, 255, 330, 10, 160, -5, 320, 255)
ell(300, 200, 90, 80, 320, 255)
print(bad)
"
    );
    expect("r_shapes_ref", &src, "0\n");
}

/// Ellipses that lie on the screen take the unclipped paths (direct writes
/// in 256x192, rt_hline in 320x256); they must match the reference too.
#[test]
fn run_ellipses_on_screen_match_reference() {
    let reference = include_str!("../tests/fixtures/shapes_ref.lz80n");
    let src = format!(
        "{reference}
word bad = 0
-- compare inside the bounding box (one pixel around it)
function ell(sword x, sword y, byte rx, byte ry)
    for pass = 0, 1 do
        if pass == 0 then
            ellipse(x, y, rx, ry, 2)
            ellipse_ref(x, y, rx, ry, 1)
            ellipse(x, y, rx, ry, 3)
        else
            fill_ellipse(x, y, rx, ry, 2)
            fill_ellipse_ref(x, y, rx, ry, 1)
            fill_ellipse(x, y, rx, ry, 3)
        end
        for py = max(y - sword(ry) - 1, 0), min(y + sword(ry) + 1, sword(layer2_height()) - 1) do
            for px = max(x - sword(rx) - 1, 0), min(x + sword(rx) + 1, sword(layer2_width()) - 1) do
                byte p = point(word(px), byte(py))
                if p == 1 or p == 2 then bad += 1 end
            end
        end
        layer2_clear(0)
    end
end
layer2_visible(true)
ell(128, 96, 60, 30)
ell(40, 150, 9, 33)
ell(128, 96, 0, 50)
ell(128, 96, 127, 0)
ell(0, 191, 0, 0)
ell(127, 95, 127, 95)        -- touches all four edges
layer2_mode(L2_320)
ell(160, 128, 150, 100)
ell(159, 127, 159, 127)
print(bad)
"
    );
    expect("r_ellipses_on_screen", &src, "0\n");
}

#[test]
fn run_vline_matches_plots() {
    let src = r#"
word bad = 0
function vref(word x, byte y, word len, byte c)
    while len ~= 0 do
        plot(x, y, c)
        len -= 1
        if y == 255 then break end
        y += 1
    end
end
function check(word x, byte y, word len, word w, byte h)
    vline(x, y, len, 2)
    vref(x, y, len, 1)
    vline(x, y, len, 3)
    for yy = 0, h do
        for xx = 0, w - 1 do
            byte p = point(xx, byte(yy))
            if p == 1 or p == 2 then bad += 1 end
        end
    end
    layer2_clear(0)
end
layer2_visible(true)
check(0, 0, 192, 256, 191)
check(255, 30, 1000, 256, 191)
check(17, 31, 2, 256, 191)
check(40, 191, 5, 256, 191)
check(40, 200, 5, 256, 191)
check(300, 10, 50, 256, 191)
check(9, 9, 0, 256, 191)
layer2_mode(L2_320)
check(319, 0, 256, 320, 255)
check(100, 250, 30, 320, 255)
check(320, 0, 10, 320, 255)
layer2_mode(L2_640)
check(639, 5, 100, 640, 255)
check(2, 200, 400, 640, 255)
print(bad)
"#;
    expect("r_vline_ref", src, "0\n");
}

#[test]
fn run_hline_matches_plots() {
    let src = r#"
word bad = 0
function check(word x, byte y, word len)
    hline(x, y, len, 2)
    for i = 0, 300 do
        if word(i) < len then plot(x + word(i), y, 1) end
    end
    hline(x, y, len, 3)
    for yy = 0, 191 do
        for xx = 0, 255 do
            byte p = point(xx, byte(yy))
            if p == 1 or p == 2 then bad += 1 end
        end
    end
    layer2_clear(0)
end
layer2_visible(true)
check(0, 0, 256)
check(10, 5, 31)
check(10, 6, 32)
check(10, 7, 33)
check(240, 100, 100)
check(0, 191, 300)
check(5, 192, 50)
check(256, 50, 10)
check(100, 33, 1)
check(100, 34, 0)
print(bad)
"#;
    expect("r_hline_ref", src, "0\n");
}

#[test]
fn run_hline_hires_matches_plots() {
    let src = r#"
word bad = 0
function check(word x, byte y, word len, word w)
    hline(x, y, len, 2)
    word i = 0
    while i < len do
        if x + i < w then plot(x + i, y, 1) end
        i += 1
    end
    hline(x, y, len, 3)
    for xx = 0, w - 1 do
        for yy = 0, 2 do
            byte p = point(xx, y + byte(yy))
            if p == 1 or p == 2 then bad += 1 end
        end
    end
    layer2_clear(0)
end
layer2_visible(true)
layer2_mode(L2_320)
check(0, 10, 320, 320)
check(31, 11, 2, 320)
check(30, 12, 70, 320)
check(300, 13, 100, 320)
check(64, 14, 1, 320)
check(320, 15, 5, 320)
check(5, 16, 0, 320)
layer2_mode(L2_640)
check(0, 10, 640, 640)
check(1, 11, 1, 640)
check(1, 12, 2, 640)
check(1, 13, 3, 640)
check(2, 14, 1, 640)
check(63, 15, 66, 640)
check(600, 16, 100, 640)
check(639, 17, 9, 640)
check(640, 18, 4, 640)
print(bad)
"#;
    expect("r_hline_hi", src, "0\n");
}

#[test]
fn run_string_append_in_place() {
    let src = r#"
string s = "ab"
s = s .. s .. "-" .. 42 .. str(word(7)) .. chr(65)
print(s)
string t = ""
for i = 1, 70 do t = t .. "x" end
print(len(t))
string u = "q"
function grab() -> string
    u = "zz"
    return "!"
end
u = u .. grab()
print(u)
"#;
    expect("r_sappend", src, "abab-427A\n64\nq!\n");
}

#[test]
fn run_plot_y_offset() {
    expect(
        "r_ploty",
        "layer2_visible(true)\nbyte y = 10\nword x = 300\nplot(5, y + 3, 7)\nplot(6, y - 2, 8)\nplot(x - 290, y + 250, 9)\nprint(point(5, 13), point(6, 8), point(10, 4))",
        "7 8 9\n",
    );
}

/// The built-in assembler must produce exactly the NEX SjASMPlus makes, for
/// every example (runs only where SjASMPlus is installed: SJASMPLUS or tools/).
#[test]
fn builtin_assembler_matches_sjasmplus() {
    let Some(sj) = sjasmplus() else { return };
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "lz80n") {
            continue;
        }
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let src = std::fs::read_to_string(&path).unwrap();
        let Ok(asm) = crate::compile_main(&src, Some(&path), "t.nex", true, &dir, false) else { continue };
        let mine = crate::asm::assemble(&asm, &dir).unwrap_or_else(|e| panic!("{stem}: {}", e[0])).nex.unwrap().1;
        let tmp = std::env::temp_dir().join(format!("lz80nc_cmp_{stem}"));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("t.asm"), &asm).unwrap();
        assert!(Command::new(&sj).current_dir(&tmp).args(["--nologo", "t.asm"]).status().unwrap().success());
        let theirs = std::fs::read(tmp.join("t.nex")).unwrap();
        assert_eq!(mine.len(), theirs.len(), "{stem}: NEX size differs");
        if let Some(i) = (0..mine.len()).find(|&i| mine[i] != theirs[i]) {
            panic!("{stem}: NEX differs at offset {i} (${i:X}): built-in {:02X}, sjasmplus {:02X}", mine[i], theirs[i]);
        }
        checked += 1;
    }
    assert!(checked > 10);
}


fn run_sd(name: &str, src: &str, files: &[(&str, &[u8])]) -> (String, crate::emu::EsxFs) {
    let fs = crate::emu::EsxFs::with_files(files);
    let (out, cpu) = run_with(name, src, move |cpu| cpu.esx = Some(fs)).unwrap();
    (out, cpu.esx.unwrap())
}

#[test]
fn file_write_read_seek_size() {
    let (out, fs) = run_sd(
        "file_rw",
        r#"
byte buf[16]
byte h = file_open("out.txt", FILE_WRITE)
print(h ~= FILE_ERROR, file_status)
for i = 0, 9 do buf[i] = '0' + i end
print(file_write(h, addr(buf), 10))
file_close(h)

h = file_open("out.txt", FILE_READ)
print(file_size(h))
file_seek(h, 4)
print(file_read(h, addr(buf), 16), buf[0] - '0', file_pos(h))
print(file_read(h, addr(buf), 16))
file_close(h)

h = file_open("out.txt", FILE_APPEND)
buf[0] = 'X'
file_write(h, addr(buf), 1)
file_close(h)

h = file_open("missing.txt", FILE_READ)
print(h == FILE_ERROR, file_status)
print(file_delete("out.txt"), file_delete("out.txt"))
"#,
        &[],
    );
    assert_eq!(out, "true 0\n10\n10\n6 4 10\n0\ntrue 5\ntrue false\n");
    assert!(fs.files.is_empty());
    assert_eq!(fs.open_handles(), 0);
    assert!(!fs.mmu_not_rom, "esxDOS called with RAM in MMU slot 0/1");
    assert!(!fs.with_interrupts, "esxDOS called with interrupts enabled");
}

#[test]
fn file_append_keeps_contents() {
    let (_, fs) = run_sd(
        "file_app",
        r#"
byte b[1]
b[0] = '!'
byte h = file_open("log.txt", FILE_APPEND)
file_write(h, addr(b), 1)
file_close(h)
"#,
        &[("log.txt", b"abc")],
    );
    assert_eq!(fs.files["log.txt"], b"abc!");
}

/// Byte-by-byte reading through the buffer, across 256-byte refills, and a
/// whole file loaded into a far (banked) array in chunks.
#[test]
fn file_getc_and_load_into_far_array() {
    let data: Vec<u8> = (0..3000u32).map(|i| (i * 7 + i / 256) as u8).collect();
    let sum: u32 = data.iter().map(|&b| b as u32).sum();
    let (out, _) = run_sd(
        "file_getc",
        r#"
far byte big[20000]
byte chunk[256]

-- 1: byte by byte
byte h = file_open("data.bin", FILE_READ)
word n = 0
word sum = 0
word c = file_getc(h)
while c ~= FILE_EOF do
    sum += c
    n += 1
    c = file_getc(h)
end
file_close(h)
print(n, sum)

-- 2: into banked memory
h = file_open("data.bin", FILE_READ)
word total = 0
word got = file_read(h, addr(chunk), 256)
while got > 0 do
    for i = 0, got - 1 do big[total + i] = chunk[i] end
    total += got
    got = file_read(h, addr(chunk), 256)
end
file_close(h)
word s2 = 0
for i = 0, total - 1 do s2 += big[i] end
print(total, s2, big[2999])
"#,
        &[("data.bin", &data)],
    );
    assert_eq!(out, format!("3000 {}\n3000 {} {}\n", sum as u16, sum as u16, data[2999]));
}

/// The Rust lexer's tokens in the record format of selfhost/lexer.lz80n.
fn token_records(src: &str) -> Vec<u8> {
    use crate::lexer::Tok::*;
    let mut out = vec![];
    let head = |out: &mut Vec<u8>, kind: u8, line: u32, col: u32| {
        out.push(kind);
        out.extend((line as u16).to_le_bytes());
        out.extend((col as u16).to_le_bytes());
    };
    let text = |out: &mut Vec<u8>, s: &[u8]| {
        out.extend((s.len() as u16).to_le_bytes());
        out.extend(s);
    };
    let (toks, err) = crate::lexer::lex_partial(src);
    for t in &toks {
        let kind: u8 = match &t.tok {
            Ident(_) => 0, Int(_) => 1, DotDot => 2, Fixed(_) => 3, Str(_) => 4, AsmBody(_) => 5,
            And => 6, Or => 7, Not => 8, If => 9, Then => 10, Elseif => 11, Else => 12, End => 13,
            While => 14, Do => 15, For => 16, Function => 17, Return => 18, Break => 19, Const => 20,
            Var => 21, True => 22, False => 23, Asm => 24, Struct => 25, Class => 26, Asset => 27,
            Interrupt => 28, Far => 29, Include => 30,
            Plus => 31, Minus => 32, Star => 33, Slash => 34, Percent => 35, Amp => 36, Pipe => 37,
            Tilde => 38, Shl => 39, Shr => 40, EqEq => 41, NotEq => 42, Lt => 43, Le => 44, Gt => 45,
            Ge => 46, Assign => 47, PlusEq => 48, MinusEq => 49, StarEq => 50, AmpEq => 51,
            PipeEq => 52, SlashEq => 53, PercentEq => 54, ShlEq => 55, ShrEq => 56,
            LParen => 57, RParen => 58, LBracket => 59, RBracket => 60, LBrace => 61, RBrace => 62,
            Comma => 63, Dot => 64, Arrow => 65, Eof => 66,
        };
        head(&mut out, kind, t.pos.line, t.pos.col);
        match &t.tok {
            Ident(s) | AsmBody(s) => text(&mut out, s.as_bytes()),
            Str(s) => text(&mut out, &s.chars().map(|c| c as u32 as u8).collect::<Vec<_>>()),
            Int(v) | Fixed(v) => out.extend((*v as u32).to_le_bytes()),
            _ => {}
        }
    }
    if let Some(e) = err {
        head(&mut out, 255, e.pos.line, e.pos.col);
        let m = e.msg.as_bytes();
        text(&mut out, &m[..m.len().min(64)]);
    }
    out
}

/// Run selfhost/lexer.lz80n in the emulator on `src`; its tokens.bin.
fn selfhost_lex(nex: &[u8], syms: &HashMap<String, u16>, src: &str) -> Vec<u8> {
    let mut cpu = load_nex(nex).unwrap();
    cpu.esx = Some(crate::emu::EsxFs::with_files(&[("in.lz80n", src.as_bytes())]));
    let end = syms["__main_end"];
    let mut n = 0u64;
    while cpu.pc != end {
        cpu.tick().unwrap();
        n += 1;
        assert!(n < 2_000_000_000, "self-hosted lexer did not finish");
    }
    cpu.esx.unwrap().files.remove("tokens.bin").expect("no tokens.bin")
}

#[test]
fn selfhost_lexer_matches_rust() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let lexer_src = std::fs::read_to_string(root.join("selfhost/lexer.lz80n")).unwrap();
    let (nex, syms) = build("selfhost_lexer", &lexer_src).unwrap();
    let mut inputs: Vec<(String, String)> = vec![];
    for entry in std::fs::read_dir(root.join("examples")).unwrap() {
        let p = entry.unwrap().path();
        if p.extension().is_some_and(|e| e == "lz80n") {
            inputs.push((p.display().to_string(), std::fs::read_to_string(&p).unwrap()));
        }
    }
    inputs.push(("selfhost/lexer.lz80n".into(), lexer_src.clone()));
    for (name, m) in [
        ("FILE", crate::prelude::FILE),
        ("SOUND", crate::prelude::SOUND),
        ("LAYER2", crate::prelude::LAYER2),
        ("SPRITES", crate::prelude::SPRITES),
        ("PALETTE", crate::prelude::PALETTE),
        ("COPPER", crate::prelude::COPPER),
    ] {
        inputs.push((format!("prelude {name}"), m.to_string()));
    }
    for (i, s) in [
        "x = 0x1F + $ff - 0b101 + 1_000 + 4294967295 'a' '\\n' '\\0'",
        "a <<= 1 b >>= 2 c != d ~= e -> f .. g . h <= >= << >> == = += -= *= /= %= &= |=",
        "f = 1.5 + 0.1 + 32767.99 + 3.14159265358979 + 0.5",
        "s = \"tab\\t nl\\n q\\\" bs\\\\ z\\0\"",
        "--[[ block\n comment ]] x --[[ unterminated",
        "x = 1 @",
        "y = 4294967296",
        "z = 0x",
        "w = 12ab",
        "s = \"open",
        "s = \"bad \\q\"",
        "c = 'ab'",
        "f = 32768.5",
        "asm x\nend",
        "asm\n  ld a,1  \n  nop ; end\n end ; done\nx",
        "asm\n  nop\n",
        "if a then b elseif c else d end while x do end for i = 1, 2 do break end",
        "! x",
        "",
        "   \n\t  -- only a comment",
    ]
    .iter()
    .enumerate()
    {
        inputs.push((format!("case {i}: {s:?}"), s.to_string()));
    }
    for (name, src) in &inputs {
        let want = token_records(src);
        let got = selfhost_lex(&nex, &syms, src);
        if got != want {
            let i = (0..want.len().min(got.len())).find(|&i| got[i] != want[i]).unwrap_or(want.len().min(got.len()));
            panic!(
                "{name}: token streams differ at byte {i} (lengths {} / {})\n rust: {:?}\n self: {:?}",
                want.len(),
                got.len(),
                &want[i.saturating_sub(8)..(i + 24).min(want.len())],
                &got[i.saturating_sub(8)..(i + 24).min(got.len())]
            );
        }
    }
}



/// The canonical text form of a syntax tree (the output of selfhost/parser.lz80n).
mod ast_text {
    use crate::ast::*;

    fn quoted(out: &mut String, bytes: &[u8]) {
        out.push('"');
        for &c in bytes {
            if !(32..=126).contains(&c) || c == b'"' || c == b'\\' {
                out.push_str(&format!("\\x{c:02X}"));
            } else {
                out.push(c as char);
            }
        }
        out.push('"');
    }

    fn open(out: &mut String, kind: &str, pos: crate::diag::Pos) {
        out.push_str(&format!("({kind} {}:{}", pos.line, pos.col));
    }

    fn ty(t: &TypeRef) -> String {
        match t {
            TypeRef::Prim(p) => p.name().to_string(),
            TypeRef::Named(n) => n.clone(),
        }
    }

    fn bin(op: BinOp) -> &'static str {
        use BinOp::*;
        match op {
            Add => "add", Sub => "sub", Mul => "mul", Div => "div", Mod => "mod", Concat => "concat",
            And => "and", Or => "or", Xor => "xor", Shl => "shl", Shr => "shr", Eq => "eq", Ne => "ne",
            Lt => "lt", Le => "le", Gt => "gt", Ge => "ge", LAnd => "land", LOr => "lor",
        }
    }

    fn opt(out: &mut String, e: &Option<Expr>) {
        match e {
            Some(e) => {
                out.push(' ');
                expr(out, e);
            }
            None => out.push_str(" _"),
        }
    }

    fn exprs(out: &mut String, list: &[Expr]) {
        for e in list {
            out.push(' ');
            expr(out, e);
        }
    }

    pub fn expr(out: &mut String, e: &Expr) {
        match &e.kind {
            ExprKind::Int(v) => {
                open(out, "int", e.pos);
                out.push_str(&format!(" {v}"));
            }
            ExprKind::Fixed(v) => {
                open(out, "fixed", e.pos);
                out.push_str(&format!(" {v}"));
            }
            ExprKind::Bool(b) => {
                open(out, "bool", e.pos);
                out.push_str(if *b { " true" } else { " false" });
            }
            ExprKind::Str(s) => {
                open(out, "str", e.pos);
                out.push(' ');
                quoted(out, &s.chars().map(|c| c as u32 as u8).collect::<Vec<_>>());
            }
            ExprKind::Name(n) => {
                open(out, "name", e.pos);
                out.push_str(&format!(" {n}"));
            }
            ExprKind::Index(b, i) => {
                open(out, "index", e.pos);
                out.push(' ');
                expr(out, b);
                out.push(' ');
                expr(out, i);
            }
            ExprKind::Field(b, n) => {
                open(out, "field", e.pos);
                out.push(' ');
                expr(out, b);
                out.push_str(&format!(" {n}"));
            }
            ExprKind::Call(n, args) => {
                open(out, "call", e.pos);
                out.push_str(&format!(" {n}"));
                exprs(out, args);
            }
            ExprKind::MethodCall(o, n, args) => {
                open(out, "method", e.pos);
                out.push(' ');
                expr(out, o);
                out.push_str(&format!(" {n}"));
                exprs(out, args);
            }
            ExprKind::Unary(op, x) => {
                open(out, "un", e.pos);
                out.push_str(match op {
                    UnOp::Neg => " neg",
                    UnOp::Not => " not",
                    UnOp::BitNot => " bitnot",
                });
                out.push(' ');
                expr(out, x);
            }
            ExprKind::Binary(op, l, r) => {
                open(out, "bin", e.pos);
                out.push_str(&format!(" {}", bin(*op)));
                out.push(' ');
                expr(out, l);
                out.push(' ');
                expr(out, r);
            }
            ExprKind::List(items) => {
                open(out, "list", e.pos);
                exprs(out, items);
            }
        }
        out.push(')');
    }

    fn body(out: &mut String, list: &[Stmt]) {
        out.push_str(" (body");
        for s in list {
            out.push(' ');
            stmt(out, s);
        }
        out.push(')');
    }

    pub fn stmt(out: &mut String, s: &Stmt) {
        match &s.kind {
            StmtKind::Far(inner) => {
                open(out, "far", s.pos);
                out.push(' ');
                stmt(out, inner);
            }
            StmtKind::VarDecl { ty: t, name, array, init, is_const } => {
                open(out, "var", s.pos);
                out.push_str(&format!(" {}", t.as_ref().map(ty).unwrap_or("_".into())));
                out.push_str(&format!(" {name}"));
                opt(out, array);
                opt(out, init);
                out.push_str(if *is_const { " const" } else { " -" });
            }
            StmtKind::Assign { target, op, value } => {
                open(out, "assign", s.pos);
                out.push_str(&format!(" {}", op.map(bin).unwrap_or("=")));
                out.push(' ');
                expr(out, target);
                out.push(' ');
                expr(out, value);
            }
            StmtKind::If { arms, otherwise } => {
                open(out, "if", s.pos);
                for (c, b) in arms {
                    out.push_str(" (arm ");
                    expr(out, c);
                    body(out, b);
                    out.push(')');
                }
                if let Some(o) = otherwise {
                    out.push_str(" (else");
                    for s in o {
                        out.push(' ');
                        stmt(out, s);
                    }
                    out.push(')');
                }
            }
            StmtKind::While { cond, body: b } => {
                open(out, "while", s.pos);
                out.push(' ');
                expr(out, cond);
                body(out, b);
            }
            StmtKind::For { var, start, end, step, body: b } => {
                open(out, "for", s.pos);
                out.push_str(&format!(" {var} "));
                expr(out, start);
                out.push(' ');
                expr(out, end);
                opt(out, step);
                body(out, b);
            }
            StmtKind::Return(e) => {
                open(out, "return", s.pos);
                opt(out, e);
            }
            StmtKind::Break => open(out, "break", s.pos),
            StmtKind::Expr(e) => {
                open(out, "expr", s.pos);
                out.push(' ');
                expr(out, e);
            }
            StmtKind::Asm(b) => {
                open(out, "asm", s.pos);
                out.push(' ');
                quoted(out, b.as_bytes());
            }
            StmtKind::Asset { name, path, image } => {
                open(out, "asset", s.pos);
                out.push_str(&format!(" {name} "));
                quoted(out, &path.chars().map(|c| c as u32 as u8).collect::<Vec<_>>());
                out.push_str(if *image { " image" } else { " file" });
            }
        }
        out.push(')');
    }

    fn func(out: &mut String, f: &Func) {
        open(out, "func", f.pos);
        out.push_str(&format!(" {} (params", f.name));
        for (n, t) in &f.params {
            out.push_str(&format!(" (p {n} {})", ty(t)));
        }
        out.push(')');
        let ret = match (&f.ret, f.ret_str, &f.ret_named) {
            (Some(t), _, _) => t.name().to_string(),
            (None, true, _) => "string".into(),
            (None, false, Some(n)) => n.clone(),
            _ => "_".into(),
        };
        out.push_str(&format!(" {ret}"));
        body(out, &f.body);
        out.push(')');
    }

    pub fn program(p: &Program) -> String {
        let mut out = String::new();
        for item in &p.items {
            match item {
                Item::Func(f) => func(&mut out, f),
                Item::Interrupt(line, b, pos) => {
                    open(&mut out, "interrupt", *pos);
                    match line {
                        Some(e) => {
                            out.push(' ');
                            expr(&mut out, e);
                        }
                        None => out.push_str(" frame"),
                    }
                    body(&mut out, b);
                    out.push(')');
                }
                Item::Struct(sd) => {
                    open(&mut out, "struct", sd.pos);
                    out.push_str(&format!(" {} {} (fields", sd.name, sd.parent.as_deref().unwrap_or("_")));
                    for f in &sd.fields {
                        out.push(' ');
                        open(&mut out, "f", f.pos);
                        out.push_str(&format!(" {} {}", f.name, ty(&f.ty)));
                        opt(&mut out, &f.array);
                        opt(&mut out, &f.init);
                        out.push(')');
                    }
                    out.push_str(") (methods");
                    for m in &sd.methods {
                        out.push(' ');
                        func(&mut out, m);
                    }
                    out.push_str("))");
                }
                Item::Stmt(s) => stmt(&mut out, s),
            }
            out.push('\n');
        }
        out
    }

    /// Lex + parse with the Rust front end; the tree or `error L:C message`.
    pub fn of_source(src: &str) -> String {
        let r = crate::lexer::lex(src).and_then(crate::parser::parse);
        match r {
            Ok(p) => program(&p),
            Err(d) => format!("error {}:{} {}\n", d.pos.line, d.pos.col, d.msg),
        }
    }
}

/// Run a self-hosted stage (a NEX) on the virtual SD card `files`; the files afterwards.
fn run_stage(
    nex: &[u8],
    syms: &HashMap<String, u16>,
    files: std::collections::HashMap<String, Vec<u8>>,
) -> std::collections::HashMap<String, Vec<u8>> {
    let mut cpu = load_nex(nex).unwrap();
    let mut fs = crate::emu::EsxFs::default();
    fs.files = files;
    cpu.esx = Some(fs);
    let end = syms["__main_end"];
    let mut n = 0u64;
    while cpu.pc != end {
        if let Err(e) = cpu.tick() {
            panic!("self-hosted stage crashed: {e}");
        }
        n += 1;
        assert!(n < 4_000_000_000, "self-hosted stage did not finish");
    }
    cpu.esx.unwrap().files
}

#[test]
fn selfhost_parser_matches_rust() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let lexer_src = std::fs::read_to_string(root.join("selfhost/lexer.lz80n")).unwrap();
    let parser_src = std::fs::read_to_string(root.join("selfhost/parser.lz80n")).unwrap();
    let (lex_nex, lex_syms) = build("selfhost_lexer2", &lexer_src).unwrap();
    let (par_nex, par_syms) = build_in(&root.join("selfhost"), &parser_src);
    let mut inputs: Vec<(String, String)> = vec![];
    for entry in std::fs::read_dir(root.join("examples")).unwrap() {
        let p = entry.unwrap().path();
        if p.extension().is_some_and(|e| e == "lz80n") {
            inputs.push((p.display().to_string(), std::fs::read_to_string(&p).unwrap()));
        }
    }
    inputs.push(("selfhost/lexer.lz80n".into(), lexer_src.clone()));
    inputs.push(("selfhost/parser.lz80n".into(), parser_src.clone()));
    inputs.push(("selfhost/parser_core.lz80n".into(), std::fs::read_to_string(root.join("selfhost/parser_core.lz80n")).unwrap()));
    for (name, m) in [
        ("FILE", crate::prelude::FILE),
        ("SOUND", crate::prelude::SOUND),
        ("LAYER2", crate::prelude::LAYER2),
        ("SPRITES", crate::prelude::SPRITES),
        ("PALETTE", crate::prelude::PALETTE),
        ("COPPER", crate::prelude::COPPER),
    ] {
        inputs.push((format!("prelude {name}"), m.to_string()));
    }
    for (i, s) in [
        "x = -5 y = --5 z = -1.5 w = -(3) v = - - 2.25",
        "a = 1 + 2 * 3 - 4 / 5 % 6 .. \"s\" << 1 >> 2 & 3 ~ 4 | 5 == 6 ~= 7 < 8 <= 9 > 10 >= 11 and not b or c",
        "x += 1 x -= 1 x *= 2 x /= 2 x %= 3 x &= 4 x |= 5 x <<= 1 x >>= 1 a[i].f.g[2] = h(1, 2).m(3)",
        "if a then b() elseif c then d() elseif e then else f() end if x then end",
        "for i = 1, 10, 2 do break end for j = a, b do return end while x do return 5 end",
        "byte a[] = {1, 2, 3} word m[2][3] = {{1,2,3},{4,5,6}} far byte big[1000] const K = 4 var v = K",
        "Vec2 p = {1, 2} string s = \"x\\ty\" bool b",
        "struct P\n byte x = 1\n word t[4]\nend\nclass B extends P\n function m(byte a, P o) -> word return a end\n function n() end\nend",
        "function f(byte a, word b) -> string return \"\" end function g() -> Vec2 end function h() end",
        "interrupt raster(96) nextreg[0x4A] = 1 end interrupt frame ticks += 1 end",
        "asset s = \"a.png\" asset i = image(\"b.png\")",
        "asm\n ld a,1\nend",
        "x = 4294967295 y = 0x80000000",
        "x = ",
        "byte a[] = 5",
        "byte b = {1}",
        "byte c[]",
        "byte d[2][]",
        "1 = x",
        "f",
        "f(1)(2)",
        "far byte q",
        "if x then",
        "function f() function g() end end",
        "interrupt timer end",
        "asset a = 5",
        "asset a = image(5)",
        "struct S 5 end",
        "x = {}",
        "x = (1",
        "x = 1.5.y",
        "return",
        "x = 99999999999",
        "f(1, )",
        "a.b.5",
    ]
    .iter()
    .enumerate()
    {
        inputs.push((format!("case {i}: {s:?}"), s.to_string()));
    }
    for (name, src) in &inputs {
        let want = ast_text::of_source(src);
        let mut files = std::collections::HashMap::new();
        files.insert("in.lz80n".to_string(), src.as_bytes().to_vec());
        let files = run_stage(&lex_nex, &lex_syms, files);
        let files = run_stage(&par_nex, &par_syms, files);
        let got = String::from_utf8_lossy(files.get("ast.txt").expect("no ast.txt")).to_string();
        if got != want {
            let (mut wl, mut gl) = (want.lines(), got.lines());
            let mut line = 1;
            loop {
                match (wl.next(), gl.next()) {
                    (Some(a), Some(b)) if a == b => line += 1,
                    (a, b) => {
                        let cut = |s: Option<&str>| s.map(|s| s.chars().take(600).collect::<String>()).unwrap_or_default();
                        let (a, b) = (cut(a), cut(b));
                        let i = a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count();
                        panic!("{name}: line {line} differs at column {i}\n rust: {}\n self: {}", &a[i.saturating_sub(60).min(a.len())..], &b[i.saturating_sub(60).min(b.len())..]);
                    }
                }
            }
        }
    }
}


/// Assemble `asm` with selfhost/asm.lz80n in the emulator: (NEX or the
/// asm.err text, T-states).
fn selfhost_assemble(nex: &[u8], syms: &HashMap<String, u16>, asm: &str) -> (Result<Vec<u8>, String>, u64) {
    let mut cpu = load_nex(nex).unwrap();
    cpu.esx = Some(crate::emu::EsxFs::with_files(&[("in.asm", asm.as_bytes())]));
    let end = syms["__main_end"];
    let mut n = 0u64;
    while cpu.pc != end {
        if let Err(e) = cpu.tick() {
            panic!("self-hosted assembler crashed: {e}");
        }
        n += 1;
        assert!(n < 20_000_000_000, "self-hosted assembler did not finish");
    }
    let t = cpu.t_states;
    let mut files = cpu.esx.unwrap().files;
    if let Some(e) = files.remove("asm.err") {
        return (Err(String::from_utf8_lossy(&e).to_string()), t);
    }
    (files.remove("t.nex").ok_or_else(|| "no t.nex written".to_string()), t)
}

fn selfhost_asm_check(names: &[&str]) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let asm_src = std::fs::read_to_string(root.join("selfhost/asm.lz80n")).unwrap();
    let (nex, syms) = build("selfhost_asm", &asm_src).unwrap();
    let dir = root.join("examples");
    for name in names {
        let path = dir.join(format!("{name}.lz80n"));
        let src = std::fs::read_to_string(&path).unwrap();
        let asm = crate::compile_main(&src, Some(&path), "t.nex", true, &dir, false).unwrap();
        let want = crate::asm::assemble(&asm, &dir).unwrap().nex.unwrap().1;
        let (got, t) = selfhost_assemble(&nex, &syms, &asm);
        let got = got.unwrap_or_else(|e| panic!("{name}: self-hosted assembler: {e}"));
        eprintln!("{name}: {} lines, {:.1} s at 28 MHz", asm.lines().count(), t as f64 / 28e6);
        assert_eq!(got.len(), want.len(), "{name}: NEX size differs");
        if let Some(i) = (0..want.len()).find(|&i| got[i] != want[i]) {
            let a = i.saturating_sub(6);
            let r = crate::asm::assemble(&asm, &dir).unwrap();
            let mut near: Vec<(&String, &i64)> = r.symbols.iter().filter(|(_, v)| **v >= 0x8000 && **v <= 0x8000 + (i as i64 - 512)).collect();
            near.sort_by_key(|x| *x.1);
            panic!(
                "{name}: NEX differs at offset {i} (${i:X}): self-hosted {:02X}, Rust {:02X}\n self {:02X?}\n rust {:02X?}\n after {:?}",
                got[i], want[i], &got[a..i + 10], &want[a..i + 10], near.last()
            );
        }
    }
}

/// selfhost/asm.lz80n makes the same NEX as the Rust assembler.
#[test]
fn selfhost_asm_matches_rust() {
    selfhost_asm_check(&["hello", "strings"]);
}

/// Every example (slow: `cargo test --release selfhost_asm_all -- --ignored`).
#[test]
#[ignore]
fn selfhost_asm_all_examples() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| {
            let p = e.unwrap().path();
            (p.extension()? == "lz80n").then(|| p.file_stem().unwrap().to_string_lossy().to_string())
        })
        .collect();
    names.sort();
    selfhost_asm_check(&names.iter().map(|s| s.as_str()).collect::<Vec<_>>());
}

/// Every instruction form of the Rust encoder's test table, plus data
/// directives and expressions, through both assemblers.
#[test]
fn selfhost_asm_all_instruction_forms() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let enc = std::fs::read_to_string(root.join("compiler/src/asm/encode.rs")).unwrap();
    let mut body = String::from("    DEVICE ZXSPECTRUMNEXT\n    ORG $8000\nlab equ $8010\nlen equ 7\nstart:\n");
    for line in enc.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("(\"") {
            if let Some((insn, _)) = rest.split_once("\", &[") {
                // jr/djnz targets are relative: tested below with a near label
                if insn.starts_with("jr") || insn.starts_with("djnz") {
                    continue;
                }
                body.push_str(&format!("    {}\n", insn.replace("\\\"", "\"")));
            }
        }
    }
    body.push_str(
        r#"    db "text",0,'a','bc',"q\"\n"
    db -1, 255, %1010, 0b11, 0x1F, 1Fh, 101b, 99d, 'A'+1, high $1234, low($1234), (lab >> 8) & $FF
    dw lab, -2, $FFFF, lab - start, 2 * 3 + 4, 17 / 5, 17 % 5, 1 << 4 | 1, ~0 & $FF, !0, 3 > 2, 2 <= 1
    dd 70000, -1
    ds 5
    ds 3, $AA
    align 16
.local: dw .local, start.local
    ex (sp),iy
    in f,(c)
    bsra de,b
    bsrf de,b
    brlc de,b
    mirror
    mul
    sll b
    sli c
    ld iyh,5
    ld (ix-128),b
    out (c),b
    jr .local
    jr nc,.local
    djnz .local
    jr $
    MMU 1 n, 40
    ORG $3FFE
    db 1,2,3,4
    SAVENEX OPEN "t.nex", start, $FDFD, 2
    SAVENEX CORE 3,1,10
    SAVENEX CFG 5, $1234, 1
    SAVENEX AUTO
    SAVENEX CLOSE
"#,
    );
    let want = crate::asm::assemble(&body, &root).unwrap_or_else(|e| panic!("rust: {}\n{body}", e[0])).nex.unwrap().1;
    let src = std::fs::read_to_string(root.join("selfhost/asm.lz80n")).unwrap();
    let (nex, syms) = build("selfhost_asm_forms", &src).unwrap();
    let got = selfhost_assemble(&nex, &syms, &body).0.unwrap_or_else(|e| panic!("self-hosted: {e}"));
    assert_eq!(got.len(), want.len());
    if let Some(i) = (0..want.len()).find(|&i| got[i] != want[i]) {
        panic!("differs at offset {i}: self {:02X?} rust {:02X?}", &got[i.saturating_sub(4)..i + 8], &want[i.saturating_sub(4)..i + 8]);
    }
}

#[test]
fn selfhost_asm_errors() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let src = std::fs::read_to_string(root.join("selfhost/asm.lz80n")).unwrap();
    let (nex, syms) = build("selfhost_asm_err", &src).unwrap();
    for (asm, want) in [
        ("    ORG $8000\n    ld a,nosuch\n", "2: undefined symbol `nosuch`"),
        ("    ORG $8000\n    jr far\n    ds 200\nfar: nop\n", "2: jr/djnz target out of range"),
        ("    ORG $8000\n    frobnicate a\n", "2: unknown instruction `frobnicate`"),
        ("    ORG $8000\n    ld (hl),(hl)\n", "2: invalid instruction `(hl),(hl)`"),
        ("x: nop\nx: nop\n", "2: label `x` defined twice"),
        ("    ORG $8000\n    ld a,300\n", "2: value does not fit in a byte: `300`"),
        ("    MMU 1 e, 28\n    ORG $3FFF\n    db 1,2\n", "3: data does not fit in an MMU slot"),
        ("    ASSERT 1 > 2, \"too big\"\n", "1: too big"),
    ] {
        let (r, _) = selfhost_assemble(&nex, &syms, asm);
        assert_eq!(r.err().as_deref().map(str::trim_end), Some(want), "{asm}");
        assert!(crate::asm::assemble(asm, &root).is_err(), "the Rust assembler accepts {asm}");
    }
}

/// The runtime files the self-hosted code generator copies from the SD card.
fn selfhost_runtime_files() -> Vec<(String, Vec<u8>)> {
    crate::selfhost_support_files()
}

struct SelfHost {
    lex: (Vec<u8>, HashMap<String, u16>),
    cg: (Vec<u8>, HashMap<String, u16>),
}

impl SelfHost {
    fn new() -> SelfHost {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let lexer_src = std::fs::read_to_string(root.join("selfhost/lexer.lz80n")).unwrap();
        let cg_src = std::fs::read_to_string(root.join("selfhost/codegen.lz80n")).unwrap();
        SelfHost { lex: build("sh_lex", &lexer_src).unwrap(), cg: build_in(&root.join("selfhost"), &cg_src) }
    }

    /// The assembly the self-hosted compiler makes for `src`, or its error.
    fn compile(&self, src: &str) -> Result<String, String> {
        self.compile_files(&[("in.lz80n", src.as_bytes())])
    }

    /// Like compile, with more source files on the SD card (includes).
    fn compile_files(&self, sources: &[(&str, &[u8])]) -> Result<String, String> {
        let mut files = std::collections::HashMap::new();
        for (n, d) in sources {
            files.insert(n.to_string(), d.to_vec());
        }
        for (n, d) in selfhost_runtime_files() {
            files.insert(n, d);
        }
        let files = run_stage(&self.lex.0, &self.lex.1, files);
        let files = run_stage(&self.cg.0, &self.cg.1, files);
        if let Some(e) = files.get("codegen.err") {
            return Err(String::from_utf8_lossy(e).trim_end().to_string());
        }
        Ok(String::from_utf8_lossy(files.get("out.asm").expect("no out.asm")).to_string())
    }
}

/// Run an assembled program in the emulator: what it prints.
fn run_asm(asm: &str) -> String {
    run_asm_cpu(asm).0
}

/// Run an assembled program in the emulator: what it prints, and the machine.
fn run_asm_cpu(asm: &str) -> (String, Cpu) {
    let out = crate::asm::assemble(asm, std::path::Path::new(".")).unwrap_or_else(|e| panic!("assembler: {}", e[0]));
    let syms = out.symbols16();
    let mut cpu = load_nex(&out.nex.unwrap().1).unwrap();
    cpu.esx = Some(crate::emu::EsxFs::default());
    let (putc, newline, end) = (syms["rt_putc"], syms["rt_newline"], syms["__main_end"]);
    let pchr = syms.get("rt_pp_chr").copied().unwrap_or(0xFFFF);
    let mut s = String::new();
    for _ in 0..200_000_000u64 {
        if cpu.pc == end {
            return (s, cpu);
        }
        if (cpu.pc == putc || cpu.pc == pchr) && cpu.a != 10 {
            s.push(cpu.a as char);
        } else if cpu.pc == newline {
            s.push('\n');
        }
        cpu.tick().unwrap_or_else(|e| panic!("emulator: {e}\noutput so far:\n{s}"));
    }
    panic!("program did not terminate; output so far:\n{s}");
}

/// Programs compiled by selfhost/codegen.lz80n print what the same programs
/// compiled by the Rust compiler print.
#[test]
fn selfhost_codegen_runs_like_rust() {
    let sh = SelfHost::new();
    let programs: &[&str] = &[
        r#"print("Hi", 42, true, false, 300, 70000 - 4465)"#,
        r#"
byte a = 200
byte b = 100
word w = a + b
byte c = a + b
print(w, c, a * 2, a / 3, a % 7, a - b, b - a)
sbyte s = -5
sword t = -1000
print(s, t, s * 3, t / 7, t % 7, s - 120, -s, 0 - t)
print(a & 0x0F, a | 1, a ~ 0xFF, ~a, a << 2, a >> 3, s >> 1, t >> 2, w << 7)
word big = 65535
print(big + 1, big * big, word(s), sword(big), byte(t), sbyte(200), sword(s))
"#,
        r#"
const K = 3 * 4
const NEG = -7
byte x = K
print(K, NEG, x + NEG, K << 2)
var v = 1000
var u = 7
print(v, u)
"#,
        r#"
byte a = 5
word w = 300
sbyte s = -3
print(a < 6, a <= 5, a > 5, a >= 6, a == 5, a ~= 5, w > a, s < a, s < 0, w == 300)
print(a > 3 and w < 1000, a > 9 or s < 0, not (a == 5), not true)
bool b = a > 4
print(b, not b)
"#,
        r#"
byte n = 7
if n > 5 then
    print("big")
elseif n > 2 then
    print("mid")
else
    print("small")
end
for i = 1, 5 do write(i, " ") end
print("")
for i = 10, 0, -3 do write(i, " ") end
print("")
for i = 250, 255 do write(i, " ") end
print("")
for i = 0, 20, 7 do write(i, " ") end
print("")
for i = -3, 3 do write(i, " ") end
print("")
word k = 0
while true do
    k += 1
    if k == 4 then break end
end
print(k)
byte total = 0
for i = 1, 10 do
    if i == 8 then break end
    total += i
end
print(total)
"#,
        r#"
function add(byte x, word y) -> word
    return x + y
end
function sq(byte x) -> word
    return x * x
end
function neg(sword v) -> sword
    return -v
end
function even(byte v) -> bool
    return v % 2 == 0
end
function hello()
    print("hello")
end
hello()
print(add(5, 1000), sq(200), neg(-300), neg(7), even(4), even(7), add(byte(sq(3)), sq(byte(sq(2)))))
"#,
        r#"
byte t[5]
word ws[] = {100, 200, 300}
byte pr[] = {2, 3, 5, 7, 11}
for i = 0, 4 do t[i] = i * i end
t[2] += 10
print(t[0], t[1], t[2], t[3], t[4], ws[0] + ws[2], pr[4], ws[1])
word sum = 0
for i = 0, 4 do sum += pr[i] * t[i] end
print(sum)
memory[0xC000] = 77
print(memory[0xC000])
function fill(byte v)
    byte loc[3]
    for i = 0, 2 do loc[i] = v + i end
    print(loc[0], loc[1], loc[2])
end
fill(9)
"#,
        r#"
word f0 = 0
word f1 = 1
for i = 1, 20 do
    word f2 = f0 + f1
    f0 = f1
    f1 = f2
end
print(f1)
function gcd(word a, word b) -> word
    while b ~= 0 do
        word t = a % b
        a = b
        b = t
    end
    return a
end
print(gcd(1071, 462), gcd(17, 5))
word primes = 0
for n = 2, 200 do
    bool p = true
    for d = 2, 14 do
        if d < n and n % d == 0 then
            p = false
            break
        end
    end
    if p then primes += 1 end
end
print(primes)
"#,
        r#"
byte r
asm
    ld a,42
    ld (g_r),a
end
print(r)
"#,
    ];
    for (i, src) in programs.iter().enumerate() {
        let want = run(&format!("sh_cg_{i}"), src).unwrap();
        let asm = sh.compile(src).unwrap_or_else(|e| panic!("program {i}: self-hosted compiler: {e}\n{src}"));
        let got = run_asm(&asm);
        assert_eq!(got, want, "program {i}:\n{src}");
    }
}

/// Strings, long, far arrays, addr(), the file module, includes and code
/// banking through the self-hosted compiler.
#[test]
fn selfhost_codegen_strings_long_far_files() {
    let sh = SelfHost::new();
    let programs: &[&str] = &[
        r#"
string a = "Hello"
string b = a .. ", " .. "Next" .. "!"
print(b, len(b), asc(b, 1), asc(b, 99))
string c = chr(65) .. chr(66) .. 7 .. true .. -3
print(c, str(1234) .. "x", c == "AB7true-3", c ~= b, "abc" < "abd", "b" > "abc", "ab" <= "ab", "" < "a")
string arr[] = {"zero", "one", "two"}
arr[1] = arr[1] .. "!"
print(arr[0], arr[1], arr[2], len(arr[2]))
function shout(string s, byte n) -> byte
    s = s .. "?"
    return len(s) + n
end
string m = "hm"
print(shout(m, 1), m)
string w = ""
for i = 0, 9 do w = w .. i end
print(w)
"#,
        r#"
long x = 100000
long y = -70000
print(x + y, x - y, x * 3, y / 7, y % 7, -x, x / 3 * 3, x > y, y < 0, x == 100000)
long big = 0x7FFFFFFF
print(big, big + 1, x << 4, y >> 3, x & 0xFFFF, x | 1, x ~ 255)
word w = 65535
long z = w + 1
print(z, long(w) * long(w), word(z + 5), byte(x), sword(y))
function sq(long v) -> long
    return v * v
end
print(sq(46340), sq(-3))
long acc = 0
for i = 1, 1000 do acc += i * i end
print(acc)
"#,
        r#"
far byte big[20000]
far word ws[5000]
for i = 0, 19999, 997 do big[i] = byte(i % 251) end
ws[4999] = 12345
ws[0] = ws[4999] + 1
word s = 0
for i = 0, 19999, 997 do s += big[i] end
print(s, ws[0], ws[4999], big[1])
byte buf[4]
buf[2] = 9
print(addr(buf) + 2 == addr(buf[2]), memory[addr(buf) + 2])
"#,
        r#"
byte b[8]
byte h = file_open("t.txt", FILE_WRITE)
for i = 0, 7 do b[i] = 'a' + i end
print(file_write(h, addr(b), 8))
file_close(h)
h = file_open("t.txt", FILE_READ)
print(file_size(h))
word c = file_getc(h)
string got = ""
while c ~= FILE_EOF do
    got = got .. chr(byte(c))
    c = file_getc(h)
end
file_close(h)
print(got, file_delete("t.txt"), file_open("t.txt", FILE_READ) == FILE_ERROR)
"#,
    ];
    for (i, src) in programs.iter().enumerate() {
        let (want, _) = run_sd(&format!("sh_cg2_{i}"), src, &[]);
        let asm = sh.compile(src).unwrap_or_else(|e| panic!("program {i}: self-hosted compiler: {e}\n{src}"));
        let got = run_asm(&asm);
        assert_eq!(got, want, "program {i}:\n{src}");
    }

    // include (each file once)
    let main = "include \"lib.lz80n\"\ninclude \"lib.lz80n\"\nprint(twice(21), K)\n";
    let lib = "const K = 5\nfunction twice(byte x) -> word\n    return x * 2\nend\n";
    let asm = sh.compile_files(&[("in.lz80n", main.as_bytes()), ("lib.lz80n", lib.as_bytes())]).unwrap();
    assert_eq!(run_asm(&asm), "42 5\n");

    // code banking: more code than the resident area holds
    let mut big = String::new();
    for f in 0..50 {
        big.push_str(&format!("function f{f}(word x) -> word\n    word r = x\n"));
        for k in 0..10 {
            big.push_str(&format!("    r = r * 3 + {k} - (r >> 2) + (r & {f})\n"));
        }
        big.push_str("    return r\nend\n");
    }
    big.push_str("word t = 0\n");
    for f in 0..50 {
        big.push_str(&format!("t = t + f{f}({f})\n"));
    }
    big.push_str("print(t)\n");
    let want = run("sh_cg2_big", &big).unwrap();
    let asm = sh.compile(&big).unwrap_or_else(|e| panic!("big: {e}"));
    assert!(asm.contains("MMU 2 e"), "no banked code");
    assert_eq!(run_asm(&asm), want);
}

/// Run a NEX made from `asm` with SD card `files`; the files afterwards.
fn run_asm_files(asm: &str, files: std::collections::HashMap<String, Vec<u8>>) -> std::collections::HashMap<String, Vec<u8>> {
    let out = crate::asm::assemble(asm, std::path::Path::new(".")).unwrap_or_else(|e| panic!("assembler: {}", e[0]));
    let syms = out.symbols16();
    run_stage(&out.nex.unwrap().1, &syms, files)
}

/// The self-hosted compiler compiles the self-hosted lexer; that lexer
/// makes the same tokens as the lexer compiled by lz80nc.
#[test]
fn selfhost_compiles_lexer() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let sh = SelfHost::new();
    let lexer_src = std::fs::read_to_string(root.join("selfhost/lexer.lz80n")).unwrap();
    let asm = sh.compile(&lexer_src).unwrap_or_else(|e| panic!("self-hosted compiler: {e}"));
    for input in ["examples/hello.lz80n", "examples/shooter.lz80n", "selfhost/lexer.lz80n"] {
        let src = std::fs::read(root.join(input)).unwrap();
        let mut files = std::collections::HashMap::new();
        files.insert("in.lz80n".to_string(), src.clone());
        let want = run_stage(&sh.lex.0, &sh.lex.1, files.clone()).remove("tokens.bin").unwrap();
        let got = run_asm_files(&asm, files).remove("tokens.bin").expect("no tokens.bin");
        assert_eq!(got.len(), want.len(), "{input}: token stream length");
        assert!(got == want, "{input}: token streams differ");
    }
}

/// The self-hosted compiler compiles the self-hosted parser and assembler;
/// they give the same results as their lz80nc-compiled versions.
#[test]
fn selfhost_compiles_parser_and_assembler() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let sh = SelfHost::new();
    let rd = |p: &str| std::fs::read(root.join(p)).unwrap();
    // parser
    let asm = sh
        .compile_files(&[("in.lz80n", &rd("selfhost/parser.lz80n")), ("parser_core.lz80n", &rd("selfhost/parser_core.lz80n"))])
        .unwrap_or_else(|e| panic!("self-hosted compiler (parser): {e}"));
    let (par_nex, par_syms) = build_in(&root.join("selfhost"), &String::from_utf8(rd("selfhost/parser.lz80n")).unwrap());
    for input in ["examples/hello.lz80n", "examples/shooter.lz80n"] {
        let mut files = std::collections::HashMap::new();
        files.insert("in.lz80n".to_string(), rd(input));
        let toks = run_stage(&sh.lex.0, &sh.lex.1, files);
        let want = run_stage(&par_nex, &par_syms, toks.clone()).remove("ast.txt").unwrap();
        let got = run_asm_files(&asm, toks).remove("ast.txt").expect("no ast.txt");
        assert!(got == want, "{input}: syntax trees differ");
    }
    // assembler
    let asm_src = String::from_utf8(rd("selfhost/asm.lz80n")).unwrap();
    let asm = sh.compile(&asm_src).unwrap_or_else(|e| panic!("self-hosted compiler (asm): {e}"));
    let dir = root.join("examples");
    let path = dir.join("hello.lz80n");
    let prog = crate::compile_main(&std::fs::read_to_string(&path).unwrap(), Some(&path), "t.nex", true, &dir, false).unwrap();
    let want = crate::asm::assemble(&prog, &dir).unwrap().nex.unwrap().1;
    let mut files = std::collections::HashMap::new();
    files.insert("in.asm".to_string(), prog.into_bytes());
    let got = run_asm_files(&asm, files);
    assert!(got.get("asm.err").is_none(), "{}", String::from_utf8_lossy(&got["asm.err"]));
    assert!(got["t.nex"] == want, "hello: NEX differs");
}

/// Bootstrap: the code generator compiled by lz80nc (stage 1) compiles its
/// own source (stage 2). Stage 2 must make the same assembly as stage 1,
/// for test programs and for its own source (a fixed point).
#[test]
fn selfhost_bootstrap() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let rd = |p: &str| std::fs::read(root.join(p)).unwrap();
    let mut sh = SelfHost::new();
    let sources: Vec<(&str, Vec<u8>)> = vec![("in.lz80n", rd("selfhost/codegen.lz80n")), ("parser_core.lz80n", rd("selfhost/parser_core.lz80n"))];
    let as_refs = |v: &Vec<(&'static str, Vec<u8>)>| v.iter().map(|(n, d)| (*n, d.clone())).collect::<Vec<_>>();
    let srcs = as_refs(&sources);
    let refs: Vec<(&str, &[u8])> = srcs.iter().map(|(n, d)| (*n, d.as_slice())).collect();
    let t = std::time::Instant::now();
    let asm2 = sh.compile_files(&refs).unwrap_or_else(|e| panic!("stage 1 compiling codegen.lz80n: {e}"));
    eprintln!("stage 1 -> stage 2: {} lines of assembly, {:.1} s", asm2.lines().count(), t.elapsed().as_secs_f64());
    let out = crate::asm::assemble(&asm2, std::path::Path::new(".")).unwrap_or_else(|e| panic!("assembling stage 2: {}", e[0]));
    let stage1 = std::mem::replace(&mut sh.cg, (out.nex.clone().unwrap().1, out.symbols16()));
    // test programs: same assembly from both stages
    let progs = [
        "print(\"Hi\", 42)",
        "function f(byte n) -> word\n    if n < 2 then return 1 end\n    return n * f(n - 1)\nend\nstring s = \"x\" .. f(5)\nlong l = 100000 * 3\nprint(s, l)\n",
    ];
    for p in progs {
        let a2 = sh.compile(p).unwrap();
        let sh1 = SelfHost { lex: sh.lex.clone(), cg: stage1.clone() };
        let a1 = sh1.compile(p).unwrap();
        assert!(a1 == a2, "stage 1 and stage 2 differ for:\n{p}");
    }
    // the fixed point
    let t = std::time::Instant::now();
    let asm3 = sh.compile_files(&refs).unwrap_or_else(|e| panic!("stage 2 compiling codegen.lz80n: {e}"));
    eprintln!("stage 2 -> stage 3: {:.1} s", t.elapsed().as_secs_f64());
    assert!(asm3 == asm2, "stage 2 compiles itself differently from stage 1");
}


/// lz80nc --export-selfhost, then the whole chain on the virtual SD card:
/// lexer.nex starts codegen.nex, which starts asm.nex (M_EXECCMD
/// `nexload`), which writes out.nex; out.nex prints what it should.
#[test]
fn selfhost_export_and_chain() {
    let dir = std::env::temp_dir().join("lz80n_export_test");
    let _ = std::fs::remove_dir_all(&dir);
    let built = crate::export_selfhost(&dir).unwrap();
    let mut fs = crate::emu::EsxFs::default();
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        fs.files.insert(p.file_name().unwrap().to_string_lossy().to_string(), std::fs::read(&p).unwrap());
    }
    let prog = "function sq(word x) -> word\n    return x * x\nend\nstring s = \"sq\" .. sq(12)\nprint(s, 1000000 / 3)\n";
    fs.files.insert("in.lz80n".into(), prog.as_bytes().to_vec());
    let mut next = "lexer.nex".to_string();
    let mut stages = vec![];
    loop {
        let (_, nex, syms) = built.iter().find(|b| b.0 == next).unwrap_or_else(|| panic!("no {next}"));
        stages.push(next.clone());
        let mut cpu = load_nex(nex).unwrap();
        fs.exec.clear();
        cpu.esx = Some(fs);
        let end = syms["__main_end"] as u16;
        let mut n = 0u64;
        while cpu.pc != end {
            cpu.tick().unwrap();
            n += 1;
            assert!(n < 4_000_000_000, "{next} did not finish");
            if !cpu.esx.as_ref().unwrap().exec.is_empty() {
                break;
            }
        }
        fs = cpu.esx.unwrap();
        match fs.exec.first().and_then(|c| c.strip_prefix("nexload ")) {
            Some(n) => next = n.to_string(),
            None => break,
        }
    }
    assert_eq!(stages, ["lexer.nex", "codegen.nex", "asm.nex"]);
    assert!(fs.files.get("asm.err").is_none(), "{}", String::from_utf8_lossy(&fs.files["asm.err"]));
    let asm = String::from_utf8(fs.files["out.asm"].clone()).unwrap();
    let rust = crate::asm::assemble(&asm, &dir).unwrap();
    assert!(fs.files["out.nex"] == rust.nex.as_ref().unwrap().1, "out.nex differs from the Rust assembler's");
    let want = run("sh_chain", prog).unwrap();
    assert_eq!(run_asm(&asm), want);
}

/// Layer 2 graphics, palettes and hardware registers through the
/// self-hosted compiler: the same pictures as with lz80nc.
#[test]
fn selfhost_codegen_layer2() {
    let sh = SelfHost::new();
    let programs: &[&str] = &[
        r#"
layer2_visible(true)
layer2_clear(rgb(0, 0, 1))
for i = 0, 255 do plot(i, i % 192, byte(i)) end
hline(10, 100, 200, rgb(7, 7, 0))
vline(5, 0, 192, 0x1C)
line(0, 0, 255, 191, 0xE0)
line(255, 0, 0, 191, 0x03)
rect(20, 20, 60, 40, 0xFF)
fill_rect(100, 120, 50, 30, 0x92)
circle(128, 96, 50, 0x1F)
fill_circle(60, 150, 20, 0xE3)
ellipse(180, 60, 40, 20, 0x6D)
fill_ellipse(200, 150, 30, 15, 0x49)
triangle(10, 180, 60, 130, 110, 185, 0xFC)
fill_triangle(150, 10, 240, 40, 170, 90, 0x1C)
print(point(10, 100), point(0, 0), point(128, 46), rgb(7, 7, 3), TRANSPARENT)
print(layer2_width(), layer2_height())
"#,
        r#"
layer2_mode(L2_320)
layer2_clear(0)
for y = 0, 255, 17 do hline(0, byte(y), 320, byte(y)) end
for x = 0, 319, 13 do plot(x, 128, 0xFF) end
circle(160, 128, 100, 0xE0)
print(layer2_width(), layer2_height(), point(300, 128))
"#,
        r#"
palette(PAL_LAYER2, 5, rgb(1, 2, 3))
palette9(PAL_TEXT, 3, rgb9(7, 0, 7))
nextreg[0x4A] = 0x1C
byte r = 0x4A
print(nextreg[r], nextreg[0x4A] + 1)
border(3)
copper_begin()
copper_wait(96)
copper_move(0x4A, 0xE0)
copper_start()
port[0x123B] = 2
print(abs(-5), abs(7), min(3, 9), max(3, 9), min(-2, 5), max(word(70000 - 4465), 7))
print(frames() < 1000, kempston() & 0, key(KEY_A), random(1))
"#,
    ];
    for (i, src) in programs.iter().enumerate() {
        let (want, rcpu) = run_with(&format!("sh_l2_{i}"), src, |_| {}).unwrap();
        let asm = sh.compile(src).unwrap_or_else(|e| panic!("program {i}: self-hosted compiler: {e}\n{src}"));
        let (got, scpu) = run_asm_cpu(&asm);
        assert_eq!(got, want, "program {i}: output\n{src}");
        // the Layer 2 buffers (banks 8..17: pages 16..35)
        for page in 16u8..36 {
            let a = rcpu.pages.get(&page).cloned().unwrap_or_else(|| vec![0; 8192]);
            let b = scpu.pages.get(&page).cloned().unwrap_or_else(|| vec![0; 8192]);
            if let Some(k) = (0..8192).find(|&k| a[k] != b[k]) {
                panic!("program {i}: Layer 2 page {page} differs at {k}: lz80nc {:02X}, self-hosted {:02X}", a[k], b[k]);
            }
        }
        assert_eq!(rcpu.palettes, scpu.palettes, "program {i}: palettes");
        assert_eq!(rcpu.nextreg[0x4A], scpu.nextreg[0x4A], "program {i}: NextReg $4A");
        assert_eq!(rcpu.copper, scpu.copper, "program {i}: copper list");
        assert_eq!(rcpu.border, scpu.border, "program {i}: border");
    }
}



