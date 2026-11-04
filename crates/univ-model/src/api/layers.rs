//! Layers as clients create, configure, list and follow them (create-and-configure-a-layer,
//! read-a-layer, follow-a-layers-changes).

use serde::{Deserialize, Serialize};

/// Who may read a layer: anyone, or only its owning key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    /// Readable only with the owning key; a layer is private unless created public.
    #[default]
    Private,
}

/// What a client asks to create.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct CreateLayer {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub visibility: Visibility,
}

/// What the owning key asks to change of a layer, each part kept where absent.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct LayerChange {
    pub title: Option<String>,
    pub description: Option<String>,
    pub visibility: Option<Visibility>,
}

/// A layer as it is read and listed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LayerRecord {
    pub id: String,
    /// The id of the key that owns it, which alone may change it.
    pub owner: String,
    pub title: String,
    pub description: String,
    pub visibility: Visibility,
    /// RFC 3339 UTC.
    pub created_at: String,
    /// The number of the layer's last change; 0 until its first.
    pub sequence: u64,
    /// The layer's current items that are not retracted.
    pub item_count: u64,
}

/// Every layer a caller may read: the public ones, and the caller's key's private ones.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LayerList {
    pub layers: Vec<LayerRecord>,
}

/// What kind of change a layer's change is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    LayerConfigured,
    ItemPublished,
    ItemRevised,
    ItemRetracted,
    LayerRetired,
}

/// One change of a layer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Change {
    pub sequence: u64,
    /// RFC 3339 UTC.
    pub at: String,
    pub kind: ChangeKind,
    /// The item it concerns; absent for a change of the layer itself.
    pub item: Option<String>,
    /// The item's version it made.
    pub version: Option<u64>,
}

/// A page of a layer's changes after a cursor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ChangePage {
    pub layer: String,
    pub changes: Vec<Change>,
    /// The sequence number of the last change this page holds, to ask for what follows; the
    /// cursor asked when it holds none.
    pub cursor: u64,
    /// The layer's change sequence now.
    pub sequence: u64,
}

/// What a key has written, against its quotas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Usage {
    /// Its layers not retired.
    pub layers: u64,
    /// The number of items its largest layer holds, retracted ones included.
    pub largest_layer_items: u64,
}

/// What kind of version an item's version is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum VersionKind {
    /// The item as first published.
    Published,
    /// A revision that brings the item up to date.
    Update,
    /// A revision that corrects an earlier version.
    Correction,
    /// The item withdrawn, with the reason.
    Retraction,
}

/// What a version holds beyond its number, time and kind, as stored.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct VersionBody {
    /// The item's content; absent from a retraction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
    /// The version a correction corrects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corrects: Option<u64>,
    /// What was wrong with the version a correction corrects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub what_was_wrong: Option<String>,
    /// Why a retraction withdrew the item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// One version of an item.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ItemVersion {
    pub item: String,
    pub version: u64,
    /// RFC 3339 UTC: when the server stored it.
    pub stored_at: String,
    pub kind: VersionKind,
    #[serde(flatten)]
    pub body: VersionBody,
}

/// A version as an item's list of versions gives it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct VersionSummary {
    pub version: u64,
    pub stored_at: String,
    pub kind: VersionKind,
}

/// An item: its current version and the list of all its versions, oldest first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ItemRecord {
    pub layer: String,
    pub current: ItemVersion,
    pub retracted: bool,
    pub versions: Vec<VersionSummary>,
}

/// A layer and one page of its current items that are not retracted, by id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LayerPage {
    pub layer: LayerRecord,
    pub items: Vec<ItemVersion>,
    /// The cursor of the next page, as `?cursor=`; absent on the last page.
    pub next: Option<String>,
}

/// How an item relates to the reference object or structure it links to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    /// The object hosts the item, as a galaxy hosts a supernova.
    Host,
    /// The object is the item's counterpart in another messenger or band.
    Counterpart,
    /// The item is a member of the object or structure.
    MemberOf,
    /// The item lies within the object or structure.
    LocatedIn,
}

/// A link from an item to one reference object, by its id, or one structure, by its id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub relation: Relation,
    /// The id of an object some release published.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<u64>,
    /// The id of a structure some release published.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structure: Option<String>,
}

/// A source an item cites: its title, and at least one of an ADS bibcode, a DOI or a URL.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ItemSource {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bibcode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// What an item holds: a title, a summary, a geometry in a named frame, its time records, its
/// kind of value, its links and at least one source.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ItemContent {
    pub title: String,
    pub summary: String,
    pub geometry: crate::geometry::Shape,
    #[serde(default)]
    pub time_records: Vec<crate::time::TimeRecord>,
    pub kind_of_value: crate::value::KindOfValue,
    #[serde(default)]
    pub links: Vec<Link>,
    pub sources: Vec<ItemSource>,
}

/// An item to publish: its id, chosen by the client and unique within the layer, and its
/// content.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PublishItem {
    pub id: String,
    #[serde(flatten)]
    pub content: ItemContent,
}

/// What kind of revision a revision is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RevisionKind {
    Update,
    Correction,
}

/// A revision: the version it replaces, which must be the current one, and the content it
/// stores as the next version. A correction names the version it corrects and what was wrong.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ReviseItem {
    pub replaces: u64,
    pub revision: RevisionKind,
    /// The version a correction corrects; the one it replaces where absent.
    pub corrects: Option<u64>,
    /// What was wrong; required of a correction.
    pub what_was_wrong: Option<String>,
    pub content: ItemContent,
}

/// A retraction: the version it replaces, which must be the current one, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct RetractItem {
    pub replaces: u64,
    pub reason: String,
}
