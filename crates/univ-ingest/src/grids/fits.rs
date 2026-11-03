//! FITS image HDUs (Pence et al. 2010, A&A 524, A42, FITS 4.0): the primary HDU and IMAGE
//! extensions, their header cards, and their data scaled by BSCALE and BZERO.

use std::io::Read;

use super::{GridReadError, read_numbers};

/// FITS files are written in blocks of this many bytes; a header is 36 cards of 80.
const BLOCK: usize = 2880;
const CARD: usize = 80;

/// One header card: its keyword, its value as written (a string without its quotes), and its
/// comment.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub keyword: String,
    pub value: Option<String>,
    pub comment: Option<String>,
}

/// One image HDU: its cards and its physical values, `BZERO + BSCALE × stored`, in the file's
/// order, NAXIS1 fastest. An integer image's BLANK value reads as NaN.
#[derive(Clone, Debug, PartialEq)]
pub struct FitsImage {
    pub cards: Vec<Card>,
    /// The stored sample type: 8, 16, 32, 64 for integers, -32, -64 for IEEE floats.
    pub bitpix: i32,
    /// NAXIS1, NAXIS2, …: the first runs fastest.
    pub axes: Vec<usize>,
    pub bscale: f64,
    pub bzero: f64,
    pub values: Vec<f64>,
}

impl FitsImage {
    /// Reads the primary HDU from the start of a FITS file.
    ///
    /// # Errors
    ///
    /// Fails on a reader error, a header that is not a FITS primary header, an unsupported BITPIX,
    /// or data shorter than its header declares.
    pub fn read_primary(mut reader: impl Read) -> Result<Self, GridReadError> {
        let cards = read_header(&mut reader)?.ok_or_else(|| fits("the file is empty"))?;
        Self::read_data(&mut reader, cards, true)
    }

    /// Reads the primary HDU and every IMAGE extension after it, to the end of the file.
    ///
    /// # Errors
    ///
    /// Fails as [`Self::read_primary`] does, or on an extension that is not an IMAGE.
    pub fn read_all(mut reader: impl Read) -> Result<Vec<Self>, GridReadError> {
        let mut hdus = vec![Self::read_primary(&mut reader)?];
        while let Some(cards) = read_header(&mut reader)? {
            hdus.push(Self::read_data(&mut reader, cards, false)?);
        }
        Ok(hdus)
    }

    /// The card with this keyword.
    #[must_use]
    pub fn card(&self, keyword: &str) -> Option<&Card> {
        self.cards.iter().find(|c| c.keyword == keyword)
    }

    fn read_data(
        reader: &mut impl Read,
        cards: Vec<Card>,
        primary: bool,
    ) -> Result<Self, GridReadError> {
        let first = cards
            .first()
            .map(|c| (c.keyword.as_str(), c.value.as_deref()));
        match (primary, first) {
            (true, Some(("SIMPLE", Some("T")))) | (false, Some(("XTENSION", Some("IMAGE")))) => {}
            (true, _) => return Err(fits("the first card is not SIMPLE = T")),
            (false, _) => return Err(fits("an extension is not an IMAGE")),
        }
        let number = |keyword: &str| -> Result<Option<f64>, GridReadError> {
            let Some(text) = cards
                .iter()
                .find(|c| c.keyword == keyword)
                .and_then(|c| c.value.as_deref())
            else {
                return Ok(None);
            };
            text.replace(['D', 'd'], "E")
                .parse()
                .map(Some)
                .map_err(|_| fits(&format!("{keyword} = {text} is not a number")))
        };
        let size = |keyword: &str| -> Result<usize, GridReadError> {
            let x = number(keyword)?.ok_or_else(|| fits(&format!("no {keyword}")))?;
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "checked to be a whole number in [0, 2^32)"
            )]
            let n = x as usize;
            if x.fract() == 0.0 && (0.0..4_294_967_296.0).contains(&x) {
                Ok(n)
            } else {
                Err(fits(&format!("{keyword} = {x} is not a size")))
            }
        };
        let bitpix = match size_signed(number("BITPIX")?)? {
            b @ (8 | 16 | 32 | 64 | -32 | -64) => b,
            b => return Err(fits(&format!("BITPIX = {b} is not a FITS sample type"))),
        };
        let axes = (1..=size("NAXIS")?)
            .map(|n| size(&format!("NAXIS{n}")))
            .collect::<Result<Vec<_>, _>>()?;
        let count = if axes.is_empty() {
            0
        } else {
            axes.iter().product()
        };
        let bscale = number("BSCALE")?.unwrap_or(1.0);
        let bzero = number("BZERO")?.unwrap_or(0.0);
        let blank = number("BLANK")?.filter(|_| bitpix > 0);
        let width = usize::try_from(bitpix.unsigned_abs() / 8).unwrap_or(8);
        let values = read_numbers(reader, count, width, |b| {
            let stored = decode(bitpix, b);
            Ok(if blank == Some(stored) {
                f64::NAN
            } else {
                bzero + bscale * stored
            })
        })?;
        skip_padding(reader, count * width)?;
        Ok(Self {
            cards,
            bitpix,
            axes,
            bscale,
            bzero,
            values,
        })
    }
}

fn fits(reason: &str) -> GridReadError {
    GridReadError::Fits(reason.to_string())
}

fn size_signed(bitpix: Option<f64>) -> Result<i32, GridReadError> {
    let x = bitpix.ok_or_else(|| fits("no BITPIX"))?;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "matched against the six sample types after"
    )]
    Ok(x as i32)
}

/// A stored big-endian sample of this BITPIX as a number; `bytes` has the sample's width.
fn decode(bitpix: i32, bytes: &[u8]) -> f64 {
    let mut b = [0_u8; 8];
    b[..bytes.len()].copy_from_slice(bytes);
    match bitpix {
        8 => f64::from(b[0]),
        16 => f64::from(i16::from_be_bytes([b[0], b[1]])),
        32 => f64::from(i32::from_be_bytes([b[0], b[1], b[2], b[3]])),
        -32 => f64::from(f32::from_be_bytes([b[0], b[1], b[2], b[3]])),
        #[expect(
            clippy::cast_precision_loss,
            reason = "a 64-bit integer image is read as the nearest float"
        )]
        64 => i64::from_be_bytes(b) as f64,
        _ => f64::from_be_bytes(b),
    }
}

/// Reads header blocks up to and including the one holding END; `None` at a clean end of file.
fn read_header(reader: &mut impl Read) -> Result<Option<Vec<Card>>, GridReadError> {
    let mut cards = Vec::new();
    let mut block = vec![0_u8; BLOCK];
    loop {
        let filled = fill(reader, &mut block)?;
        if filled == 0 && cards.is_empty() {
            return Ok(None);
        }
        if filled < BLOCK {
            return Err(fits("the header ends without an END card"));
        }
        for raw in block.chunks(CARD) {
            let text: String = raw.iter().map(|&b| char::from(b)).collect();
            let keyword = text[..8].trim_end().to_string();
            if keyword == "END" {
                return Ok(Some(cards));
            }
            if !keyword.is_empty() {
                cards.push(parse_card(keyword, &text));
            }
        }
    }
}

/// A card's value and comment, when columns 9 and 10 hold the value indicator `= `.
fn parse_card(keyword: String, text: &str) -> Card {
    if &text[8..10] != "= " {
        let comment = text[8..].trim();
        let comment = (!comment.is_empty()).then(|| comment.to_string());
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
        (value.trim_end().to_string(), &quoted[end..])
    } else {
        let end = field.find('/').unwrap_or(field.len());
        (field[..end].trim().to_string(), &field[end..])
    };
    let comment = rest
        .trim_start()
        .strip_prefix('/')
        .map(|c| c.trim().to_string());
    Card {
        keyword,
        value: Some(value),
        comment,
    }
}

/// Fills `buf` from the reader as far as it goes; the count is short only at the end of file.
fn fill(reader: &mut impl Read, buf: &mut [u8]) -> Result<usize, GridReadError> {
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
fn skip_padding(reader: &mut impl Read, bytes: usize) -> Result<(), GridReadError> {
    let pad = (BLOCK - bytes % BLOCK) % BLOCK;
    fill(reader, &mut vec![0; pad])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A header block from cards, padded with blanks, END included.
    fn header(cards: &[&str]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for card in cards.iter().chain(&["END"]) {
            let mut card = card.as_bytes().to_vec();
            card.resize(CARD, b' ');
            bytes.extend(card);
        }
        bytes.resize(bytes.len().div_ceil(BLOCK) * BLOCK, b' ');
        bytes
    }

    fn padded(mut data: Vec<u8>) -> Vec<u8> {
        data.resize(data.len().div_ceil(BLOCK) * BLOCK, 0);
        data
    }

    #[test]
    fn a_scaled_integer_image_reads_its_physical_values_and_cards() {
        let mut file = header(&[
            "SIMPLE  =                    T / conforms to FITS standard",
            "BITPIX  =                   16",
            "NAXIS   =                    2",
            "NAXIS1  =                    3",
            "NAXIS2  =                    2",
            "BSCALE  =                  0.5",
            "BZERO   =               1.0D+1",
            "BLANK   =                 -999",
            "OBJECT  = 'it''s a test'       / quoted",
            "COMMENT a note",
        ]);
        let stored: [i16; 6] = [0, 2, -4, -999, 20, 1];
        file.extend(padded(
            stored.iter().flat_map(|s| s.to_be_bytes()).collect(),
        ));
        let image = FitsImage::read_primary(file.as_slice()).unwrap();
        assert_eq!(image.axes, [3, 2]);
        assert_eq!(image.bitpix, 16);
        assert_eq!(&image.values[..3], [10.0, 11.0, 8.0]);
        assert!(image.values[3].is_nan());
        assert_eq!(&image.values[4..], [20.0, 10.5]);
        let object = image.card("OBJECT").unwrap();
        assert_eq!(object.value.as_deref(), Some("it's a test"));
        assert_eq!(object.comment.as_deref(), Some("quoted"));
        assert_eq!(image.card("COMMENT").unwrap().value, None);
    }

    #[test]
    fn a_float_primary_and_an_image_extension_both_read() {
        let mut file = header(&[
            "SIMPLE  =                    T",
            "BITPIX  =                  -64",
            "NAXIS   =                    1",
            "NAXIS1  =                    2",
            "EXTEND  =                    T",
        ]);
        file.extend(padded(
            [1.5_f64, -2.0]
                .iter()
                .flat_map(|x| x.to_be_bytes())
                .collect(),
        ));
        file.extend(header(&[
            "XTENSION= 'IMAGE   '",
            "BITPIX  =                  -32",
            "NAXIS   =                    1",
            "NAXIS1  =                    1",
            "PCOUNT  =                    0",
            "GCOUNT  =                    1",
        ]));
        file.extend(padded(3.25_f32.to_be_bytes().to_vec()));
        let hdus = FitsImage::read_all(file.as_slice()).unwrap();
        assert_eq!(hdus.len(), 2);
        assert_eq!(hdus[0].values, [1.5, -2.0]);
        assert_eq!(hdus[1].values, [3.25]);
    }

    #[test]
    fn files_that_are_not_fits_images_are_refused() {
        let not_simple = header(&[
            "SIMPLE  =                    F",
            "BITPIX  =  8",
            "NAXIS = 0",
        ]);
        assert!(FitsImage::read_primary(not_simple.as_slice()).is_err());
        let bad_bitpix = header(&[
            "SIMPLE  =                    T",
            "BITPIX  =  12",
            "NAXIS = 0",
        ]);
        assert!(FitsImage::read_primary(bad_bitpix.as_slice()).is_err());
        let short = header(&[
            "SIMPLE  =                    T",
            "BITPIX  =                  -64",
            "NAXIS   =                    1",
            "NAXIS1  =                    4",
        ]);
        assert!(FitsImage::read_primary(short.as_slice()).is_err());
        assert!(FitsImage::read_primary(&b""[..]).is_err());
    }
}
