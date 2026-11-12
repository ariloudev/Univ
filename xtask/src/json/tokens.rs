//! SQL text as tokens: words, parameters, literals and punctuation, without comments.

/// Words that never name a table, an alias or a column.
const KEYWORDS: &[&str] = &[
    "all",
    "and",
    "as",
    "asc",
    "between",
    "blob",
    "by",
    "case",
    "cast",
    "collate",
    "cross",
    "default",
    "delete",
    "desc",
    "distinct",
    "else",
    "end",
    "escape",
    "except",
    "exists",
    "false",
    "from",
    "full",
    "glob",
    "group",
    "having",
    "in",
    "indexed",
    "inner",
    "insert",
    "int",
    "integer",
    "intersect",
    "into",
    "is",
    "join",
    "left",
    "like",
    "limit",
    "natural",
    "not",
    "null",
    "numeric",
    "offset",
    "on",
    "or",
    "order",
    "outer",
    "real",
    "recursive",
    "replace",
    "returning",
    "right",
    "select",
    "set",
    "text",
    "then",
    "true",
    "union",
    "update",
    "using",
    "values",
    "when",
    "where",
    "window",
    "with",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Word,
    Param,
    Literal,
    Punct,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Token<'a> {
    pub kind: Kind,
    pub text: &'a str,
    pub at: usize,
}

impl Token<'_> {
    pub(super) fn is_punct(&self, text: &str) -> bool {
        self.kind == Kind::Punct && self.text == text
    }

    pub(super) fn is_word(&self, word: &str) -> bool {
        self.kind == Kind::Word && self.text.eq_ignore_ascii_case(word)
    }

    /// A word that may name a table, an alias or a column.
    pub(super) fn is_name(&self) -> bool {
        self.kind == Kind::Word && !KEYWORDS.contains(&self.text.to_ascii_lowercase().as_str())
    }

    pub(super) fn end(&self) -> usize {
        self.at + self.text.len()
    }
}

/// The tokens of `sql`, without its comments and white space.
pub(super) fn tokens(sql: &str) -> Vec<Token<'_>> {
    let bytes = sql.as_bytes();
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || !b.is_ascii();
    let after = |from: usize, end: &str| {
        sql[from..]
            .find(end)
            .map_or(bytes.len(), |i| from + i + end.len())
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let next = bytes.get(i + 1).copied();
        let kind = match bytes[i] {
            b if b.is_ascii_whitespace() => {
                i += 1;
                continue;
            }
            b'-' if next == Some(b'-') => {
                i = after(i, "\n");
                continue;
            }
            b'/' if next == Some(b'*') => {
                i = after(i + 2, "*/");
                continue;
            }
            b'\'' => {
                i += 1;
                while i < bytes.len() {
                    i += 1;
                    if bytes[i - 1] == b'\'' {
                        if bytes.get(i) != Some(&b'\'') {
                            break;
                        }
                        i += 1;
                    }
                }
                Kind::Literal
            }
            b'"' => {
                let end = after(i + 1, "\"");
                out.push(Token {
                    kind: Kind::Word,
                    text: &sql[i + 1..end.saturating_sub(1).max(i + 1)],
                    at: i + 1,
                });
                i = end;
                continue;
            }
            b'?' => {
                i += 1;
                while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                }
                Kind::Param
            }
            b':' | b'@' | b'$' if next.is_some_and(|b| b.is_ascii_alphabetic() || b == b'_') => {
                i += 1;
                while bytes.get(i).is_some_and(|b| word(*b)) {
                    i += 1;
                }
                Kind::Param
            }
            b if b.is_ascii_digit() => {
                while bytes
                    .get(i)
                    .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'.')
                {
                    i += 1;
                }
                Kind::Literal
            }
            b if word(b) => {
                while bytes.get(i).is_some_and(|b| word(*b)) {
                    i += 1;
                }
                Kind::Word
            }
            b'-' if next == Some(b'>') => {
                i += if bytes.get(i + 2) == Some(&b'>') {
                    3
                } else {
                    2
                };
                Kind::Punct
            }
            _ => {
                i += 1;
                Kind::Punct
            }
        };
        out.push(Token {
            kind,
            text: &sql[start..i],
            at: start,
        });
    }
    out
}
