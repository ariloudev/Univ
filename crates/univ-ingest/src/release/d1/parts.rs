//! A table's statements gathered into files of at most [`MAX_FILE_BYTES`], each put once full:
//! its rows' parts, `{nnnn}-{table}-{part}.sql`, and the parts that retire the rows of the
//! release before it no longer holds, `{nnnn}-{table}-r{part}.sql`, loaded with the others, as
//! publish groups a table's files by name.

use univ_model::release::keys;
use univ_model::release::shards::Database;

use super::PartSink;

/// The largest D1 file written, unless one statement alone is larger.
pub const MAX_FILE_BYTES: usize = 32 << 20;

/// What a file of a table holds.
#[derive(Clone, Copy)]
pub(super) enum Kind {
    Rows,
    Retired,
}

/// Where a part of a table's files goes: its database, the table's place `at` (from 0) in
/// `TABLES` and name, and what it holds.
#[derive(Clone, Copy)]
pub(super) struct Place<'a> {
    pub database: Database,
    pub at: usize,
    pub table: &'a str,
    pub kind: Kind,
}

impl Place<'_> {
    /// The path of part `part`, from 1.
    fn path(&self, part: usize) -> String {
        let kind = match self.kind {
            Kind::Rows => "",
            Kind::Retired => "r",
        };
        format!(
            "{}/{}/{:04}-{}-{kind}{part:03}.sql",
            keys::D1_DIR,
            self.database.dir(),
            self.at + 1,
            self.table
        )
    }
}

/// The statements of the part being gathered, and how many parts were put.
#[derive(Default)]
pub(super) struct Parts {
    text: String,
    put: usize,
}

impl Parts {
    /// Adds `statement`, putting the part first when it would grow beyond [`MAX_FILE_BYTES`].
    pub(super) fn push(
        &mut self,
        sink: &mut dyn PartSink,
        place: Place<'_>,
        statement: &str,
    ) -> Result<(), String> {
        if !self.text.is_empty() && self.text.len() + statement.len() > MAX_FILE_BYTES {
            self.put_part(sink, place)?;
        }
        self.text.push_str(statement);
        Ok(())
    }

    /// Puts the part being gathered, if it holds anything.
    pub(super) fn finish(
        &mut self,
        sink: &mut dyn PartSink,
        place: Place<'_>,
    ) -> Result<(), String> {
        if self.text.is_empty() {
            return Ok(());
        }
        self.put_part(sink, place)
    }

    fn put_part(&mut self, sink: &mut dyn PartSink, place: Place<'_>) -> Result<(), String> {
        self.put += 1;
        sink.put(place.path(self.put), self.text.as_bytes())?;
        self.text.clear();
        Ok(())
    }
}
