//! Canonical v5 Presentation transaction boundary.
//!
//! A [`Deck`] is the only mutable domain state in this crate. Commands are semantic and typed;
//! renderers may consume a [`PresentationChangeSet`], but neither Canvas nor DOM state enters the
//! journal. The engine deliberately stores inverse mutations for only the affected entities rather
//! than cloning a whole deck for each command.

pub mod v5_projection;

use std::collections::{BTreeMap, BTreeSet};

use oo_protocol::{EntityRef, Invalidation, MutationRecord};
use oo_schema::presentation_v5::{
    AnimationEntry, AssetRef, ChartSpec, ConnectorEndpoint, Deck, DeckTheme, ImageNode, MediaNode,
    NodeTransform, PresentationRichText, SceneNode, SceneNodeKind, ShapeGeometry, ShapeStyle,
    Slide, SlideBackground, SlideLayout, SlideMaster, SlidePageSpec, SlideTransition, TableCell,
    TableCellStyle, TableNode, TextFrame, Timeline,
};
use serde::{Deserialize, Serialize};
use v5_projection::{DeckProjection, ProjectionChange};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationCommandBatch {
    pub base_revision: u64,
    pub commands: Vec<PresentationCommand>,
}

/// Stable table-cell identity inside one table node. For merged regions the
/// only valid address is the region's top-left anchor; the engine rejects
/// interior coordinates so UI hit testing cannot silently mutate a neighbour.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableCellAddress {
    pub row: u32,
    pub column: u32,
}

/// Typed domain intents. There is intentionally no generic `updateNode(attrs)` or patch-shaped
/// escape hatch: a node inspector must choose a concrete presentation capability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PresentationCommand {
    /// Registers a verified artifact asset with the canonical Deck before a node may reference it.
    /// Uploading bytes alone never mutates the Deck or makes an image renderable.
    RegisterAsset {
        asset: AssetRef,
    },
    SetPageSpec {
        page_spec: SlidePageSpec,
    },
    /// Adds a complete, schema-validated master. Master editing is entity based:
    /// no renderer may mutate individual placeholder attributes through a patch.
    CreateMaster {
        master: SlideMaster,
    },
    /// Replaces exactly one master definition while retaining its stable id.
    UpdateMaster {
        master: SlideMaster,
    },
    /// Deletes an unused master. Layout references are rejected by the engine.
    DeleteMaster {
        master_id: String,
    },
    /// Adds a complete layout bound to one existing master.
    CreateLayout {
        layout: SlideLayout,
    },
    /// Replaces exactly one layout definition while retaining its stable id.
    UpdateLayout {
        layout: SlideLayout,
    },
    /// Deletes an unused layout. Slide and placeholder references are rejected.
    DeleteLayout {
        layout_id: String,
    },
    CreateSlide {
        slide: Slide,
        index: usize,
    },
    DeleteSlide {
        slide_id: String,
    },
    MoveSlide {
        slide_id: String,
        index: usize,
    },
    /// Clones one slide inside the engine. The caller owns all newly allocated
    /// ids, while the engine rewrites every same-slide reference atomically.
    /// This keeps the browser from materialising and writing a mutable Deck.
    DuplicateSlide {
        source_slide_id: String,
        slide_id: String,
        order_key: String,
        name: String,
        node_id_map: Vec<PresentationIdMapping>,
        animation_id_map: Vec<PresentationIdMapping>,
        index: usize,
    },
    InsertNode {
        slide_id: String,
        node: SceneNode,
        index: usize,
    },
    DeleteNode {
        slide_id: String,
        node_id: String,
    },
    /// Moves one node to a Group parent (or the slide root) and places it among that parent's
    /// siblings. Parent/child hierarchy is domain data; it is never derived from view selection.
    MoveNode {
        slide_id: String,
        node_id: String,
        parent_id: Option<String>,
        index: usize,
    },
    ReorderNode {
        slide_id: String,
        node_id: String,
        index: usize,
    },
    GroupNodes {
        slide_id: String,
        group: SceneNode,
        child_ids: Vec<String>,
        index: usize,
    },
    UngroupNodes {
        slide_id: String,
        group_id: String,
    },
    SetNodeTransform {
        slide_id: String,
        node_id: String,
        transform: NodeTransform,
    },
    /// Persists a node lock. Pointer handlers read the canonical node value;
    /// selection state is never used as an implicit lock.
    SetNodeLocked {
        slide_id: String,
        node_id: String,
        locked: bool,
    },
    /// Aligns a set of unlocked nodes against their shared selection bounds.
    /// The engine, rather than a renderer, computes the resulting coordinates.
    AlignNodes {
        slide_id: String,
        node_ids: Vec<String>,
        alignment: NodeAlignment,
    },
    /// Distributes unlocked nodes while preserving the outer two selection bounds.
    DistributeNodes {
        slide_id: String,
        node_ids: Vec<String>,
        axis: NodeDistributionAxis,
    },
    SetShapeStyle {
        slide_id: String,
        node_id: String,
        style: ShapeStyle,
    },
    /// Changes the primitive of an existing shape without recreating its node.
    SetShapeGeometry {
        slide_id: String,
        node_id: String,
        geometry: ShapeGeometry,
    },
    /// Replaces the complete, validated data specification of exactly one
    /// chart.  The chart subset deliberately has no renderer-owned point or
    /// OOXML patch data.
    SetChartSpec {
        slide_id: String,
        node_id: String,
        spec: ChartSpec,
    },
    /// Reconnects exactly one connector atomically. Rendering never persists
    /// two half-updated endpoints as independent node patches.
    SetConnectorEndpoints {
        slide_id: String,
        node_id: String,
        start: ConnectorEndpoint,
        end: ConnectorEndpoint,
    },
    /// Replaces the rich text body of exactly one table cell anchor. A merged
    /// cell is addressed only by its top-left anchor, never by a renderer-side
    /// hit-test coordinate.
    SetTableCellContent {
        slide_id: String,
        node_id: String,
        row: u32,
        column: u32,
        content: PresentationRichText,
    },
    /// Replaces one complete cell style across explicit table cell anchors.
    /// The command is batch-safe without allowing an unbounded table patch.
    SetTableCellStyle {
        slide_id: String,
        node_id: String,
        cells: Vec<TableCellAddress>,
        style: TableCellStyle,
    },
    /// Inserts complete grid rows. `index` is a grid boundary in `0..=rows`.
    InsertTableRows {
        slide_id: String,
        node_id: String,
        index: u32,
        count: u32,
    },
    /// Inserts complete grid columns. `index` is a grid boundary in `0..=columns`.
    InsertTableColumns {
        slide_id: String,
        node_id: String,
        index: u32,
        count: u32,
    },
    /// Deletes one grid row. A presentation table must retain at least one row.
    DeleteTableRow {
        slide_id: String,
        node_id: String,
        index: u32,
    },
    /// Deletes one grid column. A presentation table must retain at least one column.
    DeleteTableColumn {
        slide_id: String,
        node_id: String,
        index: u32,
    },
    /// Merges a rectangular, fully-unmerged-or-contained grid range.
    MergeTableCells {
        slide_id: String,
        node_id: String,
        start: TableCellAddress,
        end: TableCellAddress,
    },
    /// Splits one merged cell by its canonical top-left anchor.
    SplitTableCell {
        slide_id: String,
        node_id: String,
        row: u32,
        column: u32,
    },
    SetTextContent {
        slide_id: String,
        node_id: String,
        body: PresentationRichText,
    },
    SetTextFrame {
        slide_id: String,
        node_id: String,
        frame: TextFrame,
    },
    SetImageConfig {
        slide_id: String,
        node_id: String,
        image: ImageNode,
    },
    /// Replaces the immutable media references of an existing audio or video node.
    /// The engine verifies both the referenced assets and their media families;
    /// renderers never infer or repair an invalid reference.
    SetMediaConfig {
        slide_id: String,
        node_id: String,
        media: MediaNode,
    },
    SetSlideNotes {
        slide_id: String,
        notes: Option<String>,
    },
    SetSlideBackground {
        slide_id: String,
        background: SlideBackground,
    },
    SetSlideLayout {
        slide_id: String,
        layout_id: Option<String>,
    },
    SetTheme {
        theme: DeckTheme,
    },
    /// Changes the slide-level transition without replacing the animation timeline.
    SetSlideTransition {
        slide_id: String,
        transition: Option<SlideTransition>,
    },
    /// Inserts or replaces one animation entry.  A timeline is not a JSON patch bag:
    /// this command owns exactly one stable animation id.
    UpsertAnimation {
        slide_id: String,
        animation: AnimationEntry,
    },
    DeleteAnimation {
        slide_id: String,
        animation_id: String,
    },
    MoveAnimation {
        slide_id: String,
        animation_id: String,
        index: usize,
    },
}

/// An explicit source-to-target identity mapping used by clone commands.
/// Vectors, rather than an untyped JSON object, keep the wire contract
/// deterministic and let the engine reject duplicates and omissions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresentationIdMapping {
    pub source_id: String,
    pub target_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeAlignment {
    Left,
    Center,
    Right,
    Top,
    Middle,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeDistributionAxis {
    Horizontal,
    Vertical,
}

/// Public, serializable facts emitted after a successful transaction. They are intentionally
/// smaller than the private journal: consumers need invalidation facts, not retained snapshots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PresentationMutation {
    AssetRegistered {
        asset_id: String,
    },
    AssetUnregistered {
        asset_id: String,
    },
    PageSpecSet,
    MasterCreated {
        master_id: String,
    },
    MasterUpdated {
        master_id: String,
    },
    MasterDeleted {
        master_id: String,
    },
    LayoutCreated {
        layout_id: String,
    },
    LayoutUpdated {
        layout_id: String,
    },
    LayoutDeleted {
        layout_id: String,
    },
    SlideInserted {
        slide_id: String,
    },
    SlideDeleted {
        slide_id: String,
    },
    SlideMoved {
        slide_id: String,
    },
    SlideDuplicated {
        source_slide_id: String,
        slide_id: String,
    },
    NodeInserted {
        slide_id: String,
        node_id: String,
    },
    NodeDeleted {
        slide_id: String,
        node_id: String,
    },
    NodeMoved {
        slide_id: String,
        node_id: String,
    },
    NodeReordered {
        slide_id: String,
        node_id: String,
    },
    NodesGrouped {
        slide_id: String,
        group_id: String,
    },
    NodesUngrouped {
        slide_id: String,
        group_id: String,
    },
    NodeTransformSet {
        slide_id: String,
        node_id: String,
    },
    NodeLockSet {
        slide_id: String,
        node_id: String,
    },
    NodesAligned {
        slide_id: String,
        node_ids: Vec<String>,
    },
    NodesDistributed {
        slide_id: String,
        node_ids: Vec<String>,
    },
    ShapeStyleSet {
        slide_id: String,
        node_id: String,
    },
    ShapeGeometrySet {
        slide_id: String,
        node_id: String,
    },
    ChartSpecSet {
        slide_id: String,
        node_id: String,
    },
    ConnectorEndpointsSet {
        slide_id: String,
        node_id: String,
    },
    TableCellContentSet {
        slide_id: String,
        node_id: String,
        row: u32,
        column: u32,
    },
    TableCellStyleSet {
        slide_id: String,
        node_id: String,
        cells: Vec<TableCellAddress>,
    },
    TableRowsInserted {
        slide_id: String,
        node_id: String,
        index: u32,
        count: u32,
    },
    TableColumnsInserted {
        slide_id: String,
        node_id: String,
        index: u32,
        count: u32,
    },
    TableRowDeleted {
        slide_id: String,
        node_id: String,
        index: u32,
    },
    TableColumnDeleted {
        slide_id: String,
        node_id: String,
        index: u32,
    },
    TableCellsMerged {
        slide_id: String,
        node_id: String,
        start: TableCellAddress,
        end: TableCellAddress,
    },
    TableCellSplit {
        slide_id: String,
        node_id: String,
        row: u32,
        column: u32,
    },
    TextContentSet {
        slide_id: String,
        node_id: String,
    },
    TextFrameSet {
        slide_id: String,
        node_id: String,
    },
    ImageConfigSet {
        slide_id: String,
        node_id: String,
    },
    MediaConfigSet {
        slide_id: String,
        node_id: String,
    },
    SlideNotesSet {
        slide_id: String,
    },
    SlideBackgroundSet {
        slide_id: String,
    },
    SlideLayoutSet {
        slide_id: String,
    },
    ThemeSet,
    SlideTransitionSet {
        slide_id: String,
    },
    AnimationUpserted {
        slide_id: String,
        animation_id: String,
    },
    AnimationDeleted {
        slide_id: String,
        animation_id: String,
    },
    AnimationMoved {
        slide_id: String,
        animation_id: String,
    },
}

impl PresentationMutation {
    pub fn type_id(&self) -> &'static str {
        match self {
            Self::AssetRegistered { .. } => "presentation.assetRegistered",
            Self::AssetUnregistered { .. } => "presentation.assetUnregistered",
            Self::PageSpecSet => "presentation.pageSpecSet",
            Self::MasterCreated { .. } => "presentation.masterCreated",
            Self::MasterUpdated { .. } => "presentation.masterUpdated",
            Self::MasterDeleted { .. } => "presentation.masterDeleted",
            Self::LayoutCreated { .. } => "presentation.layoutCreated",
            Self::LayoutUpdated { .. } => "presentation.layoutUpdated",
            Self::LayoutDeleted { .. } => "presentation.layoutDeleted",
            Self::SlideInserted { .. } => "presentation.slideInserted",
            Self::SlideDeleted { .. } => "presentation.slideDeleted",
            Self::SlideMoved { .. } => "presentation.slideMoved",
            Self::SlideDuplicated { .. } => "presentation.slideDuplicated",
            Self::NodeInserted { .. } => "presentation.nodeInserted",
            Self::NodeDeleted { .. } => "presentation.nodeDeleted",
            Self::NodeMoved { .. } => "presentation.nodeMoved",
            Self::NodeReordered { .. } => "presentation.nodeReordered",
            Self::NodesGrouped { .. } => "presentation.nodesGrouped",
            Self::NodesUngrouped { .. } => "presentation.nodesUngrouped",
            Self::NodeTransformSet { .. } => "presentation.nodeTransformSet",
            Self::NodeLockSet { .. } => "presentation.nodeLockSet",
            Self::NodesAligned { .. } => "presentation.nodesAligned",
            Self::NodesDistributed { .. } => "presentation.nodesDistributed",
            Self::ShapeStyleSet { .. } => "presentation.shapeStyleSet",
            Self::ShapeGeometrySet { .. } => "presentation.shapeGeometrySet",
            Self::ChartSpecSet { .. } => "presentation.chartSpecSet",
            Self::ConnectorEndpointsSet { .. } => "presentation.connectorEndpointsSet",
            Self::TableCellContentSet { .. } => "presentation.tableCellContentSet",
            Self::TableCellStyleSet { .. } => "presentation.tableCellStyleSet",
            Self::TableRowsInserted { .. } => "presentation.tableRowsInserted",
            Self::TableColumnsInserted { .. } => "presentation.tableColumnsInserted",
            Self::TableRowDeleted { .. } => "presentation.tableRowDeleted",
            Self::TableColumnDeleted { .. } => "presentation.tableColumnDeleted",
            Self::TableCellsMerged { .. } => "presentation.tableCellsMerged",
            Self::TableCellSplit { .. } => "presentation.tableCellSplit",
            Self::TextContentSet { .. } => "presentation.textContentSet",
            Self::TextFrameSet { .. } => "presentation.textFrameSet",
            Self::ImageConfigSet { .. } => "presentation.imageConfigSet",
            Self::MediaConfigSet { .. } => "presentation.mediaConfigSet",
            Self::SlideNotesSet { .. } => "presentation.slideNotesSet",
            Self::SlideBackgroundSet { .. } => "presentation.slideBackgroundSet",
            Self::SlideLayoutSet { .. } => "presentation.slideLayoutSet",
            Self::ThemeSet => "presentation.themeSet",
            Self::SlideTransitionSet { .. } => "presentation.slideTransitionSet",
            Self::AnimationUpserted { .. } => "presentation.animationUpserted",
            Self::AnimationDeleted { .. } => "presentation.animationDeleted",
            Self::AnimationMoved { .. } => "presentation.animationMoved",
        }
    }

    pub fn to_record(&self) -> Result<MutationRecord, serde_json::Error> {
        Ok(MutationRecord {
            type_id: self.type_id().into(),
            payload: serde_json::to_value(self)?,
        })
    }

    fn entity_refs(&self) -> (Vec<EntityRef>, Vec<EntityRef>, bool) {
        let deck = || EntityRef {
            entity_type: "presentation.deck".into(),
            entity_id: "deck".into(),
        };
        let slide = |slide_id: &str| EntityRef {
            entity_type: "presentation.slide".into(),
            entity_id: slide_id.into(),
        };
        let node = |slide_id: &str, node_id: &str| EntityRef {
            entity_type: "presentation.node".into(),
            entity_id: format!("{slide_id}/{node_id}"),
        };
        let master = |master_id: &str| EntityRef {
            entity_type: "presentation.master".into(),
            entity_id: master_id.into(),
        };
        let layout = |layout_id: &str| EntityRef {
            entity_type: "presentation.layout".into(),
            entity_id: layout_id.into(),
        };
        match self {
            Self::AssetRegistered { asset_id } | Self::AssetUnregistered { asset_id } => (
                vec![EntityRef {
                    entity_type: "presentation.asset".into(),
                    entity_id: asset_id.clone(),
                }],
                vec![deck()],
                true,
            ),
            Self::PageSpecSet | Self::ThemeSet => (vec![deck()], vec![deck()], true),
            Self::MasterCreated { master_id }
            | Self::MasterUpdated { master_id }
            | Self::MasterDeleted { master_id } => (vec![master(master_id)], vec![deck()], true),
            Self::LayoutCreated { layout_id }
            | Self::LayoutUpdated { layout_id }
            | Self::LayoutDeleted { layout_id } => (vec![layout(layout_id)], vec![deck()], true),
            Self::SlideInserted { slide_id }
            | Self::SlideDeleted { slide_id }
            | Self::SlideMoved { slide_id } => (vec![slide(slide_id)], vec![deck()], true),
            Self::SlideDuplicated {
                source_slide_id,
                slide_id,
            } => (
                vec![slide(source_slide_id), slide(slide_id)],
                vec![deck()],
                true,
            ),
            Self::NodeInserted { slide_id, node_id }
            | Self::NodeDeleted { slide_id, node_id }
            | Self::NodeMoved { slide_id, node_id }
            | Self::NodeReordered { slide_id, node_id }
            | Self::NodeTransformSet { slide_id, node_id }
            | Self::NodeLockSet { slide_id, node_id }
            | Self::ShapeStyleSet { slide_id, node_id }
            | Self::ShapeGeometrySet { slide_id, node_id }
            | Self::ChartSpecSet { slide_id, node_id }
            | Self::ConnectorEndpointsSet { slide_id, node_id }
            | Self::TableCellContentSet {
                slide_id, node_id, ..
            }
            | Self::TableCellStyleSet {
                slide_id, node_id, ..
            }
            | Self::TableRowsInserted {
                slide_id, node_id, ..
            }
            | Self::TableColumnsInserted {
                slide_id, node_id, ..
            }
            | Self::TableRowDeleted {
                slide_id, node_id, ..
            }
            | Self::TableColumnDeleted {
                slide_id, node_id, ..
            }
            | Self::TableCellsMerged {
                slide_id, node_id, ..
            }
            | Self::TableCellSplit {
                slide_id, node_id, ..
            }
            | Self::TextContentSet { slide_id, node_id }
            | Self::TextFrameSet { slide_id, node_id }
            | Self::ImageConfigSet { slide_id, node_id }
            | Self::MediaConfigSet { slide_id, node_id } => (
                vec![slide(slide_id), node(slide_id, node_id)],
                vec![deck()],
                matches!(
                    self,
                    Self::NodeInserted { .. }
                        | Self::NodeDeleted { .. }
                        | Self::NodeMoved { .. }
                        | Self::NodeReordered { .. }
                ),
            ),
            Self::NodesAligned { slide_id, node_ids }
            | Self::NodesDistributed { slide_id, node_ids } => (
                node_ids
                    .iter()
                    .map(|node_id| node(slide_id, node_id))
                    .chain(std::iter::once(slide(slide_id)))
                    .collect(),
                vec![deck()],
                false,
            ),
            Self::NodesGrouped { slide_id, group_id }
            | Self::NodesUngrouped { slide_id, group_id } => (
                vec![slide(slide_id), node(slide_id, group_id)],
                vec![deck()],
                true,
            ),
            Self::SlideNotesSet { slide_id }
            | Self::SlideBackgroundSet { slide_id }
            | Self::SlideLayoutSet { slide_id }
            | Self::SlideTransitionSet { slide_id }
            | Self::AnimationUpserted { slide_id, .. }
            | Self::AnimationDeleted { slide_id, .. }
            | Self::AnimationMoved { slide_id, .. } => (vec![slide(slide_id)], vec![deck()], false),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationChangeSet {
    pub revision: u64,
    pub invalidation: Invalidation,
    /// Read-only cache invalidation for slide thumbnails. This is derived from
    /// the canonical DeckProjection after a successful mutation; it is never
    /// persisted in a Deck or supplied by a renderer.
    pub dirty_thumbnail_ids: Vec<String>,
    pub mutations: Vec<PresentationMutation>,
}

#[derive(Debug, Clone)]
struct NodePosition {
    node_id: String,
    parent_id: Option<String>,
    order_key: String,
}

#[derive(Debug, Clone)]
enum InverseMutation {
    RemoveAsset {
        asset_id: String,
    },
    SetPageSpec(SlidePageSpec),
    RemoveMaster {
        master_id: String,
    },
    RestoreMaster {
        master: SlideMaster,
        index: usize,
    },
    SetMaster(SlideMaster),
    RemoveLayout {
        layout_id: String,
    },
    RestoreLayout {
        layout: SlideLayout,
        index: usize,
    },
    SetLayout(SlideLayout),
    RemoveSlide {
        slide_id: String,
    },
    RestoreSlide {
        slide: Slide,
        index: usize,
    },
    MoveSlide {
        slide_id: String,
        index: usize,
    },
    RemoveNodeAndRestorePositions {
        slide_id: String,
        node_id: String,
        positions: Vec<NodePosition>,
    },
    RestoreNode {
        slide_id: String,
        node: SceneNode,
        index: usize,
    },
    RestoreNodePositions {
        slide_id: String,
        positions: Vec<NodePosition>,
    },
    UndoGroup {
        slide_id: String,
        group_id: String,
        child_positions: Vec<NodePosition>,
    },
    UndoUngroup {
        slide_id: String,
        group: SceneNode,
        index: usize,
        child_positions: Vec<NodePosition>,
    },
    SetNodeTransform {
        slide_id: String,
        node_id: String,
        transform: NodeTransform,
    },
    SetNodeLocked {
        slide_id: String,
        node_id: String,
        locked: bool,
    },
    RestoreNodeTransforms {
        slide_id: String,
        transforms: Vec<(String, NodeTransform)>,
    },
    SetShapeStyle {
        slide_id: String,
        node_id: String,
        style: ShapeStyle,
    },
    SetShapeGeometry {
        slide_id: String,
        node_id: String,
        geometry: ShapeGeometry,
    },
    SetChartSpec {
        slide_id: String,
        node_id: String,
        spec: ChartSpec,
    },
    SetConnectorEndpoints {
        slide_id: String,
        node_id: String,
        start: ConnectorEndpoint,
        end: ConnectorEndpoint,
    },
    RestoreTableCellContents {
        slide_id: String,
        node_id: String,
        cells: Vec<(TableCellAddress, PresentationRichText)>,
    },
    RestoreTableCellStyles {
        slide_id: String,
        node_id: String,
        cells: Vec<(TableCellAddress, TableCellStyle)>,
    },
    RestoreTableNode {
        slide_id: String,
        node_id: String,
        table: TableNode,
    },
    SetTextFrame {
        slide_id: String,
        node_id: String,
        frame: TextFrame,
    },
    SetImageConfig {
        slide_id: String,
        node_id: String,
        image: ImageNode,
    },
    SetMediaConfig {
        slide_id: String,
        node_id: String,
        media: MediaNode,
    },
    SetSlideNotes {
        slide_id: String,
        notes: Option<String>,
    },
    SetSlideBackground {
        slide_id: String,
        background: SlideBackground,
    },
    SetSlideLayout {
        slide_id: String,
        layout_id: Option<String>,
    },
    SetTheme(DeckTheme),
    SetSlideTransition {
        slide_id: String,
        transition: Option<SlideTransition>,
    },
    SetSlideTimeline {
        slide_id: String,
        timeline: Timeline,
    },
}

#[derive(Debug)]
struct JournalEntry {
    commands: Vec<PresentationCommand>,
    inverses: Vec<InverseMutation>,
    mutations: Vec<PresentationMutation>,
}

/// The canonical writer for v5 presentation state. History is a typed inverse-mutation journal;
/// it never serializes or clones whole decks to implement undo/redo.
#[derive(Debug)]
pub struct PresentationEngine {
    deck: Deck,
    revision: u64,
    undo: Vec<JournalEntry>,
    redo: Vec<JournalEntry>,
}

impl PresentationEngine {
    pub fn new(deck: Deck, revision: u64) -> Result<Self, PresentationEngineError> {
        deck.validate()?;
        Ok(Self {
            deck,
            revision,
            undo: Vec::new(),
            redo: Vec::new(),
        })
    }

    pub fn deck(&self) -> &Deck {
        &self.deck
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Applies a semantic batch atomically. The mutation journal holds only inverse data for the
    /// touched slide/node/deck fields. If a command or final schema validation fails, inverses are
    /// replayed before this method returns an error.
    pub fn execute(
        &mut self,
        batch: PresentationCommandBatch,
    ) -> Result<PresentationChangeSet, PresentationEngineError> {
        if batch.base_revision != self.revision {
            return Err(PresentationEngineError::RevisionConflict {
                expected: self.revision,
                actual: batch.base_revision,
            });
        }
        if batch.commands.is_empty() {
            return Err(PresentationEngineError::EmptyBatch);
        }
        let entry = self.apply_commands_atomically(batch.commands)?;
        self.redo.clear();
        self.revision += 1;
        let change_set = change_set(self.revision, &self.deck, entry.mutations.clone());
        self.undo.push(entry);
        Ok(change_set)
    }

    /// Replays inverse mutations in reverse command order. Undo itself advances revision so a
    /// caller can persist it as a normal immutable snapshot transaction.
    pub fn undo(
        &mut self,
        base_revision: u64,
    ) -> Result<PresentationChangeSet, PresentationEngineError> {
        if base_revision != self.revision {
            return Err(PresentationEngineError::RevisionConflict {
                expected: self.revision,
                actual: base_revision,
            });
        }
        let entry = self
            .undo
            .pop()
            .ok_or(PresentationEngineError::NothingToUndo)?;
        let undo_mutations = reverse_mutations(&entry.mutations);
        let mut entry = entry;
        let inverses = std::mem::take(&mut entry.inverses);
        if let Err(error) = self.apply_inverses_atomically(inverses) {
            self.undo.push(entry);
            return Err(error);
        }
        self.redo.push(entry);
        self.revision += 1;
        Ok(change_set(self.revision, &self.deck, undo_mutations))
    }

    /// Reapplies the original typed commands, not a serialized snapshot. Redo intentionally does
    /// not clear remaining redo entries, preserving normal editor history semantics.
    pub fn redo(
        &mut self,
        base_revision: u64,
    ) -> Result<PresentationChangeSet, PresentationEngineError> {
        if base_revision != self.revision {
            return Err(PresentationEngineError::RevisionConflict {
                expected: self.revision,
                actual: base_revision,
            });
        }
        let expected = self
            .redo
            .pop()
            .ok_or(PresentationEngineError::NothingToRedo)?;
        let reapplied = match self.apply_commands_atomically(expected.commands.clone()) {
            Ok(value) => value,
            Err(error) => {
                self.redo.push(expected);
                return Err(error);
            }
        };
        self.revision += 1;
        let change_set = change_set(self.revision, &self.deck, reapplied.mutations.clone());
        self.undo.push(reapplied);
        Ok(change_set)
    }

    fn apply_commands_atomically(
        &mut self,
        commands: Vec<PresentationCommand>,
    ) -> Result<JournalEntry, PresentationEngineError> {
        let mut inverses = Vec::with_capacity(commands.len());
        let mut mutations = Vec::with_capacity(commands.len());
        for command in &commands {
            match apply_command(&mut self.deck, command.clone()) {
                Ok((inverse, mutation)) => {
                    inverses.push(inverse);
                    mutations.push(mutation);
                }
                Err(error) => {
                    rollback(&mut self.deck, inverses)?;
                    return Err(error);
                }
            }
        }
        if let Err(error) = self.deck.validate() {
            rollback(&mut self.deck, inverses)?;
            return Err(error.into());
        }
        Ok(JournalEntry {
            commands,
            inverses,
            mutations,
        })
    }

    fn apply_inverses_atomically(
        &mut self,
        inverses: Vec<InverseMutation>,
    ) -> Result<(), PresentationEngineError> {
        // Inverse operations were created from a validated state. Still validate the final state
        // defensively; this guards future command additions from creating corrupt history.
        for inverse in inverses.into_iter().rev() {
            apply_inverse(&mut self.deck, inverse)?;
        }
        self.deck.validate()?;
        Ok(())
    }
}

fn change_set(
    revision: u64,
    deck: &Deck,
    mutations: Vec<PresentationMutation>,
) -> PresentationChangeSet {
    let invalidation = invalidation(&mutations);
    // DeckProjection owns the reverse-reference and rendering invalidation
    // rules. Keep thumbnail invalidation here rather than letting each UI
    // infer it from command payloads.
    let dirty_thumbnail_ids = DeckProjection::new(deck)
        .map(|projection| {
            projection
                .invalidate(&thumbnail_projection_changes(&mutations))
                .dirty_thumbnails
                .into_iter()
                .collect()
        })
        // The deck was validated by the engine before a ChangeSet is emitted.
        // Retain a safe deterministic fallback if a future invariant regresses.
        .unwrap_or_else(|_| deck.slides.iter().map(|slide| slide.id.clone()).collect());
    PresentationChangeSet {
        revision,
        invalidation,
        dirty_thumbnail_ids,
        mutations,
    }
}

fn thumbnail_projection_changes(mutations: &[PresentationMutation]) -> Vec<ProjectionChange> {
    mutations
        .iter()
        .map(|mutation| match mutation {
            // An asset by itself has no visual footprint. Its companion InsertNode mutation
            // invalidates the owning slide; this only targets existing asset consumers.
            PresentationMutation::AssetRegistered { asset_id }
            | PresentationMutation::AssetUnregistered { asset_id } => {
                ProjectionChange::AssetChanged {
                    asset_id: asset_id.clone(),
                }
            }
            PresentationMutation::PageSpecSet | PresentationMutation::ThemeSet => {
                ProjectionChange::DeckThemeChanged
            }
            PresentationMutation::MasterCreated { master_id }
            | PresentationMutation::MasterUpdated { master_id }
            | PresentationMutation::MasterDeleted { master_id } => {
                ProjectionChange::MasterChanged {
                    master_id: master_id.clone(),
                }
            }
            PresentationMutation::LayoutCreated { layout_id }
            | PresentationMutation::LayoutUpdated { layout_id }
            | PresentationMutation::LayoutDeleted { layout_id } => {
                ProjectionChange::LayoutChanged {
                    layout_id: layout_id.clone(),
                }
            }
            PresentationMutation::SlideDeleted { slide_id } => ProjectionChange::SlideRemoved {
                slide_id: slide_id.clone(),
            },
            PresentationMutation::SlideInserted { slide_id }
            | PresentationMutation::SlideMoved { slide_id }
            | PresentationMutation::SlideNotesSet { slide_id }
            | PresentationMutation::SlideBackgroundSet { slide_id }
            | PresentationMutation::SlideLayoutSet { slide_id }
            | PresentationMutation::SlideTransitionSet { slide_id } => {
                ProjectionChange::SlideChanged {
                    slide_id: slide_id.clone(),
                }
            }
            PresentationMutation::SlideDuplicated { slide_id, .. } => {
                ProjectionChange::SlideChanged {
                    slide_id: slide_id.clone(),
                }
            }
            PresentationMutation::AnimationUpserted { slide_id, .. }
            | PresentationMutation::AnimationDeleted { slide_id, .. }
            | PresentationMutation::AnimationMoved { slide_id, .. } => {
                ProjectionChange::TimelineChanged {
                    slide_id: slide_id.clone(),
                }
            }
            PresentationMutation::NodeInserted { slide_id, .. }
            | PresentationMutation::NodeDeleted { slide_id, .. }
            | PresentationMutation::NodeMoved { slide_id, .. }
            | PresentationMutation::NodeReordered { slide_id, .. }
            | PresentationMutation::NodesGrouped { slide_id, .. }
            | PresentationMutation::NodesUngrouped { slide_id, .. }
            | PresentationMutation::NodeTransformSet { slide_id, .. }
            | PresentationMutation::NodeLockSet { slide_id, .. }
            | PresentationMutation::NodesAligned { slide_id, .. }
            | PresentationMutation::NodesDistributed { slide_id, .. }
            | PresentationMutation::ShapeStyleSet { slide_id, .. }
            | PresentationMutation::ShapeGeometrySet { slide_id, .. }
            | PresentationMutation::ChartSpecSet { slide_id, .. }
            | PresentationMutation::ConnectorEndpointsSet { slide_id, .. }
            | PresentationMutation::TableCellContentSet { slide_id, .. }
            | PresentationMutation::TableCellStyleSet { slide_id, .. }
            | PresentationMutation::TableRowsInserted { slide_id, .. }
            | PresentationMutation::TableColumnsInserted { slide_id, .. }
            | PresentationMutation::TableRowDeleted { slide_id, .. }
            | PresentationMutation::TableColumnDeleted { slide_id, .. }
            | PresentationMutation::TableCellsMerged { slide_id, .. }
            | PresentationMutation::TableCellSplit { slide_id, .. }
            | PresentationMutation::TextContentSet { slide_id, .. }
            | PresentationMutation::TextFrameSet { slide_id, .. }
            | PresentationMutation::ImageConfigSet { slide_id, .. }
            | PresentationMutation::MediaConfigSet { slide_id, .. } => {
                ProjectionChange::SlideChanged {
                    slide_id: slide_id.clone(),
                }
            }
        })
        .collect()
}

fn apply_command(
    deck: &mut Deck,
    command: PresentationCommand,
) -> Result<(InverseMutation, PresentationMutation), PresentationEngineError> {
    match command {
        PresentationCommand::RegisterAsset { asset } => {
            if deck
                .assets
                .iter()
                .any(|current| current.asset_id == asset.asset_id)
            {
                return Err(PresentationEngineError::DuplicateAsset(asset.asset_id));
            }
            let asset_id = asset.asset_id.clone();
            deck.assets.push(asset);
            Ok((
                InverseMutation::RemoveAsset {
                    asset_id: asset_id.clone(),
                },
                PresentationMutation::AssetRegistered { asset_id },
            ))
        }
        PresentationCommand::SetPageSpec { page_spec } => {
            let previous = std::mem::replace(&mut deck.page_spec, page_spec);
            Ok((
                InverseMutation::SetPageSpec(previous),
                PresentationMutation::PageSpecSet,
            ))
        }
        PresentationCommand::CreateMaster { master } => {
            if deck.masters.iter().any(|current| current.id == master.id) {
                return Err(PresentationEngineError::DuplicateMaster(master.id));
            }
            let master_id = master.id.clone();
            deck.masters.push(master);
            Ok((
                InverseMutation::RemoveMaster {
                    master_id: master_id.clone(),
                },
                PresentationMutation::MasterCreated { master_id },
            ))
        }
        PresentationCommand::UpdateMaster { master } => {
            let index = master_index(deck, &master.id)?;
            let previous = std::mem::replace(&mut deck.masters[index], master);
            let master_id = previous.id.clone();
            Ok((
                InverseMutation::SetMaster(previous),
                PresentationMutation::MasterUpdated { master_id },
            ))
        }
        PresentationCommand::DeleteMaster { master_id } => {
            if deck
                .layouts
                .iter()
                .any(|layout| layout.master_id == master_id)
            {
                return Err(PresentationEngineError::MasterInUse(master_id));
            }
            let index = master_index(deck, &master_id)?;
            let master = deck.masters.remove(index);
            Ok((
                InverseMutation::RestoreMaster { master, index },
                PresentationMutation::MasterDeleted { master_id },
            ))
        }
        PresentationCommand::CreateLayout { layout } => {
            if deck.layouts.iter().any(|current| current.id == layout.id) {
                return Err(PresentationEngineError::DuplicateLayout(layout.id));
            }
            if !deck
                .masters
                .iter()
                .any(|master| master.id == layout.master_id)
            {
                return Err(PresentationEngineError::MissingMaster(layout.master_id));
            }
            let layout_id = layout.id.clone();
            deck.layouts.push(layout);
            Ok((
                InverseMutation::RemoveLayout {
                    layout_id: layout_id.clone(),
                },
                PresentationMutation::LayoutCreated { layout_id },
            ))
        }
        PresentationCommand::UpdateLayout { layout } => {
            if !deck
                .masters
                .iter()
                .any(|master| master.id == layout.master_id)
            {
                return Err(PresentationEngineError::MissingMaster(layout.master_id));
            }
            let index = layout_index(deck, &layout.id)?;
            let previous = std::mem::replace(&mut deck.layouts[index], layout);
            let layout_id = previous.id.clone();
            Ok((
                InverseMutation::SetLayout(previous),
                PresentationMutation::LayoutUpdated { layout_id },
            ))
        }
        PresentationCommand::DeleteLayout { layout_id } => {
            if deck
                .slides
                .iter()
                .any(|slide| slide.layout_id.as_deref() == Some(layout_id.as_str()))
            {
                return Err(PresentationEngineError::LayoutInUse(layout_id));
            }
            let index = layout_index(deck, &layout_id)?;
            let layout = deck.layouts.remove(index);
            Ok((
                InverseMutation::RestoreLayout { layout, index },
                PresentationMutation::LayoutDeleted { layout_id },
            ))
        }
        PresentationCommand::CreateSlide { slide, index } => {
            if deck.slides.iter().any(|value| value.id == slide.id) {
                return Err(PresentationEngineError::DuplicateSlide(slide.id));
            }
            let position = index.min(deck.slides.len());
            let id = slide.id.clone();
            deck.slides.insert(position, slide);
            Ok((
                InverseMutation::RemoveSlide {
                    slide_id: id.clone(),
                },
                PresentationMutation::SlideInserted { slide_id: id },
            ))
        }
        PresentationCommand::DeleteSlide { slide_id } => {
            let index = slide_index(deck, &slide_id)?;
            let slide = deck.slides.remove(index);
            Ok((
                InverseMutation::RestoreSlide { slide, index },
                PresentationMutation::SlideDeleted { slide_id },
            ))
        }
        PresentationCommand::MoveSlide { slide_id, index } => {
            let old_index = slide_index(deck, &slide_id)?;
            let slide = deck.slides.remove(old_index);
            let new_index = index.min(deck.slides.len());
            deck.slides.insert(new_index, slide);
            Ok((
                InverseMutation::MoveSlide {
                    slide_id: slide_id.clone(),
                    index: old_index,
                },
                PresentationMutation::SlideMoved { slide_id },
            ))
        }
        PresentationCommand::DuplicateSlide {
            source_slide_id,
            slide_id,
            order_key,
            name,
            node_id_map,
            animation_id_map,
            index,
        } => {
            if deck.slides.iter().any(|value| value.id == slide_id) {
                return Err(PresentationEngineError::DuplicateSlide(slide_id));
            }
            let source = deck.slides[slide_index(deck, &source_slide_id)?].clone();
            let mut cloned = clone_slide(
                source,
                slide_id.clone(),
                order_key,
                name,
                node_id_map,
                animation_id_map,
            )?;
            // The clone helper has rewritten all fields that carry same-slide
            // identities; retain source order keys and all asset references.
            // The target id exists only after this single insertion succeeds.
            let position = index.min(deck.slides.len());
            cloned.id = slide_id.clone();
            deck.slides.insert(position, cloned);
            Ok((
                InverseMutation::RemoveSlide {
                    slide_id: slide_id.clone(),
                },
                PresentationMutation::SlideDuplicated {
                    source_slide_id,
                    slide_id,
                },
            ))
        }
        PresentationCommand::InsertNode {
            slide_id,
            node,
            index,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            if slide.nodes.iter().any(|value| value.id == node.id) {
                return Err(PresentationEngineError::DuplicateNode(node.id));
            }
            validate_parent_target(slide, node.parent_id.as_deref(), &node.id)?;
            let position = index.min(slide.nodes.len());
            let id = node.id.clone();
            let parent_id = node.parent_id.clone();
            let positions = capture_positions(slide, &[], parent_id.as_deref());
            slide.nodes.insert(position, node);
            reorder_within_parent(slide, &id, parent_id.as_deref(), index)?;
            Ok((
                InverseMutation::RemoveNodeAndRestorePositions {
                    slide_id: slide_id.clone(),
                    node_id: id.clone(),
                    positions,
                },
                PresentationMutation::NodeInserted {
                    slide_id,
                    node_id: id,
                },
            ))
        }
        PresentationCommand::DeleteNode { slide_id, node_id } => {
            let slide = slide_mut(deck, &slide_id)?;
            if slide
                .nodes
                .iter()
                .any(|node| node.parent_id.as_deref() == Some(node_id.as_str()))
            {
                return Err(PresentationEngineError::HasChildren(node_id));
            }
            let index = node_index(slide, &node_id)?;
            let node = slide.nodes.remove(index);
            Ok((
                InverseMutation::RestoreNode {
                    slide_id: slide_id.clone(),
                    node,
                    index,
                },
                PresentationMutation::NodeDeleted { slide_id, node_id },
            ))
        }
        PresentationCommand::MoveNode {
            slide_id,
            node_id,
            parent_id,
            index,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let positions = capture_positions(slide, &[node_id.as_str()], parent_id.as_deref());
            move_node(slide, &node_id, parent_id, index)?;
            Ok((
                InverseMutation::RestoreNodePositions {
                    slide_id: slide_id.clone(),
                    positions,
                },
                PresentationMutation::NodeMoved { slide_id, node_id },
            ))
        }
        PresentationCommand::ReorderNode {
            slide_id,
            node_id,
            index,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let parent_id = slide.nodes[node_index(slide, &node_id)?].parent_id.clone();
            let positions = capture_positions(slide, &[node_id.as_str()], parent_id.as_deref());
            reorder_within_parent(slide, &node_id, parent_id.as_deref(), index)?;
            Ok((
                InverseMutation::RestoreNodePositions {
                    slide_id: slide_id.clone(),
                    positions,
                },
                PresentationMutation::NodeReordered { slide_id, node_id },
            ))
        }
        PresentationCommand::GroupNodes {
            slide_id,
            group,
            child_ids,
            index,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            if !matches!(&group.kind, SceneNodeKind::Group(_)) {
                return Err(PresentationEngineError::NotGroupNode(group.id));
            }
            if child_ids.len() < 2 {
                return Err(PresentationEngineError::GroupRequiresTwoNodes);
            }
            if slide.nodes.iter().any(|node| node.id == group.id) {
                return Err(PresentationEngineError::DuplicateNode(group.id));
            }
            let exact_child_positions = capture_exact_positions(slide, &child_ids)?;
            let shared_parent = exact_child_positions
                .first()
                .and_then(|entry| entry.parent_id.clone());
            if exact_child_positions
                .iter()
                .any(|entry| entry.parent_id != shared_parent)
            {
                return Err(PresentationEngineError::GroupChildrenMustShareParent);
            }
            validate_parent_target(slide, group.parent_id.as_deref(), &group.id)?;
            if group.parent_id != shared_parent {
                return Err(PresentationEngineError::GroupParentMustMatchChildren);
            }
            let child_refs: Vec<&str> = child_ids.iter().map(String::as_str).collect();
            let child_positions = capture_positions(slide, &child_refs, shared_parent.as_deref());
            let group_id = group.id.clone();
            slide.nodes.push(group);
            for child_id in &child_ids {
                let child_index = node_index(slide, child_id)?;
                slide.nodes[child_index].parent_id = Some(group_id.clone());
            }
            reorder_within_parent(slide, &group_id, shared_parent.as_deref(), index)?;
            normalize_sibling_order(slide, Some(group_id.as_str()));
            Ok((
                InverseMutation::UndoGroup {
                    slide_id: slide_id.clone(),
                    group_id: group_id.clone(),
                    child_positions,
                },
                PresentationMutation::NodesGrouped { slide_id, group_id },
            ))
        }
        PresentationCommand::UngroupNodes { slide_id, group_id } => {
            let slide = slide_mut(deck, &slide_id)?;
            let group_index = node_index(slide, &group_id)?;
            let group = slide.nodes[group_index].clone();
            if !matches!(&group.kind, SceneNodeKind::Group(_)) {
                return Err(PresentationEngineError::NotGroupNode(group_id));
            }
            let child_ids: Vec<String> = slide
                .nodes
                .iter()
                .filter(|node| node.parent_id.as_deref() == Some(group_id.as_str()))
                .map(|node| node.id.clone())
                .collect();
            if child_ids.is_empty() {
                return Err(PresentationEngineError::EmptyGroup(group_id));
            }
            let parent_id = group.parent_id.clone();
            let child_refs: Vec<&str> = child_ids.iter().map(String::as_str).collect();
            let child_positions = capture_positions(slide, &child_refs, parent_id.as_deref());
            slide.nodes.remove(group_index);
            for child_id in &child_ids {
                let child_index = node_index(slide, child_id)?;
                slide.nodes[child_index].parent_id = parent_id.clone();
            }
            normalize_sibling_order(slide, parent_id.as_deref());
            Ok((
                InverseMutation::UndoUngroup {
                    slide_id: slide_id.clone(),
                    group,
                    index: group_index,
                    child_positions,
                },
                PresentationMutation::NodesUngrouped { slide_id, group_id },
            ))
        }
        PresentationCommand::SetNodeTransform {
            slide_id,
            node_id,
            transform,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let index = node_index(slide, &node_id)?;
            let previous = std::mem::replace(&mut slide.nodes[index].transform, transform);
            Ok((
                InverseMutation::SetNodeTransform {
                    slide_id: slide_id.clone(),
                    node_id: node_id.clone(),
                    transform: previous,
                },
                PresentationMutation::NodeTransformSet { slide_id, node_id },
            ))
        }
        PresentationCommand::SetNodeLocked {
            slide_id,
            node_id,
            locked,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let index = node_index(slide, &node_id)?;
            let previous = std::mem::replace(&mut slide.nodes[index].locked, locked);
            Ok((
                InverseMutation::SetNodeLocked {
                    slide_id: slide_id.clone(),
                    node_id: node_id.clone(),
                    locked: previous,
                },
                PresentationMutation::NodeLockSet { slide_id, node_id },
            ))
        }
        PresentationCommand::AlignNodes {
            slide_id,
            node_ids,
            alignment,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let previous = align_nodes(slide, &node_ids, alignment)?;
            Ok((
                InverseMutation::RestoreNodeTransforms {
                    slide_id: slide_id.clone(),
                    transforms: previous,
                },
                PresentationMutation::NodesAligned { slide_id, node_ids },
            ))
        }
        PresentationCommand::DistributeNodes {
            slide_id,
            node_ids,
            axis,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let previous = distribute_nodes(slide, &node_ids, axis)?;
            Ok((
                InverseMutation::RestoreNodeTransforms {
                    slide_id: slide_id.clone(),
                    transforms: previous,
                },
                PresentationMutation::NodesDistributed { slide_id, node_ids },
            ))
        }
        PresentationCommand::SetShapeStyle {
            slide_id,
            node_id,
            style,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let index = node_index(slide, &node_id)?;
            let SceneNodeKind::Shape(shape) = &mut slide.nodes[index].kind else {
                return Err(PresentationEngineError::NotShapeNode(node_id));
            };
            let previous = std::mem::replace(&mut shape.style, style);
            Ok((
                InverseMutation::SetShapeStyle {
                    slide_id: slide_id.clone(),
                    node_id: node_id.clone(),
                    style: previous,
                },
                PresentationMutation::ShapeStyleSet { slide_id, node_id },
            ))
        }
        PresentationCommand::SetShapeGeometry {
            slide_id,
            node_id,
            geometry,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let index = node_index(slide, &node_id)?;
            let SceneNodeKind::Shape(shape) = &mut slide.nodes[index].kind else {
                return Err(PresentationEngineError::NotShapeNode(node_id));
            };
            let previous = std::mem::replace(&mut shape.geometry, geometry);
            Ok((
                InverseMutation::SetShapeGeometry {
                    slide_id: slide_id.clone(),
                    node_id: node_id.clone(),
                    geometry: previous,
                },
                PresentationMutation::ShapeGeometrySet { slide_id, node_id },
            ))
        }
        PresentationCommand::SetChartSpec {
            slide_id,
            node_id,
            spec,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let index = node_index(slide, &node_id)?;
            let SceneNodeKind::Chart(chart) = &mut slide.nodes[index].kind else {
                return Err(PresentationEngineError::NotChartNode(node_id));
            };
            let previous = std::mem::replace(&mut chart.spec, spec);
            Ok((
                InverseMutation::SetChartSpec {
                    slide_id: slide_id.clone(),
                    node_id: node_id.clone(),
                    spec: previous,
                },
                PresentationMutation::ChartSpecSet { slide_id, node_id },
            ))
        }
        PresentationCommand::SetConnectorEndpoints {
            slide_id,
            node_id,
            start,
            end,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            validate_connector_endpoints(slide, &node_id, &start, &end)?;
            let index = node_index(slide, &node_id)?;
            let SceneNodeKind::Connector(connector) = &mut slide.nodes[index].kind else {
                return Err(PresentationEngineError::NotConnectorNode(node_id));
            };
            let previous_start = std::mem::replace(&mut connector.start, start);
            let previous_end = std::mem::replace(&mut connector.end, end);
            Ok((
                InverseMutation::SetConnectorEndpoints {
                    slide_id: slide_id.clone(),
                    node_id: node_id.clone(),
                    start: previous_start,
                    end: previous_end,
                },
                PresentationMutation::ConnectorEndpointsSet { slide_id, node_id },
            ))
        }
        PresentationCommand::SetTableCellContent {
            slide_id,
            node_id,
            row,
            column,
            content,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let table = table_mut(slide, &node_id)?;
            let index = table_cell_index(table, row, column, &node_id)?;
            let previous = std::mem::replace(&mut table.cells[index].content, content);
            Ok((
                InverseMutation::RestoreTableCellContents {
                    slide_id: slide_id.clone(),
                    node_id: node_id.clone(),
                    cells: vec![(TableCellAddress { row, column }, previous)],
                },
                PresentationMutation::TableCellContentSet {
                    slide_id,
                    node_id,
                    row,
                    column,
                },
            ))
        }
        PresentationCommand::SetTableCellStyle {
            slide_id,
            node_id,
            cells,
            style,
        } => {
            if cells.is_empty() {
                return Err(PresentationEngineError::EmptyTableCellSelection);
            }
            let slide = slide_mut(deck, &slide_id)?;
            let table = table_mut(slide, &node_id)?;
            let mut seen = std::collections::HashSet::new();
            let mut previous = Vec::with_capacity(cells.len());
            for address in &cells {
                if !seen.insert((address.row, address.column)) {
                    return Err(PresentationEngineError::DuplicateTableCellAddress {
                        row: address.row,
                        column: address.column,
                    });
                }
                let index = table_cell_index(table, address.row, address.column, &node_id)?;
                previous.push((
                    address.clone(),
                    std::mem::replace(&mut table.cells[index].style, style.clone()),
                ));
            }
            Ok((
                InverseMutation::RestoreTableCellStyles {
                    slide_id: slide_id.clone(),
                    node_id: node_id.clone(),
                    cells: previous,
                },
                PresentationMutation::TableCellStyleSet {
                    slide_id,
                    node_id,
                    cells,
                },
            ))
        }
        PresentationCommand::InsertTableRows {
            slide_id,
            node_id,
            index,
            count,
        } => {
            let table = table_mut(slide_mut(deck, &slide_id)?, &node_id)?;
            validate_table_insert(index, count, table.rows, "row")?;
            let previous = table.clone();
            insert_table_rows(table, index, count);
            Ok((
                InverseMutation::RestoreTableNode {
                    slide_id: slide_id.clone(),
                    node_id: node_id.clone(),
                    table: previous,
                },
                PresentationMutation::TableRowsInserted {
                    slide_id,
                    node_id,
                    index,
                    count,
                },
            ))
        }
        PresentationCommand::InsertTableColumns {
            slide_id,
            node_id,
            index,
            count,
        } => {
            let table = table_mut(slide_mut(deck, &slide_id)?, &node_id)?;
            validate_table_insert(index, count, table.columns, "column")?;
            let previous = table.clone();
            insert_table_columns(table, index, count);
            Ok((
                InverseMutation::RestoreTableNode {
                    slide_id: slide_id.clone(),
                    node_id: node_id.clone(),
                    table: previous,
                },
                PresentationMutation::TableColumnsInserted {
                    slide_id,
                    node_id,
                    index,
                    count,
                },
            ))
        }
        PresentationCommand::DeleteTableRow {
            slide_id,
            node_id,
            index,
        } => {
            let table = table_mut(slide_mut(deck, &slide_id)?, &node_id)?;
            validate_table_delete(index, table.rows, "row")?;
            let previous = table.clone();
            delete_table_row(table, index);
            Ok((
                InverseMutation::RestoreTableNode {
                    slide_id: slide_id.clone(),
                    node_id: node_id.clone(),
                    table: previous,
                },
                PresentationMutation::TableRowDeleted {
                    slide_id,
                    node_id,
                    index,
                },
            ))
        }
        PresentationCommand::DeleteTableColumn {
            slide_id,
            node_id,
            index,
        } => {
            let table = table_mut(slide_mut(deck, &slide_id)?, &node_id)?;
            validate_table_delete(index, table.columns, "column")?;
            let previous = table.clone();
            delete_table_column(table, index);
            Ok((
                InverseMutation::RestoreTableNode {
                    slide_id: slide_id.clone(),
                    node_id: node_id.clone(),
                    table: previous,
                },
                PresentationMutation::TableColumnDeleted {
                    slide_id,
                    node_id,
                    index,
                },
            ))
        }
        PresentationCommand::MergeTableCells {
            slide_id,
            node_id,
            start,
            end,
        } => {
            let table = table_mut(slide_mut(deck, &slide_id)?, &node_id)?;
            let previous = table.clone();
            merge_table_cells(table, &start, &end, &node_id)?;
            Ok((
                InverseMutation::RestoreTableNode {
                    slide_id: slide_id.clone(),
                    node_id: node_id.clone(),
                    table: previous,
                },
                PresentationMutation::TableCellsMerged {
                    slide_id,
                    node_id,
                    start,
                    end,
                },
            ))
        }
        PresentationCommand::SplitTableCell {
            slide_id,
            node_id,
            row,
            column,
        } => {
            let table = table_mut(slide_mut(deck, &slide_id)?, &node_id)?;
            let previous = table.clone();
            split_table_cell(table, row, column, &node_id)?;
            Ok((
                InverseMutation::RestoreTableNode {
                    slide_id: slide_id.clone(),
                    node_id: node_id.clone(),
                    table: previous,
                },
                PresentationMutation::TableCellSplit {
                    slide_id,
                    node_id,
                    row,
                    column,
                },
            ))
        }
        PresentationCommand::SetTextContent {
            slide_id,
            node_id,
            body,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let index = node_index(slide, &node_id)?;
            let SceneNodeKind::Text(text) = &mut slide.nodes[index].kind else {
                return Err(PresentationEngineError::NotTextNode(node_id));
            };
            let previous = std::mem::replace(&mut text.frame.body, body);
            Ok((
                InverseMutation::SetTextFrame {
                    slide_id: slide_id.clone(),
                    node_id: node_id.clone(),
                    frame: TextFrame {
                        body: previous,
                        vertical_align: text.frame.vertical_align,
                        padding: text.frame.padding.clone(),
                        auto_fit: text.frame.auto_fit,
                    },
                },
                PresentationMutation::TextContentSet { slide_id, node_id },
            ))
        }
        PresentationCommand::SetTextFrame {
            slide_id,
            node_id,
            frame,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let index = node_index(slide, &node_id)?;
            let SceneNodeKind::Text(text) = &mut slide.nodes[index].kind else {
                return Err(PresentationEngineError::NotTextNode(node_id));
            };
            let previous = std::mem::replace(&mut text.frame, frame);
            Ok((
                InverseMutation::SetTextFrame {
                    slide_id: slide_id.clone(),
                    node_id: node_id.clone(),
                    frame: previous,
                },
                PresentationMutation::TextFrameSet { slide_id, node_id },
            ))
        }
        PresentationCommand::SetImageConfig {
            slide_id,
            node_id,
            image,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let index = node_index(slide, &node_id)?;
            let SceneNodeKind::Image(current) = &mut slide.nodes[index].kind else {
                return Err(PresentationEngineError::NotImageNode(node_id));
            };
            let previous = std::mem::replace(current, image);
            Ok((
                InverseMutation::SetImageConfig {
                    slide_id: slide_id.clone(),
                    node_id: node_id.clone(),
                    image: previous,
                },
                PresentationMutation::ImageConfigSet { slide_id, node_id },
            ))
        }
        PresentationCommand::SetMediaConfig {
            slide_id,
            node_id,
            media,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let index = node_index(slide, &node_id)?;
            let current = match &mut slide.nodes[index].kind {
                SceneNodeKind::Video(current) | SceneNodeKind::Audio(current) => current,
                _ => return Err(PresentationEngineError::NotMediaNode(node_id)),
            };
            let previous = std::mem::replace(current, media);
            Ok((
                InverseMutation::SetMediaConfig {
                    slide_id: slide_id.clone(),
                    node_id: node_id.clone(),
                    media: previous,
                },
                PresentationMutation::MediaConfigSet { slide_id, node_id },
            ))
        }
        PresentationCommand::SetSlideNotes { slide_id, notes } => {
            let previous = std::mem::replace(&mut slide_mut(deck, &slide_id)?.notes, notes);
            Ok((
                InverseMutation::SetSlideNotes {
                    slide_id: slide_id.clone(),
                    notes: previous,
                },
                PresentationMutation::SlideNotesSet { slide_id },
            ))
        }
        PresentationCommand::SetSlideBackground {
            slide_id,
            background,
        } => {
            let previous =
                std::mem::replace(&mut slide_mut(deck, &slide_id)?.background, background);
            Ok((
                InverseMutation::SetSlideBackground {
                    slide_id: slide_id.clone(),
                    background: previous,
                },
                PresentationMutation::SlideBackgroundSet { slide_id },
            ))
        }
        PresentationCommand::SetSlideLayout {
            slide_id,
            layout_id,
        } => {
            let previous = std::mem::replace(&mut slide_mut(deck, &slide_id)?.layout_id, layout_id);
            Ok((
                InverseMutation::SetSlideLayout {
                    slide_id: slide_id.clone(),
                    layout_id: previous,
                },
                PresentationMutation::SlideLayoutSet { slide_id },
            ))
        }
        PresentationCommand::SetTheme { theme } => {
            let previous = std::mem::replace(&mut deck.theme, theme);
            Ok((
                InverseMutation::SetTheme(previous),
                PresentationMutation::ThemeSet,
            ))
        }
        PresentationCommand::SetSlideTransition {
            slide_id,
            transition,
        } => {
            let previous =
                std::mem::replace(&mut slide_mut(deck, &slide_id)?.transition, transition);
            Ok((
                InverseMutation::SetSlideTransition {
                    slide_id: slide_id.clone(),
                    transition: previous,
                },
                PresentationMutation::SlideTransitionSet { slide_id },
            ))
        }
        PresentationCommand::UpsertAnimation {
            slide_id,
            animation,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let previous = slide.timeline.clone();
            if let Some(index) = slide
                .timeline
                .entries
                .iter()
                .position(|entry| entry.id == animation.id)
            {
                slide.timeline.entries[index] = animation.clone();
            } else {
                slide.timeline.entries.push(animation.clone());
            }
            Ok((
                InverseMutation::SetSlideTimeline {
                    slide_id: slide_id.clone(),
                    timeline: previous,
                },
                PresentationMutation::AnimationUpserted {
                    slide_id,
                    animation_id: animation.id,
                },
            ))
        }
        PresentationCommand::DeleteAnimation {
            slide_id,
            animation_id,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let previous = slide.timeline.clone();
            let Some(index) = slide
                .timeline
                .entries
                .iter()
                .position(|entry| entry.id == animation_id)
            else {
                return Err(PresentationEngineError::MissingAnimation(animation_id));
            };
            slide.timeline.entries.remove(index);
            Ok((
                InverseMutation::SetSlideTimeline {
                    slide_id: slide_id.clone(),
                    timeline: previous,
                },
                PresentationMutation::AnimationDeleted {
                    slide_id,
                    animation_id,
                },
            ))
        }
        PresentationCommand::MoveAnimation {
            slide_id,
            animation_id,
            index,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let previous = slide.timeline.clone();
            let Some(current) = slide
                .timeline
                .entries
                .iter()
                .position(|entry| entry.id == animation_id)
            else {
                return Err(PresentationEngineError::MissingAnimation(animation_id));
            };
            let animation = slide.timeline.entries.remove(current);
            slide
                .timeline
                .entries
                .insert(index.min(slide.timeline.entries.len()), animation);
            normalize_timeline_order(&mut slide.timeline);
            Ok((
                InverseMutation::SetSlideTimeline {
                    slide_id: slide_id.clone(),
                    timeline: previous,
                },
                PresentationMutation::AnimationMoved {
                    slide_id,
                    animation_id,
                },
            ))
        }
    }
}

/// Creates a target slide only from canonical source data.  The mapping is
/// deliberately exhaustive: a clone cannot accidentally retain references to
/// a source-slide node or animation id.
fn clone_slide(
    mut source: Slide,
    slide_id: String,
    order_key: String,
    name: String,
    node_id_map: Vec<PresentationIdMapping>,
    animation_id_map: Vec<PresentationIdMapping>,
) -> Result<Slide, PresentationEngineError> {
    let node_ids = validate_duplicate_id_map(
        "node",
        source.nodes.iter().map(|node| node.id.as_str()),
        node_id_map,
    )?;
    let animation_ids = validate_duplicate_id_map(
        "animation",
        source
            .timeline
            .entries
            .iter()
            .map(|entry| entry.id.as_str()),
        animation_id_map,
    )?;

    for node in &mut source.nodes {
        let previous_id = node.id.clone();
        node.id = node_ids[&previous_id].clone();
        node.parent_id = node
            .parent_id
            .as_ref()
            .map(|parent_id| node_ids[parent_id].clone());
        if let SceneNodeKind::Connector(connector) = &mut node.kind {
            rewrite_connector_endpoint(&mut connector.start, &node_ids);
            rewrite_connector_endpoint(&mut connector.end, &node_ids);
        }
    }
    for entry in &mut source.timeline.entries {
        let previous_id = entry.id.clone();
        entry.id = animation_ids[&previous_id].clone();
        entry.target_node_id = node_ids[&entry.target_node_id].clone();
    }

    source.id = slide_id;
    source.order_key = order_key;
    source.name = name;
    Ok(source)
}

fn rewrite_connector_endpoint(
    endpoint: &mut ConnectorEndpoint,
    node_ids: &BTreeMap<String, String>,
) {
    if let ConnectorEndpoint::Node { node_id, .. } = endpoint {
        *node_id = node_ids[node_id].clone();
    }
}

fn validate_duplicate_id_map<'a>(
    kind: &str,
    expected_ids: impl IntoIterator<Item = &'a str>,
    mappings: Vec<PresentationIdMapping>,
) -> Result<BTreeMap<String, String>, PresentationEngineError> {
    let expected = expected_ids
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let mut result = BTreeMap::new();
    let mut target_ids = BTreeSet::new();
    for mapping in mappings {
        if mapping.source_id.is_empty() || mapping.target_id.is_empty() {
            return Err(PresentationEngineError::InvalidDuplicateSlideMapping(
                format!("{kind} 映射 id 不能为空"),
            ));
        }
        if !expected.contains(&mapping.source_id) {
            return Err(PresentationEngineError::InvalidDuplicateSlideMapping(
                format!("{kind} 映射包含不存在的 sourceId：{}", mapping.source_id),
            ));
        }
        if result
            .insert(mapping.source_id.clone(), mapping.target_id.clone())
            .is_some()
        {
            return Err(PresentationEngineError::InvalidDuplicateSlideMapping(
                format!("{kind} 映射重复 sourceId：{}", mapping.source_id),
            ));
        }
        if !target_ids.insert(mapping.target_id.clone()) {
            return Err(PresentationEngineError::InvalidDuplicateSlideMapping(
                format!("{kind} 映射重复 targetId：{}", mapping.target_id),
            ));
        }
    }
    if result.keys().collect::<BTreeSet<_>>() != expected.iter().collect::<BTreeSet<_>>() {
        return Err(PresentationEngineError::InvalidDuplicateSlideMapping(
            format!("{kind} 映射必须覆盖 source 中全部 id"),
        ));
    }
    if target_ids
        .iter()
        .any(|target_id| expected.contains(target_id))
    {
        return Err(PresentationEngineError::InvalidDuplicateSlideMapping(
            format!("{kind} 映射 targetId 不能复用 source id"),
        ));
    }
    Ok(result)
}

fn apply_inverse(deck: &mut Deck, inverse: InverseMutation) -> Result<(), PresentationEngineError> {
    match inverse {
        InverseMutation::RemoveAsset { asset_id } => {
            let index = deck
                .assets
                .iter()
                .position(|asset| asset.asset_id == asset_id)
                .ok_or_else(|| PresentationEngineError::MissingAsset(asset_id.clone()))?;
            deck.assets.remove(index);
        }
        InverseMutation::SetPageSpec(page_spec) => deck.page_spec = page_spec,
        InverseMutation::RemoveMaster { master_id } => {
            deck.masters.remove(master_index(deck, &master_id)?);
        }
        InverseMutation::RestoreMaster { master, index } => {
            deck.masters.insert(index.min(deck.masters.len()), master);
        }
        InverseMutation::SetMaster(master) => {
            let index = master_index(deck, &master.id)?;
            deck.masters[index] = master;
        }
        InverseMutation::RemoveLayout { layout_id } => {
            deck.layouts.remove(layout_index(deck, &layout_id)?);
        }
        InverseMutation::RestoreLayout { layout, index } => {
            deck.layouts.insert(index.min(deck.layouts.len()), layout);
        }
        InverseMutation::SetLayout(layout) => {
            let index = layout_index(deck, &layout.id)?;
            deck.layouts[index] = layout;
        }
        InverseMutation::RemoveSlide { slide_id } => {
            deck.slides.remove(slide_index(deck, &slide_id)?);
        }
        InverseMutation::RestoreSlide { slide, index } => {
            deck.slides.insert(index.min(deck.slides.len()), slide)
        }
        InverseMutation::MoveSlide { slide_id, index } => {
            let current = slide_index(deck, &slide_id)?;
            let slide = deck.slides.remove(current);
            deck.slides.insert(index.min(deck.slides.len()), slide);
        }
        InverseMutation::RemoveNodeAndRestorePositions {
            slide_id,
            node_id,
            positions,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            slide.nodes.remove(node_index(slide, &node_id)?);
            restore_positions(slide, &positions)?;
        }
        InverseMutation::RestoreNode {
            slide_id,
            node,
            index,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let insertion = index.min(slide.nodes.len());
            slide.nodes.insert(insertion, node);
        }
        InverseMutation::RestoreNodePositions {
            slide_id,
            positions,
        } => restore_positions(slide_mut(deck, &slide_id)?, &positions)?,
        InverseMutation::UndoGroup {
            slide_id,
            group_id,
            child_positions,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            slide.nodes.remove(node_index(slide, &group_id)?);
            restore_positions(slide, &child_positions)?;
        }
        InverseMutation::UndoUngroup {
            slide_id,
            group,
            index,
            child_positions,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            slide.nodes.insert(index.min(slide.nodes.len()), group);
            restore_positions(slide, &child_positions)?;
        }
        InverseMutation::SetNodeTransform {
            slide_id,
            node_id,
            transform,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let index = node_index(slide, &node_id)?;
            slide.nodes[index].transform = transform;
        }
        InverseMutation::SetNodeLocked {
            slide_id,
            node_id,
            locked,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let index = node_index(slide, &node_id)?;
            slide.nodes[index].locked = locked;
        }
        InverseMutation::RestoreNodeTransforms {
            slide_id,
            transforms,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            for (node_id, transform) in transforms {
                let index = node_index(slide, &node_id)?;
                slide.nodes[index].transform = transform;
            }
        }
        InverseMutation::SetShapeStyle {
            slide_id,
            node_id,
            style,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let index = node_index(slide, &node_id)?;
            let SceneNodeKind::Shape(shape) = &mut slide.nodes[index].kind else {
                return Err(PresentationEngineError::NotShapeNode(node_id));
            };
            shape.style = style;
        }
        InverseMutation::SetShapeGeometry {
            slide_id,
            node_id,
            geometry,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let index = node_index(slide, &node_id)?;
            let SceneNodeKind::Shape(shape) = &mut slide.nodes[index].kind else {
                return Err(PresentationEngineError::NotShapeNode(node_id));
            };
            shape.geometry = geometry;
        }
        InverseMutation::SetChartSpec {
            slide_id,
            node_id,
            spec,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let index = node_index(slide, &node_id)?;
            let SceneNodeKind::Chart(chart) = &mut slide.nodes[index].kind else {
                return Err(PresentationEngineError::NotChartNode(node_id));
            };
            chart.spec = spec;
        }
        InverseMutation::SetConnectorEndpoints {
            slide_id,
            node_id,
            start,
            end,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let index = node_index(slide, &node_id)?;
            let SceneNodeKind::Connector(connector) = &mut slide.nodes[index].kind else {
                return Err(PresentationEngineError::NotConnectorNode(node_id));
            };
            connector.start = start;
            connector.end = end;
        }
        InverseMutation::RestoreTableCellContents {
            slide_id,
            node_id,
            cells,
        } => {
            let table = table_mut(slide_mut(deck, &slide_id)?, &node_id)?;
            for (address, content) in cells {
                let index = table_cell_index(table, address.row, address.column, &node_id)?;
                table.cells[index].content = content;
            }
        }
        InverseMutation::RestoreTableCellStyles {
            slide_id,
            node_id,
            cells,
        } => {
            let table = table_mut(slide_mut(deck, &slide_id)?, &node_id)?;
            for (address, style) in cells {
                let index = table_cell_index(table, address.row, address.column, &node_id)?;
                table.cells[index].style = style;
            }
        }
        InverseMutation::RestoreTableNode {
            slide_id,
            node_id,
            table,
        } => {
            *table_mut(slide_mut(deck, &slide_id)?, &node_id)? = table;
        }
        InverseMutation::SetTextFrame {
            slide_id,
            node_id,
            frame,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let index = node_index(slide, &node_id)?;
            let SceneNodeKind::Text(text) = &mut slide.nodes[index].kind else {
                return Err(PresentationEngineError::NotTextNode(node_id));
            };
            text.frame = frame;
        }
        InverseMutation::SetImageConfig {
            slide_id,
            node_id,
            image,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let index = node_index(slide, &node_id)?;
            let SceneNodeKind::Image(current) = &mut slide.nodes[index].kind else {
                return Err(PresentationEngineError::NotImageNode(node_id));
            };
            *current = image;
        }
        InverseMutation::SetMediaConfig {
            slide_id,
            node_id,
            media,
        } => {
            let slide = slide_mut(deck, &slide_id)?;
            let index = node_index(slide, &node_id)?;
            let current = match &mut slide.nodes[index].kind {
                SceneNodeKind::Video(current) | SceneNodeKind::Audio(current) => current,
                _ => return Err(PresentationEngineError::NotMediaNode(node_id)),
            };
            *current = media;
        }
        InverseMutation::SetSlideNotes { slide_id, notes } => {
            slide_mut(deck, &slide_id)?.notes = notes
        }
        InverseMutation::SetSlideBackground {
            slide_id,
            background,
        } => slide_mut(deck, &slide_id)?.background = background,
        InverseMutation::SetSlideLayout {
            slide_id,
            layout_id,
        } => slide_mut(deck, &slide_id)?.layout_id = layout_id,
        InverseMutation::SetTheme(theme) => deck.theme = theme,
        InverseMutation::SetSlideTransition {
            slide_id,
            transition,
        } => slide_mut(deck, &slide_id)?.transition = transition,
        InverseMutation::SetSlideTimeline { slide_id, timeline } => {
            slide_mut(deck, &slide_id)?.timeline = timeline
        }
    }
    Ok(())
}

fn rollback(
    deck: &mut Deck,
    inverses: Vec<InverseMutation>,
) -> Result<(), PresentationEngineError> {
    for inverse in inverses.into_iter().rev() {
        apply_inverse(deck, inverse)?;
    }
    Ok(())
}

fn slide_index(deck: &Deck, id: &str) -> Result<usize, PresentationEngineError> {
    deck.slides
        .iter()
        .position(|slide| slide.id == id)
        .ok_or_else(|| PresentationEngineError::MissingSlide(id.into()))
}
fn master_index(deck: &Deck, id: &str) -> Result<usize, PresentationEngineError> {
    deck.masters
        .iter()
        .position(|master| master.id == id)
        .ok_or_else(|| PresentationEngineError::MissingMaster(id.into()))
}
fn layout_index(deck: &Deck, id: &str) -> Result<usize, PresentationEngineError> {
    deck.layouts
        .iter()
        .position(|layout| layout.id == id)
        .ok_or_else(|| PresentationEngineError::MissingLayout(id.into()))
}
fn slide_mut<'a>(deck: &'a mut Deck, id: &str) -> Result<&'a mut Slide, PresentationEngineError> {
    let index = slide_index(deck, id)?;
    Ok(&mut deck.slides[index])
}
fn node_index(slide: &Slide, id: &str) -> Result<usize, PresentationEngineError> {
    slide
        .nodes
        .iter()
        .position(|node| node.id == id)
        .ok_or_else(|| PresentationEngineError::MissingNode(id.into()))
}

fn table_mut<'a>(
    slide: &'a mut Slide,
    node_id: &str,
) -> Result<&'a mut TableNode, PresentationEngineError> {
    let index = node_index(slide, node_id)?;
    let SceneNodeKind::Table(table) = &mut slide.nodes[index].kind else {
        return Err(PresentationEngineError::NotTableNode(node_id.into()));
    };
    Ok(table)
}

/// Resolves an anchor only. A coordinate covered by a merged cell but not its
/// anchor is intentionally rejected, forcing callers to preserve canonical
/// merged-cell identity instead of guessing from visual grid coordinates.
fn table_cell_index(
    table: &TableNode,
    row: u32,
    column: u32,
    node_id: &str,
) -> Result<usize, PresentationEngineError> {
    table
        .cells
        .iter()
        .position(|cell| cell.row == row && cell.column == column)
        .ok_or_else(|| PresentationEngineError::MissingTableCell {
            node_id: node_id.into(),
            row,
            column,
        })
}

fn validate_table_insert(
    index: u32,
    count: u32,
    extent: u32,
    axis: &str,
) -> Result<(), PresentationEngineError> {
    if count == 0 || index > extent {
        return Err(PresentationEngineError::InvalidTableStructure(format!(
            "{axis} insert index/count 无效"
        )));
    }
    Ok(())
}

fn validate_table_delete(
    index: u32,
    extent: u32,
    axis: &str,
) -> Result<(), PresentationEngineError> {
    if extent <= 1 || index >= extent {
        return Err(PresentationEngineError::InvalidTableStructure(format!(
            "{axis} delete index 无效或会删除最后一个 {axis}"
        )));
    }
    Ok(())
}

fn canonicalize_table_cells(table: &mut TableNode) {
    let mut occupied = std::collections::HashSet::new();
    for cell in &table.cells {
        for row in cell.row..cell.row + cell.row_span {
            for column in cell.column..cell.column + cell.column_span {
                occupied.insert((row, column));
            }
        }
    }
    for row in 0..table.rows {
        for column in 0..table.columns {
            if !occupied.contains(&(row, column)) {
                table.cells.push(TableCell {
                    row,
                    column,
                    row_span: 1,
                    column_span: 1,
                    content: PresentationRichText::default(),
                    style: TableCellStyle::default(),
                });
            }
        }
    }
    table.cells.sort_by_key(|cell| (cell.row, cell.column));
}

fn insert_table_rows(table: &mut TableNode, index: u32, count: u32) {
    for cell in &mut table.cells {
        if cell.row >= index {
            cell.row += count;
        } else if index < cell.row + cell.row_span {
            cell.row_span += count;
        }
    }
    table.rows += count;
    canonicalize_table_cells(table);
}

fn insert_table_columns(table: &mut TableNode, index: u32, count: u32) {
    for cell in &mut table.cells {
        if cell.column >= index {
            cell.column += count;
        } else if index < cell.column + cell.column_span {
            cell.column_span += count;
        }
    }
    table.columns += count;
    canonicalize_table_cells(table);
}

fn delete_table_row(table: &mut TableNode, index: u32) {
    let mut retained = Vec::with_capacity(table.cells.len());
    for mut cell in std::mem::take(&mut table.cells) {
        let end = cell.row + cell.row_span;
        if cell.row > index {
            cell.row -= 1;
            retained.push(cell);
        } else if cell.row == index {
            if cell.row_span > 1 {
                cell.row_span -= 1;
                retained.push(cell);
            }
        } else if end > index {
            cell.row_span -= 1;
            retained.push(cell);
        } else {
            retained.push(cell);
        }
    }
    table.rows -= 1;
    table.cells = retained;
    canonicalize_table_cells(table);
}

fn delete_table_column(table: &mut TableNode, index: u32) {
    let mut retained = Vec::with_capacity(table.cells.len());
    for mut cell in std::mem::take(&mut table.cells) {
        let end = cell.column + cell.column_span;
        if cell.column > index {
            cell.column -= 1;
            retained.push(cell);
        } else if cell.column == index {
            if cell.column_span > 1 {
                cell.column_span -= 1;
                retained.push(cell);
            }
        } else if end > index {
            cell.column_span -= 1;
            retained.push(cell);
        } else {
            retained.push(cell);
        }
    }
    table.columns -= 1;
    table.cells = retained;
    canonicalize_table_cells(table);
}

fn merge_table_cells(
    table: &mut TableNode,
    start: &TableCellAddress,
    end: &TableCellAddress,
    node_id: &str,
) -> Result<(), PresentationEngineError> {
    if start.row > end.row
        || start.column > end.column
        || end.row >= table.rows
        || end.column >= table.columns
    {
        return Err(PresentationEngineError::InvalidTableStructure(
            "merge range 越界或反向".into(),
        ));
    }
    if start.row == end.row && start.column == end.column {
        return Err(PresentationEngineError::InvalidTableStructure(
            "merge range 至少需要两个单元格".into(),
        ));
    }
    let selected = |cell: &TableCell| {
        cell.row >= start.row
            && cell.column >= start.column
            && cell.row + cell.row_span - 1 <= end.row
            && cell.column + cell.column_span - 1 <= end.column
    };
    let overlaps = |cell: &TableCell| {
        cell.row <= end.row
            && cell.row + cell.row_span > start.row
            && cell.column <= end.column
            && cell.column + cell.column_span > start.column
    };
    if table
        .cells
        .iter()
        .any(|cell| overlaps(cell) && !selected(cell))
    {
        return Err(PresentationEngineError::InvalidTableStructure(
            "merge range 不能切穿已有合并单元格".into(),
        ));
    }
    let anchor_index = table_cell_index(table, start.row, start.column, node_id)?;
    if !selected(&table.cells[anchor_index]) {
        return Err(PresentationEngineError::InvalidTableStructure(
            "merge range 必须从锚点开始".into(),
        ));
    }
    let anchor = table.cells[anchor_index].clone();
    table.cells.retain(|cell| !selected(cell));
    table.cells.push(TableCell {
        row: start.row,
        column: start.column,
        row_span: end.row - start.row + 1,
        column_span: end.column - start.column + 1,
        content: anchor.content,
        style: anchor.style,
    });
    canonicalize_table_cells(table);
    Ok(())
}

fn split_table_cell(
    table: &mut TableNode,
    row: u32,
    column: u32,
    node_id: &str,
) -> Result<(), PresentationEngineError> {
    let index = table_cell_index(table, row, column, node_id)?;
    let cell = table.cells[index].clone();
    if cell.row_span == 1 && cell.column_span == 1 {
        return Err(PresentationEngineError::InvalidTableStructure(
            "只能拆分已合并单元格".into(),
        ));
    }
    table.cells.remove(index);
    for current_row in cell.row..cell.row + cell.row_span {
        for current_column in cell.column..cell.column + cell.column_span {
            table.cells.push(TableCell {
                row: current_row,
                column: current_column,
                row_span: 1,
                column_span: 1,
                content: if current_row == cell.row && current_column == cell.column {
                    cell.content.clone()
                } else {
                    PresentationRichText::default()
                },
                style: cell.style.clone(),
            });
        }
    }
    canonicalize_table_cells(table);
    Ok(())
}

fn selected_node_indices(
    slide: &Slide,
    node_ids: &[String],
    minimum: usize,
) -> Result<Vec<usize>, PresentationEngineError> {
    if node_ids.len() < minimum {
        return Err(PresentationEngineError::SelectionRequiresAtLeast {
            required: minimum,
            actual: node_ids.len(),
        });
    }
    let mut indices: Vec<usize> = Vec::with_capacity(node_ids.len());
    for node_id in node_ids {
        if indices
            .iter()
            .any(|index| slide.nodes[*index].id == *node_id)
        {
            return Err(PresentationEngineError::DuplicateSelectionNode(
                node_id.clone(),
            ));
        }
        let index = node_index(slide, node_id)?;
        if slide.nodes[index].locked {
            return Err(PresentationEngineError::LockedNode(node_id.clone()));
        }
        indices.push(index);
    }
    Ok(indices)
}

fn align_nodes(
    slide: &mut Slide,
    node_ids: &[String],
    alignment: NodeAlignment,
) -> Result<Vec<(String, NodeTransform)>, PresentationEngineError> {
    let indices = selected_node_indices(slide, node_ids, 2)?;
    let left = indices
        .iter()
        .map(|index| slide.nodes[*index].transform.x)
        .fold(f64::INFINITY, f64::min);
    let top = indices
        .iter()
        .map(|index| slide.nodes[*index].transform.y)
        .fold(f64::INFINITY, f64::min);
    let right = indices
        .iter()
        .map(|index| slide.nodes[*index].transform.x + slide.nodes[*index].transform.width)
        .fold(f64::NEG_INFINITY, f64::max);
    let bottom = indices
        .iter()
        .map(|index| slide.nodes[*index].transform.y + slide.nodes[*index].transform.height)
        .fold(f64::NEG_INFINITY, f64::max);
    let previous = indices
        .iter()
        .map(|index| {
            (
                slide.nodes[*index].id.clone(),
                slide.nodes[*index].transform.clone(),
            )
        })
        .collect::<Vec<_>>();
    for index in indices {
        let transform = &mut slide.nodes[index].transform;
        match alignment {
            NodeAlignment::Left => transform.x = left,
            NodeAlignment::Center => transform.x = (left + right - transform.width) / 2.0,
            NodeAlignment::Right => transform.x = right - transform.width,
            NodeAlignment::Top => transform.y = top,
            NodeAlignment::Middle => transform.y = (top + bottom - transform.height) / 2.0,
            NodeAlignment::Bottom => transform.y = bottom - transform.height,
        }
    }
    Ok(previous)
}

fn distribute_nodes(
    slide: &mut Slide,
    node_ids: &[String],
    axis: NodeDistributionAxis,
) -> Result<Vec<(String, NodeTransform)>, PresentationEngineError> {
    let mut indices = selected_node_indices(slide, node_ids, 3)?;
    indices.sort_by(|left, right| {
        let left_value = match axis {
            NodeDistributionAxis::Horizontal => slide.nodes[*left].transform.x,
            NodeDistributionAxis::Vertical => slide.nodes[*left].transform.y,
        };
        let right_value = match axis {
            NodeDistributionAxis::Horizontal => slide.nodes[*right].transform.x,
            NodeDistributionAxis::Vertical => slide.nodes[*right].transform.y,
        };
        left_value.total_cmp(&right_value)
    });
    let previous = indices
        .iter()
        .map(|index| {
            (
                slide.nodes[*index].id.clone(),
                slide.nodes[*index].transform.clone(),
            )
        })
        .collect::<Vec<_>>();
    let last = *indices.last().expect("selection is nonempty");
    let start = match axis {
        NodeDistributionAxis::Horizontal => slide.nodes[indices[0]].transform.x,
        NodeDistributionAxis::Vertical => slide.nodes[indices[0]].transform.y,
    };
    let end = match axis {
        NodeDistributionAxis::Horizontal => {
            slide.nodes[last].transform.x + slide.nodes[last].transform.width
        }
        NodeDistributionAxis::Vertical => {
            slide.nodes[last].transform.y + slide.nodes[last].transform.height
        }
    };
    let occupied = indices
        .iter()
        .map(|index| match axis {
            NodeDistributionAxis::Horizontal => slide.nodes[*index].transform.width,
            NodeDistributionAxis::Vertical => slide.nodes[*index].transform.height,
        })
        .sum::<f64>();
    let gap = (end - start - occupied) / (indices.len() - 1) as f64;
    let mut cursor = start;
    for index in indices {
        let transform = &mut slide.nodes[index].transform;
        match axis {
            NodeDistributionAxis::Horizontal => transform.x = cursor,
            NodeDistributionAxis::Vertical => transform.y = cursor,
        }
        cursor += match axis {
            NodeDistributionAxis::Horizontal => transform.width,
            NodeDistributionAxis::Vertical => transform.height,
        } + gap;
    }
    Ok(previous)
}

fn validate_parent_target(
    slide: &Slide,
    parent_id: Option<&str>,
    node_id: &str,
) -> Result<(), PresentationEngineError> {
    let Some(parent_id) = parent_id else {
        return Ok(());
    };
    if parent_id == node_id {
        return Err(PresentationEngineError::InvalidParent(node_id.into()));
    }
    let parent = &slide.nodes[node_index(slide, parent_id)?];
    if !matches!(&parent.kind, SceneNodeKind::Group(_)) {
        return Err(PresentationEngineError::ParentMustBeGroup(parent_id.into()));
    }
    Ok(())
}

/// Connector anchors are validated before the node is mutated.  The final
/// schema validation remains the defensive backstop, but command-level checks
/// keep a malformed endpoint from relying on a renderer or a later command in
/// the batch to discover a dangling graph edge.
fn validate_connector_endpoints(
    slide: &Slide,
    connector_id: &str,
    start: &ConnectorEndpoint,
    end: &ConnectorEndpoint,
) -> Result<(), PresentationEngineError> {
    for endpoint in [start, end] {
        let ConnectorEndpoint::Node { node_id, .. } = endpoint else {
            continue;
        };
        if node_id.trim().is_empty() {
            return Err(PresentationEngineError::InvalidConnectorEndpoint(
                "target node id 不能为空".into(),
            ));
        }
        if node_id == connector_id {
            return Err(PresentationEngineError::InvalidConnectorEndpoint(
                "connector 不能连接自身".into(),
            ));
        }
        if !slide.nodes.iter().any(|node| node.id == *node_id) {
            return Err(PresentationEngineError::InvalidConnectorEndpoint(format!(
                "connector target 不存在：{node_id}"
            )));
        }
    }
    Ok(())
}

fn move_node(
    slide: &mut Slide,
    node_id: &str,
    parent_id: Option<String>,
    index: usize,
) -> Result<(), PresentationEngineError> {
    validate_parent_target(slide, parent_id.as_deref(), node_id)?;
    if parent_id
        .as_deref()
        .is_some_and(|candidate| is_descendant(slide, candidate, node_id))
    {
        return Err(PresentationEngineError::InvalidParent(node_id.into()));
    }
    let node_index = node_index(slide, node_id)?;
    slide.nodes[node_index].parent_id = parent_id.clone();
    reorder_within_parent(slide, node_id, parent_id.as_deref(), index)
}

fn is_descendant(slide: &Slide, candidate_id: &str, ancestor_id: &str) -> bool {
    let mut cursor = Some(candidate_id);
    while let Some(id) = cursor {
        if id == ancestor_id {
            return true;
        }
        cursor = slide
            .nodes
            .iter()
            .find(|node| node.id == id)
            .and_then(|node| node.parent_id.as_deref());
    }
    false
}

fn capture_positions(
    slide: &Slide,
    node_ids: &[&str],
    new_parent: Option<&str>,
) -> Vec<NodePosition> {
    let mut parent_ids: Vec<Option<&str>> = node_ids
        .iter()
        .filter_map(|id| {
            slide
                .nodes
                .iter()
                .find(|node| node.id == *id)
                .map(|node| node.parent_id.as_deref())
        })
        .collect();
    parent_ids.push(new_parent);
    parent_ids.sort();
    parent_ids.dedup();
    slide
        .nodes
        .iter()
        .filter(|node| parent_ids.contains(&node.parent_id.as_deref()))
        .map(position_of)
        .collect()
}

fn capture_exact_positions(
    slide: &Slide,
    node_ids: &[String],
) -> Result<Vec<NodePosition>, PresentationEngineError> {
    node_ids
        .iter()
        .map(|id| {
            slide
                .nodes
                .get(node_index(slide, id)?)
                .map(position_of)
                .ok_or_else(|| PresentationEngineError::MissingNode(id.clone()))
        })
        .collect()
}
fn position_of(node: &SceneNode) -> NodePosition {
    NodePosition {
        node_id: node.id.clone(),
        parent_id: node.parent_id.clone(),
        order_key: node.order_key.clone(),
    }
}
fn restore_positions(
    slide: &mut Slide,
    positions: &[NodePosition],
) -> Result<(), PresentationEngineError> {
    for position in positions {
        let index = node_index(slide, &position.node_id)?;
        slide.nodes[index].parent_id = position.parent_id.clone();
        slide.nodes[index].order_key = position.order_key.clone();
    }
    Ok(())
}

fn reorder_within_parent(
    slide: &mut Slide,
    node_id: &str,
    parent_id: Option<&str>,
    index: usize,
) -> Result<(), PresentationEngineError> {
    let node_index = node_index(slide, node_id)?;
    let mut siblings: Vec<usize> = slide
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| {
            (node.parent_id.as_deref() == parent_id && index != node_index).then_some(index)
        })
        .collect();
    siblings.sort_by(|left, right| {
        slide.nodes[*left]
            .order_key
            .cmp(&slide.nodes[*right].order_key)
    });
    let insertion = index.min(siblings.len());
    siblings.insert(insertion, node_index);
    for (position, node_index) in siblings.into_iter().enumerate() {
        slide.nodes[node_index].order_key = order_key(position);
    }
    Ok(())
}
fn normalize_sibling_order(slide: &mut Slide, parent_id: Option<&str>) {
    let mut siblings: Vec<usize> = slide
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| (node.parent_id.as_deref() == parent_id).then_some(index))
        .collect();
    siblings.sort_by(|left, right| {
        slide.nodes[*left]
            .order_key
            .cmp(&slide.nodes[*right].order_key)
    });
    for (position, node_index) in siblings.into_iter().enumerate() {
        slide.nodes[node_index].order_key = order_key(position);
    }
}
fn order_key(index: usize) -> String {
    format!("{index:016x}")
}

/// Timeline order keys are canonical engine-owned ordering metadata.  UI callers
/// ask for an index and never synthesize ordering strings themselves.
fn normalize_timeline_order(timeline: &mut Timeline) {
    timeline
        .entries
        .sort_by(|left, right| left.order_key.cmp(&right.order_key));
    for (index, entry) in timeline.entries.iter_mut().enumerate() {
        entry.order_key = order_key(index);
    }
}

fn reverse_mutations(mutations: &[PresentationMutation]) -> Vec<PresentationMutation> {
    mutations
        .iter()
        .rev()
        .map(|mutation| match mutation {
            PresentationMutation::AssetRegistered { asset_id } => {
                PresentationMutation::AssetUnregistered {
                    asset_id: asset_id.clone(),
                }
            }
            PresentationMutation::AssetUnregistered { asset_id } => {
                PresentationMutation::AssetRegistered {
                    asset_id: asset_id.clone(),
                }
            }
            PresentationMutation::SlideInserted { slide_id } => {
                PresentationMutation::SlideDeleted {
                    slide_id: slide_id.clone(),
                }
            }
            PresentationMutation::SlideDuplicated { slide_id, .. } => {
                PresentationMutation::SlideDeleted {
                    slide_id: slide_id.clone(),
                }
            }
            PresentationMutation::SlideDeleted { slide_id } => {
                PresentationMutation::SlideInserted {
                    slide_id: slide_id.clone(),
                }
            }
            PresentationMutation::NodeInserted { slide_id, node_id } => {
                PresentationMutation::NodeDeleted {
                    slide_id: slide_id.clone(),
                    node_id: node_id.clone(),
                }
            }
            PresentationMutation::NodeDeleted { slide_id, node_id } => {
                PresentationMutation::NodeInserted {
                    slide_id: slide_id.clone(),
                    node_id: node_id.clone(),
                }
            }
            PresentationMutation::NodesGrouped { slide_id, group_id } => {
                PresentationMutation::NodesUngrouped {
                    slide_id: slide_id.clone(),
                    group_id: group_id.clone(),
                }
            }
            PresentationMutation::NodesUngrouped { slide_id, group_id } => {
                PresentationMutation::NodesGrouped {
                    slide_id: slide_id.clone(),
                    group_id: group_id.clone(),
                }
            }
            other => other.clone(),
        })
        .collect()
}

fn invalidation(mutations: &[PresentationMutation]) -> Invalidation {
    let mut result = Invalidation::default();
    for mutation in mutations {
        let (entities, containers, structural) = mutation.entity_refs();
        result.changed_entities.extend(entities);
        result.changed_containers.extend(containers);
        result.structure_changed |= structural;
    }
    result.changed_entities.sort_by(|a, b| {
        a.entity_type
            .cmp(&b.entity_type)
            .then(a.entity_id.cmp(&b.entity_id))
    });
    result.changed_entities.dedup();
    result.changed_containers.sort_by(|a, b| {
        a.entity_type
            .cmp(&b.entity_type)
            .then(a.entity_id.cmp(&b.entity_id))
    });
    result.changed_containers.dedup();
    result
}

#[derive(Debug, thiserror::Error)]
pub enum PresentationEngineError {
    #[error("Presentation revision 冲突：期望 {expected}，实际 {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("Presentation command batch 不能为空")]
    EmptyBatch,
    #[error("重复的 Presentation asset：{0}")]
    DuplicateAsset(String),
    #[error("不存在的 Presentation asset：{0}")]
    MissingAsset(String),
    #[error("没有可撤销的 Presentation mutation")]
    NothingToUndo,
    #[error("没有可重做的 Presentation mutation")]
    NothingToRedo,
    #[error("重复的 slide：{0}")]
    DuplicateSlide(String),
    #[error("重复的 master：{0}")]
    DuplicateMaster(String),
    #[error("不存在的 master：{0}")]
    MissingMaster(String),
    #[error("master {0} 仍被 layout 引用，不能删除")]
    MasterInUse(String),
    #[error("重复的 layout：{0}")]
    DuplicateLayout(String),
    #[error("不存在的 layout：{0}")]
    MissingLayout(String),
    #[error("layout {0} 仍被 slide 引用，不能删除")]
    LayoutInUse(String),
    #[error("复制 slide 的 id 映射无效：{0}")]
    InvalidDuplicateSlideMapping(String),
    #[error("重复的 node：{0}")]
    DuplicateNode(String),
    #[error("不存在的 slide：{0}")]
    MissingSlide(String),
    #[error("不存在的 node：{0}")]
    MissingNode(String),
    #[error("对象选区重复包含 node：{0}")]
    DuplicateSelectionNode(String),
    #[error("对象 {0} 已锁定，不能参与此操作")]
    LockedNode(String),
    #[error("对象操作至少需要 {required} 个对象，当前为 {actual}")]
    SelectionRequiresAtLeast { required: usize, actual: usize },
    #[error("不存在的 animation：{0}")]
    MissingAnimation(String),
    #[error("node {0} 仍有子节点，必须先显式 ungroup 或移动子节点")]
    HasChildren(String),
    #[error("node {0} 不是 text node")]
    NotTextNode(String),
    #[error("node {0} 不是 shape node")]
    NotShapeNode(String),
    #[error("node {0} 不是 chart node")]
    NotChartNode(String),
    #[error("node {0} 不是 connector node")]
    NotConnectorNode(String),
    #[error("connector endpoint 无效：{0}")]
    InvalidConnectorEndpoint(String),
    #[error("node {0} 不是 table node")]
    NotTableNode(String),
    #[error("table node {node_id} 不存在以 ({row}, {column}) 为锚点的单元格")]
    MissingTableCell {
        node_id: String,
        row: u32,
        column: u32,
    },
    #[error("table 单元格样式选区不能为空")]
    EmptyTableCellSelection,
    #[error("table 单元格锚点重复：({row}, {column})")]
    DuplicateTableCellAddress { row: u32, column: u32 },
    #[error("table 结构操作无效：{0}")]
    InvalidTableStructure(String),
    #[error("node {0} 不是 image node")]
    NotImageNode(String),
    #[error("node {0} 不是 audio 或 video node")]
    NotMediaNode(String),
    #[error("node {0} 不是 group node")]
    NotGroupNode(String),
    #[error("node {0} 的 parent 无效或形成环")]
    InvalidParent(String),
    #[error("node {0} 不能作为 parent：只有 group node 可以包含子节点")]
    ParentMustBeGroup(String),
    #[error("group 至少需要两个节点")]
    GroupRequiresTwoNodes,
    #[error("group 中的节点必须共享同一 parent")]
    GroupChildrenMustShareParent,
    #[error("group 的 parent 必须与待分组节点的 parent 一致")]
    GroupParentMustMatchChildren,
    #[error("group {0} 没有可解除的子节点")]
    EmptyGroup(String),
    #[error("Presentation schema 校验失败：{0}")]
    Schema(#[from] oo_schema::SchemaValidationError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use oo_schema::presentation_v5::{
        Anchor, AnimationEntry, AnimationPreset, AnimationTrigger, AssetRef, ChartNode,
        ChartSeries, ChartSpec, ChartType, ConnectorNode, DeckTheme, GroupNode, Insets,
        LayoutPlaceholder, MasterPlaceholder, MediaNode, PlaceholderKind, Point, SceneNodeKind,
        ShapeGeometry, ShapeNode, SlideLayout, SlideMaster, TableCell, TableCellStyle, TableNode,
        TextAutoFit, TextNode, TextVerticalAlign,
    };

    fn slide(id: &str) -> Slide {
        Slide {
            id: id.into(),
            order_key: id.into(),
            name: String::new(),
            layout_id: None,
            background: Default::default(),
            notes: None,
            transition: None,
            nodes: vec![],
            timeline: Default::default(),
        }
    }
    fn transform() -> NodeTransform {
        NodeTransform {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
            rotation: 0.0,
        }
    }
    fn text_node(id: &str, order: &str) -> SceneNode {
        SceneNode {
            id: id.into(),
            parent_id: None,
            order_key: order.into(),
            name: None,
            alt_text: None,
            layout_placeholder_id: None,
            transform: transform(),
            visible: true,
            locked: false,
            opacity: 1.0,
            kind: SceneNodeKind::Text(TextNode {
                frame: TextFrame {
                    body: PresentationRichText::default(),
                    vertical_align: TextVerticalAlign::Top,
                    padding: Insets::default(),
                    auto_fit: TextAutoFit::None,
                },
            }),
        }
    }
    fn group_node(id: &str, order: &str) -> SceneNode {
        SceneNode {
            id: id.into(),
            parent_id: None,
            order_key: order.into(),
            name: None,
            alt_text: None,
            layout_placeholder_id: None,
            transform: transform(),
            visible: true,
            locked: false,
            opacity: 1.0,
            kind: SceneNodeKind::Group(GroupNode::default()),
        }
    }
    fn shape_node(id: &str, order: &str) -> SceneNode {
        SceneNode {
            id: id.into(),
            parent_id: None,
            order_key: order.into(),
            name: None,
            alt_text: None,
            layout_placeholder_id: None,
            transform: transform(),
            visible: true,
            locked: false,
            opacity: 1.0,
            kind: SceneNodeKind::Shape(ShapeNode {
                geometry: ShapeGeometry::Rectangle,
                style: ShapeStyle::default(),
            }),
        }
    }
    fn table_node(id: &str, order: &str) -> SceneNode {
        SceneNode {
            id: id.into(),
            parent_id: None,
            order_key: order.into(),
            name: Some("表格".into()),
            alt_text: None,
            layout_placeholder_id: None,
            transform: transform(),
            visible: true,
            locked: false,
            opacity: 1.0,
            kind: SceneNodeKind::Table(TableNode {
                rows: 2,
                columns: 2,
                cells: (0..2)
                    .flat_map(|row| {
                        (0..2).map(move |column| TableCell {
                            row,
                            column,
                            row_span: 1,
                            column_span: 1,
                            content: PresentationRichText::default(),
                            style: TableCellStyle::default(),
                        })
                    })
                    .collect(),
            }),
        }
    }
    fn chart_node(id: &str, order: &str) -> SceneNode {
        SceneNode {
            id: id.into(),
            parent_id: None,
            order_key: order.into(),
            name: Some("图表".into()),
            alt_text: None,
            layout_placeholder_id: None,
            transform: transform(),
            visible: true,
            locked: false,
            opacity: 1.0,
            kind: SceneNodeKind::Chart(ChartNode {
                spec: ChartSpec {
                    chart_type: ChartType::Column,
                    title: Some("营收".into()),
                    categories: vec!["Q1".into(), "Q2".into()],
                    series: vec![ChartSeries {
                        name: "实际".into(),
                        values: vec![10.0, 20.0],
                        color: None,
                    }],
                },
            }),
        }
    }
    fn deck() -> Deck {
        Deck {
            theme: DeckTheme {
                id: "theme".into(),
                ..DeckTheme::default()
            },
            ..Deck::default()
        }
    }
    fn run(
        engine: &mut PresentationEngine,
        commands: Vec<PresentationCommand>,
    ) -> PresentationChangeSet {
        engine
            .execute(PresentationCommandBatch {
                base_revision: engine.revision(),
                commands,
            })
            .unwrap()
    }

    #[test]
    fn registering_an_asset_and_inserting_an_image_is_one_reversible_batch() {
        let mut engine = PresentationEngine::new(deck(), 0).unwrap();
        run(
            &mut engine,
            vec![PresentationCommand::CreateSlide {
                slide: slide("s1"),
                index: 0,
            }],
        );
        let asset = AssetRef {
            asset_id: "image-asset".into(),
            digest: "sha256:image-asset".into(),
            mime_type: "image/png".into(),
            width: Some(400),
            height: Some(200),
            original_asset_id: None,
        };
        let image = SceneNode {
            id: "image".into(),
            parent_id: None,
            order_key: "a".into(),
            name: Some("图片".into()),
            alt_text: None,
            layout_placeholder_id: None,
            transform: transform(),
            visible: true,
            locked: false,
            opacity: 1.0,
            kind: SceneNodeKind::Image(ImageNode {
                asset_id: asset.asset_id.clone(),
                original_asset_id: None,
                crop: Default::default(),
                flip_h: false,
                flip_v: false,
                caption: None,
            }),
        };
        let change = run(
            &mut engine,
            vec![
                PresentationCommand::RegisterAsset { asset },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: image,
                    index: 0,
                },
            ],
        );
        assert!(change.mutations.iter().any(|mutation| matches!(mutation, PresentationMutation::AssetRegistered { asset_id } if asset_id == "image-asset")));
        assert_eq!(engine.deck().assets.len(), 1);
        assert_eq!(engine.deck().slides[0].nodes.len(), 1);

        let undo = engine.undo(engine.revision()).unwrap();
        assert!(undo.mutations.iter().any(|mutation| matches!(mutation, PresentationMutation::AssetUnregistered { asset_id } if asset_id == "image-asset")));
        assert!(engine.deck().assets.is_empty());
        assert!(engine.deck().slides[0].nodes.is_empty());
    }

    #[test]
    fn duplicate_slide_rewrites_node_hierarchy_and_timeline_and_is_reversible() {
        let mut source = slide("source");
        source.name = "原始幻灯片".into();
        let group = group_node("group", "a");
        let mut child = text_node("child", "a");
        child.parent_id = Some("group".into());
        let mut connector = text_node("connector", "b");
        connector.kind = SceneNodeKind::Connector(ConnectorNode {
            start: ConnectorEndpoint::Node {
                node_id: "child".into(),
                anchor: Anchor::Right,
            },
            end: ConnectorEndpoint::Free(Point { x: 120.0, y: 60.0 }),
        });
        source.nodes = vec![group, child, connector];
        source.timeline.entries.push(AnimationEntry {
            id: "animation".into(),
            target_node_id: "child".into(),
            trigger: AnimationTrigger::OnClick,
            preset: AnimationPreset::Appear,
            duration_ms: 200,
            delay_ms: 0,
            order_key: "a".into(),
        });
        let mut initial = deck();
        initial.slides.push(source);
        let mut engine = PresentationEngine::new(initial, 0).unwrap();

        let change = run(
            &mut engine,
            vec![PresentationCommand::DuplicateSlide {
                source_slide_id: "source".into(),
                slide_id: "copy".into(),
                order_key: "copy".into(),
                name: "原始幻灯片 副本".into(),
                node_id_map: vec![
                    PresentationIdMapping {
                        source_id: "group".into(),
                        target_id: "copy-group".into(),
                    },
                    PresentationIdMapping {
                        source_id: "child".into(),
                        target_id: "copy-child".into(),
                    },
                    PresentationIdMapping {
                        source_id: "connector".into(),
                        target_id: "copy-connector".into(),
                    },
                ],
                animation_id_map: vec![PresentationIdMapping {
                    source_id: "animation".into(),
                    target_id: "copy-animation".into(),
                }],
                index: 1,
            }],
        );
        assert!(
            matches!(change.mutations.as_slice(), [PresentationMutation::SlideDuplicated { source_slide_id, slide_id }] if source_slide_id == "source" && slide_id == "copy")
        );
        assert_eq!(change.dirty_thumbnail_ids, vec!["copy"]);
        let copied = &engine.deck().slides[1];
        assert_eq!(copied.name, "原始幻灯片 副本");
        assert_eq!(copied.nodes[1].id, "copy-child");
        assert_eq!(copied.nodes[1].parent_id.as_deref(), Some("copy-group"));
        assert!(
            matches!(&copied.nodes[2].kind, SceneNodeKind::Connector(ConnectorNode { start: ConnectorEndpoint::Node { node_id, .. }, .. }) if node_id == "copy-child")
        );
        assert_eq!(copied.timeline.entries[0].id, "copy-animation");
        assert_eq!(copied.timeline.entries[0].target_node_id, "copy-child");

        engine.undo(engine.revision()).unwrap();
        assert_eq!(engine.deck().slides.len(), 1);
        engine.redo(engine.revision()).unwrap();
        assert_eq!(
            engine.deck().slides[1].timeline.entries[0].target_node_id,
            "copy-child"
        );
    }

    #[test]
    fn duplicate_slide_rejects_incomplete_identity_mapping_without_mutating_the_deck() {
        let mut source = slide("source");
        source.nodes = vec![text_node("one", "a"), text_node("two", "b")];
        let mut initial = deck();
        initial.slides.push(source);
        let mut engine = PresentationEngine::new(initial, 0).unwrap();

        let error = engine
            .execute(PresentationCommandBatch {
                base_revision: 0,
                commands: vec![PresentationCommand::DuplicateSlide {
                    source_slide_id: "source".into(),
                    slide_id: "copy".into(),
                    order_key: "copy".into(),
                    name: "副本".into(),
                    node_id_map: vec![PresentationIdMapping {
                        source_id: "one".into(),
                        target_id: "copy-one".into(),
                    }],
                    animation_id_map: vec![],
                    index: 1,
                }],
            })
            .unwrap_err();
        assert!(matches!(
            error,
            PresentationEngineError::InvalidDuplicateSlideMapping(_)
        ));
        assert_eq!(engine.revision(), 0);
        assert_eq!(engine.deck().slides.len(), 1);
    }

    #[test]
    fn node_lock_is_a_typed_undoable_semantic_mutation() {
        let mut engine = PresentationEngine::new(deck(), 0).unwrap();
        run(
            &mut engine,
            vec![PresentationCommand::CreateSlide {
                slide: slide("s1"),
                index: 0,
            }],
        );
        run(
            &mut engine,
            vec![PresentationCommand::InsertNode {
                slide_id: "s1".into(),
                node: text_node("n1", "a"),
                index: 0,
            }],
        );
        run(
            &mut engine,
            vec![PresentationCommand::SetNodeLocked {
                slide_id: "s1".into(),
                node_id: "n1".into(),
                locked: true,
            }],
        );
        assert!(engine.deck().slides[0].nodes[0].locked);
        assert!(engine
            .undo(engine.revision())
            .unwrap()
            .mutations
            .iter()
            .any(|mutation| matches!(mutation, PresentationMutation::NodeLockSet { slide_id, node_id } if slide_id == "s1" && node_id == "n1")));
        assert!(!engine.deck().slides[0].nodes[0].locked);
        engine.redo(engine.revision()).unwrap();
        assert!(engine.deck().slides[0].nodes[0].locked);
    }

    #[test]
    fn alignment_and_distribution_are_typed_reversible_selection_operations() {
        let mut engine = PresentationEngine::new(deck(), 0).unwrap();
        run(
            &mut engine,
            vec![PresentationCommand::CreateSlide {
                slide: slide("s1"),
                index: 0,
            }],
        );
        let mut first = shape_node("a", "a");
        first.transform = NodeTransform {
            x: 10.0,
            y: 20.0,
            width: 50.0,
            height: 20.0,
            rotation: 0.0,
        };
        let mut second = shape_node("b", "b");
        second.transform = NodeTransform {
            x: 80.0,
            y: 60.0,
            width: 20.0,
            height: 30.0,
            rotation: 0.0,
        };
        let mut third = shape_node("c", "c");
        third.transform = NodeTransform {
            x: 180.0,
            y: 120.0,
            width: 40.0,
            height: 10.0,
            rotation: 0.0,
        };
        run(
            &mut engine,
            vec![
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: first,
                    index: 0,
                },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: second,
                    index: 1,
                },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: third,
                    index: 2,
                },
            ],
        );

        let aligned = run(
            &mut engine,
            vec![PresentationCommand::AlignNodes {
                slide_id: "s1".into(),
                node_ids: vec!["a".into(), "b".into()],
                alignment: NodeAlignment::Left,
            }],
        );
        assert!(matches!(
            aligned.mutations.as_slice(),
            [PresentationMutation::NodesAligned { .. }]
        ));
        assert_eq!(engine.deck().slides[0].nodes[0].transform.x, 10.0);
        assert_eq!(engine.deck().slides[0].nodes[1].transform.x, 10.0);
        let distributed = run(
            &mut engine,
            vec![PresentationCommand::DistributeNodes {
                slide_id: "s1".into(),
                node_ids: vec!["a".into(), "b".into(), "c".into()],
                axis: NodeDistributionAxis::Vertical,
            }],
        );
        assert!(matches!(
            distributed.mutations.as_slice(),
            [PresentationMutation::NodesDistributed { .. }]
        ));
        assert_eq!(engine.deck().slides[0].nodes[0].transform.y, 20.0);
        assert_eq!(engine.deck().slides[0].nodes[2].transform.y, 120.0);
        engine.undo(engine.revision()).unwrap();
        assert_eq!(engine.deck().slides[0].nodes[1].transform.y, 60.0);
    }

    #[test]
    fn connector_endpoints_are_atomic_reversible_and_schema_validated() {
        let mut engine = PresentationEngine::new(deck(), 0).unwrap();
        let mut connector = text_node("connector", "b");
        connector.kind = SceneNodeKind::Connector(ConnectorNode {
            start: ConnectorEndpoint::Free(Point { x: 10.0, y: 20.0 }),
            end: ConnectorEndpoint::Free(Point { x: 30.0, y: 40.0 }),
        });
        run(
            &mut engine,
            vec![
                PresentationCommand::CreateSlide {
                    slide: slide("s1"),
                    index: 0,
                },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: text_node("target", "a"),
                    index: 0,
                },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: connector,
                    index: 1,
                },
            ],
        );

        let change = run(
            &mut engine,
            vec![PresentationCommand::SetConnectorEndpoints {
                slide_id: "s1".into(),
                node_id: "connector".into(),
                start: ConnectorEndpoint::Node {
                    node_id: "target".into(),
                    anchor: Anchor::Right,
                },
                end: ConnectorEndpoint::Free(Point { x: 90.0, y: 100.0 }),
            }],
        );
        assert!(
            matches!(change.mutations.as_slice(), [PresentationMutation::ConnectorEndpointsSet { slide_id, node_id }] if slide_id == "s1" && node_id == "connector")
        );
        assert!(
            matches!(engine.deck().slides[0].nodes[1].kind, SceneNodeKind::Connector(ConnectorNode { start: ConnectorEndpoint::Node { ref node_id, .. }, .. }) if node_id == "target")
        );
        engine.undo(engine.revision()).unwrap();
        assert!(matches!(
            engine.deck().slides[0].nodes[1].kind,
            SceneNodeKind::Connector(ConnectorNode {
                start: ConnectorEndpoint::Free(_),
                ..
            })
        ));
        engine.redo(engine.revision()).unwrap();

        let before = engine.deck().clone();
        let error = engine
            .execute(PresentationCommandBatch {
                base_revision: engine.revision(),
                commands: vec![PresentationCommand::SetConnectorEndpoints {
                    slide_id: "s1".into(),
                    node_id: "connector".into(),
                    start: ConnectorEndpoint::Node {
                        node_id: "connector".into(),
                        anchor: Anchor::Center,
                    },
                    end: ConnectorEndpoint::Free(Point { x: 1.0, y: 1.0 }),
                }],
            })
            .unwrap_err();
        assert!(matches!(
            error,
            PresentationEngineError::InvalidConnectorEndpoint(_)
        ));
        assert_eq!(engine.deck(), &before);
    }

    #[test]
    fn failed_batch_rolls_back_without_copying_unrelated_slides() {
        let mut engine = PresentationEngine::new(deck(), 0).unwrap();
        run(
            &mut engine,
            vec![
                PresentationCommand::CreateSlide {
                    slide: slide("s1"),
                    index: 0,
                },
                PresentationCommand::CreateSlide {
                    slide: slide("s2"),
                    index: 1,
                },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: text_node("n1", "a"),
                    index: 0,
                },
            ],
        );
        let untouched = engine.deck().slides[1].clone();
        let before = engine.deck().clone();
        let error = engine
            .execute(PresentationCommandBatch {
                base_revision: engine.revision(),
                commands: vec![
                    PresentationCommand::SetTextContent {
                        slide_id: "s1".into(),
                        node_id: "n1".into(),
                        body: PresentationRichText {
                            text: "changed".into(),
                            runs: vec![],
                        },
                    },
                    PresentationCommand::SetNodeTransform {
                        slide_id: "s1".into(),
                        node_id: "missing".into(),
                        transform: transform(),
                    },
                ],
            })
            .unwrap_err();
        assert!(matches!(error, PresentationEngineError::MissingNode(_)));
        assert_eq!(engine.deck(), &before);
        assert_eq!(engine.deck().slides[1], untouched);
        assert_eq!(engine.revision(), 1);
    }

    #[test]
    fn journal_undo_redo_restores_semantic_state_and_revisions() {
        let mut engine = PresentationEngine::new(deck(), 10).unwrap();
        run(
            &mut engine,
            vec![
                PresentationCommand::CreateSlide {
                    slide: slide("s1"),
                    index: 0,
                },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: text_node("n1", "a"),
                    index: 0,
                },
                PresentationCommand::SetTextContent {
                    slide_id: "s1".into(),
                    node_id: "n1".into(),
                    body: PresentationRichText {
                        text: "hello".into(),
                        runs: vec![],
                    },
                },
            ],
        );
        assert!(engine.can_undo());
        let after = engine.deck().clone();
        let undo = engine.undo(11).unwrap();
        assert_eq!(undo.revision, 12);
        assert!(engine.deck().slides.is_empty());
        assert!(engine.can_redo());
        let redo = engine.redo(12).unwrap();
        assert_eq!(redo.revision, 13);
        assert_eq!(engine.deck(), &after);
        assert!(redo
            .mutations
            .iter()
            .any(|mutation| matches!(mutation, PresentationMutation::TextContentSet { .. })));
    }

    #[test]
    fn node_group_move_reorder_and_ungroup_are_typed_and_reversible() {
        let mut engine = PresentationEngine::new(deck(), 0).unwrap();
        run(
            &mut engine,
            vec![
                PresentationCommand::CreateSlide {
                    slide: slide("s1"),
                    index: 0,
                },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: text_node("a", "a"),
                    index: 0,
                },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: text_node("b", "b"),
                    index: 1,
                },
            ],
        );
        run(
            &mut engine,
            vec![PresentationCommand::GroupNodes {
                slide_id: "s1".into(),
                group: group_node("g", "c"),
                child_ids: vec!["a".into(), "b".into()],
                index: 0,
            }],
        );
        let grouped = &engine.deck().slides[0];
        assert!(
            grouped
                .nodes
                .iter()
                .filter(|node| node.parent_id.as_deref() == Some("g"))
                .count()
                == 2
        );
        run(
            &mut engine,
            vec![PresentationCommand::UngroupNodes {
                slide_id: "s1".into(),
                group_id: "g".into(),
            }],
        );
        assert!(engine.deck().slides[0]
            .nodes
            .iter()
            .all(|node| node.parent_id.is_none()));
        engine.undo(engine.revision()).unwrap();
        assert!(engine.deck().slides[0]
            .nodes
            .iter()
            .any(|node| node.id == "g"));
        engine.deck().validate().unwrap();
    }

    #[test]
    fn grouping_journal_restores_every_affected_sibling_order_key() {
        let mut engine = PresentationEngine::new(deck(), 0).unwrap();
        run(
            &mut engine,
            vec![
                PresentationCommand::CreateSlide {
                    slide: slide("s1"),
                    index: 0,
                },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: text_node("a", "a"),
                    index: 0,
                },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: text_node("b", "b"),
                    index: 1,
                },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: text_node("c", "c"),
                    index: 2,
                },
            ],
        );
        let before = engine.deck().clone();
        run(
            &mut engine,
            vec![PresentationCommand::GroupNodes {
                slide_id: "s1".into(),
                group: group_node("g", "group"),
                child_ids: vec!["a".into(), "b".into()],
                index: 1,
            }],
        );
        engine.undo(engine.revision()).unwrap();
        assert_eq!(engine.deck(), &before);
    }

    #[test]
    fn insert_node_uses_sibling_index_and_undo_restores_prior_order_keys() {
        let mut engine = PresentationEngine::new(deck(), 0).unwrap();
        run(
            &mut engine,
            vec![
                PresentationCommand::CreateSlide {
                    slide: slide("s1"),
                    index: 0,
                },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: text_node("a", "a"),
                    index: 0,
                },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: text_node("c", "c"),
                    index: 1,
                },
            ],
        );
        let before = engine.deck().clone();
        run(
            &mut engine,
            vec![PresentationCommand::InsertNode {
                slide_id: "s1".into(),
                node: text_node("b", "untrusted-caller-order"),
                index: 1,
            }],
        );
        let slide = &engine.deck().slides[0];
        let mut order: Vec<&SceneNode> = slide.nodes.iter().collect();
        order.sort_by(|left, right| left.order_key.cmp(&right.order_key));
        assert_eq!(
            order
                .iter()
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );
        engine.undo(engine.revision()).unwrap();
        assert_eq!(engine.deck(), &before);
    }

    #[test]
    fn page_slide_and_node_specific_commands_emit_local_mutations() {
        let mut engine = PresentationEngine::new(deck(), 0).unwrap();
        run(
            &mut engine,
            vec![
                PresentationCommand::CreateSlide {
                    slide: slide("s1"),
                    index: 0,
                },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: shape_node("shape", "a"),
                    index: 0,
                },
            ],
        );
        let result = run(
            &mut engine,
            vec![
                PresentationCommand::SetPageSpec {
                    page_spec: SlidePageSpec {
                        width: 1000.0,
                        height: 500.0,
                        unit: oo_schema::presentation_v5::PageUnit::Point,
                        safe_area: None,
                    },
                },
                PresentationCommand::SetShapeStyle {
                    slide_id: "s1".into(),
                    node_id: "shape".into(),
                    style: ShapeStyle::default(),
                },
                PresentationCommand::SetSlideNotes {
                    slide_id: "s1".into(),
                    notes: Some("speaker note".into()),
                },
                PresentationCommand::SetSlideTransition {
                    slide_id: "s1".into(),
                    transition: None,
                },
            ],
        );
        assert!(result.mutations.iter().any(|mutation| matches!(mutation, PresentationMutation::ShapeStyleSet { slide_id, node_id } if slide_id == "s1" && node_id == "shape")));
        assert!(result
            .invalidation
            .changed_entities
            .iter()
            .any(|entity| entity.entity_id == "s1"));
        // Thumbnail work is a derived read-side effect, not an extra command
        // or a renderer-maintained Deck clone.
        assert_eq!(result.dirty_thumbnail_ids, vec!["s1"]);
        assert!(result
            .invalidation
            .changed_entities
            .iter()
            .any(|entity| entity.entity_id == "s1/shape"));
    }

    #[test]
    fn changing_shape_geometry_preserves_the_node_and_is_reversible() {
        let mut engine = PresentationEngine::new(deck(), 0).unwrap();
        run(
            &mut engine,
            vec![
                PresentationCommand::CreateSlide {
                    slide: slide("s1"),
                    index: 0,
                },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: shape_node("shape", "a"),
                    index: 0,
                },
            ],
        );

        let result = run(
            &mut engine,
            vec![PresentationCommand::SetShapeGeometry {
                slide_id: "s1".into(),
                node_id: "shape".into(),
                geometry: ShapeGeometry::Arrow,
            }],
        );
        let node = &engine.deck().slides[0].nodes[0];
        assert_eq!(node.id, "shape");
        assert!(
            matches!(&node.kind, SceneNodeKind::Shape(shape) if shape.geometry == ShapeGeometry::Arrow)
        );
        assert!(result.mutations.iter().any(|mutation| matches!(mutation, PresentationMutation::ShapeGeometrySet { slide_id, node_id } if slide_id == "s1" && node_id == "shape")));
        assert!(result
            .invalidation
            .changed_entities
            .iter()
            .any(|entity| entity.entity_id == "s1/shape"));

        engine.undo(engine.revision()).unwrap();
        assert!(
            matches!(&engine.deck().slides[0].nodes[0].kind, SceneNodeKind::Shape(shape) if shape.geometry == ShapeGeometry::Rectangle)
        );
    }

    #[test]
    fn changing_chart_spec_is_local_and_reversible() {
        let mut engine = PresentationEngine::new(deck(), 0).unwrap();
        run(
            &mut engine,
            vec![
                PresentationCommand::CreateSlide {
                    slide: slide("s1"),
                    index: 0,
                },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: chart_node("chart", "a"),
                    index: 0,
                },
            ],
        );
        let result = run(
            &mut engine,
            vec![PresentationCommand::SetChartSpec {
                slide_id: "s1".into(),
                node_id: "chart".into(),
                spec: ChartSpec {
                    chart_type: ChartType::Line,
                    title: Some("预测".into()),
                    categories: vec!["Q1".into(), "Q2".into()],
                    series: vec![ChartSeries {
                        name: "预测".into(),
                        values: vec![15.0, 25.0],
                        color: None,
                    }],
                },
            }],
        );
        assert!(result.mutations.iter().any(|mutation| matches!(mutation, PresentationMutation::ChartSpecSet { slide_id, node_id } if slide_id == "s1" && node_id == "chart")));
        assert!(result
            .invalidation
            .changed_entities
            .iter()
            .any(|entity| entity.entity_id == "s1/chart"));
        assert!(
            matches!(&engine.deck().slides[0].nodes[0].kind, SceneNodeKind::Chart(chart) if chart.spec.chart_type == ChartType::Line)
        );

        engine.undo(engine.revision()).unwrap();
        assert!(
            matches!(&engine.deck().slides[0].nodes[0].kind, SceneNodeKind::Chart(chart) if chart.spec.chart_type == ChartType::Column)
        );
    }

    #[test]
    fn table_cell_commands_use_anchor_identity_are_atomic_and_reversible() {
        let mut engine = PresentationEngine::new(deck(), 0).unwrap();
        run(
            &mut engine,
            vec![
                PresentationCommand::CreateSlide {
                    slide: slide("s1"),
                    index: 0,
                },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: table_node("table", "a"),
                    index: 0,
                },
            ],
        );
        let body = PresentationRichText {
            text: "Revenue".into(),
            runs: vec![],
        };
        let style = TableCellStyle::default();
        let change = run(
            &mut engine,
            vec![
                PresentationCommand::SetTableCellContent {
                    slide_id: "s1".into(),
                    node_id: "table".into(),
                    row: 0,
                    column: 1,
                    content: body.clone(),
                },
                PresentationCommand::SetTableCellStyle {
                    slide_id: "s1".into(),
                    node_id: "table".into(),
                    cells: vec![
                        TableCellAddress { row: 0, column: 0 },
                        TableCellAddress { row: 1, column: 1 },
                    ],
                    style: style.clone(),
                },
            ],
        );
        assert!(change.mutations.iter().any(|mutation| matches!(
            mutation,
            PresentationMutation::TableCellContentSet {
                row: 0,
                column: 1,
                ..
            }
        )));
        assert!(change.mutations.iter().any(|mutation| matches!(mutation, PresentationMutation::TableCellStyleSet { cells, .. } if cells.len() == 2)));
        let SceneNodeKind::Table(table) = &engine.deck().slides[0].nodes[0].kind else {
            panic!("expected table")
        };
        assert_eq!(
            table
                .cells
                .iter()
                .find(|cell| cell.row == 0 && cell.column == 1)
                .unwrap()
                .content,
            body
        );
        engine.undo(engine.revision()).unwrap();
        let SceneNodeKind::Table(table) = &engine.deck().slides[0].nodes[0].kind else {
            panic!("expected table")
        };
        assert!(table.cells.iter().all(|cell| cell.content.text.is_empty()));
    }

    #[test]
    fn table_cell_style_rejects_empty_duplicate_or_non_anchor_selection() {
        let mut engine = PresentationEngine::new(deck(), 0).unwrap();
        run(
            &mut engine,
            vec![
                PresentationCommand::CreateSlide {
                    slide: slide("s1"),
                    index: 0,
                },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: table_node("table", "a"),
                    index: 0,
                },
            ],
        );
        let failure = engine
            .execute(PresentationCommandBatch {
                base_revision: engine.revision(),
                commands: vec![PresentationCommand::SetTableCellStyle {
                    slide_id: "s1".into(),
                    node_id: "table".into(),
                    cells: vec![],
                    style: TableCellStyle::default(),
                }],
            })
            .unwrap_err();
        assert!(matches!(
            failure,
            PresentationEngineError::EmptyTableCellSelection
        ));
        let failure = engine
            .execute(PresentationCommandBatch {
                base_revision: engine.revision(),
                commands: vec![PresentationCommand::SetTableCellStyle {
                    slide_id: "s1".into(),
                    node_id: "table".into(),
                    cells: vec![
                        TableCellAddress { row: 0, column: 0 },
                        TableCellAddress { row: 0, column: 0 },
                    ],
                    style: TableCellStyle::default(),
                }],
            })
            .unwrap_err();
        assert!(matches!(
            failure,
            PresentationEngineError::DuplicateTableCellAddress { row: 0, column: 0 }
        ));
        let failure = engine
            .execute(PresentationCommandBatch {
                base_revision: engine.revision(),
                commands: vec![PresentationCommand::SetTableCellContent {
                    slide_id: "s1".into(),
                    node_id: "table".into(),
                    row: 8,
                    column: 0,
                    content: PresentationRichText::default(),
                }],
            })
            .unwrap_err();
        assert!(matches!(
            failure,
            PresentationEngineError::MissingTableCell {
                row: 8,
                column: 0,
                ..
            }
        ));
    }

    #[test]
    fn table_structure_commands_preserve_anchors_merge_round_trip_and_undo() {
        let mut engine = PresentationEngine::new(deck(), 0).unwrap();
        run(
            &mut engine,
            vec![
                PresentationCommand::CreateSlide {
                    slide: slide("s1"),
                    index: 0,
                },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: table_node("table", "a"),
                    index: 0,
                },
            ],
        );
        let before = engine.deck().clone();
        let change = run(
            &mut engine,
            vec![
                PresentationCommand::InsertTableRows {
                    slide_id: "s1".into(),
                    node_id: "table".into(),
                    index: 1,
                    count: 2,
                },
                PresentationCommand::InsertTableColumns {
                    slide_id: "s1".into(),
                    node_id: "table".into(),
                    index: 1,
                    count: 1,
                },
                PresentationCommand::MergeTableCells {
                    slide_id: "s1".into(),
                    node_id: "table".into(),
                    start: TableCellAddress { row: 0, column: 0 },
                    end: TableCellAddress { row: 1, column: 1 },
                },
                PresentationCommand::SplitTableCell {
                    slide_id: "s1".into(),
                    node_id: "table".into(),
                    row: 0,
                    column: 0,
                },
                PresentationCommand::DeleteTableRow {
                    slide_id: "s1".into(),
                    node_id: "table".into(),
                    index: 1,
                },
                PresentationCommand::DeleteTableColumn {
                    slide_id: "s1".into(),
                    node_id: "table".into(),
                    index: 1,
                },
            ],
        );
        assert!(change
            .mutations
            .iter()
            .any(|mutation| matches!(mutation, PresentationMutation::TableCellsMerged { .. })));
        let SceneNodeKind::Table(table) = &engine.deck().slides[0].nodes[0].kind else {
            panic!("expected table")
        };
        assert_eq!((table.rows, table.columns), (3, 2));
        assert_eq!(table.cells.len(), 6);
        engine.deck().validate().unwrap();
        engine.undo(engine.revision()).unwrap();
        assert_eq!(engine.deck(), &before);
    }

    #[test]
    fn table_merge_rejects_partial_existing_merged_region_atomically() {
        let mut engine = PresentationEngine::new(deck(), 0).unwrap();
        run(
            &mut engine,
            vec![
                PresentationCommand::CreateSlide {
                    slide: slide("s1"),
                    index: 0,
                },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: table_node("table", "a"),
                    index: 0,
                },
                PresentationCommand::MergeTableCells {
                    slide_id: "s1".into(),
                    node_id: "table".into(),
                    start: TableCellAddress { row: 0, column: 0 },
                    end: TableCellAddress { row: 1, column: 1 },
                },
            ],
        );
        let before = engine.deck().clone();
        let error = engine
            .execute(PresentationCommandBatch {
                base_revision: engine.revision(),
                commands: vec![PresentationCommand::MergeTableCells {
                    slide_id: "s1".into(),
                    node_id: "table".into(),
                    start: TableCellAddress { row: 0, column: 1 },
                    end: TableCellAddress { row: 1, column: 1 },
                }],
            })
            .unwrap_err();
        assert!(matches!(
            error,
            PresentationEngineError::InvalidTableStructure(_)
        ));
        assert_eq!(engine.deck(), &before);
    }

    #[test]
    fn transition_and_animation_commands_are_granular_reversible_and_invalidate_timeline() {
        let mut engine = PresentationEngine::new(deck(), 0).unwrap();
        run(
            &mut engine,
            vec![
                PresentationCommand::CreateSlide {
                    slide: slide("s1"),
                    index: 0,
                },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: shape_node("shape", "a"),
                    index: 0,
                },
            ],
        );
        let change = run(
            &mut engine,
            vec![
                PresentationCommand::SetSlideTransition {
                    slide_id: "s1".into(),
                    transition: Some(SlideTransition {
                        kind: oo_schema::presentation_v5::TransitionKind::Fade,
                        duration_ms: 240,
                    }),
                },
                PresentationCommand::UpsertAnimation {
                    slide_id: "s1".into(),
                    animation: AnimationEntry {
                        id: "fade-in".into(),
                        target_node_id: "shape".into(),
                        trigger: AnimationTrigger::OnClick,
                        preset: AnimationPreset::Fade,
                        duration_ms: 180,
                        delay_ms: 0,
                        order_key: "z".into(),
                    },
                },
            ],
        );
        assert!(change.mutations.iter().any(|mutation| matches!(mutation, PresentationMutation::SlideTransitionSet { slide_id } if slide_id == "s1")));
        assert!(change.mutations.iter().any(|mutation| matches!(mutation, PresentationMutation::AnimationUpserted { slide_id, animation_id } if slide_id == "s1" && animation_id == "fade-in")));
        assert!(change.dirty_thumbnail_ids.contains(&"s1".into()));
        assert_eq!(engine.deck().slides[0].timeline.entries.len(), 1);

        let moved = run(
            &mut engine,
            vec![PresentationCommand::MoveAnimation {
                slide_id: "s1".into(),
                animation_id: "fade-in".into(),
                index: 0,
            }],
        );
        assert!(moved
            .mutations
            .iter()
            .any(|mutation| matches!(mutation, PresentationMutation::AnimationMoved { .. })));
        assert_eq!(
            engine.deck().slides[0].timeline.entries[0].order_key,
            "0000000000000000"
        );
        engine.undo(engine.revision()).unwrap();
        engine.undo(engine.revision()).unwrap();
        assert!(engine.deck().slides[0].timeline.entries.is_empty());
        assert!(engine.deck().slides[0].transition.is_none());
    }

    #[test]
    fn media_config_is_typed_reversible_and_rejects_dangling_assets() {
        let mut deck = deck();
        deck.assets.push(AssetRef {
            asset_id: "video-v1".into(),
            digest: "sha256:video-v1".into(),
            mime_type: "video/mp4".into(),
            width: None,
            height: None,
            original_asset_id: None,
        });
        deck.assets.push(AssetRef {
            asset_id: "poster-v1".into(),
            digest: "sha256:poster-v1".into(),
            mime_type: "image/png".into(),
            width: None,
            height: None,
            original_asset_id: None,
        });
        let mut engine = PresentationEngine::new(deck, 0).unwrap();
        let video = SceneNode {
            id: "video".into(),
            parent_id: None,
            order_key: "a".into(),
            name: None,
            alt_text: None,
            layout_placeholder_id: None,
            transform: transform(),
            visible: true,
            locked: false,
            opacity: 1.0,
            kind: SceneNodeKind::Video(MediaNode {
                asset_id: "video-v1".into(),
                poster_asset_id: None,
            }),
        };
        run(
            &mut engine,
            vec![
                PresentationCommand::CreateSlide {
                    slide: slide("s1"),
                    index: 0,
                },
                PresentationCommand::InsertNode {
                    slide_id: "s1".into(),
                    node: video,
                    index: 0,
                },
            ],
        );
        let change = run(
            &mut engine,
            vec![PresentationCommand::SetMediaConfig {
                slide_id: "s1".into(),
                node_id: "video".into(),
                media: MediaNode {
                    asset_id: "video-v1".into(),
                    poster_asset_id: Some("poster-v1".into()),
                },
            }],
        );
        assert!(change
            .mutations
            .iter()
            .any(|mutation| matches!(mutation, PresentationMutation::MediaConfigSet { .. })));
        assert_eq!(
            engine.deck().slides[0].nodes[0].kind,
            SceneNodeKind::Video(MediaNode {
                asset_id: "video-v1".into(),
                poster_asset_id: Some("poster-v1".into())
            })
        );
        engine.undo(engine.revision()).unwrap();
        assert_eq!(
            engine.deck().slides[0].nodes[0].kind,
            SceneNodeKind::Video(MediaNode {
                asset_id: "video-v1".into(),
                poster_asset_id: None
            })
        );
        let error = engine
            .execute(PresentationCommandBatch {
                base_revision: engine.revision(),
                commands: vec![PresentationCommand::SetMediaConfig {
                    slide_id: "s1".into(),
                    node_id: "video".into(),
                    media: MediaNode {
                        asset_id: "missing".into(),
                        poster_asset_id: None,
                    },
                }],
            })
            .unwrap_err();
        assert!(matches!(error, PresentationEngineError::Schema(_)));
    }

    #[test]
    fn master_layout_lifecycle_is_typed_reversible_and_invalidates_consumers() {
        let mut engine = PresentationEngine::new(deck(), 0).unwrap();
        let master = SlideMaster {
            id: "master-default".into(),
            name: "默认母版".into(),
            background: SlideBackground::default(),
            placeholders: vec![MasterPlaceholder {
                id: "master-title".into(),
                kind: PlaceholderKind::Title,
                transform: transform(),
                default_text: None,
            }],
        };
        let layout = SlideLayout {
            id: "layout-title".into(),
            master_id: "master-default".into(),
            name: "标题页".into(),
            placeholders: vec![LayoutPlaceholder {
                id: "layout-title-placeholder".into(),
                kind: PlaceholderKind::Title,
                master_placeholder_id: Some("master-title".into()),
                transform: transform(),
                default_text: None,
            }],
        };
        run(
            &mut engine,
            vec![
                PresentationCommand::CreateMaster {
                    master: master.clone(),
                },
                PresentationCommand::CreateLayout {
                    layout: layout.clone(),
                },
                PresentationCommand::CreateSlide {
                    slide: Slide {
                        layout_id: Some(layout.id.clone()),
                        ..slide("s1")
                    },
                    index: 0,
                },
            ],
        );
        let change = run(
            &mut engine,
            vec![PresentationCommand::UpdateMaster {
                master: SlideMaster {
                    name: "新版默认母版".into(),
                    ..master.clone()
                },
            }],
        );
        assert!(change.mutations.iter().any(|mutation| matches!(
            mutation,
            PresentationMutation::MasterUpdated { master_id } if master_id == "master-default"
        )));
        assert_eq!(change.dirty_thumbnail_ids, vec!["s1"]);
        let before = engine.deck().clone();
        let error = engine
            .execute(PresentationCommandBatch {
                base_revision: engine.revision(),
                commands: vec![PresentationCommand::DeleteLayout {
                    layout_id: layout.id.clone(),
                }],
            })
            .unwrap_err();
        assert!(matches!(error, PresentationEngineError::LayoutInUse(id) if id == "layout-title"));
        assert_eq!(engine.deck(), &before);
        let error = engine
            .execute(PresentationCommandBatch {
                base_revision: engine.revision(),
                commands: vec![PresentationCommand::DeleteMaster {
                    master_id: master.id.clone(),
                }],
            })
            .unwrap_err();
        assert!(
            matches!(error, PresentationEngineError::MasterInUse(id) if id == "master-default")
        );

        run(
            &mut engine,
            vec![PresentationCommand::SetSlideLayout {
                slide_id: "s1".into(),
                layout_id: None,
            }],
        );
        run(
            &mut engine,
            vec![PresentationCommand::DeleteLayout {
                layout_id: layout.id.clone(),
            }],
        );
        engine.undo(engine.revision()).unwrap();
        assert_eq!(engine.deck().layouts, vec![layout]);
    }
}
