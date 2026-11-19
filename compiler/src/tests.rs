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

/// Compile and assemble; checks sjasmplus accepts the generated code.
fn assembles(name: &str, src: &str) {
    build(name, src);
}

/// Compile + assemble, returning (nex bytes, symbols). Panics on failure.
fn build(name: &str, src: &str) -> Option<(Vec<u8>, HashMap<String, u16>)> {
    // relative asset paths resolve against examples/
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let asm = match crate::compile_with(src, "t.nex", true, &base) {
        Ok(a) => a,
        Err(e) => panic!("unexpected errors: {:?}", e.iter().map(|d| d.to_string()).collect::<Vec<_>>()),
    };
    let sj = sjasmplus()?;
    let dir = std::env::temp_dir().join(format!("lz80nc_test_{name}"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("t.asm"), &asm).unwrap();
    let out = Command::new(sj)
        .current_dir(&dir)
        .args(["--nologo", "--msg=war", "--sym=t.sym", "t.asm"])
        .output()
        .unwrap();
    assert!(
        out.status.success() && dir.join("t.nex").exists(),
        "sjasmplus failed:\n{}{}\n--- asm ---\n{asm}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let mut syms = HashMap::new();
    for line in std::fs::read_to_string(dir.join("t.sym")).unwrap().lines() {
        if let Some((name, val)) = line.split_once(": EQU ") {
            if let Ok(v) = u32::from_str_radix(val.trim().trim_start_matches("0x"), 16) {
                syms.insert(name.trim().to_string(), v as u16);
            }
        }
    }
    Some((std::fs::read(dir.join("t.nex")).unwrap(), syms))
}

/// Run a program in the test emulator and return everything it printed.
/// Returns None when sjasmplus is not available.
fn run_with(name: &str, src: &str, setup: impl FnOnce(&mut Cpu)) -> Option<(String, Cpu)> {
    let (nex, syms) = build(name, src)?;
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

/// Assert the printed output (skipped without sjasmplus).
fn expect(name: &str, src: &str, want: &str) {
    if let Some(out) = run(name, src) {
        assert_eq!(out, want, "program:\n{src}");
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
    assert!(err("function f() -> byte\n return f()\nend\nprint(f())").contains("recursion"));
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
    assert!(err("struct A\n byte x\nend\nstruct B\n byte x\nend\nA a\nB b\na = b").contains("expected a `A`"));
    assert!(err("class C\n function f()\n self.f()\n end\nend\nC c\nc.f()").contains("recursion"));
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
        assert_eq!(cpu.sprite_attr[5], [40, 50, 0x20, 0x40, 0]);
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
    let Some(sj) = sjasmplus() else { return };
    let dir = std::env::temp_dir().join("lz80nc_test_ex_sprites_b");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("t.asm"), asm).unwrap();
    assert!(Command::new(sj).current_dir(&dir).args(["--nologo", "t.asm"]).status().unwrap().success());
    let mut cpu = load_nex(&std::fs::read(dir.join("t.nex")).unwrap()).unwrap();
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
        for y in 0..192 {
            let pal = state(32 + y).1;
            for x in 0..256 {
                let c = pal[1][cpu.l2_pixel(x, y) as usize];
                if c != 0xE3 {
                    put(&mut img, 32 + x, 32 + y, c);
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
                        let c = state(row * 8 + py).1[3][idx as usize];
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
                        put(&mut img, x + sx, y + sy, state(y + sy).1[2][c as usize]);
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
    assert!(err("struct P\n byte x\nend\nP ps[] = {1}").contains("arrays of structs"));
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

#[test]
fn far_debug_tmp() {
    let src = "far byte cave[64][256]\nfar byte roof[256]\nbyte c[] = {0, 5, 6, 7}\nfor x = 0, 3 do\n roof[x] = 9\n for y = 0, 63 do\n  cave[y][x] = 1\n end\nend\ncave[10][2] = 2\nprint(cave[0][0], cave[10][2], roof[3], c[cave[10][2]])\nfill_rect(0, 0, 8, 8, c[cave[10][2]])\nprint(point(1, 1))";
    expect("r_far_dbg", src, "1 2 9 6\n6\n");
}
