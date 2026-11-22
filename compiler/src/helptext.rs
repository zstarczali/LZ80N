//! The manuals (Markdown) as the help text of the IDE on the Next.
//!
//! The Next's font has the characters 32..127 only, and the IDE shows the
//! text in 80 columns, so the Markdown is turned into plain, ready-wrapped
//! lines here: headings, lists, code blocks and tables are laid out, the
//! Markdown marks (`#`, `**`, backticks, link targets, table bars) go, and
//! the letters outside ASCII become their nearest ASCII ones.
//!
//! A line may start with a byte that says what it is (the IDE colours it):
//! `\x11` a chapter heading (`#`, `##`), `\x12` a section heading (`###`
//! ...), `\x13` code, `\x14` a table's header, `\x15` a table row, `\x16` a
//! rule. Inside a line `\x01` ... `\x02` mark inline code.

/// The width of a line (the IDE shows 78 columns of text).
const WIDTH: usize = 77;

pub const CHAPTER: char = '\x11';
pub const SECTION: char = '\x12';
pub const CODE: char = '\x13';
pub const TABLE_HEAD: char = '\x14';
pub const TABLE_ROW: char = '\x15';
pub const RULE: char = '\x16';
pub const CODE_ON: char = '\x01';
pub const CODE_OFF: char = '\x02';

/// The letters outside ASCII as ASCII ones.
fn ascii(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' => "a".to_string(),
            'é' => "e".into(),
            'í' => "i".into(),
            'ó' | 'ö' | 'ő' => "o".into(),
            'ú' | 'ü' | 'ű' => "u".into(),
            'Á' => "A".into(),
            'É' => "E".into(),
            'Í' => "I".into(),
            'Ó' | 'Ö' | 'Ő' => "O".into(),
            'Ú' | 'Ü' | 'Ű' => "U".into(),
            '–' | '—' => "-".into(),
            '„' | '”' | '“' => "\"".into(),
            '’' | '‘' => "'".into(),
            '…' => "...".into(),
            '→' => "->".into(),
            '↔' => "<->".into(),
            '×' => "x".into(),
            '±' => "+/-".into(),
            '°' => " deg".into(),
            '\r' => String::new(),
            '\t' => "    ".into(),
            c if c.is_ascii() => c.to_string(),
            _ => "?".into(),
        })
        .collect()
}

/// Inline Markdown: links keep their text, `**` and `__` go, code spans
/// are marked with CODE_ON / CODE_OFF, `\x` escapes give x.
fn inline(s: &str) -> String {
    let mut out = String::new();
    let b: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == '`' {
            // a code span: up to the next backtick, as it is
            if let Some(len) = b[i + 1..].iter().position(|&x| x == '`') {
                out.push(CODE_ON);
                // (`\|` in a table cell is a bar)
                out.push_str(&b[i + 1..i + 1 + len].iter().collect::<String>().replace("\\|", "|"));
                out.push(CODE_OFF);
                i += len + 2;
                continue;
            }
        }
        if c == '\\' && i + 1 < b.len() && !b[i + 1].is_alphanumeric() {
            out.push(b[i + 1]);
            i += 2;
            continue;
        }
        if c == '*' && i + 1 < b.len() && b[i + 1] == '*' {
            i += 2;
            continue;
        }
        if c == '[' {
            // [text](target) -> text
            if let Some(close) = b[i + 1..].iter().position(|&x| x == ']') {
                let close = i + 1 + close;
                if close + 1 < b.len() && b[close + 1] == '(' {
                    if let Some(end) = b[close + 2..].iter().position(|&x| x == ')') {
                        out.push_str(&inline(&b[i + 1..close].iter().collect::<String>()));
                        i = close + 2 + end + 1;
                        continue;
                    }
                }
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// The visible width (the marks take no column).
fn width(s: &str) -> usize {
    s.chars().filter(|&c| c != CODE_ON && c != CODE_OFF).count()
}

/// Words of `text` in lines of at most `w` columns: the first line starts
/// with `first`, the others with `rest` (an indent). A code span that ends
/// a line goes on in the next one (its marks are closed and opened again).
fn wrap(text: &str, first: &str, rest: &str, w: usize) -> Vec<String> {
    let mut lines = vec![];
    let mut line = first.to_string();
    let mut empty = true;
    let mut in_code = false;
    for word in text.split(' ').filter(|w| !w.is_empty()) {
        let ww = width(word);
        if !empty && width(&line) + 1 + ww > w {
            if in_code {
                line.push(CODE_OFF);
            }
            lines.push(line);
            line = rest.to_string();
            if in_code {
                line.push(CODE_ON);
            }
            empty = true;
        }
        if !empty {
            line.push(' ');
        }
        line.push_str(word);
        empty = false;
        for c in word.chars() {
            if c == CODE_ON {
                in_code = true;
            } else if c == CODE_OFF {
                in_code = false;
            }
        }
    }
    if !empty || lines.is_empty() {
        lines.push(line);
    }
    lines
}

/// A table: aligned columns when they fit, else one row after the other.
fn table(rows: &[Vec<String>], out: &mut Vec<String>) {
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut widths = vec![0; cols];
    for r in rows {
        for (i, c) in r.iter().enumerate() {
            widths[i] = widths[i].max(width(c));
        }
    }
    let total: usize = widths.iter().sum::<usize>() + 2 * cols.saturating_sub(1);
    if total <= WIDTH {
        for (n, r) in rows.iter().enumerate() {
            let mut line = String::from(if n == 0 { TABLE_HEAD } else { TABLE_ROW });
            for (i, c) in r.iter().enumerate() {
                line.push_str(c);
                if i + 1 < r.len() {
                    line.push_str(&" ".repeat(widths[i] - width(c) + 2));
                }
            }
            out.push(line.trim_end().to_string());
        }
        return;
    }
    // too wide: the first cell, then the others, indented
    let head = &rows[0];
    for (n, r) in rows.iter().enumerate() {
        if n == 0 {
            continue;
        }
        let mut first = true;
        for (i, c) in r.iter().enumerate() {
            if c.is_empty() {
                continue;
            }
            if first {
                for l in wrap(c, &TABLE_HEAD.to_string(), &format!("{TABLE_HEAD}  "), WIDTH + 1) {
                    out.push(l);
                }
                first = false;
            } else {
                let label = head.get(i).map(|h| format!("{h}: ")).unwrap_or_default();
                for l in wrap(&format!("{label}{c}"), &format!("{TABLE_ROW}    "), &format!("{TABLE_ROW}      "), WIDTH + 1) {
                    out.push(l);
                }
            }
        }
    }
}

/// A manual as the IDE's help text.
pub fn help_text(md: &str) -> String {
    let md = ascii(md);
    let lines: Vec<&str> = md.lines().collect();
    let mut out: Vec<String> = vec![];
    let mut i = 0;
    // a paragraph's lines are joined, then wrapped
    let mut para: Vec<String> = vec![];
    let mut para_first = String::new();
    let mut para_rest = String::new();
    let flush = |para: &mut Vec<String>, first: &str, rest: &str, out: &mut Vec<String>| {
        if para.is_empty() {
            return;
        }
        let text = para.join(" ");
        out.extend(wrap(&text, first, rest, WIDTH));
        para.clear();
    };
    while i < lines.len() {
        let line = lines[i];
        let t = line.trim_start();
        let indent = line.len() - t.len();
        if t.starts_with("```") {
            // a code block, as it is
            flush(&mut para, &para_first, &para_rest, &mut out);
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                let l = lines[i];
                let l = if l.len() >= indent && l[..indent].trim().is_empty() { &l[indent..] } else { l.trim_start() };
                let mut s = format!("{CODE}  {l}");
                if s.len() > WIDTH + 1 {
                    s.truncate(WIDTH + 1);
                }
                out.push(s);
                i += 1;
            }
            i += 1;
            continue;
        }
        if t.is_empty() {
            flush(&mut para, &para_first, &para_rest, &mut out);
            if out.last().is_some_and(|l| !l.is_empty()) {
                out.push(String::new());
            }
            i += 1;
            continue;
        }
        if let Some(q) = t.strip_prefix("> ") {
            // a quote: as a paragraph of its own
            flush(&mut para, &para_first, &para_rest, &mut out);
            out.extend(wrap(&inline(q), "  ", "  ", WIDTH));
            i += 1;
            continue;
        }
        if t.starts_with('#') {
            flush(&mut para, &para_first, &para_rest, &mut out);
            let level = t.chars().take_while(|&c| c == '#').count();
            let text = inline(t[level..].trim());
            let mark = if level <= 2 { CHAPTER } else { SECTION };
            let text = if level == 1 { text.to_uppercase() } else { text };
            out.extend(wrap(&text, &mark.to_string(), &mark.to_string(), WIDTH + 1));
            i += 1;
            continue;
        }
        if t == "---" || t == "***" || t == "___" {
            flush(&mut para, &para_first, &para_rest, &mut out);
            out.push(format!("{RULE}{}", "-".repeat(WIDTH)));
            i += 1;
            continue;
        }
        if t.starts_with('|') {
            flush(&mut para, &para_first, &para_rest, &mut out);
            let mut rows: Vec<Vec<String>> = vec![];
            while i < lines.len() && lines[i].trim_start().starts_with('|') {
                let r = lines[i].trim();
                // the cells: split at bars that are not escaped
                let mut cells = vec![];
                let mut cell = String::new();
                let mut chars = r.chars().peekable();
                chars.next();
                while let Some(c) = chars.next() {
                    if c == '\\' && chars.peek() == Some(&'|') {
                        cell.push('\\');
                        cell.push('|');
                        chars.next();
                    } else if c == '|' {
                        cells.push(inline(cell.trim()));
                        cell.clear();
                    } else {
                        cell.push(c);
                    }
                }
                let sep = cells.iter().all(|c| !c.is_empty() && c.chars().all(|x| x == '-' || x == ':'));
                if !sep {
                    rows.push(cells);
                }
                i += 1;
            }
            if !rows.is_empty() {
                table(&rows, &mut out);
            }
            continue;
        }
        // list items: `* `, `- `, `1. `
        let bullet = if t.starts_with("* ") || t.starts_with("- ") {
            Some(("- ".to_string(), 2))
        } else {
            let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
            if digits > 0 && t[digits..].starts_with(". ") { Some((t[..digits + 2].to_string(), digits + 2)) } else { None }
        };
        if let Some((mark, skip)) = bullet {
            flush(&mut para, &para_first, &para_rest, &mut out);
            let pad = " ".repeat(indent);
            para_first = format!("{pad}{mark}");
            para_rest = format!("{pad}{}", " ".repeat(mark.len()));
            para.push(inline(&t[skip..]));
            i += 1;
            continue;
        }
        if para.is_empty() {
            para_first = " ".repeat(indent);
            para_rest = para_first.clone();
        }
        para.push(inline(t));
        i += 1;
    }
    flush(&mut para, &para_first, &para_rest, &mut out);
    let mut s = out.join("\n");
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_to_help() {
        let md = "# Manual\n\nSome **bold** and `code` and [a link](#x).\n\n## 9. Strings\n\n| A | B |\n|---|---|\n| `x` | y \\| z |\n\n```lua\nprint(1)\n```\n\n* item one\n  more\n\n---\n";
        let h = help_text(md);
        let lines: Vec<&str> = h.lines().collect();
        assert_eq!(lines[0], "\x11MANUAL");
        assert_eq!(lines[2], "Some bold and \x01code\x02 and a link.");
        assert_eq!(lines[4], "\x119. Strings");
        assert_eq!(lines[6], "\x14A  B");
        assert_eq!(lines[7], "\x15\x01x\x02  y | z");
        assert_eq!(lines[9], "\x13  print(1)");
        assert_eq!(lines[11], "- item one more");
        assert!(lines[13].starts_with("\x16---"));
        for l in &lines {
            assert!(width(l.trim_start_matches(|c: char| ('\x11'..='\x16').contains(&c))) <= WIDTH, "{l:?}");
        }
    }
}
