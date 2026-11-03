//! FITS files at the level of their headers: 2880-byte blocks of 80-byte cards ending at `END`,
//! the data each header announces, and `BSCALE`/`BZERO` (FITS Standard 4.0, Pence et al. 2010,
//! A&A 524, A42, §§3–4). The image reader of `grids` and the table reader of `normalize` build
//! on it; it decodes no data itself.

use std::io::Read;
use std::ops::Range;

use thiserror::Error;

/// FITS files are written in blocks of this many bytes; a header is 36 cards of 80.
pub const BLOCK: usize = 2_880;
/// The bytes of a header card.
pub const CARD: usize = 80;

/// A FITS file that cannot be read.
#[derive(Debug, Error)]
pub enum FitsError {
    #[error("reading the file failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Malformed(String),
}

fn malformed(reason: impl Into<String>) -> FitsError {
    FitsError::Malformed(reason.into())
}

/// One header card: its keyword, its value as written (a string without its quotes), and its
/// comment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    pub keyword: String,
    pub value: Option<String>,
    pub comment: Option<String>,
}

/// A header: its cards, in order, `END` and blank cards left out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Header {
    pub cards: Vec<Card>,
}

/// A header-data unit of a file in memory: its header and where its data lies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hdu {
    pub header: Header,
    pub data: Range<usize>,
}

impl Header {
    /// Reads header blocks up to and including the one that holds `END`; none at a clean end
    /// of file.
    ///
    /// # Errors
    ///
    /// Fails on a reader error, or a file that ends before `END`.
    pub fn read_from(reader: &mut impl Read) -> Result<Option<Self>, FitsError> {
        let mut cards = Vec::new();
        let mut block = vec![0_u8; BLOCK];
        loop {
            let filled = fill(reader, &mut block)?;
            if filled == 0 && cards.is_empty() {
                return Ok(None);
            }
            if filled < BLOCK {
                return Err(malformed("the header ends without an END card"));
            }
            for raw in block.chunks(CARD) {
                let text: String = raw.iter().map(|&b| char::from(b)).collect();
                let keyword = text[..8].trim_end().to_owned();
                if keyword == "END" {
                    return Ok(Some(Self { cards }));
                }
                if !keyword.is_empty() {
                    cards.push(parse_card(keyword, &text));
                }
            }
        }
    }

    /// Reads the header that starts at `*at` in `fits`, leaving `*at` at the block after `END`.
    ///
    /// # Errors
    ///
    /// Fails when the file ends before `END`.
    pub fn read(fits: &[u8], at: &mut usize) -> Result<Self, FitsError> {
        let mut rest = fits.get(*at..).unwrap_or_default();
        let header = Self::read_from(&mut rest)?
            .ok_or_else(|| malformed("the file ends before a header"))?;
        *at = fits.len() - rest.len();
        Ok(header)
    }

    /// The card with this keyword.
    #[must_use]
    pub fn card(&self, keyword: &str) -> Option<&Card> {
        self.cards.iter().find(|c| c.keyword == keyword)
    }

    /// The value of `keyword`, as text without its quotes or comment.
    #[must_use]
    pub fn get(&self, keyword: &str) -> Option<&str> {
        self.card(keyword).and_then(|c| c.value.as_deref())
    }

    /// The number `keyword` holds, if the header has it. FITS writes an exponent with `D` as
    /// well as `E`.
    ///
    /// # Errors
    ///
    /// Fails when the value is not a number.
    pub fn number(&self, keyword: &str) -> Result<Option<f64>, FitsError> {
        self.get(keyword)
            .map(|text| {
                text.replace(['D', 'd'], "E")
                    .parse()
                    .map_err(|_| malformed(format!("{keyword} = {text} is not a number")))
            })
            .transpose()
    }

    /// The whole number `keyword` holds, or `default` when the header lacks it.
    ///
    /// # Errors
    ///
    /// Fails when the value is not a whole number.
    pub fn whole(&self, keyword: &str, default: i64) -> Result<i64, FitsError> {
        self.get(keyword).map_or(Ok(default), |v| {
            v.parse()
                .map_err(|_| malformed(format!("{keyword} = {v} is not a whole number")))
        })
    }

    /// `BSCALE` and `BZERO`, 1 and 0 by default: a stored array value `s` stands for
    /// `BZERO + BSCALE × s`.
    ///
    /// # Errors
    ///
    /// Fails when either is not a number.
    pub fn scaling(&self) -> Result<(f64, f64), FitsError> {
        Ok((
            self.number("BSCALE")?.unwrap_or(1.0),
            self.number("BZERO")?.unwrap_or(0.0),
        ))
    }

    /// The bytes of data the header announces, before padding:
    /// `|BITPIX| / 8 × GCOUNT × (PCOUNT + NAXIS1 × … × NAXISn)`, none when `NAXIS` is 0.
    ///
    /// # Errors
    ///
    /// Fails on a negative or malformed axis, count or `BITPIX`.
    pub fn data_bytes(&self) -> Result<usize, FitsError> {
        let count = |keyword: &str, default: i64| {
            usize::try_from(self.whole(keyword, default)?)
                .map_err(|_| malformed(format!("{keyword} is negative")))
        };
        let axes = count("NAXIS", 0)?;
        if axes == 0 {
            return Ok(0);
        }
        let mut elements = 1;
        for i in 1..=axes {
            elements *= count(&format!("NAXIS{i}"), 0)?;
        }
        let bits = self.whole("BITPIX", 8)?.unsigned_abs();
        let bytes = usize::try_from(bits / 8).map_err(|_| malformed("BITPIX is too large"))?;
        Ok(bytes * count("GCOUNT", 1)? * (count("PCOUNT", 0)? + elements))
    }
}

/// Reads the header-data unit at `*at` of a file in memory, leaving `*at` at the next one.
///
/// # Errors
///
/// Fails on a header that cannot be read or data that runs past the end of the file.
pub fn next_hdu(fits: &[u8], at: &mut usize) -> Result<Hdu, FitsError> {
    let header = Header::read(fits, at)?;
    let start = *at;
    let end = start + header.data_bytes()?;
    if end > fits.len() {
        return Err(malformed("the file ends inside the data of a header"));
    }
    *at = end.div_ceil(BLOCK) * BLOCK;
    Ok(Hdu {
        header,
        data: start..end,
    })
}

/// A card's value and comment, when columns 9 and 10 hold the value indicator `= `.
fn parse_card(keyword: String, text: &str) -> Card {
    if &text[8..10] != "= " {
        let comment = text[8..].trim();
        let comment = (!comment.is_empty()).then(|| comment.to_owned());
        return Card {
            keyword,
            value: None,
            comment,
        };
    }
    let field = text[10..].trim_start();
    let (value, rest) = if let Some(quoted) = field.strip_prefix('\'') {
        // A string ends at a quote not doubled; a doubled quote stands for one.
        let mut value = String::new();
        let mut chars = quoted.char_indices().peekable();
        let mut end = quoted.len();
        while let Some((i, c)) = chars.next() {
            if c == '\'' {
                if chars.peek().map(|&(_, n)| n) == Some('\'') {
                    chars.next();
                } else {
                    end = i + 1;
                    break;
                }
            }
            value.push(c);
        }
        (value.trim_end().to_owned(), &quoted[end..])
    } else {
        let end = field.find('/').unwrap_or(field.len());
        (field[..end].trim().to_owned(), &field[end..])
    };
    let comment = rest
        .trim_start()
        .strip_prefix('/')
        .map(|c| c.trim().to_owned());
    Card {
        keyword,
        value: Some(value),
        comment,
    }
}

/// Fills `buf` from the reader as far as it goes; the count is short only at the end of file.
///
/// # Errors
///
/// Fails on a reader error.
pub fn fill(reader: &mut impl Read, buf: &mut [u8]) -> Result<usize, FitsError> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(filled)
}

/// Skips the fill after `bytes` of data up to the end of its block; a file may end without it.
///
/// # Errors
///
/// Fails on a reader error.
pub fn skip_padding(reader: &mut impl Read, bytes: usize) -> Result<(), FitsError> {
    let pad = (BLOCK - bytes % BLOCK) % BLOCK;
    fill(reader, &mut vec![0; pad])?;
    Ok(())
}

/// The file names a FITS file is published under: alone, gzipped, or in a gzipped tar.
#[must_use]
pub fn is_fits(path: &str) -> bool {
    [".fits", ".fits.gz", ".fits.tgz"]
        .iter()
        .any(|e| path.ends_with(e))
}

/// The FITS file in `bytes`, named `name`: the first `.fits` member of a gzipped tar, a gzipped
/// file unpacked, or the bytes as they are.
///
/// # Errors
///
/// Fails when the archive cannot be read or holds no `.fits` member.
pub fn unpack(name: &str, bytes: Vec<u8>) -> Result<Vec<u8>, String> {
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
        for entry in archive.entries().map_err(|e| e.to_string())? {
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

/// Test helper: a header block from cards, `END` added, padded with blanks.
#[cfg(test)]
pub(crate) fn header_block(cards: &[&str]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for card in cards.iter().chain(&["END"]) {
        bytes.extend(card.as_bytes());
        bytes.resize(bytes.len().div_ceil(CARD) * CARD, b' ');
    }
    bytes.resize(bytes.len().div_ceil(BLOCK) * BLOCK, b' ');
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_gives_its_values_without_quotes_or_comments_and_ends_its_block() {
        let fits = header_block(&[
            "SIMPLE  =                    T / conforms",
            "BITPIX  =                  -32",
            "NAXIS   =                    2",
            "NAXIS1  =                    3",
            "NAXIS2  =                    2",
            "BSCALE  =               2.0D0 / a D exponent",
            "OBJECT  = 'it''s a grid'       / a quote doubled",
            "COMMENT no value here",
        ]);
        let mut at = 0;
        let header = Header::read(&fits, &mut at).unwrap();
        assert_eq!(at, BLOCK);
        assert_eq!(header.get("SIMPLE"), Some("T"));
        let object = header.card("OBJECT").unwrap();
        assert_eq!(object.value.as_deref(), Some("it's a grid"));
        assert_eq!(object.comment.as_deref(), Some("a quote doubled"));
        assert_eq!(header.get("COMMENT"), None);
        assert_eq!(header.scaling().unwrap(), (2.0, 0.0));
        assert_eq!(header.data_bytes().unwrap(), 4 * 3 * 2);
    }

    #[test]
    fn each_unit_starts_on_a_block_after_the_padded_data_of_the_last() {
        let mut fits = header_block(&[
            "SIMPLE  =                    T",
            "BITPIX  =                    8",
            "NAXIS   =                    1",
            "NAXIS1  =                   10",
        ]);
        fits.extend([7; 10]);
        fits.resize(2 * BLOCK, 0);
        fits.extend(header_block(&[
            "XTENSION= 'IMAGE   '",
            "BITPIX  =                    8",
            "NAXIS   =                    0",
        ]));
        let mut at = 0;
        let primary = next_hdu(&fits, &mut at).unwrap();
        assert_eq!((primary.data, at), (BLOCK..BLOCK + 10, 2 * BLOCK));
        let image = next_hdu(&fits, &mut at).unwrap();
        assert_eq!(image.header.get("XTENSION"), Some("IMAGE"));
        assert_eq!((image.data.len(), at), (0, 3 * BLOCK));
        assert!(
            next_hdu(&fits[..BLOCK + 5], &mut 0).is_err(),
            "data past the end"
        );
        assert!(Header::read(&fits[..CARD], &mut 0).is_err(), "no END");
    }
}
