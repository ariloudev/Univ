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

impl Normalizer<'_> {
    /// Reads every row of the gzipped CSV file `file`.
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be read or unpacked, or as [`Normalizer::table`] does.
    pub fn csv_file(&mut self, file: &Path) -> Result<(), NormalizeError> {
        self.csv_file_in_pieces(file, PIECE)
    }

    fn csv_file_in_pieces(&mut self, file: &Path, piece: usize) -> Result<(), NormalizeError> {
        let unreadable = |e: std::io::Error| NormalizeError::Read {
            source_id: self.definition.id.clone(),
            file: file.to_path_buf(),
            error: e.to_string(),
        };
        let opened = std::fs::File::open(file).map_err(unreadable)?;
        let mut lines = BufReader::new(MultiGzDecoder::new(opened));
        let mut header = Vec::new();
        lines.read_until(b'\n', &mut header).map_err(unreadable)?;
        let mut body = header.clone();
        loop {
            let read = lines.read_until(b'\n', &mut body).map_err(unreadable)?;
            if read == 0 || body.len() - header.len() >= piece {
                if body.len() > header.len() {
                    self.rows(file, TableReader::csv(&body))?;
                }
                if read == 0 {
                    return Ok(());
                }
                body.truncate(header.len());
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

    #[test]
    fn a_gzipped_csv_file_is_read_whole_in_pieces_of_lines() {
        let definition: SourceDefinition = serde_norway::from_str(DEFINITION).unwrap();
        let csv = "id,ra,dec,z\n1,10.0,1.0,0.10\n2,11.0,2.0,0.20\n3,12.0,3.0,0.30\n";
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(csv.as_bytes()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.csv.gz");
        std::fs::write(&path, gz.finish().unwrap()).unwrap();
        let correction = CmbCorrection::new(DipoleModel::Planck2018);
        for piece in [1, 20, 1 << 20] {
            let mut n = Normalizer::new(&definition, &correction).unwrap();
            n.csv_file_in_pieces(&path, piece).unwrap();
            let done = n.finish();
            assert_eq!(done.report.published, 3, "pieces of {piece} bytes");
        }
    }
}
