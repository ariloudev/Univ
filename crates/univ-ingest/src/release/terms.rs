//! What a release numbers once and its rows name by number: its terms, its sources and its
//! distinct transformation chains.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use serde::ser::{self, Impossible};
use univ_model::ids::SourceId;
use univ_model::release::rows::{ChainRow, TermRow};

use super::kept::next_after;

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

/// The terms of a release, each numbered once: by the number the release before gave it, or,
/// when new, by the next number never given, in the order first named.
#[derive(Clone, Debug, Default)]
pub struct Terms {
    ids: HashMap<String, u32>,
    next: u32,
    /// The terms this release names, by id.
    named: BTreeMap<u32, String>,
}

impl Terms {
    /// The terms numbered as `kept`, the release before's numbering, gave them.
    #[must_use]
    pub fn kept(kept: &BTreeMap<String, u32>) -> Self {
        Self {
            ids: kept.iter().map(|(t, id)| (t.clone(), *id)).collect(),
            next: next_after(kept.values()),
            named: BTreeMap::new(),
        }
    }

    /// The id of `term`, numbering it when new.
    pub fn id(&mut self, term: &str) -> u32 {
        let id = if let Some(id) = self.ids.get(term) {
            *id
        } else {
            let id = self.next;
            self.next = self.next.saturating_add(1);
            self.ids.insert(term.to_owned(), id);
            id
        };
        self.named.entry(id).or_insert_with(|| term.to_owned());
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

    /// The rows of release `release`: the terms it names.
    #[must_use]
    pub fn rows(&self, release: u32) -> Vec<TermRow> {
        self.named
            .iter()
            .map(|(id, term)| TermRow {
                release,
                id: *id,
                term: term.clone(),
            })
            .collect()
    }

    /// Every term ever numbered, by its id, for the next release to keep.
    #[must_use]
    pub fn numbering(&self) -> BTreeMap<String, u32> {
        self.ids.iter().map(|(t, id)| (t.clone(), *id)).collect()
    }
}

/// The index of each source of a release, `sources.n`: the index the release before gave it,
/// or, for a source new to it, the next index never given, the release's sources first, in
/// order, then any other source a row names.
#[derive(Clone, Debug, Default)]
pub struct SourceIndex {
    ids: HashMap<SourceId, u32>,
    next: u32,
}

impl SourceIndex {
    /// The index of the release's `sources`, in order, keeping the indexes of `kept`.
    #[must_use]
    pub fn new<'a>(
        kept: &BTreeMap<String, u32>,
        sources: impl IntoIterator<Item = &'a SourceId>,
    ) -> Self {
        let mut index = Self {
            ids: kept
                .iter()
                .filter_map(|(id, n)| Some((SourceId::new(id).ok()?, *n)))
                .collect(),
            next: next_after(kept.values()),
        };
        for s in sources {
            index.n(s);
        }
        index
    }

    /// The index of `source`, numbering it when new.
    pub fn n(&mut self, source: &SourceId) -> u32 {
        if let Some(n) = self.ids.get(source) {
            return *n;
        }
        let n = self.next;
        self.next = self.next.saturating_add(1);
        self.ids.insert(source.clone(), n);
        n
    }

    /// Every source ever indexed, by its id, for the next release to keep.
    #[must_use]
    pub fn numbering(&self) -> BTreeMap<String, u32> {
        self.ids.iter().map(|(s, n)| (s.to_string(), *n)).collect()
    }
}

/// The distinct transformation chains of a release, by their template's JSON, each with the id
/// the release before gave it or, when new, the next id never given.
#[derive(Clone, Debug, Default)]
pub struct Chains {
    ids: HashMap<String, u32>,
    next: u32,
    /// The place in `list` of each chain this release holds, by id.
    places: HashMap<u32, usize>,
    list: Vec<ChainRow>,
}

impl Chains {
    /// The chains numbered as `kept`, the release before's numbering, gave them.
    #[must_use]
    pub fn kept(kept: &BTreeMap<String, u32>) -> Self {
        Self {
            ids: kept.iter().map(|(t, id)| (t.clone(), *id)).collect(),
            next: next_after(kept.values()),
            ..Self::default()
        }
    }

    /// The id of the chain whose template is `steps`, taking `params` parameters, counting one
    /// more object of it.
    pub fn id(&mut self, release: u32, steps: String, params: usize) -> u32 {
        let id = if let Some(&id) = self.ids.get(&steps) {
            id
        } else {
            let id = self.next;
            self.next = self.next.saturating_add(1);
            self.ids.insert(steps.clone(), id);
            id
        };
        if let Some(row) = self.places.get(&id).and_then(|&at| self.list.get_mut(at)) {
            row.objects += 1;
        } else {
            self.places.insert(id, self.list.len());
            self.list.push(ChainRow {
                release,
                id,
                steps,
                params: u32::try_from(params).unwrap_or(u32::MAX),
                objects: 1,
            });
        }
        id
    }

    /// Each chain's row.
    #[must_use]
    pub fn rows(&self) -> &[ChainRow] {
        &self.list
    }

    /// Every chain ever numbered, by its template, for the next release to keep.
    #[must_use]
    pub fn numbering(&self) -> BTreeMap<String, u32> {
        self.ids.iter().map(|(t, id)| (t.clone(), *id)).collect()
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

    #[test]
    fn kept_numbers_stay_and_new_ones_follow_the_highest_ever_given() {
        let kept = BTreeMap::from([("parsec".to_owned(), 3), ("gone".to_owned(), 7)]);
        let mut terms = Terms::kept(&kept);
        assert_eq!(terms.id("megaparsec"), 8);
        assert_eq!(terms.id("parsec"), 3);
        // Only the terms this release names are its rows; a term it no longer names keeps its
        // number for good.
        let ids: Vec<u32> = terms.rows(2).iter().map(|r| r.id).collect();
        assert_eq!(ids, [3, 8]);
        assert_eq!(terms.numbering().get("gone"), Some(&7));

        let a = SourceId::new("a").unwrap();
        let b = SourceId::new("b").unwrap();
        let kept = BTreeMap::from([("b".to_owned(), 0)]);
        let mut sources = SourceIndex::new(&kept, [&a, &b]);
        assert_eq!((sources.n(&b), sources.n(&a)), (0, 1));
        assert_eq!(sources.numbering().len(), 2);

        let kept = BTreeMap::from([("[2]".to_owned(), 5)]);
        let mut chains = Chains::kept(&kept);
        assert_eq!(chains.id(2, "[1]".to_owned(), 0), 6);
        assert_eq!(chains.id(2, "[2]".to_owned(), 1), 5);
        assert_eq!(chains.id(2, "[2]".to_owned(), 1), 5);
        let counts: Vec<(u32, u64)> = chains.rows().iter().map(|r| (r.id, r.objects)).collect();
        assert_eq!(counts, [(6, 1), (5, 2)]);
    }
}
