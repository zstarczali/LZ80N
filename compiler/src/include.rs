//! `include "file.lz80n"`: splits a program into several source files.
//!
//! Includes are expanded on the token stream before parsing. The path is
//! relative to the including file. Every file is included once (a second
//! `include` of the same file is skipped), so files can include what they
//! need without worrying about cycles. `include` is only allowed at the top
//! level (not inside a function, class or block).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::diag::{Diag, Pos};
use crate::lexer::{self, Tok, Token};

/// The expanded program.
pub struct Sources {
    pub tokens: Vec<Token>,
    /// Path of every file, by `Pos::file`; entry 0 is the main file.
    pub files: Vec<PathBuf>,
    /// Directory of every file (relative asset and include paths).
    pub dirs: Vec<PathBuf>,
    /// All source text, for the standard library detection.
    pub text: String,
}

struct State {
    seen: HashSet<PathBuf>,
    files: Vec<PathBuf>,
    dirs: Vec<PathBuf>,
    text: String,
    out: Vec<Token>,
}

/// Errors in included files come back with `Diag::file` set.
pub fn expand(src: &str, main: Option<&Path>, base_dir: &Path) -> Result<Sources, Diag> {
    let mut st = State {
        seen: HashSet::new(),
        files: vec![main.map(Path::to_path_buf).unwrap_or_default()],
        dirs: vec![base_dir.to_path_buf()],
        text: src.to_string(),
        out: Vec::new(),
    };
    if let Some(canon) = main.and_then(|m| m.canonicalize().ok()) {
        st.seen.insert(canon);
    }
    if let Err(mut d) = file(src, 0, &mut st) {
        if d.pos.file > 0 {
            d.file = Some(st.files[d.pos.file as usize].display().to_string());
        }
        return Err(d);
    }
    // "end of file" errors point at the last token, in whichever file it is
    let eof = st.out.last().map_or(Pos { line: 1, col: 1, file: 0 }, |t| t.pos);
    st.out.push(Token { tok: Tok::Eof, pos: eof });
    Ok(Sources { tokens: st.out, files: st.files, dirs: st.dirs, text: st.text })
}

fn file(src: &str, idx: u16, st: &mut State) -> Result<(), Diag> {
    let mut toks = lexer::lex(src).map_err(|mut d| {
        d.pos.file = idx;
        d
    })?;
    for t in &mut toks {
        t.pos.file = idx;
    }
    let mut depth = 0u32;
    let mut it = toks.into_iter();
    while let Some(t) = it.next() {
        match t.tok {
            Tok::Eof => break,
            Tok::Include => {
                if depth > 0 {
                    return Err(Diag::new(t.pos, "`include` is only allowed at the top level of a file"));
                }
                let path = match it.next() {
                    Some(Token { tok: Tok::Str(p), .. }) => p,
                    _ => return Err(Diag::new(t.pos, "expected a file name: `include \"enemies.lz80n\"`")),
                };
                let full = st.dirs[idx as usize].join(&path);
                let canon = full
                    .canonicalize()
                    .map_err(|e| Diag::new(t.pos, format!("cannot open `{}`: {e}", full.display())))?;
                if !st.seen.insert(canon) {
                    continue; // already included
                }
                let text = std::fs::read_to_string(&full)
                    .map_err(|e| Diag::new(t.pos, format!("cannot read `{}`: {e}", full.display())))?;
                if st.files.len() >= u16::MAX as usize {
                    return Err(Diag::new(t.pos, "too many included files"));
                }
                let n = st.files.len() as u16;
                st.dirs.push(full.parent().map(Path::to_path_buf).unwrap_or_default());
                st.files.push(full);
                st.text.push('\n');
                st.text.push_str(&text);
                file(&text, n, st)?;
            }
            _ => {
                match t.tok {
                    Tok::If | Tok::While | Tok::For | Tok::Function | Tok::Struct | Tok::Class | Tok::Interrupt
                    | Tok::Asm => depth += 1,
                    Tok::End => depth = depth.saturating_sub(1),
                    _ => {}
                }
                st.out.push(t);
            }
        }
    }
    Ok(())
}
