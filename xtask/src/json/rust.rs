//! The string literals of Rust source, where its SQL is: comments, characters and lifetimes are
//! passed over.

/// Each string literal of `code`: the byte offset of its text in `code`, and its text with each
/// escape blanked to spaces of the same length, so that an offset in it plus the first is an
/// offset in `code`. A `\` ending a line keeps its line break.
pub(super) fn literals(code: &str) -> Vec<(usize, String)> {
    let b = code.as_bytes();
    let ident = |i: usize| i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                i = code[i..].find('\n').map_or(b.len(), |e| i + e + 1);
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i = code[i + 2..].find("*/").map_or(b.len(), |e| i + 2 + e + 2);
            }
            b'"' => {
                let (end, text) = escaped(code, i + 1);
                out.push((i + 1, text));
                i = end + 1;
            }
            b'b' | b'r' if !ident(i) => {
                let mut j = i + usize::from(b[i] == b'b');
                if b.get(j) == Some(&b'r') {
                    j += 1;
                    let hashes = b[j..].iter().take_while(|c| **c == b'#').count();
                    if b.get(j + hashes) == Some(&b'"') {
                        let start = j + hashes + 1;
                        let close = format!("\"{}", "#".repeat(hashes));
                        let end = code[start..].find(&close).map_or(b.len(), |e| start + e);
                        out.push((start, code[start..end].to_owned()));
                        i = (end + close.len()).min(b.len());
                        continue;
                    }
                }
                i += 1;
            }
            b'\'' => i = after_quote(code, i),
            _ => i += 1,
        }
    }
    out
}

/// Past the character literal or lifetime whose `'` is at `i`.
fn after_quote(code: &str, i: usize) -> usize {
    let b = code.as_bytes();
    if b.get(i + 1) == Some(&b'\\') {
        return code[i + 2..].find('\'').map_or(b.len(), |e| i + 2 + e + 1);
    }
    let width = code[i + 1..].chars().next().map_or(0, char::len_utf8);
    if b.get(i + 1 + width) == Some(&b'\'') {
        i + 2 + width
    } else {
        i + 1
    }
}

/// The text of the literal starting at `start`, escapes blanked, and the offset of its closing
/// quote.
fn escaped(code: &str, start: usize) -> (usize, String) {
    let b = code.as_bytes();
    let mut text = Vec::new();
    let mut i = start;
    while i < b.len() && b[i] != b'"' {
        if b[i] != b'\\' {
            text.push(b[i]);
            i += 1;
            continue;
        }
        text.push(b' ');
        let Some(&next) = b.get(i + 1) else { break };
        let blank = match next {
            b'x' => 3,
            b'u' => code[i + 1..].find('}').map_or(1, |e| e + 1),
            b'\n' | b'"' | b'\\' | b'\'' => {
                text.push(next);
                i += 2;
                continue;
            }
            _ => 1,
        };
        text.extend(std::iter::repeat_n(b' ', blank));
        i += 1 + blank;
    }
    (i, String::from_utf8_lossy(&text).into_owned())
}
