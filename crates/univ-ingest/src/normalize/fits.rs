//! FITS binary tables, as the eRASS1 catalogues publish them in gzipped tar archives: the
//! first `BINTABLE` extension of a file, its cells read as text so that a row goes through the
//! same reader as a provider's table (FITS Standard 4.0, Pence et al. 2010, A&A 524, A42, §7.3).

use std::borrow::Cow;
use std::io::Read as _;

/// The bytes of a FITS block, header or data.
const BLOCK: usize = 2_880;
/// The bytes of a header card.
const CARD: usize = 80;

/// The file names a FITS table is published under: alone, gzipped, or in a gzipped tar.
pub(super) fn is_fits(path: &str) -> bool {
    [".fits", ".fits.gz", ".fits.tgz"]
        .iter()
        .any(|e| path.ends_with(e))
}

/// The FITS file in `bytes`: the first `.fits` member of a gzipped tar, a gzipped file
/// unpacked, or the bytes as they are.
pub(super) fn unpack(name: &str, bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    let gunzip = |bytes: &[u8]| {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(bytes)
            .read_to_end(&mut out)
            .map(|_| out)
            .map_err(|e| format!("it is not gzip: {e}"))
    };
    if name.ends_with(".fits.tgz") {
        let tar = gunzip(&bytes)?;
        let mut archive = tar::Archive::new(tar.as_slice());
        let entries = archive.entries().map_err(|e| e.to_string())?;
        for entry in entries {
            let mut entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path().map_err(|e| e.to_string())?;
            if path.to_string_lossy().ends_with(".fits") {
                let mut out = Vec::new();
                entry.read_to_end(&mut out).map_err(|e| e.to_string())?;
                return Ok(out);
            }
        }
        Err("the archive holds no .fits file".to_owned())
    } else if name.ends_with(".fits.gz") {
        gunzip(&bytes)
    } else {
        Ok(bytes)
    }
}

/// A column of a binary table.
#[derive(Clone, Debug)]
pub(super) struct BinColumn {
    pub name: String,
    /// The FITS type letter: `A`, `L`, `B`, `I`, `J`, `K`, `E` or `D` are read.
    kind: u8,
    repeat: usize,
    offset: usize,
    /// The integer that marks a null, `TNULLn`.
    null: Option<i64>,
    /// Whether `TSCALn` or `TZEROn` scale the stored values, which this reader does not do.
    scaled: bool,
}

/// A binary table: its columns and rows.
pub(super) struct BinTable {
    pub columns: Vec<BinColumn>,
    pub rows: usize,
    row_len: usize,
    data: Vec<u8>,
}

/// A header's cards as keyword and value.
fn header(fits: &[u8], at: &mut usize) -> Result<Vec<(String, String)>, String> {
    let mut cards = Vec::new();
    loop {
        let card = fits
            .get(*at..*at + CARD)
            .ok_or("the file ends inside a header")?;
        *at += CARD;
        let text = std::str::from_utf8(card).map_err(|_| "a header card is not ASCII")?;
        let keyword = text[..8].trim_end();
        if keyword == "END" {
            *at = at.div_ceil(BLOCK) * BLOCK;
            return Ok(cards);
        }
        if &text[8..10] == "= " {
            cards.push((keyword.to_owned(), value(&text[10..])));
        }
    }
}

/// A card's value: a quoted string with `''` for a quote, or what precedes a `/` comment.
fn value(text: &str) -> String {
    let text = text.trim_start();
    if let Some(quoted) = text.strip_prefix('\'') {
        let mut out = String::new();
        let mut chars = quoted.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\'' {
                if chars.peek() == Some(&'\'') {
                    out.push('\'');
                    chars.next();
                } else {
                    break;
                }
            } else {
                out.push(c);
            }
        }
        return out.trim_end().to_owned();
    }
    text.split('/').next().unwrap_or("").trim().to_owned()
}

/// The bytes one element of a FITS type takes; `X` counts bits.
fn width(kind: u8, repeat: usize) -> Option<usize> {
    Some(match kind {
        b'L' | b'B' | b'A' => repeat,
        b'X' => repeat.div_ceil(8),
        b'I' => 2 * repeat,
        b'J' | b'E' => 4 * repeat,
        b'K' | b'D' | b'C' | b'P' => 8 * repeat,
        b'M' | b'Q' => 16 * repeat,
        _ => return None,
    })
}

/// The first binary table extension of `fits`.
pub(super) fn first_table(fits: &[u8]) -> Result<BinTable, String> {
    let mut at = 0;
    while at < fits.len() {
        let cards = header(fits, &mut at)?;
        let get = |k: &str| {
            cards
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
        };
        let number = |k: &str| -> Result<usize, String> {
            get(k)
                .unwrap_or("0")
                .parse()
                .map_err(|e| format!("{k} is not a whole number: {e}"))
        };
        let axes = number("NAXIS")?;
        // A header of no axes has no data.
        let mut size = usize::from(axes != 0);
        for i in 1..=axes {
            size *= number(&format!("NAXIS{i}"))?;
        }
        let bits: i64 = get("BITPIX").unwrap_or("8").parse().map_err(|_| "BITPIX")?;
        let bytes =
            size * usize::try_from(bits.unsigned_abs() / 8).unwrap_or(1) + number("PCOUNT")?;
        if get("XTENSION") == Some("BINTABLE") {
            let (row_len, rows) = (number("NAXIS1")?, number("NAXIS2")?);
            let columns = columns(&get, number("TFIELDS")?, row_len)?;
            let data = fits
                .get(at..at + row_len * rows)
                .ok_or("the file ends inside the table")?
                .to_vec();
            return Ok(BinTable {
                columns,
                rows,
                row_len,
                data,
            });
        }
        at += bytes.div_ceil(BLOCK) * BLOCK;
    }
    Err("the file holds no binary table".to_owned())
}

/// The columns a table header describes, checked to fill a row of `row_len` bytes.
fn columns<'a>(
    get: &impl Fn(&str) -> Option<&'a str>,
    fields: usize,
    row_len: usize,
) -> Result<Vec<BinColumn>, String> {
    let mut columns = Vec::new();
    let mut offset = 0;
    for i in 1..=fields {
        let form = get(&format!("TFORM{i}")).ok_or(format!("TFORM{i} is missing"))?;
        // rTa: a repeat count, a type letter and characters the standard leaves free, such as
        // the optical file's `E4`.
        let digits = form.bytes().take_while(u8::is_ascii_digit).count();
        let repeat = if digits == 0 {
            1
        } else {
            form[..digits]
                .parse()
                .map_err(|_| format!("TFORM{i} {form}"))?
        };
        let kind = form.as_bytes().get(digits).copied().unwrap_or(b' ');
        let bytes = width(kind, repeat).ok_or(format!("TFORM{i} {form} is not a FITS type"))?;
        let differs = |key: String, default: f64| {
            get(&key).is_some_and(|v| v.parse::<f64>().ok() != Some(default))
        };
        columns.push(BinColumn {
            name: get(&format!("TTYPE{i}")).unwrap_or_default().to_owned(),
            kind,
            repeat,
            offset,
            null: get(&format!("TNULL{i}")).and_then(|v| v.parse().ok()),
            scaled: differs(format!("TSCAL{i}"), 1.0) || differs(format!("TZERO{i}"), 0.0),
        });
        offset += bytes;
    }
    if offset == row_len {
        Ok(columns)
    } else {
        Err(format!(
            "the columns fill {offset} bytes of a {row_len}-byte row"
        ))
    }
}

impl BinColumn {
    /// Whether a cell of this column reads as text: a string, or a single unscaled number or
    /// logical.
    pub fn readable(&self) -> bool {
        self.kind == b'A' || (self.repeat == 1 && !self.scaled && b"LBIJKED".contains(&self.kind))
    }
}

impl BinTable {
    /// The cell of `column` in `row` as text: blank for a null, a NaN or a column that is not
    /// [`BinColumn::readable`]; a string without its trailing NUL bytes and spaces; a number
    /// in the shortest form that reads back as the same value.
    pub fn text(&self, row: usize, column: &BinColumn) -> Cow<'_, str> {
        let start = row * self.row_len + column.offset;
        let Some(bytes) = self
            .data
            .get(start..start + width(column.kind, column.repeat).unwrap_or(0))
        else {
            return Cow::Borrowed("");
        };
        let array = |n: usize| -> [u8; 8] {
            let mut out = [0; 8];
            out[..n].copy_from_slice(&bytes[..n]);
            out
        };
        let integer = |i: i64| {
            if column.null == Some(i) {
                Cow::Borrowed("")
            } else {
                Cow::Owned(i.to_string())
            }
        };
        let float = |x: f64, text: String| {
            if x.is_nan() {
                Cow::Borrowed("")
            } else {
                Cow::Owned(text)
            }
        };
        if !column.readable() {
            return Cow::Borrowed("");
        }
        match column.kind {
            b'A' => {
                let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
                String::from_utf8_lossy(&bytes[..end])
                    .trim()
                    .to_owned()
                    .into()
            }
            b'L' => Cow::Borrowed(match bytes[0] {
                b'T' => "T",
                b'F' => "F",
                _ => "",
            }),
            b'B' => integer(i64::from(bytes[0])),
            b'I' => integer(i64::from(i16::from_be_bytes([bytes[0], bytes[1]]))),
            b'J' => integer(i64::from(i32::from_be_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3],
            ]))),
            b'K' => integer(i64::from_be_bytes(array(8))),
            b'E' => {
                let x = f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                float(f64::from(x), x.to_string())
            }
            b'D' => {
                let x = f64::from_be_bytes(array(8));
                float(x, x.to_string())
            }
            _ => Cow::Borrowed(""),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(text: &str) -> String {
        format!("{text:<80}")
    }

    /// A FITS file of an empty primary header and a table of a 4-byte float and a 3-byte string.
    fn table(rows: &[(f32, &str)]) -> Vec<u8> {
        let mut text = [
            "SIMPLE  =                    T",
            "BITPIX  =                    8",
            "NAXIS   =                    0",
            "END",
        ]
        .map(card)
        .concat();
        text += &" ".repeat(BLOCK - text.len());
        let head = [
            "XTENSION= 'BINTABLE'".to_owned(),
            "BITPIX  =                    8".to_owned(),
            "NAXIS   =                    2".to_owned(),
            "NAXIS1  =                    7".to_owned(),
            format!("NAXIS2  = {:>20}", rows.len()),
            "PCOUNT  =                    0".to_owned(),
            "TFIELDS =                    2".to_owned(),
            "TTYPE1  = 'Z       '".to_owned(),
            "TFORM1  = 'E5      '           / extra characters after the type".to_owned(),
            "TTYPE2  = 'KIND''S '".to_owned(),
            "TFORM2  = '3A      '".to_owned(),
            "END".to_owned(),
        ]
        .map(|c| card(&c))
        .concat();
        let mut bytes = text.into_bytes();
        bytes.extend(head.as_bytes());
        bytes.resize(2 * BLOCK, b' ');
        for (z, kind) in rows {
            bytes.extend(z.to_be_bytes());
            let mut cell = kind.as_bytes().to_vec();
            cell.resize(3, 0);
            bytes.extend(cell);
        }
        bytes
    }

    #[test]
    fn a_table_reads_numbers_in_their_shortest_form_and_strings_without_padding() {
        let t = first_table(&table(&[(0.315_28, "ph"), (f32::NAN, "sp ")])).unwrap();
        assert_eq!(t.rows, 2);
        assert_eq!(t.columns[1].name, "KIND'S");
        assert_eq!(t.text(0, &t.columns[0]), "0.31528");
        assert_eq!(t.text(0, &t.columns[1]), "ph");
        assert_eq!(t.text(1, &t.columns[0]), "", "NaN is a null");
        assert_eq!(t.text(1, &t.columns[1]), "sp");
    }

    #[test]
    fn a_file_that_ends_inside_its_table_is_refused() {
        let mut bytes = table(&[(1.0, "a")]);
        bytes.truncate(2 * BLOCK + 3);
        assert!(first_table(&bytes).is_err());
        assert!(first_table(&bytes[..BLOCK]).is_err());
    }
}
