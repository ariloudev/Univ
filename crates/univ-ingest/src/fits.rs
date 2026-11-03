//! FITS files at the level of their headers: 2880-byte blocks of 80-byte cards ending at `END`,
//! the data each header announces, `BSCALE`/`BZERO`, and the big-endian samples arrays and table
//! columns store (FITS Standard 4.0, Pence et al. 2010, A&A 524, A42, §§3–5). The image reader of
//! `grids` and the table reader of `normalize` build on it.
//!
//! Files come from the network, so every size a header declares is checked: a missing axis, an
//! axis count over 999, no groups, or a size past the memory's or the file's is an error.

use std::io::Read;
use std::ops::Range;

use thiserror::Error;

mod sample;

pub use sample::{Sample, SampleKind, sample};

/// FITS files are written in blocks of this many bytes; a header is 36 cards of 80.
pub const BLOCK: usize = 2_880;
/// The bytes of a header card.
pub const CARD: usize = 80;
/// The most axes an array has, and the most fields a table has (FITS 4.0 §4.4.1.1, §7.3.1).
pub const MAX_AXES: usize = 999;
/// The most bytes a gzipped file or a tar member is unpacked to: 1 GiB, a bound of this reader
/// far above the catalogues it reads, so that a hostile archive cannot fill the memory.
pub const MAX_UNPACKED: usize = 1 << 30;

/// A FITS file that cannot be read.
#[derive(Debug, Error)]
pub enum FitsError {
    #[error("reading the file failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Malformed(String),
}

pub(crate) fn malformed(reason: impl Into<String>) -> FitsError {
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
    /// Fails on a reader error, a card that is not ASCII text, or a file that ends before `END`.
    pub fn read_from(reader: &mut impl Read) -> Result<Option<Self>, FitsError> {
        let mut cards = Vec::new();
        let mut block = vec![0_u8; BLOCK];
        let mut number = 0;
        loop {
            let filled = fill(reader, &mut block)?;
            if filled == 0 && number == 0 {
                return Ok(None);
            }
            if filled < BLOCK {
                return Err(malformed("the header ends without an END card"));
            }
            for raw in block.chunks(CARD) {
                number += 1;
                // FITS 4.0 §4.1.1: a card is ASCII text, so each byte below is one character.
                if !raw.is_ascii() {
                    return Err(malformed(format!(
                        "header card {number} ({}) holds a byte that is not ASCII",
                        String::from_utf8_lossy(&raw[..8]).trim_end()
                    )));
                }
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
    /// Fails as [`Self::read_from`] does, or when the file ends before a header.
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

    /// The whole number `keyword` holds, if the header has it.
    ///
    /// # Errors
    ///
    /// Fails when the value is not a whole number.
    pub fn integer(&self, keyword: &str) -> Result<Option<i64>, FitsError> {
        self.get(keyword)
            .map(|v| {
                v.parse()
                    .map_err(|_| malformed(format!("{keyword} = {v} is not a whole number")))
            })
            .transpose()
    }

    /// The whole number `keyword` holds, or `default` when the header lacks it.
    ///
    /// # Errors
    ///
    /// Fails when the value is not a whole number.
    pub fn whole(&self, keyword: &str, default: i64) -> Result<i64, FitsError> {
        Ok(self.integer(keyword)?.unwrap_or(default))
    }

    /// The count `keyword` holds, or `default` when the header lacks it; with no default, a
    /// missing count is an error.
    ///
    /// # Errors
    ///
    /// Fails when the count is missing with no default, negative or not a whole number.
    pub fn count(&self, keyword: &str, default: Option<usize>) -> Result<usize, FitsError> {
        match (self.integer(keyword)?, default) {
            (Some(v), _) => {
                usize::try_from(v).map_err(|_| malformed(format!("{keyword} = {v} is not a count")))
            }
            (None, Some(d)) => Ok(d),
            (None, None) => Err(malformed(format!("{keyword} is missing"))),
        }
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

    /// The sample type `BITPIX` names.
    ///
    /// # Errors
    ///
    /// Fails when `BITPIX` is missing or not one of 8, 16, 32, 64, −32 and −64.
    pub fn sample_kind(&self) -> Result<SampleKind, FitsError> {
        let bitpix = self
            .integer("BITPIX")?
            .ok_or_else(|| malformed("BITPIX is missing"))?;
        SampleKind::from_bitpix(bitpix)
            .ok_or_else(|| malformed(format!("BITPIX = {bitpix} is not a FITS sample type")))
    }

    /// `NAXIS1`, `NAXIS2`, …: the first runs fastest; empty when `NAXIS` is 0.
    ///
    /// # Errors
    ///
    /// Fails when `NAXIS` or one of its axes is missing or not a count, or `NAXIS` is over 999.
    pub fn axes(&self) -> Result<Vec<usize>, FitsError> {
        let n = self.count("NAXIS", None)?;
        if n > MAX_AXES {
            return Err(malformed(format!(
                "NAXIS = {n} is more than the {MAX_AXES} axes FITS allows"
            )));
        }
        (1..=n)
            .map(|i| self.count(&format!("NAXIS{i}"), None))
            .collect()
    }

    /// The bytes of data the header announces, before padding:
    /// `|BITPIX| / 8 × GCOUNT × (PCOUNT + NAXIS1 × … × NAXISn)`, none when `NAXIS` is 0.
    ///
    /// # Errors
    ///
    /// Fails on a missing or malformed axis, count or `BITPIX`, on `GCOUNT` = 0, or on a size
    /// that does not fit in memory.
    pub fn data_bytes(&self) -> Result<usize, FitsError> {
        let kind = self.sample_kind()?;
        let axes = self.axes()?;
        if axes.is_empty() {
            return Ok(0);
        }
        let groups = self.count("GCOUNT", Some(1))?;
        if groups == 0 {
            return Err(malformed("GCOUNT = 0: the unit holds no groups"));
        }
        let parameters = self.count("PCOUNT", Some(0))?;
        axes.iter()
            .try_fold(1_usize, |n, &a| n.checked_mul(a))
            .and_then(|elements| elements.checked_add(parameters))
            .and_then(|n| n.checked_mul(groups))
            .and_then(|n| n.checked_mul(kind.width()))
            .ok_or_else(|| malformed("the data the header declares are too large to hold"))
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
    let end = start
        .checked_add(header.data_bytes()?)
        .filter(|&end| end <= fits.len())
        .ok_or_else(|| malformed("the file ends inside the data of a header"))?;
    // `end` is within the file, so the block after it is too, or just past its end.
    *at = end.div_ceil(BLOCK).saturating_mul(BLOCK);
    Ok(Hdu {
        header,
        data: start..end,
    })
}

/// A card's value and comment, when columns 9 and 10 hold the value indicator `= `. `text` is
/// the 80 ASCII characters of the card.
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
pub fn fill(reader: &mut (impl Read + ?Sized), buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
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
/// file unpacked, or the bytes as they are. Nothing is unpacked past [`MAX_UNPACKED`] bytes.
///
/// # Errors
///
/// Fails when the archive cannot be read, unpacks past the bound or holds no `.fits` member.
pub fn unpack(name: &str, bytes: Vec<u8>) -> Result<Vec<u8>, FitsError> {
    unpack_within(name, bytes, MAX_UNPACKED)
}

/// [`unpack`], unpacking no more than `limit` bytes.
fn unpack_within(name: &str, bytes: Vec<u8>, limit: usize) -> Result<Vec<u8>, FitsError> {
    let bounded = |reader: &mut dyn Read, what: &str| {
        let mut out = Vec::new();
        let cap = u64::try_from(limit).map_or(u64::MAX, |l| l.saturating_add(1));
        reader
            .take(cap)
            .read_to_end(&mut out)
            .map_err(|e| malformed(format!("{what} cannot be read: {e}")))?;
        if out.len() > limit {
            return Err(malformed(format!(
                "{what} unpacks to more than {limit} bytes"
            )));
        }
        Ok(out)
    };
    let gunzip = |bytes: &[u8]| bounded(&mut flate2::read::GzDecoder::new(bytes), "the gzip");
    if name.ends_with(".fits.tgz") {
        let tar = gunzip(&bytes)?;
        let mut archive = tar::Archive::new(tar.as_slice());
        for entry in archive.entries()? {
            let mut entry = entry?;
            if entry.path()?.to_string_lossy().ends_with(".fits") {
                return bounded(&mut entry, "the tar member");
            }
        }
        Err(malformed("the archive holds no .fits file"))
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
mod tests;
