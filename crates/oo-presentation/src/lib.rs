//! Canonical v5 Presentation transaction boundary.
//!
//! A [`Deck`] is the only mutable domain state in this crate. Commands are semantic and typed;
//! renderers may consume a [`PresentationChangeSet`], but neither Canvas nor DOM state enters the
//! journal. The engine deliberately stores inverse mutations for only the affected entities rather
//! than cloning a whole deck for each command.

pub mod v5_projection;

use oo_protocol::{EntityRef, Invalidation, MutationRecord};
use oo_schema::presentation_v5::{
    AnimationEntry, Deck, DeckTheme, ImageNode, MediaNode, NodeTransform, PresentationRichText,
    SceneNode, SceneNodeKind, ShapeStyle, Slide, SlideBackground, SlidePageSpec, SlideTransition,
    TextFrame, Timeline,
};
use serde::{Deserialize, Serialize};
use v5_projection::{DeckProjection, ProjectionChange};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationCommandBatch {
    pub base_revision: u64,
    pub commands: Vec<PresentationCommand>,
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
    SetPageSpec {
        page_spec: SlidePageSpec,
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
    SetShapeStyle {
        slide_id: String,
        node_id: String,
        style: ShapeStyle,
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

/// Public, serializable facts emitted after a successful transaction. They are intentionally
/// smaller than the private journal: consumers need invalidation facts, not retained snapshots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PresentationMutation {
    PageSpecSet,
    SlideInserted {
        slide_id: String,
    },
    SlideDeleted {
        slide_id: String,
    },
    SlideMoved {
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
    ShapeStyleSet {
        slide_id: String,
        node_id: String,
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
            Self::PageSpecSet => "presentation.pageSpecSet",
            Self::SlideInserted { .. } => "presentation.slideInserted",
            Self::SlideDeleted { .. } => "presentation.slideDeleted",
            Self::SlideMoved { .. } => "presentation.slideMoved",
            Self::NodeInserted { .. } => "presentation.nodeInserted",
            Self::NodeDeleted { .. } => "presentation.nodeDeleted",
            Self::NodeMoved { .. } => "presentation.nodeMoved",
            Self::NodeReordered { .. } => "presentation.nodeReordered",
            Self::NodesGrouped { .. } => "presentation.nodesGrouped",
            Self::NodesUngrouped { .. } => "presentation.nodesUngrouped",
            Self::NodeTransformSet { .. } => "presentation.nodeTransformSet",
            Self::ShapeStyleSet { .. } => "presentation.shapeStyleSet",
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
        match self {
            Self::PageSpecSet | Self::ThemeSet => (vec![deck()], vec![deck()], true),
            Self::SlideInserted { slide_id }
            | Self::SlideDeleted { slide_id }
            | Self::SlideMoved { slide_id } => (vec![slide(slide_id)], vec![deck()], true),
            Self::NodeInserted { slide_id, node_id }
            | Self::NodeDeleted { slide_id, node_id }
            | Self::NodeMoved { slide_id, node_id }
            | Self::NodeReordered { slide_id, node_id }
            | Self::NodeTransformSet { slide_id, node_id }
            | Self::ShapeStyleSet { slide_id, node_id }
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
    SetPageSpec(SlidePageSpec),
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
    SetShapeStyle {
        slide_id: String,
        node_id: String,
        style: ShapeStyle,
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
            PresentationMutation::PageSpecSet | PresentationMutation::ThemeSet => {
                ProjectionChange::DeckThemeChanged
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
            | PresentationMutation::ShapeStyleSet { slide_id, .. }
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
        PresentationCommand::SetPageSpec { page_spec } => {
            let previous = std::mem::replace(&mut deck.page_spec, page_spec);
            Ok((
                InverseMutation::SetPageSpec(previous),
                PresentationMutation::PageSpecSet,
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

fn apply_inverse(deck: &mut Deck, inverse: InverseMutation) -> Result<(), PresentationEngineError> {
    match inverse {
        InverseMutation::SetPageSpec(page_spec) => deck.page_spec = page_spec,
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
            PresentationMutation::SlideInserted { slide_id } => {
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
    #[error("没有可撤销的 Presentation mutation")]
    NothingToUndo,
    #[error("没有可重做的 Presentation mutation")]
    NothingToRedo,
    #[error("重复的 slide：{0}")]
    DuplicateSlide(String),
    #[error("重复的 node：{0}")]
    DuplicateNode(String),
    #[error("不存在的 slide：{0}")]
    MissingSlide(String),
    #[error("不存在的 node：{0}")]
    MissingNode(String),
    #[error("不存在的 animation：{0}")]
    MissingAnimation(String),
    #[error("node {0} 仍有子节点，必须先显式 ungroup 或移动子节点")]
    HasChildren(String),
    #[error("node {0} 不是 text node")]
    NotTextNode(String),
    #[error("node {0} 不是 shape node")]
    NotShapeNode(String),
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
        AnimationPreset, AnimationTrigger, AssetRef, DeckTheme, GroupNode, Insets, MediaNode,
        SceneNodeKind, ShapeGeometry, ShapeNode, TextAutoFit, TextNode, TextVerticalAlign,
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
}
