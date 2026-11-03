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
