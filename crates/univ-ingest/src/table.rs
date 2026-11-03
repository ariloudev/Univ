//! Reading a provider's table a row at a time: `VizieR`'s `asu-tsv` and the CSV of `SkyServer`
//! and TAP services. Each row's cells are lent as slices of the answer, so a cell is copied
//! only when CSV quoting must be undone.

use std::borrow::Cow;

use crate::source::Access;

/// The format of a provider's table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    /// `VizieR`'s `asu-tsv`: `#` comments, a header, a units line and a line of dashes, then one
    /// tab-separated row per line.
    VizierTsv,
    /// CSV: `#` lines skipped, a header, then rows, with fields in double quotes where they hold
    /// a comma.
    Csv,
}

/// A table being read: its header, and its rows one at a time.
pub struct TableReader<'a> {
    columns: Vec<Cow<'a, str>>,
    lines: Box<dyn Iterator<Item = &'a str> + 'a>,
    format: Format,
    row: Vec<Cow<'a, str>>,
}

impl<'a> TableReader<'a> {
    /// Starts reading `body`, an answer of `access`, and reads its header. An answer of `VizieR`
    /// comments only is a table of no columns and no rows, unless a comment reports an error.
    ///
    /// # Errors
    ///
    /// Fails when the answer is not UTF-8 or not a table of that format, such as an error page.
    pub fn new(access: &Access, body: &'a [u8]) -> Result<Self, String> {
        let text =
            std::str::from_utf8(body).map_err(|e| format!("the answer is not UTF-8: {e}"))?;
        let format = match access {
            Access::VizierTsv { .. } => Format::VizierTsv,
            Access::SkyserverSql { .. } | Access::Tap { .. } => Format::Csv,
            Access::Files { .. } | Access::Repository { .. } => {
                return Err("files are read by their own readers".to_owned());
            }
        };
        // Blank lines before the header are layout, as is the empty line ending an answer; a
        // blank line between rows is a blank row.
        let mut lines = without_final_blank_lines(text)
            .lines()
            .filter(|l| !l.starts_with('#'))
            .skip_while(|l| l.trim().is_empty());
        let mut reader = Self {
            columns: Vec::new(),
            lines: Box::new(std::iter::empty()),
            format,
            row: Vec::new(),
        };
        let Some(header) = lines.next() else {
            return match (format, text.lines().find(|l| vizier_error(l))) {
                (Format::Csv, _) => Err("the answer holds no table".to_owned()),
                (Format::VizierTsv, Some(line)) => {
                    Err(format!("VizieR reports an error: {}", line.trim()))
                }
                (Format::VizierTsv, None) => Ok(reader),
            };
        };
        match format {
            Format::VizierTsv => {
                lines.next().ok_or("the table has no units line")?;
                let dashes = lines.next().ok_or("the table has no line of dashes")?;
                if !dashes.trim_start().starts_with('-') {
                    return Err(format!(
                        "expected a line of dashes under the header, found {dashes:?}"
                    ));
                }
            }
            Format::Csv if header.trim_start().starts_with('<') => {
                return Err(format!(
                    "the answer is markup, not CSV: {}",
                    header.chars().take(120).collect::<String>()
                ));
            }
            Format::Csv => {}
        }
        split(format, header, &mut reader.columns);
        reader.lines = Box::new(lines);
        Ok(reader)
    }

    /// The names of the columns, in order; none for an empty `VizieR` answer.
    #[must_use]
    pub fn columns(&self) -> &[Cow<'a, str>] {
        &self.columns
    }

    /// The position of the column `name`, if the table has it.
    #[must_use]
    pub fn index(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c == name)
    }

    /// The next row's cells, trimmed, or none once the table ends. An empty line between rows
    /// is a row of blank cells.
    ///
    /// # Errors
    ///
    /// Fails on a row whose number of cells is not the number of columns.
    pub fn next_row(&mut self) -> Option<Result<&[Cow<'a, str>], String>> {
        let line = self.lines.next()?;
        if line.trim().is_empty() {
            self.row.clear();
            self.row.resize(self.columns.len(), Cow::Borrowed(""));
            return Some(Ok(&self.row));
        }
        split(self.format, line, &mut self.row);
        if self.row.len() == self.columns.len() {
            Some(Ok(&self.row))
        } else {
            Some(Err(format!(
                "a row has {} cells for {} columns: {line:?}",
                self.row.len(),
                self.columns.len()
            )))
        }
    }

    /// The number of rows left, each checked for its width.
    ///
    /// # Errors
    ///
    /// Fails on the first row whose width is wrong.
    pub fn count_rows(mut self) -> Result<u64, String> {
        let mut rows = 0;
        while let Some(row) = self.next_row() {
            row?;
            rows += 1;
        }
        Ok(rows)
    }
}

/// `text` without the blank lines that end it.
fn without_final_blank_lines(text: &str) -> &str {
    let (mut end, mut at) = (0, 0);
    for line in text.split_inclusive('\n') {
        at += line.len();
        if !line.trim().is_empty() {
            end = at;
        }
    }
    &text[..end]
}

/// Splits `line` into `cells`, replacing what they held.
fn split<'a>(format: Format, line: &'a str, cells: &mut Vec<Cow<'a, str>>) {
    cells.clear();
    match format {
        Format::VizierTsv => cells.extend(line.split('\t').map(|c| Cow::Borrowed(c.trim()))),
        Format::Csv => csv_fields(line, cells),
    }
}

/// A `VizieR` comment that reports an error: an `#INFO` line naming one, or a `#***` line.
fn vizier_error(line: &str) -> bool {
    line.starts_with("#***")
        || (line.starts_with("#INFO") && line.to_ascii_lowercase().contains("error"))
}

/// The trimmed fields of one CSV line: borrowed unless a field is quoted.
fn csv_fields<'a>(line: &'a str, fields: &mut Vec<Cow<'a, str>>) {
    let mut rest = line;
    loop {
        if !rest.trim_start().starts_with('"') {
            let (field, next) = rest
                .split_once(',')
                .map_or((rest, None), |(f, n)| (f, Some(n)));
            fields.push(Cow::Borrowed(field.trim()));
            match next {
                Some(next) => rest = next,
                None => return,
            }
            continue;
        }
        // A quoted field: undo doubled quotes, and take what follows the closing quote up to
        // the next comma as part of it, as a lenient reader does.
        let mut field = String::new();
        let mut chars = rest.trim_start()[1..].char_indices().peekable();
        let start = rest.len() - rest.trim_start().len() + 1;
        let mut end = rest.len();
        let mut quoted = true;
        while let Some((i, c)) = chars.next() {
            match (c, quoted) {
                ('"', true) if chars.peek().map(|p| p.1) == Some('"') => {
                    field.push('"');
                    chars.next();
                }
                ('"', _) => quoted = !quoted,
                (',', false) => {
                    end = start + i;
                    break;
                }
                _ => field.push(c),
            }
        }
        fields.push(Cow::Owned(field.trim().to_owned()));
        if end >= rest.len() {
            return;
        }
        rest = &rest[end + 1..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(line: &str) -> Vec<String> {
        let mut out = Vec::new();
        csv_fields(line, &mut out);
        out.into_iter().map(Cow::into_owned).collect()
    }

    #[test]
    fn csv_fields_keep_quoted_commas_and_doubled_quotes() {
        assert_eq!(
            fields(r#"1,"a, b","say ""hi""",  2.5 "#),
            ["1", "a, b", "say \"hi\"", "2.5"]
        );
        assert_eq!(fields(""), [""]);
        assert_eq!(fields("a,,b,"), ["a", "", "b", ""]);
        assert_eq!(fields(r#""x""#), ["x"]);
    }

    #[test]
    fn unquoted_cells_are_borrowed_from_the_answer() {
        let mut out = Vec::new();
        csv_fields("1, 2 ,\"3\"", &mut out);
        assert!(matches!(out[0], Cow::Borrowed("1")));
        assert!(matches!(out[1], Cow::Borrowed("2")));
        assert!(matches!(out[2], Cow::Owned(_)));
    }
}
