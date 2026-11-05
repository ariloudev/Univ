//! A gzipped CSV file, such as a catalogue of twenty million rows, read in pieces: each piece is
//! the file's header and as many whole lines as fit in [`PIECE`] bytes, so that the file is never
//! held unpacked.

use std::io::{BufRead, BufReader};
use std::path::Path;

use flate2::read::MultiGzDecoder;

use super::{NormalizeError, Normalizer};
use crate::table::TableReader;

/// The bytes of rows a piece holds, past which a piece ends at the next line.
const PIECE: usize = 64 << 20;

/// Whether a file of a files source is a gzipped CSV table.
pub(super) fn is_csv_gz(path: &str) -> bool {
    path.ends_with(".csv.gz")
}

impl Normalizer<'_> {
    /// Reads every row of the gzipped CSV file `file`.
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be read or unpacked, or as [`Normalizer::table`] does.
    pub fn csv_file(&mut self, file: &Path) -> Result<(), NormalizeError> {
        self.csv_file_in_pieces(file, PIECE)
    }

    /// Reads the file a piece at a time. Its header is its first line that is neither blank nor
    /// a `#` comment, as a [`TableReader`] takes it; blank lines and comments elsewhere are layout
    /// and are not read, so that where a piece ends changes nothing. A file of a header and no
    /// rows is still read as a table, so a column the definition takes and the header lacks is
    /// an error, not an empty source.
    fn csv_file_in_pieces(&mut self, file: &Path, piece: usize) -> Result<(), NormalizeError> {
        let unreadable = |e: std::io::Error| NormalizeError::read(&self.definition.id, file, &e);
        let opened = std::fs::File::open(file).map_err(unreadable)?;
        let mut lines = BufReader::new(MultiGzDecoder::new(opened));
        let mut line = Vec::new();
        // The next line that is neither blank nor a comment, into `line`; false at the end.
        let mut next = |line: &mut Vec<u8>| -> Result<bool, NormalizeError> {
            loop {
                line.clear();
                if lines.read_until(b'\n', line).map_err(unreadable)? == 0 {
                    return Ok(false);
                }
                if !(line.starts_with(b"#") || line.iter().all(u8::is_ascii_whitespace)) {
                    return Ok(true);
                }
            }
        };
        next(&mut line)?;
        let mut body = line.clone();
        let header = body.len();
        let mut first = true;
        loop {
            body.truncate(header);
            while body.len() - header < piece && next(&mut line)? {
                body.extend_from_slice(&line);
            }
            if body.len() == header && !first {
                return Ok(());
            }
            first = false;
            let table = TableReader::csv(&body)
                .map_err(|e| NormalizeError::table(&self.definition.id, file, e))?;
            self.rows(file, table)?;
            if body.len() == header {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use univ_model::redshift::{CmbCorrection, DipoleModel};

    use super::*;
    use crate::source::SourceDefinition;

    const DEFINITION: &str = r#"
id: test-csv
title: "A gzipped CSV table"
provider: test
access:
  method: files
  files:
    - { path: "t.csv.gz" }
cuts: ["all rows"]
columns:
  - { column: id, field: identifier, unit: dimensionless }
  - { column: ra, field: right_ascension, unit: degree }
  - { column: dec, field: declination, unit: degree }
  - { column: z, field: redshift, unit: dimensionless }
frame: icrs
epoch: 2000.0
citation: "a test"
reference: { bibcode: "2016ApJS..225....5B", doi: "10.3847/0067-0049/225/1/5" }
terms:
  url: "https://example.org/terms"
  quote: "a test"
  credit: "a test"
redistribution:
  status: permitted_with_credit
  review: provisional
  reasoning: "a test"
scale_domain: cosmic_web
mirrors: ["https://example.org"]
normalization:
  redshifts:
    - { column: z, frame: heliocentric, kind: photometric, basis: "a test" }
"#;

    /// `csv` gzipped into a file of a fresh directory, kept alive by the directory returned.
    fn gzipped(csv: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(csv.as_bytes()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.csv.gz");
        std::fs::write(&path, gz.finish().unwrap()).unwrap();
        (dir, path)
    }

    fn read(csv: &str, piece: usize) -> Result<super::super::Normalized, NormalizeError> {
        let definition: SourceDefinition = serde_norway::from_str(DEFINITION).unwrap();
        let (_dir, path) = gzipped(csv);
        let correction = CmbCorrection::new(DipoleModel::Planck2018);
        let mut n = Normalizer::new(&definition, &correction).unwrap();
        n.csv_file_in_pieces(&path, piece)?;
        Ok(n.finish())
    }

    #[test]
    fn a_gzipped_csv_file_is_read_whole_in_pieces_of_lines() {
        let csv = "id,ra,dec,z\n1,10.0,1.0,0.10\n2,11.0,2.0,0.20\n3,12.0,3.0,0.30\n";
        for piece in [1, 20, 1 << 20] {
            let done = read(csv, piece).unwrap();
            assert_eq!(done.report.published, 3, "pieces of {piece} bytes");
        }
    }

    #[test]
    fn comments_and_blank_lines_are_layout_wherever_a_piece_ends() {
        let csv = "# a comment\n\nid,ra,dec,z\n1,10.0,1.0,0.10\n\n2,11.0,2.0,0.20\n  \n\
                   3,12.0,3.0,0.30\n\n";
        for piece in [1, 7, 20, 1 << 20] {
            let done = read(csv, piece).unwrap();
            assert_eq!(
                (done.report.published, done.report.left_out_total()),
                (3, 0),
                "pieces of {piece} bytes"
            );
        }
    }

    #[test]
    fn a_header_without_a_defined_column_is_an_error_even_with_no_rows() {
        assert!(read("id,ra,dec\n", 1 << 20).is_err());
        assert_eq!(read("id,ra,dec,z\n", 1 << 20).unwrap().report.published, 0);
        assert!(read("", 1 << 20).is_err(), "an empty file holds no table");
    }
}
