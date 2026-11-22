//! lz80nc — LZ80N compiler.
//!
//! Usage: lz80nc <input.lz80n> [-o out.nex] [-S] [--sym] [--sjasmplus PATH]

mod asm;
mod assets;
mod ast;
mod codegen;
mod diag;
mod font;
mod include;
mod lexer;
mod parser;
mod peephole;
mod prelude;
mod pt3;
mod runtime;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

pub fn compile_source(src: &str, nex_name: &str) -> Result<String, Vec<diag::Diag>> {
    compile_with(src, nex_name, true, Path::new("."))
}

pub fn compile_with(src: &str, nex_name: &str, optimize: bool, base_dir: &Path) -> Result<String, Vec<diag::Diag>> {
    compile_opts(src, nex_name, optimize, base_dir, false)
}

/// `bank_all`: put every function into its own code page (tests).
pub fn compile_opts(
    src: &str,
    nex_name: &str,
    optimize: bool,
    base_dir: &Path,
    bank_all: bool,
) -> Result<String, Vec<diag::Diag>> {
    compile_main(src, None, nex_name, optimize, base_dir, bank_all)
}

/// `main`: the path of the main source file (so including it again is
/// recognised). Errors in included files carry their file name.
pub fn compile_main(
    src: &str,
    main: Option<&Path>,
    nex_name: &str,
    optimize: bool,
    base_dir: &Path,
    bank_all: bool,
) -> Result<String, Vec<diag::Diag>> {
    let sources = include::expand(src, main, base_dir).map_err(|d| vec![d])?;
    let files = sources.files.clone();
    let name_files = |errs: Vec<diag::Diag>| -> Vec<diag::Diag> {
        errs.into_iter()
            .map(|mut d| {
                if d.pos.file > 0 && d.file.is_none() {
                    d.file = files.get(d.pos.file as usize).map(|p| p.display().to_string());
                }
                d
            })
            .collect()
    };
    let src = sources.text.as_str();
    let mut prog = parser::parse(sources.tokens).map_err(|d| name_files(vec![d]))?;
    let reserved = library_name_errors(&prog);
    if !reserved.is_empty() {
        return Err(name_files(reserved));
    }
    for (used, lib_src) in [
        (prelude::uses_sound(src), prelude::SOUND),
        (prelude::uses_copper(src), prelude::COPPER),
        (prelude::uses_file(src), prelude::FILE),
        (prelude::uses_palette(src), prelude::PALETTE),
        (prelude::uses_layer2(src), prelude::LAYER2),
        (prelude::uses_sprites(src), prelude::SPRITES),
    ] {
        if used {
            let toks = lexer::lex(lib_src).expect("prelude lexes");
            let lib = parser::parse(toks).expect("prelude parses");
            prog.items.splice(0..0, lib.items);
        }
    }
    let opts = codegen::Options {
        nex_name: nex_name.to_string(),
        optimize,
        base_dir: base_dir.to_path_buf(),
        file_dirs: sources.dirs,
        bank_all,
        l2_hires: src.contains("layer2_mode"),
    };
    let out = codegen::compile(&prog, &opts).map_err(name_files)?;
    Ok(out.asm)
}

/// Declarations in the user's program that take a standard library name.
fn library_name_errors(prog: &ast::Program) -> Vec<diag::Diag> {
    use ast::{Item, Stmt, StmtKind};
    fn check(name: &str, pos: diag::Pos, out: &mut Vec<diag::Diag>) {
        if let Some(m) = prelude::library_module(name) {
            out.push(diag::Diag::new(pos, format!("`{name}` is a reserved name (standard library: {m} module)")));
        }
    }
    fn stmts(list: &[Stmt], out: &mut Vec<diag::Diag>) {
        for s in list {
            match &s.kind {
                StmtKind::VarDecl { name, .. } | StmtKind::Asset { name, .. } => check(name, s.pos, out),
                StmtKind::For { var, body, .. } => {
                    check(var, s.pos, out);
                    stmts(body, out);
                }
                StmtKind::While { body, .. } => stmts(body, out),
                StmtKind::Far(inner) => stmts(std::slice::from_ref(inner), out),
                StmtKind::If { arms, otherwise } => {
                    for (_, b) in arms {
                        stmts(b, out);
                    }
                    if let Some(b) = otherwise {
                        stmts(b, out);
                    }
                }
                _ => {}
            }
        }
    }
    fn func(f: &ast::Func, is_method: bool, out: &mut Vec<diag::Diag>) {
        if !is_method {
            check(&f.name, f.pos, out);
        }
        for (p, _) in &f.params {
            check(p, f.pos, out);
        }
        stmts(&f.body, out);
    }
    let mut out = Vec::new();
    for item in &prog.items {
        match item {
            Item::Func(f) => func(f, false, &mut out),
            Item::Interrupt(_, body, _) => stmts(body, &mut out),
            Item::Struct(sd) => {
                check(&sd.name, sd.pos, &mut out);
                for m in &sd.methods {
                    func(m, true, &mut out);
                }
            }
            Item::Stmt(s) => stmts(std::slice::from_ref(s), &mut out),
        }
    }
    out
}

/// The sources of the self-hosted compiler (selfhost/), built into lz80nc.
pub const SELFHOST_SOURCES: &[(&str, &str)] = &[
    ("lexer.lz80n", include_str!("../../selfhost/lexer.lz80n")),
    ("parser_core.lz80n", include_str!("../../selfhost/parser_core.lz80n")),
    ("parser.lz80n", include_str!("../../selfhost/parser.lz80n")),
    ("codegen.lz80n", include_str!("../../selfhost/codegen.lz80n")),
    ("asm.lz80n", include_str!("../../selfhost/asm.lz80n")),
    ("asset_info.lz80n", include_str!("../../selfhost/asset_info.lz80n")),
    ("buildlog.lz80n", include_str!("../../selfhost/buildlog.lz80n")),
    ("assets.lz80n", include_str!("../../selfhost/assets.lz80n")),
    ("ide.lz80n", include_str!("../../selfhost/ide.lz80n")),
];

/// The files the self-hosted compiler reads from the SD card besides the
/// program: the runtime texts, the file module, and `expand.on` (the lexer
/// expands includes and modules).
pub fn selfhost_support_files() -> Vec<(String, Vec<u8>)> {
    let startup = runtime::STARTUP.trim_end();
    let startup = startup.strip_suffix("ei").expect("startup ends with ei");
    let mut font = String::from("rt_font:\n");
    for glyph in font::bytes().chunks(8) {
        let row: Vec<String> = glyph.iter().map(|b| format!("${b:02X}")).collect();
        font.push_str(&format!("    db {}\n", row.join(",")));
    }
    vec![
        ("rt_start.asm".into(), startup.as_bytes().to_vec()),
        ("rt_lib.asm".into(), runtime::RUNTIME.as_bytes().to_vec()),
        ("rt_font.asm".into(), font.into_bytes()),
        ("rt_long.asm".into(), runtime::long().into_bytes()),
        ("rt_banked.asm".into(), runtime::BANKED.as_bytes().to_vec()),
        ("rt_images.asm".into(), runtime::IMAGES.as_bytes().to_vec()),
        ("rt_samples.asm".into(), runtime::SAMPLES.as_bytes().to_vec()),
        ("rt_notes.asm".into(), codegen::notes_asm().into_bytes()),
        ("rt_trig.asm".into(), codegen::trig_asm().into_bytes()),
        ("rt_music.asm".into(), runtime::MUSIC.as_bytes().to_vec()),
        ("lib_file.lz80n".into(), prelude::FILE.as_bytes().to_vec()),
        ("lib_layer2.lz80n".into(), prelude::LAYER2.as_bytes().to_vec()),
        ("lib_palette.lz80n".into(), prelude::PALETTE.as_bytes().to_vec()),
        ("lib_copper.lz80n".into(), prelude::COPPER.as_bytes().to_vec()),
        ("lib_sprites.lz80n".into(), prelude::SPRITES.as_bytes().to_vec()),
        ("lib_sound.lz80n".into(), prelude::SOUND.as_bytes().to_vec()),
        ("lib_consts.lz80n".into(), {
            let mut c = String::from("-- the built-in constants (compiler/src/codegen.rs)\n");
            for (n, v) in codegen::builtin_consts() {
                c.push_str(&format!("const {n} = {v}\n"));
            }
            c.into_bytes()
        }),
        ("expand.on".into(), b"1".to_vec()),
    ]
}

/// A program built for the SD card: (file name, NEX, symbols).
pub type Built = (String, Vec<u8>, std::collections::HashMap<String, i64>);

/// Write everything the self-hosted compiler needs on the Next into `dir`:
/// the support files, the sources, `chain.on` (each stage starts the next)
/// and lexer.nex, codegen.nex, asm.nex built from the sources.
pub fn export_selfhost(dir: &Path) -> Result<Vec<Built>, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let write = |name: &str, data: &[u8]| {
        std::fs::write(dir.join(name), data).map_err(|e| format!("{}: {e}", dir.join(name).display()))
    };
    for (name, data) in selfhost_support_files() {
        write(&name, &data)?;
    }
    for (name, src) in SELFHOST_SOURCES {
        write(name, src.as_bytes())?;
    }
    write("chain.on", b"1")?;
    // the IDE shows it (Help -> About, the splash screen)
    write("version.txt", format!("{}\n", env!("CARGO_PKG_VERSION")).as_bytes())?;
    let mut built = vec![];
    for stage in ["lexer", "codegen", "assets", "asm", "ide"] {
        let file = format!("{stage}.lz80n");
        let src = SELFHOST_SOURCES.iter().find(|(n, _)| *n == file).unwrap().1;
        let nex_name = format!("{stage}.nex");
        let asm = compile_main(src, Some(&dir.join(&file)), &nex_name, true, dir, false).map_err(|errs| {
            errs.iter().map(|d| format!("{file}:{d}")).collect::<Vec<_>>().join("\n")
        })?;
        let out = asm::assemble(&asm, dir).map_err(|e| format!("{file}: {}", e[0]))?;
        let nex = out.nex.ok_or("no SAVENEX")?.1;
        write(&nex_name, &nex)?;
        built.push((nex_name, nex, out.symbols));
    }
    write(
        "README.txt",
        b"LZ80N self-hosted compiler (needs a 2 MB ZX Spectrum Next)\r\n\r\n\
Copy this folder to the SD card, then in NextBASIC:\r\n\
  .cd /lz80n                (the folder you copied)\r\n\
  put your program into in.lz80n (e.g. COPY \"prog.lz80n\" TO \"in.lz80n\")\r\n\
  .nexload lexer.nex\r\n\r\n\
Or start the LZ80N IDE: .nexload ide.nex (Build -> Compile / Compile & run\r\n\
runs the stages below and comes back to the IDE; EXTEND+B / EXTEND+R).\r\n\
File -> Open hello.lz80n (or any example) to try it.\r\n\r\n\
lexer.nex -> tokens.bin, then (when the program has assets) assets.nex ->\r\n\
a_*.bin, then codegen.nex -> out.asm, then asm.nex -> out.nex. Create a file named run.on to start out.nex at the end. Errors:\r\n\
on the screen, and in codegen.err / asm.err. Delete chain.on to run the\r\n\
stages one by one. Supported language: see selfhost/README.md.\r\n",
    )?;
    Ok(built)
}

/// `--sjasmplus auto`: $SJASMPLUS, next to lz80nc, tools/sjasmplus, the PATH.
fn find_sjasmplus(explicit: PathBuf) -> PathBuf {
    if explicit.as_os_str() != "auto" {
        return explicit;
    }
    if let Ok(p) = std::env::var("SJASMPLUS") {
        return p.into();
    }
    let name = if cfg!(windows) { "sjasmplus.exe" } else { "sjasmplus" };
    if let Ok(exe) = std::env::current_exe() {
        // release packages: sjasmplus next to lz80nc
        if let Some(dir) = exe.parent() {
            let cand = dir.join(name);
            if cand.exists() {
                return cand;
            }
        }
        // tools/ folder next to the compiler workspace
        for dir in exe.ancestors() {
            let cand = dir.join("tools/sjasmplus/sjasmplus-1.24.0.win/sjasmplus.exe");
            if cand.exists() {
                return cand;
            }
        }
    }
    PathBuf::from(name)
}

/// Assemble with the built-in assembler and write the NEX (and `.sym`).
fn assemble_builtin(asm: &str, asm_path: &Path, nex: &Path, base_dir: &Path, sym: bool) -> ExitCode {
    let out = match asm::assemble(asm, base_dir) {
        Ok(o) => o,
        Err(errs) => {
            for e in &errs {
                eprintln!("{}:{e}", asm_path.display());
            }
            eprintln!("{} assembler error(s)", errs.len());
            return ExitCode::FAILURE;
        }
    };
    let Some((_, bytes)) = &out.nex else {
        eprintln!("{}: no SAVENEX in the assembly", asm_path.display());
        return ExitCode::FAILURE;
    };
    if let Err(e) = std::fs::write(nex, bytes) {
        eprintln!("{}: {e}", nex.display());
        return ExitCode::FAILURE;
    }
    if sym {
        let p = nex.with_extension("sym");
        if let Err(e) = std::fs::write(&p, out.sym_file()) {
            eprintln!("{}: {e}", p.display());
            return ExitCode::FAILURE;
        }
        println!("wrote {}", p.display());
    }
    println!("wrote {}", nex.display());
    ExitCode::SUCCESS
}

const USAGE: &str = "usage: lz80nc <input.lz80n> [-o out.nex] [-S] [-O0] [--bank-all] [--sym] [--sjasmplus PATH|auto]
       lz80nc --export-selfhost DIR
       lz80nc --help | --version";

const HELP: &str = "lz80nc — the LZ80N compiler: a Lua-like language for the ZX Spectrum Next

usage:
  lz80nc <input.lz80n> [options]
  lz80nc --help | -h
  lz80nc --version | -V

options:
  -o FILE           the NEX to write (default: the input with a .nex extension);
                    the assembly (.asm) goes next to it
  -S                stop after writing the assembly (.asm), no NEX
  -O0               no peephole optimisation of the generated assembly
  --bank-all        every function in its own 8K code page (to try code banking;
                    normally only when the program does not fit in the 31K area)
  --sym             also write a symbol file (.sym, sjasmplus format)
  --export-selfhost DIR
                    write the self-hosted compiler (lexer.nex, codegen.nex,
                    asm.nex, runtime files, sources) into DIR, for the Next
  --sjasmplus PATH  assemble with an external SjASMPlus instead of the built-in
                    assembler (writes a .lst listing too); `auto` = $SJASMPLUS,
                    then the one next to lz80nc, then tools/sjasmplus, then the PATH

`include` and `asset` paths (.png .bmp .wav .pt3 ...) are relative to the source
file that names them.

exit status: 0 = built, 1 = compile error (FILE:LINE:COL: error: message on stderr)
             or the assembler failed, 2 = bad usage";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    if args.len() == 0 {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let mut input = None;
    let mut output = None;
    let mut asm_only = false;
    let mut sjasm = None;
    let mut optimize = true;
    let mut bank_all = false;
    let mut sym = false;
    while let Some(a) = args.next() {
        match a.as_str() {
            "-o" | "--sjasmplus" => {
                let Some(v) = args.next() else {
                    eprintln!("`{a}` needs a value
{USAGE}");
                    return ExitCode::from(2);
                };
                if a == "-o" {
                    output = Some(PathBuf::from(v));
                } else {
                    sjasm = Some(PathBuf::from(v));
                }
            }
            "--export-selfhost" => {
                let Some(dir) = args.next() else {
                    eprintln!("`--export-selfhost` needs a directory\n{USAGE}");
                    return ExitCode::from(2);
                };
                return match export_selfhost(Path::new(&dir)) {
                    Ok(built) => {
                        for (name, nex, _) in &built {
                            println!("wrote {}/{name} ({} bytes)", dir, nex.len());
                        }
                        println!("the self-hosted compiler is in {dir}: see README.txt there");
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("{e}");
                        ExitCode::FAILURE
                    }
                };
            }
            "-S" => asm_only = true,
            "-O0" => optimize = false,
            "--bank-all" => bank_all = true,
            "--sym" => sym = true,
            "-V" | "--version" => {
                println!("lz80nc {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "-h" | "--help" => {
                println!("{HELP}");
                return ExitCode::SUCCESS;
            }
            _ if a.starts_with('-') && a.len() > 1 => {
                eprintln!("unknown option `{a}`
{USAGE}");
                return ExitCode::from(2);
            }
            _ if input.is_none() => input = Some(PathBuf::from(a)),
            _ => {
                eprintln!("unexpected argument `{a}` (one input file)
{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(input) = input else {
        eprintln!("no input file
{USAGE}");
        return ExitCode::from(2);
    };
    let src = match std::fs::read_to_string(&input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: {e}", input.display());
            return ExitCode::FAILURE;
        }
    };
    let nex = output.unwrap_or_else(|| input.with_extension("nex"));
    let asm_path = nex.with_extension("asm");
    let nex_file = nex.file_name().unwrap().to_string_lossy().to_string();
    let base_dir = input.parent().map(Path::to_path_buf).unwrap_or_default();
    let asm = match compile_main(&src, Some(&input), &nex_file, optimize, &base_dir, bank_all) {
        Ok(a) => a,
        Err(errs) => {
            for d in &errs {
                match &d.file {
                    Some(f) => eprintln!("{f}:{d}"),
                    None => eprintln!("{}:{d}", input.display()),
                }
            }
            eprintln!("{} error(s)", errs.len());
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = std::fs::write(&asm_path, &asm) {
        eprintln!("{}: {e}", asm_path.display());
        return ExitCode::FAILURE;
    }
    println!("wrote {}", asm_path.display());
    if asm_only {
        return ExitCode::SUCCESS;
    }
    let Some(sjasm) = sjasm else {
        return assemble_builtin(&asm, &asm_path, &nex, &base_dir, sym);
    };
    let sj = find_sjasmplus(sjasm);
    let dir = asm_path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let status = Command::new(&sj)
        .current_dir(dir)
        .arg("--nologo")
        .arg("--msg=war")
        .arg(format!("--lst={}", asm_path.with_extension("lst").file_name().unwrap().to_string_lossy()))
        .arg(asm_path.file_name().unwrap())
        .status();
    match status {
        Ok(s) if s.success() => {
            println!("wrote {}", nex.display());
            ExitCode::SUCCESS
        }
        Ok(_) => {
            eprintln!("sjasmplus failed (see messages above)");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("could not run sjasmplus ({}): {e}\nset SJASMPLUS or use --sjasmplus PATH", sj.display());
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod emu;
