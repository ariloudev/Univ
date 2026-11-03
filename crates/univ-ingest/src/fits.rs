//! FITS files at the level of their headers: 2880-byte blocks of 80-byte cards ending at `END`,
//! the data each header announces, and `BSCALE`/`BZERO` (FITS Standard 4.0, Pence et al. 2010,
//! A&A 524, A42, §§3–4). Readers of tables and images build on it; it reads no data itself.

use std::io::Read as _;
use std::ops::Range;

/// The bytes of a FITS block, header or data.
pub const BLOCK: usize = 2_880;
/// The bytes of a header card.
pub const CARD: usize = 80;

/// A header: its cards that hold a value, as keyword and value text, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Header {
    cards: Vec<(String, String)>,
}

/// A header-data unit: its header and where its data lies in the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hdu {
    pub header: Header,
    pub data: Range<usize>,
}

impl Header {
    /// Reads the header that starts at `*at`, leaving `*at` at the block after its `END` card.
    ///
    /// # Errors
    ///
    /// Fails when the file ends before `END` or a card is not ASCII.
    pub fn read(fits: &[u8], at: &mut usize) -> Result<Self, String> {
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
                return Ok(Self { cards });
            }
            if &text[8..10] == "= " {
                cards.push((keyword.to_owned(), value(&text[10..])));
            }
        }
    }

    /// The value of `keyword`, as text without its quotes or comment.
    #[must_use]
    pub fn get(&self, keyword: &str) -> Option<&str> {
        self.cards
            .iter()
            .find(|(k, _)| k == keyword)
            .map(|(_, v)| v.as_str())
    }

    /// The whole number `keyword` holds, or `default` when the header lacks it.
    ///
    /// # Errors
    ///
    /// Fails when the value is not a whole number.
    pub fn whole(&self, keyword: &str, default: i64) -> Result<i64, String> {
        self.get(keyword).map_or(Ok(default), |v| {
            v.parse()
                .map_err(|e| format!("{keyword} {v:?} is not a whole number: {e}"))
        })
    }

    /// The real number `keyword` holds, or `default` when the header lacks it. FITS writes an
    /// exponent with `D` as well as `E`.
    ///
    /// # Errors
    ///
    /// Fails when the value is not a number.
    pub fn real(&self, keyword: &str, default: f64) -> Result<f64, String> {
        self.get(keyword).map_or(Ok(default), |v| {
            v.replace(['D', 'd'], "E")
                .parse()
                .map_err(|e| format!("{keyword} {v:?} is not a number: {e}"))
        })
    }

    /// `BSCALE` and `BZERO`, 1 and 0 by default: a stored array value `s` stands for
    /// `BZERO + BSCALE × s`.
    ///
    /// # Errors
    ///
    /// Fails when either is not a number.
    pub fn scaling(&self) -> Result<(f64, f64), String> {
        Ok((self.real("BSCALE", 1.0)?, self.real("BZERO", 0.0)?))
    }

    /// The bytes of data the header announces, before padding:
    /// `|BITPIX| / 8 × GCOUNT × (PCOUNT + NAXIS1 × … × NAXISn)`, none when `NAXIS` is 0.
    ///
    /// # Errors
    ///
    /// Fails on a negative or malformed axis, count or BITPIX.
    pub fn data_bytes(&self) -> Result<usize, String> {
        let count = |keyword: &str, default: i64| {
            usize::try_from(self.whole(keyword, default)?)
                .map_err(|_| format!("{keyword} is negative"))
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
        let bytes = usize::try_from(bits / 8).map_err(|_| "BITPIX is too large")?;
        Ok(bytes * count("GCOUNT", 1)? * (count("PCOUNT", 0)? + elements))
    }
}

/// Reads the header-data unit at `*at`, leaving `*at` at the next one.
///
/// # Errors
///
/// Fails on a header that cannot be read or data that runs past the end of the file.
pub fn next_hdu(fits: &[u8], at: &mut usize) -> Result<Hdu, String> {
    let header = Header::read(fits, at)?;
    let start = *at;
    let end = start + header.data_bytes()?;
    if end > fits.len() {
        return Err("the file ends inside the data of a header".to_owned());
    }
    *at = end.div_ceil(BLOCK) * BLOCK;
    Ok(Hdu {
        header,
        data: start..end,
    })
}

/// A card's value: a quoted string with `''` for a quote and its trailing spaces dropped, or
/// what precedes a `/` comment.
fn value(text: &str) -> String {
    let text = text.trim_start();
    if let Some(quoted) = text.strip_prefix('\'') {
        let mut out = String::new();
        let mut chars = quoted.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '\'' {
                out.push(c);
            } else if chars.peek() == Some(&'\'') {
                out.push('\'');
                chars.next();
            } else {
                break;
            }
        }
        return out.trim_end().to_owned();
    }
    text.split('/').next().unwrap_or("").trim().to_owned()
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

/// Test helpers: a header written card by card and padded to its block.
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
        assert_eq!(header.get("OBJECT"), Some("it's a grid"));
        assert_eq!(header.get("COMMENT"), None);
        assert_eq!(header.scaling(), Ok((2.0, 0.0)));
        assert_eq!(header.data_bytes(), Ok(4 * 3 * 2));
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
