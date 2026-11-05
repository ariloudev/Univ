//! What a release numbers once and its rows name by number: its terms, its sources and its
//! distinct transformation chains.

use std::collections::HashMap;

use serde::Serialize;
use serde::ser::{self, Impossible};
use univ_model::ids::SourceId;
use univ_model::release::rows::{ChainRow, TermRow};

/// The name serde gives a unit variant, without writing anything: none for any other value.
struct VariantName;

/// Not a unit variant.
#[derive(Debug)]
struct NotAName;

impl std::fmt::Display for NotAName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("not a unit variant")
    }
}

impl std::error::Error for NotAName {}

impl ser::Error for NotAName {
    fn custom<T: std::fmt::Display>(_: T) -> Self {
        Self
    }
}

/// Serializer methods that refuse their value.
macro_rules! refuse {
    ($($method:ident($($arg:ty),*)),* $(,)?) => {
        $(fn $method(self, $(_: $arg),*) -> Result<&'static str, NotAName> { Err(NotAName) })*
    };
}

impl ser::Serializer for VariantName {
    type Ok = &'static str;
    type Error = NotAName;
    type SerializeSeq = Impossible<&'static str, NotAName>;
    type SerializeTuple = Impossible<&'static str, NotAName>;
    type SerializeTupleStruct = Impossible<&'static str, NotAName>;
    type SerializeTupleVariant = Impossible<&'static str, NotAName>;
    type SerializeMap = Impossible<&'static str, NotAName>;
    type SerializeStruct = Impossible<&'static str, NotAName>;
    type SerializeStructVariant = Impossible<&'static str, NotAName>;

    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<&'static str, NotAName> {
        Ok(variant)
    }

    refuse!(
        serialize_bool(bool),
        serialize_i8(i8),
        serialize_i16(i16),
        serialize_i32(i32),
        serialize_i64(i64),
        serialize_u8(u8),
        serialize_u16(u16),
        serialize_u32(u32),
        serialize_u64(u64),
        serialize_f32(f32),
        serialize_f64(f64),
        serialize_char(char),
        serialize_str(&str),
        serialize_bytes(&[u8]),
        serialize_none(),
        serialize_unit(),
        serialize_unit_struct(&'static str),
    );

    fn serialize_some<T: Serialize + ?Sized>(self, _: &T) -> Result<&'static str, NotAName> {
        Err(NotAName)
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: &T,
    ) -> Result<&'static str, NotAName> {
        Err(NotAName)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<&'static str, NotAName> {
        Err(NotAName)
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, NotAName> {
        Err(NotAName)
    }

    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, NotAName> {
        Err(NotAName)
    }

    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, NotAName> {
        Err(NotAName)
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, NotAName> {
        Err(NotAName)
    }

    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, NotAName> {
        Err(NotAName)
    }

    fn serialize_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStruct, NotAName> {
        Err(NotAName)
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, NotAName> {
        Err(NotAName)
    }
}

/// The name serde gives `value`, a unit variant such as `galaxy_group`; none for another value.
#[must_use]
pub fn variant_name<T: Serialize + ?Sized>(value: &T) -> Option<&'static str> {
    value.serialize(VariantName).ok()
}

/// The terms of a release, each numbered once in the order first named.
#[derive(Clone, Debug, Default)]
pub struct Terms {
    ids: HashMap<String, u32>,
    names: Vec<String>,
}

impl Terms {
    /// The id of `term`, numbering it when new.
    pub fn id(&mut self, term: &str) -> u32 {
        if let Some(id) = self.ids.get(term) {
            return *id;
        }
        let id = u32::try_from(self.names.len()).unwrap_or(u32::MAX);
        self.ids.insert(term.to_owned(), id);
        self.names.push(term.to_owned());
        id
    }

    /// The id of the term `value` serializes as: its variant name, or its JSON when it is not a
    /// unit variant, such as a mixed distance method.
    ///
    /// # Errors
    ///
    /// Fails when it cannot be written as JSON.
    pub fn of<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<u32, String> {
        if let Some(name) = variant_name(value) {
            Ok(self.id(name))
        } else {
            let json = super::json(value)?;
            Ok(self.id(&json))
        }
    }

    /// The rows of release `release`.
    #[must_use]
    pub fn rows(&self, release: u32) -> Vec<TermRow> {
        (0u32..)
            .zip(&self.names)
            .map(|(id, term)| TermRow {
                release,
                id,
                term: term.clone(),
            })
            .collect()
    }
}

/// The index of each source of a release, `sources.n`: its place among the release's sources,
/// then any other source a row names, numbered after them.
#[derive(Clone, Debug, Default)]
pub struct SourceIndex {
    ids: HashMap<SourceId, u32>,
}

impl SourceIndex {
    /// The index of the release's `sources`, in order.
    #[must_use]
    pub fn new<'a>(sources: impl IntoIterator<Item = &'a SourceId>) -> Self {
        let mut index = Self::default();
        for s in sources {
            index.n(s);
        }
        index
    }

    /// The index of `source`, numbering it when new.
    pub fn n(&mut self, source: &SourceId) -> u32 {
        let next = u32::try_from(self.ids.len()).unwrap_or(u32::MAX);
        *self.ids.entry(source.clone()).or_insert(next)
    }
}

/// The distinct transformation chains of a release, by their template's JSON.
#[derive(Debug, Default)]
pub struct Chains {
    ids: HashMap<String, u32>,
    list: Vec<ChainRow>,
}

impl Chains {
    /// The id of the chain whose template is `steps`, taking `params` parameters, counting one
    /// more object of it.
    pub fn id(&mut self, release: u32, steps: String, params: usize) -> u32 {
        if let Some(&id) = self.ids.get(&steps) {
            if let Some(row) = self.list.get_mut(id as usize) {
                row.objects += 1;
            }
            return id;
        }
        let id = u32::try_from(self.list.len()).unwrap_or(u32::MAX);
        self.ids.insert(steps.clone(), id);
        self.list.push(ChainRow {
            release,
            id,
            steps,
            params: u32::try_from(params).unwrap_or(u32::MAX),
            objects: 1,
        });
        id
    }

    /// Each chain's row.
    #[must_use]
    pub fn rows(&self) -> &[ChainRow] {
        &self.list
    }
}

#[cfg(test)]
mod tests {
    use univ_model::distance::Method;
    use univ_model::object::ObjectKind;
    use univ_model::units::Unit;

    use super::*;

    #[test]
    fn a_term_is_a_variant_name_or_the_json_of_another_value() {
        assert_eq!(variant_name(&ObjectKind::GalaxyGroup), Some("galaxy_group"));
        assert_eq!(variant_name(&Unit::Megaparsec), Some("megaparsec"));
        assert_eq!(variant_name(&Method::Other("x".into())), None);
        let mut terms = Terms::default();
        assert_eq!(terms.of(&Unit::Parsec).unwrap(), 0);
        assert_eq!(terms.of(&Method::Other("x".into())).unwrap(), 1);
        assert_eq!(terms.id("parsec"), 0);
        let rows = terms.rows(3);
        assert_eq!(rows[1].term, r#"{"other":"x"}"#);
        assert_eq!((rows[1].release, rows[1].id), (3, 1));
    }
}
