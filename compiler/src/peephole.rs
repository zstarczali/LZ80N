//! Peephole optimizer over the generated assembly text.
//!
//! Works on compiler-generated code only (the runtime is hand-written).
//! Inline `asm` blocks are never touched: they are bracketed by
//! `; asm begin` / `; asm end` comments and act as barriers.

pub fn optimize(code: &str) -> String {
    let mut lines: Vec<String> = code.lines().map(str::to_string).collect();
    loop {
        let before = lines.len();
        let mut changed = false;
        lines = pass(lines, &mut changed);
        lines = thread_jumps(lines, &mut changed);
        lines = drop_unused_labels(lines, &mut changed);
        if !changed && lines.len() == before {
            break;
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Remove compiler-generated `__Lnn:` labels that nothing refers to.
fn drop_unused_labels(lines: Vec<String>, changed: &mut bool) -> Vec<String> {
    use std::collections::HashSet;
    let mut used = HashSet::new();
    for l in &lines {
        if is_label(l) {
            continue;
        }
        let code = l.split(';').next().unwrap_or("");
        for tok in code.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
            if tok.starts_with("__L") {
                used.insert(tok.to_string());
            }
        }
    }
    let before = lines.len();
    let out: Vec<String> = lines
        .into_iter()
        .filter(|l| {
            let name = l.trim_end().trim_end_matches(':');
            !(is_label(l) && name.starts_with("__L") && !used.contains(name))
        })
        .collect();
    if out.len() != before {
        *changed = true;
    }
    out
}

/// A jump to a compiler label whose first instruction is an unconditional
/// `jp X` goes straight to X.
fn thread_jumps(mut lines: Vec<String>, changed: &mut bool) -> Vec<String> {
    use std::collections::HashMap;
    let mut first: HashMap<String, String> = HashMap::new();
    let mut in_asm = false;
    for (i, l) in lines.iter().enumerate() {
        let t = l.trim();
        if t == "; asm begin" {
            in_asm = true;
        } else if t == "; asm end" {
            in_asm = false;
        }
        if in_asm || !is_label(l) {
            continue;
        }
        let name = l.trim_end().trim_end_matches(':');
        if !name.starts_with("__L") {
            continue;
        }
        let next = lines[i + 1..].iter().find(|n| !is_label(n) && !is_comment(n));
        if let Some(target) = next.and_then(|n| ins(n)).and_then(|n| n.strip_prefix("jp ")) {
            if target.starts_with("__L") && !target.contains(',') && target != name {
                first.insert(name.to_string(), target.to_string());
            }
        }
    }
    let resolve = |t: &str| {
        let mut t = t.to_string();
        for _ in 0..8 {
            match first.get(&t) {
                Some(n) => t = n.clone(),
                None => break,
            }
        }
        t
    };
    in_asm = false;
    for l in lines.iter_mut() {
        let t = l.trim();
        if t == "; asm begin" {
            in_asm = true;
        } else if t == "; asm end" {
            in_asm = false;
        }
        let Some(cur) = (if in_asm { None } else { ins(l) }) else { continue };
        let Some(rest) = cur.strip_prefix("jp ") else { continue };
        let (cc, target) = match rest.split_once(',') {
            Some((c, t)) => (Some(c), t),
            None => (None, rest),
        };
        if !first.contains_key(target) {
            continue;
        }
        let new = resolve(target);
        if new != target {
            *l = match cc {
                Some(c) => format!("    jp {c},{new}"),
                None => format!("    jp {new}"),
            };
            *changed = true;
        }
    }
    lines
}

fn invert(cc: &str) -> Option<&'static str> {
    Some(match cc {
        "z" => "nz",
        "nz" => "z",
        "c" => "nc",
        "nc" => "c",
        "p" => "m",
        "m" => "p",
        "pe" => "po",
        "po" => "pe",
        _ => return None,
    })
}

fn ins(l: &str) -> Option<&str> {
    // an instruction line: indented, not a comment
    let t = l.trim();
    if l.starts_with(' ') && !t.is_empty() && !t.starts_with(';') {
        Some(t)
    } else {
        None
    }
}

fn is_label(l: &str) -> bool {
    !l.starts_with(' ') && l.trim_end().ends_with(':')
}

fn is_comment(l: &str) -> bool {
    l.trim().starts_with(';')
}

fn pass(lines: Vec<String>, changed: &mut bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut in_asm = false;
    let mut dead = false;
    let mut i = 0;
    'outer: while i < lines.len() {
        let l = &lines[i];
        let t = l.trim();
        if t == "; asm begin" {
            in_asm = true;
            dead = false;
        }
        if in_asm {
            if t == "; asm end" {
                in_asm = false;
            }
            out.push(l.clone());
            i += 1;
            continue;
        }
        if is_label(l) {
            dead = false;
        }
        if dead {
            if ins(l).is_some() {
                *changed = true;
                i += 1;
                continue;
            }
        }
        let Some(cur) = ins(l) else {
            out.push(l.clone());
            i += 1;
            continue;
        };
        // next instruction (skipping comments only)
        let mut j = i + 1;
        while j < lines.len() && is_comment(&lines[j]) && lines[j].trim() != "; asm begin" {
            j += 1;
        }
        let next = lines.get(j).map(|s| s.as_str()).unwrap_or("");
        let next_ins = ins(next);

        // jp L ; L:   -> drop the jump
        if let Some(target) = cur.strip_prefix("jp ") {
            // the target among the labels that follow
            let mut k = j;
            let mut hit = false;
            while k < lines.len() && (is_label(&lines[k]) || is_comment(&lines[k])) {
                hit |= lines[k].trim_end() == format!("{target}:");
                k += 1;
            }
            if !target.contains(',') && hit {
                *changed = true;
                i += 1;
                continue;
            }
        }
        // jp cc,L1 ; jp L2 ; L1:  ->  jp !cc,L2 ; L1:
        if let (Some((cc, l1)), Some(n)) = (cur.strip_prefix("jp ").and_then(|r| r.split_once(',')), next_ins) {
            if let (Some(inv), Some(l2)) = (invert(cc), n.strip_prefix("jp ")) {
                let mut k = j + 1;
                let mut hit = false;
                while k < lines.len() && (is_label(&lines[k]) || is_comment(&lines[k])) {
                    hit |= lines[k].trim_end() == format!("{l1}:");
                    k += 1;
                }
                if hit && !l2.contains(',') && !l2.starts_with('(') {
                    out.push(format!("    jp {inv},{l2}"));
                    *changed = true;
                    i = j + 1;
                    continue;
                }
            }
        }
        // ld (X),a ; ld a,(X)  /  ld (X),hl ; ld hl,(X)  -> drop the reload
        if let (Some(dst), Some(n)) = (cur.strip_prefix("ld ("), next_ins) {
            for (suffix, reload) in [("),a", "ld a,("), ("),hl", "ld hl,(")] {
                if let Some(x) = dst.strip_suffix(suffix) {
                    if n == format!("{reload}{x})") {
                        out.push(l.clone());
                        out.extend(lines[i + 1..j].iter().cloned());
                        *changed = true;
                        i = j + 1;
                        continue 'outer;
                    }
                }
            }
        }
        // ld a,(X) ; ld r,a ; ld a,(X)  -> drop the second load
        if let (Some(src), Some(n)) = (cur.strip_prefix("ld a,("), next_ins) {
            if matches!(n, "ld b,a" | "ld c,a" | "ld d,a" | "ld e,a" | "ld l,a" | "ld h,a") {
                if let Some(n2) = lines.get(j + 1).and_then(|s| ins(s)) {
                    if n2 == format!("ld a,({src}") {
                        out.push(l.clone());
                        out.push(lines[j].clone());
                        *changed = true;
                        i = j + 2;
                        continue;
                    }
                }
            }
        }
        // ex de,hl ; ex de,hl  -> nothing
        if cur == "ex de,hl" && next_ins == Some("ex de,hl") {
            *changed = true;
            i = j + 1;
            continue;
        }
        // cheaper equivalents (flags after these are never consumed)
        let repl = match cur {
            "add a,1" => Some("inc a"),
            "sub 1" => Some("dec a"),
            "add a,255" => Some("dec a"),
            "ld hl,0\n" => None,
            _ => None,
        };
        if let Some(r) = repl {
            out.push(format!("    {r}"));
            *changed = true;
            i += 1;
            continue;
        }
        if cur.starts_with("jp ") && !cur.contains(',') || cur == "ret" {
            dead = true;
        }
        out.push(l.clone());
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::optimize;

    #[test]
    fn rules() {
        let src = "    ld (g_x),a\n    ; c\n    ld a,(g_x)\n    jp __L1\n    xor a\n__L1:\n    add a,1\n    ret\n    ret\n";
        assert_eq!(optimize(src), "    ld (g_x),a\n    ; c\n    inc a\n    ret\n");
    }

    #[test]
    fn jumps() {
        let src = "    jp nz,__L1
    jp __L2
__L1:
    inc a
__L2:
    jp __L3
__L4:
    jp z,__L2
__L3:
    ret
";
        assert_eq!(optimize(src), "    jp z,__L3\n    inc a\n__L3:\n    ret\n");
    }

    #[test]
    fn asm_untouched() {
        let src = "    ret\n    ; asm begin\n    ld a,(1)\n    ld (1),a\n    ld a,(1)\n    ; asm end\n";
        assert_eq!(optimize(src), src);
    }
}
