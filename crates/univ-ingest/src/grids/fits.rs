//! FITS image HDUs (Pence et al. 2010, A&A 524, A42, FITS 4.0): the primary HDU and IMAGE
//! extensions and their data scaled by BSCALE and BZERO. Headers and samples are read by
//! [`crate::fits`].

use std::io::Read;

use super::{GridReadError, read_numbers};
pub use crate::fits::Card;
use crate::fits::{Header, Sample, SampleKind, malformed, sample, skip_padding};

/// One image HDU: its header and its physical values, `BZERO + BSCALE × stored`, in the file's
/// order, NAXIS1 fastest. An integer image's BLANK value reads as NaN.
#[derive(Clone, Debug, PartialEq)]
pub struct FitsImage {
    pub header: Header,
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
    /// sizes that are missing or overflow, or data shorter than its header declares.
    pub fn read_primary(mut reader: impl Read) -> Result<Self, GridReadError> {
        let header = Header::read_from(&mut reader)?.ok_or_else(|| fits("the file is empty"))?;
        Self::read_data(&mut reader, header, true)
    }

    /// Reads the primary HDU and every IMAGE extension after it, to the end of the file.
    ///
    /// # Errors
    ///
    /// Fails as [`Self::read_primary`] does, or on an extension that is not an IMAGE.
    pub fn read_all(mut reader: impl Read) -> Result<Vec<Self>, GridReadError> {
        let mut hdus = vec![Self::read_primary(&mut reader)?];
        while let Some(header) = Header::read_from(&mut reader)? {
            hdus.push(Self::read_data(&mut reader, header, false)?);
        }
        Ok(hdus)
    }

    /// The card with this keyword.
    #[must_use]
    pub fn card(&self, keyword: &str) -> Option<&Card> {
        self.header.card(keyword)
    }

    fn read_data(
        reader: &mut impl Read,
        header: Header,
        primary: bool,
    ) -> Result<Self, GridReadError> {
        let first = header
            .cards
            .first()
            .map(|c| (c.keyword.as_str(), c.value.as_deref()));
        match (primary, first) {
            (true, Some(("SIMPLE", Some("T")))) | (false, Some(("XTENSION", Some("IMAGE")))) => {}
            (true, _) => return Err(fits("the first card is not SIMPLE = T")),
            (false, _) => return Err(fits("an extension is not an IMAGE")),
        }
        let kind = header.sample_kind()?;
        let axes = header.axes()?;
        // The bytes the header declares, checked; an image has no groups or parameters, so they
        // are its samples.
        let bytes = header.data_bytes()?;
        let count = if axes.is_empty() {
            0
        } else {
            axes.iter().product()
        };
        if count.checked_mul(kind.width()) != Some(bytes) {
            return Err(fits("an image with PCOUNT or GCOUNT other than 0 and 1"));
        }
        let (bscale, bzero) = header.scaling()?;
        let blank = match kind {
            SampleKind::F32 | SampleKind::F64 => None,
            _ => header.integer("BLANK")?,
        };
        let values = read_numbers(reader, count, kind.width(), |b| {
            Ok(
                match sample(kind, b).ok_or_else(|| fits("a short sample"))? {
                    Sample::Integer(i) if blank == Some(i) => f64::NAN,
                    stored => bzero + bscale * stored.to_f64(),
                },
            )
        })?;
        skip_padding(reader, bytes)?;
        Ok(Self {
            header,
            bitpix: kind.bitpix(),
            axes,
            bscale,
            bzero,
            values,
        })
    }
}

fn fits(reason: &str) -> GridReadError {
    malformed(reason).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fits::{BLOCK, header_block as header};

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

    #[test]
    fn an_image_whose_size_overflows_or_names_more_than_999_axes_is_refused() {
        let overflow = header(&[
            "SIMPLE  =                    T",
            "BITPIX  =                  -64",
            "NAXIS   =                    3",
            "NAXIS1  =           4294967295",
            "NAXIS2  =           4294967295",
            "NAXIS3  =           4294967295",
        ]);
        assert!(FitsImage::read_primary(overflow.as_slice()).is_err());
        let mut cards = vec![
            "SIMPLE  =                    T".to_owned(),
            "BITPIX  =                    8".to_owned(),
            "NAXIS   =                 1000".to_owned(),
        ];
        cards.extend(
            (1..=1000).map(|i| format!("{:<8}=                    1", format!("NAXIS{i}"))),
        );
        let cards: Vec<&str> = cards.iter().map(String::as_str).collect();
        let mut file = header(&cards);
        file.extend(padded(vec![1]));
        assert!(FitsImage::read_primary(file.as_slice()).is_err());
    }
}
