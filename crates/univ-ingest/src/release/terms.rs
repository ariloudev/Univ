//! What a release numbers once and its rows name by number: its terms, its sources and its
//! distinct transformation chains.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use serde::ser::{self, Impossible};
use univ_model::ids::SourceId;
use univ_model::release::rows::{ChainRow, Json, TermRow};

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

/// Numbers given to names once and never again: each name keeps the number a release before
/// gave it, and a new name takes the next number after every number ever given, so a number
/// that a release no longer uses is never given to another name.
#[derive(Clone, Debug, Default)]
struct Numbers {
    /// Each name's number and, once this release uses it, its place among the names it uses.
    ids: HashMap<String, (u32, Option<usize>)>,
    next: u32,
    used: usize,
}

/// A name's number as [`Numbers::give`] gives it.
struct Given {
    number: u32,
    /// The name's place among the names this release uses.
    place: usize,
    /// Whether this is the release's first use of the name.
    first: bool,
}

impl Numbers {
    /// The numbers a release before gave, `kept`.
    fn kept(kept: &BTreeMap<String, u32>) -> Self {
        Self {
            ids: kept
                .iter()
                .map(|(n, id)| (n.clone(), (*id, None)))
                .collect(),
            next: kept.values().max().map_or(0, |m| m.saturating_add(1)),
            used: 0,
        }
    }

    /// The number of `name`, given it when new, counting this release's use of it.
    fn give(&mut self, name: &str) -> Given {
        if let Some((number, place)) = self.ids.get_mut(name) {
            let first = place.is_none();
            let place = *place.get_or_insert_with(|| {
                self.used += 1;
                self.used - 1
            });
            return Given {
                number: *number,
                place,
                first,
            };
        }
        let given = Given {
            number: self.next,
            place: self.used,
            first: true,
        };
        self.next = self.next.saturating_add(1);
        self.used += 1;
        self.ids
            .insert(name.to_owned(), (given.number, Some(given.place)));
        given
    }

    /// The number of `name`, where it has one.
    fn get(&self, name: &str) -> Option<u32> {
        self.ids.get(name).map(|(number, _)| *number)
    }

    /// Every number ever given, by its name, for the next release to keep.
    fn numbering(&self) -> BTreeMap<String, u32> {
        self.ids
            .iter()
            .map(|(n, (id, _))| (n.clone(), *id))
            .collect()
    }
}

/// The terms of a release, each numbered once by [`Numbers`], in the order first named.
#[derive(Clone, Debug, Default)]
pub struct Terms {
    numbers: Numbers,
    /// The terms this release names, as (id, term), in the order first named.
    named: Vec<(u32, String)>,
}

impl Terms {
    /// The terms numbered as `kept`, the release before's numbering, gave them.
    #[must_use]
    pub fn kept(kept: &BTreeMap<String, u32>) -> Self {
        Self {
            numbers: Numbers::kept(kept),
            named: Vec::new(),
        }
    }

    /// The id of `term`, numbering it when new.
    pub fn id(&mut self, term: &str) -> u32 {
        let given = self.numbers.give(term);
        if given.first {
            self.named.push((given.number, term.to_owned()));
        }
        given.number
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
            Ok(self.id(json.as_str()))
        }
    }

    /// The rows of release `release`: the terms it names, by id.
    #[must_use]
    pub fn rows(&self, release: u32) -> Vec<TermRow> {
        let mut rows: Vec<TermRow> = self
            .named
            .iter()
            .map(|(id, term)| TermRow {
                release,
                id: *id,
                term: term.clone(),
            })
            .collect();
        rows.sort_unstable_by_key(|r| r.id);
        rows
    }

    /// Every term ever numbered, by its id, for the next release to keep.
    #[must_use]
    pub fn numbering(&self) -> BTreeMap<String, u32> {
        self.numbers.numbering()
    }
}

/// The index of each source of a release, `sources.n`, numbered once by [`Numbers`]: the
/// release's sources first, in order, then any other source a row names.
#[derive(Clone, Debug, Default)]
pub struct SourceIndex {
    numbers: Numbers,
}

impl SourceIndex {
    /// The index of the release's `sources`, in order, keeping the indexes of `kept`.
    #[must_use]
    pub fn new<'a>(
        kept: &BTreeMap<String, u32>,
        sources: impl IntoIterator<Item = &'a SourceId>,
    ) -> Self {
        let mut index = Self {
            numbers: Numbers::kept(kept),
        };
        for s in sources {
            index.n(s);
        }
        index
    }

    /// The index of `source`, numbering it when new.
    pub fn n(&mut self, source: &SourceId) -> u32 {
        self.numbers.give(source.as_str()).number
    }

    /// The index of `source`, which the release's sources all have.
    ///
    /// # Errors
    ///
    /// Fails when `source` has none.
    pub fn indexed(&self, source: &SourceId) -> Result<u32, String> {
        self.numbers
            .get(source.as_str())
            .ok_or_else(|| format!("source {source} has no index among the release's sources"))
    }

    /// Every source ever indexed, by its id, for the next release to keep.
    #[must_use]
    pub fn numbering(&self) -> BTreeMap<String, u32> {
        self.numbers.numbering()
    }
}

/// The distinct transformation chains of a release, by their template's JSON, each numbered
/// once by [`Numbers`].
#[derive(Clone, Debug, Default)]
pub struct Chains {
    numbers: Numbers,
    /// The row of each chain this release holds, in the order first used.
    list: Vec<ChainRow>,
}

impl Chains {
    /// The chains numbered as `kept`, the release before's numbering, gave them.
    #[must_use]
    pub fn kept(kept: &BTreeMap<String, u32>) -> Self {
        Self {
            numbers: Numbers::kept(kept),
            list: Vec::new(),
        }
    }

    /// The id of the chain whose template is `steps`, taking `params` parameters, counting one
    /// more object of it.
    pub fn id(&mut self, release: u32, steps: Json, params: usize) -> u32 {
        let given = self.numbers.give(steps.as_str());
        if given.first {
            self.list.push(ChainRow {
                release,
                id: given.number,
                steps,
                params: u32::try_from(params).unwrap_or(u32::MAX),
                objects: 1,
            });
        } else if let Some(row) = self.list.get_mut(given.place) {
            row.objects += 1;
        }
        given.number
    }

    /// Each chain's row.
    #[must_use]
    pub fn rows(&self) -> &[ChainRow] {
        &self.list
    }

    /// Every chain ever numbered, by its template, for the next release to keep.
    #[must_use]
    pub fn numbering(&self) -> BTreeMap<String, u32> {
        self.numbers.numbering()
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
        assert_eq!(chains.id(2, Json::from("[1]"), 0), 6);
        assert_eq!(chains.id(2, Json::from("[2]"), 1), 5);
        assert_eq!(chains.id(2, Json::from("[2]"), 1), 5);
        let counts: Vec<(u32, u64)> = chains.rows().iter().map(|r| (r.id, r.objects)).collect();
        assert_eq!(counts, [(6, 1), (5, 2)]);
    }
}
