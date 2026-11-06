//! Rows shared between the releases they are valid in (schema 6): a row of a table of
//! [`super::Scope::PerRelease`] is valid from the release in its `release` column, which ends its
//! key, until the release in its `retired` column, if any, which a later release sets.

/// The column of a table of [`super::Scope::PerRelease`] that names the release from which its row is
/// no longer valid, NULL while it is: the schema gives it to every such table, after the
/// columns its row type writes, and only a later release's retirement sets it.
pub const RETIRED: &str = "retired";

/// The SQL condition selecting the rows of a table of [`super::Scope::PerRelease`] valid in release
/// `number`, its columns qualified by `alias` (such as `o.`, or empty for none): those whose
/// `release` is at most `number` and that no release up to `number` retired.
#[must_use]
pub fn valid_in(alias: &str, number: impl std::fmt::Display) -> String {
    format!(
        "{alias}release <= {number} AND ({alias}{RETIRED} IS NULL OR {alias}{RETIRED} > {number})"
    )
}

/// [`valid_in`] written at compile time, for a statement written as a literal: the condition
/// selecting the rows valid in the release bound to the parameter `$release` (such as `"?1"`),
/// columns qualified by `$alias` (such as `"o."`, or `""` for none). It expands to a string
/// literal, so `concat!` can take it into a statement.
#[macro_export]
macro_rules! valid_in {
    ($alias:literal, $release:literal) => {
        concat!(
            $alias,
            "release <= ",
            $release,
            " AND (",
            $alias,
            "retired IS NULL OR ",
            $alias,
            "retired > ",
            $release,
            ")"
        )
    };
}

impl super::Table {
    /// The columns of its key but `release`: the key a row has among those valid in one
    /// release, and that a later release keeps it or retires it by.
    pub fn natural_key(&self) -> impl Iterator<Item = &'static str> {
        self.key.iter().copied().filter(|c| *c != "release")
    }
}
