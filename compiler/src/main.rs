//! lz80nc — LZ80N compiler.
//!
//! Usage: lz80nc <input.lz80n> [-o out.nex] [-S] [--sjasmplus PATH]

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

fn find_sjasmplus(explicit: Option<PathBuf>) -> Option<PathBuf> {
    if explicit.is_some() {
        return explicit;
    }
    if let Ok(p) = std::env::var("SJASMPLUS") {
        return Some(p.into());
    }
    let exe = std::env::current_exe().ok()?;
    // release packages: sjasmplus next to lz80nc
    let name = if cfg!(windows) { "sjasmplus.exe" } else { "sjasmplus" };
    if let Some(dir) = exe.parent() {
        let cand = dir.join(name);
        if cand.exists() {
            return Some(cand);
        }
    }
    // tools/ folder next to the compiler workspace
    for dir in exe.ancestors() {
        let cand = dir.join("tools/sjasmplus/sjasmplus-1.24.0.win/sjasmplus.exe");
        if cand.exists() {
            return Some(cand);
        }
    }
    Some(PathBuf::from("sjasmplus"))
}

const USAGE: &str = "usage: lz80nc <input.lz80n> [-o out.nex] [-S] [-O0] [--bank-all] [--sjasmplus PATH]
       lz80nc --help | --version";

const HELP: &str = "lz80nc — the LZ80N compiler: a Lua-like language for the ZX Spectrum Next

usage:
  lz80nc <input.lz80n> [options]
  lz80nc --help | -h
  lz80nc --version | -V

options:
  -o FILE           the NEX to write (default: the input with a .nex extension);
                    the assembly (.asm) and listing (.lst) go next to it
  -S                stop after writing the assembly (.asm), no NEX
  -O0               no peephole optimisation of the generated assembly
  --bank-all        every function in its own 8K code page (to try code banking;
                    normally only when the program does not fit in the 31K area)
  --sjasmplus PATH  the SjASMPlus to assemble with (default: $SJASMPLUS, then the
                    one next to lz80nc, then tools/sjasmplus, then the PATH)

`include` and `asset` paths (.png .bmp .wav .pt3 ...) are relative to the source
file that names them.

exit status: 0 = built, 1 = compile error (FILE:LINE:COL: error: message on stderr)
             or SjASMPlus failed, 2 = bad usage";

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
            "-S" => asm_only = true,
            "-O0" => optimize = false,
            "--bank-all" => bank_all = true,
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
    let sj = find_sjasmplus(sjasm).unwrap();
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
