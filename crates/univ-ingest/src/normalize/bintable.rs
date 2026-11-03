//! FITS binary tables, as the eRASS1 catalogues publish them in gzipped tar archives: the
//! first `BINTABLE` extension of a file, its cells read as text so that a row goes through the
//! same reader as a provider's table (FITS Standard 4.0, Pence et al. 2010, A&A 524, A42, §7.3).
//! Headers and samples are read by [`crate::fits`].

use std::borrow::Cow;

use crate::fits::{FitsError, Header, MAX_AXES, Sample, SampleKind, malformed, next_hdu, sample};

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

/// The bytes one element of a FITS type takes; `X` counts bits. `None` for a letter that is
/// not a FITS type, or a repeat too large to hold.
fn width(kind: u8, repeat: usize) -> Option<usize> {
    match kind {
        b'L' | b'B' | b'A' => Some(repeat),
        b'X' => Some(repeat.div_ceil(8)),
        b'I' => repeat.checked_mul(2),
        b'J' | b'E' => repeat.checked_mul(4),
        b'K' | b'D' | b'C' | b'P' => repeat.checked_mul(8),
        b'M' | b'Q' => repeat.checked_mul(16),
        _ => None,
    }
}

/// The first binary table extension of `fits`.
///
/// # Errors
///
/// Fails on a header or size that cannot be read, columns that do not fill a row, rows that
/// run past the table's data, or a file with no binary table.
pub(super) fn first_table(fits: &[u8]) -> Result<BinTable, FitsError> {
    let mut at = 0;
    while at < fits.len() {
        let hdu = next_hdu(fits, &mut at)?;
        let header = &hdu.header;
        if header.get("XTENSION") != Some("BINTABLE") {
            continue;
        }
        let axes = header.axes()?;
        let &[row_len, rows] = axes.as_slice() else {
            return Err(malformed(format!(
                "a binary table has {} axes, not 2",
                axes.len()
            )));
        };
        let fields = header.count("TFIELDS", None)?;
        if fields > MAX_AXES {
            return Err(malformed(format!(
                "TFIELDS = {fields} is more than the {MAX_AXES} fields FITS allows"
            )));
        }
        let columns = columns(header, fields, row_len)?;
        // `next_hdu` checked that the data the header declares lie in the file; the rows must
        // lie in those data.
        let data = row_len
            .checked_mul(rows)
            .and_then(|n| hdu.data.start.checked_add(n))
            .filter(|&end| end <= hdu.data.end)
            .and_then(|end| fits.get(hdu.data.start..end))
            .ok_or_else(|| malformed("the rows run past the data of the table"))?
            .to_vec();
        return Ok(BinTable {
            columns,
            rows,
            row_len,
            data,
        });
    }
    Err(malformed("the file holds no binary table"))
}

/// The columns a table header describes, checked to fill a row of `row_len` bytes.
fn columns(header: &Header, fields: usize, row_len: usize) -> Result<Vec<BinColumn>, FitsError> {
    let get = |keyword: &str| header.get(keyword);
    let mut columns = Vec::new();
    let mut offset = 0_usize;
    for i in 1..=fields {
        let form = get(&format!("TFORM{i}")).ok_or(malformed(format!("TFORM{i} is missing")))?;
        // rTa: a repeat count, a type letter and characters the standard leaves free, such as
        // the optical file's `E4`.
        let digits = form.bytes().take_while(u8::is_ascii_digit).count();
        let repeat = if digits == 0 {
            1
        } else {
            form[..digits]
                .parse()
                .map_err(|_| malformed(format!("TFORM{i} {form}")))?
        };
        let kind = form.as_bytes().get(digits).copied().unwrap_or(b' ');
        let bytes = width(kind, repeat)
            .ok_or_else(|| malformed(format!("TFORM{i} {form} is not a FITS type")))?;
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
        offset = offset
            .checked_add(bytes)
            .ok_or_else(|| malformed("the columns are too wide to hold"))?;
    }
    if offset == row_len {
        Ok(columns)
    } else {
        Err(malformed(format!(
            "the columns fill {offset} bytes of a {row_len}-byte row"
        )))
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
    /// The cell of `column` in `row` as text: blank for a null, a NaN, a cell outside the table
    /// or a column that is not [`BinColumn::readable`]; a string without its trailing NUL bytes
    /// and spaces; a number in the shortest form that reads back as the same value.
    pub fn text(&self, row: usize, column: &BinColumn) -> Cow<'_, str> {
        let bytes = row
            .checked_mul(self.row_len)
            .and_then(|start| start.checked_add(column.offset))
            .and_then(|start| {
                let end = start.checked_add(width(column.kind, column.repeat)?)?;
                self.data.get(start..end)
            });
        let Some(bytes) = bytes.filter(|_| column.readable()) else {
            return Cow::Borrowed("");
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
        match column.kind {
            b'A' => {
                let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
                String::from_utf8_lossy(&bytes[..end])
                    .trim()
                    .to_owned()
                    .into()
            }
            b'L' => Cow::Borrowed(match bytes.first() {
                Some(b'T') => "T",
                Some(b'F') => "F",
                _ => "",
            }),
            letter => match SampleKind::from_tform(letter).and_then(|k| sample(k, bytes)) {
                Some(Sample::Integer(i)) => integer(i),
                Some(Sample::Single(x)) => float(f64::from(x), x.to_string()),
                Some(Sample::Double(x)) => float(x, x.to_string()),
                None => Cow::Borrowed(""),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fits::{BLOCK, header_block};

    /// A FITS file of an empty primary header and a table of a 4-byte float and a 3-byte string.
    fn table(rows: &[(f32, &str)]) -> Vec<u8> {
        let mut bytes = header_block(&[
            "SIMPLE  =                    T",
            "BITPIX  =                    8",
            "NAXIS   =                    0",
        ]);
        let naxis2 = format!("NAXIS2  = {:>20}", rows.len());
        bytes.extend(header_block(&[
            "XTENSION= 'BINTABLE'",
            "BITPIX  =                    8",
            "NAXIS   =                    2",
            "NAXIS1  =                    7",
            &naxis2,
            "PCOUNT  =                    0",
            "TFIELDS =                    2",
            "TTYPE1  = 'Z       '",
            "TFORM1  = 'E5      '           / extra characters after the type",
            "TTYPE2  = 'KIND''S '",
            "TFORM2  = '3A      '",
        ]));
        for (z, kind) in rows {
            bytes.extend(z.to_be_bytes());
            let mut cell = kind.as_bytes().to_vec();
            cell.resize(3, 0);
            bytes.extend(cell);
        }
        bytes.resize(bytes.len().div_ceil(BLOCK) * BLOCK, 0);
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

    #[test]
    fn a_table_of_no_groups_is_refused_before_its_rows_are_read() {
        let mut bytes = header_block(&[
            "SIMPLE  =                    T",
            "BITPIX  =                    8",
            "NAXIS   =                    0",
        ]);
        bytes.extend(header_block(&[
            "XTENSION= 'BINTABLE'",
            "BITPIX  =                    8",
            "NAXIS   =                    2",
            "NAXIS1  =                    7",
            "NAXIS2  =                 1000",
            "PCOUNT  =                    0",
            "GCOUNT  =                    0",
            "TFIELDS =                    2",
            "TFORM1  = 'E'",
            "TFORM2  = '3A'",
        ]));
        assert!(first_table(&bytes).is_err());
    }
}
