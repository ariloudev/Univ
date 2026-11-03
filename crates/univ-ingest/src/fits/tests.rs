use std::io::Write;

use super::*;

/// The header of these cards, `END` added.
fn header(cards: &[&str]) -> Header {
    Header::read(&header_block(cards), &mut 0).unwrap()
}

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

#[test]
fn a_card_that_is_not_ascii_is_refused_by_its_number() {
    let mut fits = header_block(&["SIMPLE  =                    T", "OBJECTS = 'a grid'"]);
    // The eighth byte of the second card's keyword: a character of two bytes as text.
    fits[CARD + 7] = 0xE9;
    let err = Header::read(&fits, &mut 0).unwrap_err();
    assert!(err.to_string().contains("card 2"), "{err}");
}

#[test]
fn sizes_that_overflow_or_name_more_than_999_axes_are_refused() {
    let overflow = header(&[
        "SIMPLE  =                    T",
        "BITPIX  =                  -64",
        "NAXIS   =                    2",
        "NAXIS1  =  4611686018427387904",
        "NAXIS2  =                    4",
    ]);
    assert!(overflow.data_bytes().is_err());
    let mut cards = vec![
        "SIMPLE  =                    T".to_owned(),
        "BITPIX  =                    8".to_owned(),
        "NAXIS   =                 1000".to_owned(),
    ];
    cards.extend((1..=1000).map(|i| format!("{:<8}=                    1", format!("NAXIS{i}"))));
    let cards: Vec<&str> = cards.iter().map(String::as_str).collect();
    let err = header(&cards).data_bytes().unwrap_err();
    assert!(err.to_string().contains("999"), "{err}");
    let endless = header(&[
        "SIMPLE  =                    T",
        "BITPIX  =                    8",
        "NAXIS   =  9223372036854775807",
    ]);
    assert!(endless.data_bytes().is_err());
}

#[test]
fn a_missing_axis_or_no_groups_is_refused() {
    let missing = header(&[
        "SIMPLE  =                    T",
        "BITPIX  =                    8",
        "NAXIS   =                    2",
        "NAXIS1  =                    4",
    ]);
    let err = missing.data_bytes().unwrap_err();
    assert!(err.to_string().contains("NAXIS2"), "{err}");
    let no_groups = header(&[
        "XTENSION= 'BINTABLE'",
        "BITPIX  =                    8",
        "NAXIS   =                    2",
        "NAXIS1  =                    4",
        "NAXIS2  =                    3",
        "PCOUNT  =                    0",
        "GCOUNT  =                    0",
    ]);
    assert!(no_groups.data_bytes().is_err());
    let mut fits = header_block(&[
        "SIMPLE  =                    T",
        "BITPIX  =                    8",
        "NAXIS   =                    1",
    ]);
    fits.extend([0; 4]);
    assert!(next_hdu(&fits, &mut 0).is_err(), "NAXIS1 missing");
}

#[test]
fn an_archive_that_unpacks_past_its_bound_is_refused() {
    let fits = vec![b' '; 10 * BLOCK];
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(&fits).unwrap();
    let gz = gz.finish().unwrap();
    assert_eq!(
        unpack_within("a.fits.gz", gz.clone(), 10 * BLOCK).unwrap(),
        fits
    );
    let err = unpack_within("a.fits.gz", gz, BLOCK).unwrap_err();
    assert!(err.to_string().contains("more than"), "{err}");
    let mut tar = tar::Builder::new(Vec::new());
    let mut entry = tar::Header::new_gnu();
    entry.set_size(u64::try_from(fits.len()).unwrap());
    entry.set_cksum();
    tar.append_data(&mut entry, "a.fits", fits.as_slice())
        .unwrap();
    let mut tgz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    tgz.write_all(&tar.into_inner().unwrap()).unwrap();
    let tgz = tgz.finish().unwrap();
    assert_eq!(
        unpack_within("a.fits.tgz", tgz.clone(), 12 * BLOCK).unwrap(),
        fits
    );
    assert!(unpack_within("a.fits.tgz", tgz, 9 * BLOCK).is_err());
}
