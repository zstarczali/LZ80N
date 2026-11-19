//! Diagnostics (errors with source positions).

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pos {
    pub line: u32,
    pub col: u32,
    /// Source file: 0 = the main file, others are `include`d (see include.rs).
    pub file: u16,
}

#[derive(Debug, Clone)]
pub struct Diag {
    pub pos: Pos,
    pub msg: String,
    /// File name for errors in an included file.
    pub file: Option<String>,
}

impl Diag {
    pub fn new(pos: Pos, msg: impl Into<String>) -> Self {
        Diag { pos, msg: msg.into(), file: None }
    }
}

impl fmt::Display for Diag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}: error: {}", self.pos.line, self.pos.col, self.msg)
    }
}
