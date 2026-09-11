//! Mindmap Artifact 的 Graph engine。
//!
//! 思维导图不是 Document Block：节点通过 parentId 组成有根图，文本是节点属性。本 crate
//! 负责结构事务、原子性、schema 不变量，以及只读布局 projection；边路由和最终绘制仍由
//! 上层 renderer 负责。

pub mod exchange;
pub use exchange::*;

use std::collections::{HashMap, HashSet};

use oo_protocol::{EntityRef, Invalidation, MutationRecord};
use oo_schema::{
    Color, InlineRun, InlineStyle, MindmapBoundary, MindmapConnectorStyle, MindmapEdge,
    MindmapFormula, MindmapFormulaDisplay, MindmapLayoutKind, MindmapModel, MindmapNode,
    MindmapNodeStyle, MindmapNodeSupplement, MindmapSettings, MindmapSummary, RichText,
    SchemaValidationError, VerticalAlign,
};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MindmapCommandBatch {
    pub base_revision: u64,
    pub commands: Vec<MindmapCommand>,
}

/// A half-open Unicode scalar range inside one mindmap node's RichText.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MindmapTextRange {
    pub start: usize,
    pub end: usize,
}

/// Strict tri-state patch for the renderer-independent inline style fields.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MindmapInlineStylePatch {
    #[serde(default, deserialize_with = "deserialize_tri_state")]
    pub bold: Option<Option<bool>>,
    #[serde(default, deserialize_with = "deserialize_tri_state")]
    pub italic: Option<Option<bool>>,
    #[serde(default, deserialize_with = "deserialize_tri_state")]
    pub underline: Option<Option<bool>>,
    #[serde(default, deserialize_with = "deserialize_tri_state")]
    pub strikethrough: Option<Option<bool>>,
    #[serde(default, deserialize_with = "deserialize_tri_state")]
    pub font_family: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_tri_state")]
    pub font_size: Option<Option<f32>>,
    #[serde(default, deserialize_with = "deserialize_tri_state")]
    pub color: Option<Option<Color>>,
    #[serde(default, deserialize_with = "deserialize_tri_state")]
    pub highlight: Option<Option<Color>>,
    #[serde(default, deserialize_with = "deserialize_tri_state")]
    pub vertical_align: Option<Option<VerticalAlign>>,
}

fn deserialize_tri_state<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let value = Value::deserialize(deserializer)?;
    if value.is_null() {
        Ok(Some(None))
    } else {
        T::deserialize(value)
            .map(|item| Some(Some(item)))
            .map_err(serde::de::Error::custom)
    }
}

impl MindmapInlineStylePatch {
    fn is_empty(&self) -> bool {
        self.bold.is_none()
            && self.italic.is_none()
            && self.underline.is_none()
            && self.strikethrough.is_none()
            && self.font_family.is_none()
            && self.font_size.is_none()
            && self.color.is_none()
            && self.highlight.is_none()
            && self.vertical_align.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MindmapCommand {
    SetSettings {
        settings: MindmapSettings,
    },
    AddNode {
        node_id: String,
        #[serde(default)]
        parent_id: Option<String>,
        #[serde(default)]
        content: Option<RichText>,
        #[serde(default)]
        attrs: Map<String, Value>,
        #[serde(default)]
        index: usize,
    },
    AddEdge {
        edge: MindmapEdge,
    },
    UpdateNode {
        node_id: String,
        #[serde(default)]
        content: Option<RichText>,
        #[serde(default)]
        attrs: Option<Map<String, Value>>,
    },
    ReplaceNodeText {
        node_id: String,
        content: Option<RichText>,
    },
    PatchNodeTextRange {
        node_id: String,
        range: MindmapTextRange,
        patch: MindmapInlineStylePatch,
    },
    SetNodeStyle {
        node_id: String,
        style: MindmapNodeStyle,
    },
    SetNodeSupplement {
        node_id: String,
        supplement: MindmapNodeSupplement,
    },
    SetNodeCollapsed {
        node_id: String,
        collapsed: bool,
    },
    UpdateEdge {
        edge_id: String,
        #[serde(default)]
        source_id: Option<String>,
        #[serde(default)]
        target_id: Option<String>,
        /// Omitted keeps the current label; explicit null clears it.
        #[serde(default, deserialize_with = "deserialize_tri_state")]
        label: Option<Option<RichText>>,
        #[serde(default)]
        attrs: Option<Map<String, Value>>,
    },
    SetEdgeStyle {
        edge_id: String,
        style: MindmapConnectorStyle,
    },
    DeleteEdge {
        edge_id: String,
    },
    AddSummary {
        summary: MindmapSummary,
    },
    UpdateSummary {
        summary_id: String,
        #[serde(default)]
        start_node_id: Option<String>,
        #[serde(default)]
        end_node_id: Option<String>,
        #[serde(default)]
        content: Option<RichText>,
    },
    DeleteSummary {
        summary_id: String,
    },
    AddBoundary {
        boundary: MindmapBoundary,
    },
    UpdateBoundary {
        boundary_id: String,
        #[serde(default)]
        root_node_id: Option<String>,
        #[serde(default, deserialize_with = "deserialize_tri_state")]
        label: Option<Option<RichText>>,
    },
    DeleteBoundary {
        boundary_id: String,
    },
    AddFormula {
        formula: MindmapFormula,
    },
    UpdateFormula {
        formula_id: String,
        #[serde(default)]
        node_id: Option<String>,
        #[serde(default)]
        source: Option<String>,
        #[serde(default)]
        display: Option<MindmapFormulaDisplay>,
    },
    DeleteFormula {
        formula_id: String,
    },
    MoveNode {
        node_id: String,
        #[serde(default)]
        new_parent_id: Option<String>,
        #[serde(default)]
        index: usize,
    },
    DeleteNode {
        node_id: String,
    },
}

impl MindmapCommand {
    pub const fn type_id(&self) -> &'static str {
        match self {
            Self::SetSettings { .. } => "mindmap.setSettings",
            Self::AddNode { .. } => "mindmap.addNode",
            Self::AddEdge { .. } => "mindmap.addEdge",
            Self::UpdateNode { .. } => "mindmap.updateNode",
            Self::ReplaceNodeText { .. } => "mindmap.replaceNodeText",
            Self::PatchNodeTextRange { .. } => "mindmap.patchNodeTextRange",
            Self::SetNodeStyle { .. } => "mindmap.setNodeStyle",
            Self::SetNodeSupplement { .. } => "mindmap.setNodeSupplement",
            Self::SetNodeCollapsed { .. } => "mindmap.setNodeCollapsed",
            Self::UpdateEdge { .. } => "mindmap.updateEdge",
            Self::SetEdgeStyle { .. } => "mindmap.setEdgeStyle",
            Self::DeleteEdge { .. } => "mindmap.deleteEdge",
            Self::AddSummary { .. } => "mindmap.addSummary",
            Self::UpdateSummary { .. } => "mindmap.updateSummary",
            Self::DeleteSummary { .. } => "mindmap.deleteSummary",
            Self::AddBoundary { .. } => "mindmap.addBoundary",
            Self::UpdateBoundary { .. } => "mindmap.updateBoundary",
            Self::DeleteBoundary { .. } => "mindmap.deleteBoundary",
            Self::AddFormula { .. } => "mindmap.addFormula",
            Self::UpdateFormula { .. } => "mindmap.updateFormula",
            Self::DeleteFormula { .. } => "mindmap.deleteFormula",
            Self::MoveNode { .. } => "mindmap.moveNode",
            Self::DeleteNode { .. } => "mindmap.deleteNode",
        }
    }

    pub const fn scope(&self) -> &'static str {
        match self {
            Self::SetSettings { .. } => "mindmap.graph",
            Self::AddNode { .. }
            | Self::UpdateNode { .. }
            | Self::ReplaceNodeText { .. }
            | Self::PatchNodeTextRange { .. }
            | Self::SetNodeStyle { .. }
            | Self::SetNodeSupplement { .. }
            | Self::SetNodeCollapsed { .. }
            | Self::MoveNode { .. }
            | Self::DeleteNode { .. } => "mindmap.node",
            Self::AddEdge { .. }
            | Self::UpdateEdge { .. }
            | Self::SetEdgeStyle { .. }
            | Self::DeleteEdge { .. } => "mindmap.edge",
            Self::AddSummary { .. } | Self::UpdateSummary { .. } | Self::DeleteSummary { .. } => {
                "mindmap.summary"
            }
            Self::AddBoundary { .. }
            | Self::UpdateBoundary { .. }
            | Self::DeleteBoundary { .. } => "mindmap.boundary",
            Self::AddFormula { .. } | Self::UpdateFormula { .. } | Self::DeleteFormula { .. } => {
                "mindmap.formula"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MindmapCommandDescriptor {
    pub type_id: &'static str,
    pub scope: &'static str,
}

/// The engine command registry is the single source for REST capability
/// discovery. Sample variants make the enum match exhaustive at compile time;
/// the contract test below verifies unique, decodable wire identities.
pub fn mindmap_command_registry() -> Vec<MindmapCommandDescriptor> {
    let commands = vec![
        MindmapCommand::SetSettings {
            settings: MindmapSettings::default(),
        },
        MindmapCommand::AddNode {
            node_id: String::new(),
            parent_id: None,
            content: None,
            attrs: Map::new(),
            index: 0,
        },
        MindmapCommand::AddEdge {
            edge: MindmapEdge::default(),
        },
        MindmapCommand::UpdateNode {
            node_id: String::new(),
            content: None,
            attrs: None,
        },
        MindmapCommand::ReplaceNodeText {
            node_id: String::new(),
            content: None,
        },
        MindmapCommand::PatchNodeTextRange {
            node_id: String::new(),
            range: MindmapTextRange { start: 0, end: 1 },
            patch: MindmapInlineStylePatch {
                bold: Some(Some(true)),
                ..MindmapInlineStylePatch::default()
            },
        },
        MindmapCommand::SetNodeStyle {
            node_id: String::new(),
            style: MindmapNodeStyle::default(),
        },
        MindmapCommand::SetNodeSupplement {
            node_id: String::new(),
            supplement: MindmapNodeSupplement::default(),
        },
        MindmapCommand::SetNodeCollapsed {
            node_id: String::new(),
            collapsed: false,
        },
        MindmapCommand::UpdateEdge {
            edge_id: String::new(),
            source_id: None,
            target_id: None,
            label: None,
            attrs: None,
        },
        MindmapCommand::SetEdgeStyle {
            edge_id: String::new(),
            style: MindmapConnectorStyle::default(),
        },
        MindmapCommand::DeleteEdge {
            edge_id: String::new(),
        },
        MindmapCommand::AddSummary {
            summary: MindmapSummary {
                id: String::new(),
                start_node_id: String::new(),
                end_node_id: String::new(),
                content: RichText::default(),
            },
        },
        MindmapCommand::UpdateSummary {
            summary_id: String::new(),
            start_node_id: None,
            end_node_id: None,
            content: None,
        },
        MindmapCommand::DeleteSummary {
            summary_id: String::new(),
        },
        MindmapCommand::AddBoundary {
            boundary: MindmapBoundary {
                id: String::new(),
                root_node_id: String::new(),
                label: None,
            },
        },
        MindmapCommand::UpdateBoundary {
            boundary_id: String::new(),
            root_node_id: None,
            label: None,
        },
        MindmapCommand::DeleteBoundary {
            boundary_id: String::new(),
        },
        MindmapCommand::AddFormula {
            formula: MindmapFormula {
                id: String::new(),
                node_id: String::new(),
                source: String::new(),
                display: MindmapFormulaDisplay::Inline,
            },
        },
        MindmapCommand::UpdateFormula {
            formula_id: String::new(),
            node_id: None,
            source: None,
            display: None,
        },
        MindmapCommand::DeleteFormula {
            formula_id: String::new(),
        },
        MindmapCommand::MoveNode {
            node_id: String::new(),
            new_parent_id: None,
            index: 0,
        },
        MindmapCommand::DeleteNode {
            node_id: String::new(),
        },
    ];
    commands
        .iter()
        .map(|command| MindmapCommandDescriptor {
            type_id: command.type_id(),
            scope: command.scope(),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MindmapChangeSet {
    pub revision: u64,
    pub invalidation: Invalidation,
    pub mutations: Vec<MindmapMutation>,
}

/// Typed graph mutations are the only output consumed by history/collaboration adapters.
/// Layout and viewport state never appears here because it is a renderer projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MindmapMutation {
    SettingsChanged {
        before: MindmapSettings,
        after: MindmapSettings,
    },
    AddNode {
        node: MindmapNode,
        index: usize,
    },
    AddEdge {
        edge: MindmapEdge,
    },
    UpdateNode {
        node_id: String,
        before: Box<MindmapNode>,
        after: Box<MindmapNode>,
    },
    ReplaceNodeText {
        node_id: String,
        before: Option<RichText>,
        after: Option<RichText>,
    },
    PatchNodeTextRange {
        node_id: String,
        range: MindmapTextRange,
        before: RichText,
        after: RichText,
    },
    SetNodeStyle {
        node_id: String,
        before: MindmapNodeStyle,
        after: MindmapNodeStyle,
    },
    SetNodeSupplement {
        node_id: String,
        before: MindmapNodeSupplement,
        after: MindmapNodeSupplement,
    },
    SetNodeCollapsed {
        node_id: String,
        before: bool,
        after: bool,
    },
    UpdateEdge {
        edge_id: String,
        before: MindmapEdge,
        after: MindmapEdge,
    },
    SetEdgeStyle {
        edge_id: String,
        before: MindmapConnectorStyle,
        after: MindmapConnectorStyle,
    },
    DeleteEdge {
        edge: MindmapEdge,
    },
    AddSummary {
        summary: MindmapSummary,
    },
    UpdateSummary {
        summary_id: String,
        before: MindmapSummary,
        after: MindmapSummary,
    },
    DeleteSummary {
        summary: MindmapSummary,
    },
    AddBoundary {
        boundary: MindmapBoundary,
    },
    UpdateBoundary {
        boundary_id: String,
        before: MindmapBoundary,
        after: MindmapBoundary,
    },
    DeleteBoundary {
        boundary: MindmapBoundary,
    },
    AddFormula {
        formula: MindmapFormula,
    },
    UpdateFormula {
        formula_id: String,
        before: MindmapFormula,
        after: MindmapFormula,
    },
    DeleteFormula {
        formula: MindmapFormula,
    },
    MoveNode {
        node_id: String,
        from_parent_id: Option<String>,
        from_index: usize,
        to_parent_id: Option<String>,
        to_index: usize,
    },
    DeleteNodes {
        nodes: Vec<MindmapNode>,
        #[serde(default)]
        edges: Vec<MindmapEdge>,
        #[serde(default)]
        summaries: Vec<MindmapSummary>,
        #[serde(default)]
        boundaries: Vec<MindmapBoundary>,
        #[serde(default)]
        formulas: Vec<MindmapFormula>,
        parent_id: Option<String>,
        index: usize,
    },
    RestoreNodes {
        nodes: Vec<MindmapNode>,
        #[serde(default)]
        edges: Vec<MindmapEdge>,
        #[serde(default)]
        summaries: Vec<MindmapSummary>,
        #[serde(default)]
        boundaries: Vec<MindmapBoundary>,
        #[serde(default)]
        formulas: Vec<MindmapFormula>,
        parent_id: Option<String>,
        index: usize,
    },
}

impl MindmapMutation {
    pub fn type_id(&self) -> &'static str {
        match self {
            Self::SettingsChanged { .. } => "mindmap.settingsChanged",
            Self::AddNode { .. } => "mindmap.nodeInserted",
            Self::AddEdge { .. } => "mindmap.edgeInserted",
            Self::UpdateNode { .. } => "mindmap.nodeUpdated",
            Self::ReplaceNodeText { .. } => "mindmap.nodeTextReplaced",
            Self::PatchNodeTextRange { .. } => "mindmap.nodeTextRangePatched",
            Self::SetNodeStyle { .. } => "mindmap.nodeStyleChanged",
            Self::SetNodeSupplement { .. } => "mindmap.nodeSupplementChanged",
            Self::SetNodeCollapsed { .. } => "mindmap.nodeCollapsedChanged",
            Self::UpdateEdge { .. } => "mindmap.edgeUpdated",
            Self::SetEdgeStyle { .. } => "mindmap.edgeStyleChanged",
            Self::DeleteEdge { .. } => "mindmap.edgeDeleted",
            Self::AddSummary { .. } => "mindmap.summaryInserted",
            Self::UpdateSummary { .. } => "mindmap.summaryUpdated",
            Self::DeleteSummary { .. } => "mindmap.summaryDeleted",
            Self::AddBoundary { .. } => "mindmap.boundaryInserted",
            Self::UpdateBoundary { .. } => "mindmap.boundaryUpdated",
            Self::DeleteBoundary { .. } => "mindmap.boundaryDeleted",
            Self::AddFormula { .. } => "mindmap.formulaInserted",
            Self::UpdateFormula { .. } => "mindmap.formulaUpdated",
            Self::DeleteFormula { .. } => "mindmap.formulaDeleted",
            Self::MoveNode { .. } => "mindmap.nodeMoved",
            Self::DeleteNodes { .. } => "mindmap.nodeDeleted",
            Self::RestoreNodes { .. } => "mindmap.nodeRestored",
        }
    }

    pub fn to_record(&self) -> Result<MutationRecord, serde_json::Error> {
        Ok(MutationRecord {
            type_id: self.type_id().into(),
            payload: serde_json::to_value(self)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
struct MindmapJournalEntry {
    commands: Vec<MindmapCommand>,
    inverses: Vec<MindmapInverse>,
    mutations: Vec<MindmapMutation>,
}

/// Private inverse journal. It stores only touched graph entities and exact
/// vector positions; no full-map JSON or renderer state enters history.
#[derive(Debug, Clone, PartialEq)]
enum MindmapInverse {
    RestoreSettings(MindmapSettings),
    RemoveNode {
        node_id: String,
    },
    RemoveEdge {
        edge_id: String,
    },
    RestoreNode(MindmapNode),
    RestoreCollapsed {
        node_id: String,
        collapsed: bool,
    },
    RestoreExistingEdge(MindmapEdge),
    RestoreEdge {
        edge: MindmapEdge,
        index: usize,
    },
    RemoveSummary(String),
    RestoreSummary {
        summary: MindmapSummary,
        index: usize,
    },
    RestoreExistingSummary(MindmapSummary),
    RemoveBoundary(String),
    RestoreBoundary {
        boundary: MindmapBoundary,
        index: usize,
    },
    RestoreExistingBoundary(MindmapBoundary),
    RemoveFormula(String),
    RestoreFormula {
        formula: MindmapFormula,
        index: usize,
    },
    RestoreExistingFormula(MindmapFormula),
    MoveNode {
        node_id: String,
        parent_id: Option<String>,
        index: usize,
    },
    RestoreDeleted {
        root: Option<String>,
        nodes: Vec<(usize, MindmapNode)>,
        edges: Vec<(usize, MindmapEdge)>,
        summaries: Vec<(usize, MindmapSummary)>,
        boundaries: Vec<(usize, MindmapBoundary)>,
        formulas: Vec<(usize, MindmapFormula)>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct MindmapEngine {
    model: MindmapModel,
    revision: u64,
    undo: Vec<MindmapJournalEntry>,
    redo: Vec<MindmapJournalEntry>,
}

impl MindmapEngine {
    pub fn new(model: MindmapModel, revision: u64) -> Result<Self, MindmapEngineError> {
        validate(&model)?;
        Ok(Self {
            model,
            revision,
            undo: Vec::new(),
            redo: Vec::new(),
        })
    }

    pub fn model(&self) -> &MindmapModel {
        &self.model
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

    pub fn execute(
        &mut self,
        batch: MindmapCommandBatch,
    ) -> Result<MindmapChangeSet, MindmapEngineError> {
        if batch.commands.is_empty() {
            return Err(MindmapEngineError::EmptyBatch);
        }
        if batch.base_revision != self.revision {
            return Err(MindmapEngineError::RevisionConflict {
                expected: self.revision,
                actual: batch.base_revision,
            });
        }

        let commands = batch.commands;
        let mut candidate = self.model.clone();
        let mut changed_nodes = Vec::new();
        let mut changed_edges = Vec::new();
        let mut changed_advanced = Vec::<(&'static str, String)>::new();
        let mut mutations = Vec::new();
        let mut inverses = Vec::with_capacity(commands.len());
        let mut structure_changed = false;
        for command in commands.iter().cloned() {
            match command {
                MindmapCommand::SetSettings { settings } => {
                    let before = std::mem::replace(&mut candidate.settings, settings.clone());
                    inverses.push(MindmapInverse::RestoreSettings(before.clone()));
                    mutations.push(MindmapMutation::SettingsChanged {
                        before,
                        after: settings,
                    });
                }
                MindmapCommand::AddNode {
                    node_id,
                    parent_id,
                    content,
                    attrs,
                    index,
                } => {
                    ensure_id(&node_id)?;
                    if candidate.nodes.iter().any(|node| node.id == node_id) {
                        return Err(MindmapEngineError::DuplicateNode(node_id));
                    }
                    validate_parent(&candidate, parent_id.as_deref(), None)?;
                    let node = MindmapNode {
                        id: node_id.clone(),
                        parent_id: parent_id.clone(),
                        content,
                        style: MindmapNodeStyle::default(),
                        supplement: MindmapNodeSupplement::default(),
                        attrs,
                        collapsed: false,
                    };
                    let insertion = insertion_index(&candidate, parent_id.as_deref(), index)?;
                    if parent_id.is_none() {
                        candidate.root = Some(node_id.clone());
                    }
                    candidate.nodes.insert(insertion, node);
                    inverses.push(MindmapInverse::RemoveNode {
                        node_id: node_id.clone(),
                    });
                    changed_nodes.push(node_id.clone());
                    mutations.push(MindmapMutation::AddNode {
                        node: candidate.nodes[insertion].clone(),
                        index,
                    });
                    structure_changed = true;
                }
                MindmapCommand::AddEdge { edge } => {
                    ensure_id(&edge.id)?;
                    ensure_id(&edge.source_id)?;
                    ensure_id(&edge.target_id)?;
                    if edge.source_id == edge.target_id {
                        return Err(MindmapEngineError::SelfEdge(edge.id));
                    }
                    if candidate.edges.iter().any(|item| item.id == edge.id) {
                        return Err(MindmapEngineError::DuplicateEdge(edge.id));
                    }
                    node(&candidate, &edge.source_id)?;
                    node(&candidate, &edge.target_id)?;
                    let edge_id = edge.id.clone();
                    candidate.edges.push(edge.clone());
                    inverses.push(MindmapInverse::RemoveEdge {
                        edge_id: edge_id.clone(),
                    });
                    changed_edges.push(edge_id);
                    mutations.push(MindmapMutation::AddEdge { edge });
                    structure_changed = true;
                }
                MindmapCommand::UpdateNode {
                    node_id,
                    content,
                    attrs,
                } => {
                    let before = node(&candidate, &node_id)?.clone();
                    let node = node_mut(&mut candidate, &node_id)?;
                    if let Some(content) = content {
                        node.content = Some(content);
                    }
                    if let Some(attrs) = attrs {
                        node.attrs = attrs;
                    }
                    let after = node.clone();
                    inverses.push(MindmapInverse::RestoreNode(before.clone()));
                    changed_nodes.push(node_id.clone());
                    mutations.push(MindmapMutation::UpdateNode {
                        node_id,
                        before: Box::new(before),
                        after: Box::new(after),
                    });
                }
                MindmapCommand::ReplaceNodeText { node_id, content } => {
                    let before_node = node(&candidate, &node_id)?.clone();
                    let current = node_mut(&mut candidate, &node_id)?;
                    let before = std::mem::replace(&mut current.content, content.clone());
                    inverses.push(MindmapInverse::RestoreNode(before_node));
                    changed_nodes.push(node_id.clone());
                    mutations.push(MindmapMutation::ReplaceNodeText {
                        node_id,
                        before,
                        after: content,
                    });
                }
                MindmapCommand::PatchNodeTextRange {
                    node_id,
                    range,
                    patch,
                } => {
                    if patch.is_empty() {
                        return Err(MindmapEngineError::InvalidInlinePatch);
                    }
                    let before_node = node(&candidate, &node_id)?.clone();
                    let current = node_mut(&mut candidate, &node_id)?;
                    let content = current
                        .content
                        .as_mut()
                        .ok_or_else(|| MindmapEngineError::MissingNodeText(node_id.clone()))?;
                    validate_text_range(range, content.text.chars().count())?;
                    let before = content.clone();
                    patch_rich_text_runs(content, range, &patch);
                    let after = content.clone();
                    inverses.push(MindmapInverse::RestoreNode(before_node));
                    changed_nodes.push(node_id.clone());
                    mutations.push(MindmapMutation::PatchNodeTextRange {
                        node_id,
                        range,
                        before,
                        after,
                    });
                }
                MindmapCommand::SetNodeStyle { node_id, style } => {
                    let before_node = node(&candidate, &node_id)?.clone();
                    let current = node_mut(&mut candidate, &node_id)?;
                    let before = std::mem::replace(&mut current.style, style.clone());
                    inverses.push(MindmapInverse::RestoreNode(before_node));
                    changed_nodes.push(node_id.clone());
                    mutations.push(MindmapMutation::SetNodeStyle {
                        node_id,
                        before,
                        after: style,
                    });
                }
                MindmapCommand::SetNodeSupplement {
                    node_id,
                    supplement,
                } => {
                    let before_node = node(&candidate, &node_id)?.clone();
                    let current = node_mut(&mut candidate, &node_id)?;
                    let before = std::mem::replace(&mut current.supplement, supplement.clone());
                    inverses.push(MindmapInverse::RestoreNode(before_node));
                    changed_nodes.push(node_id.clone());
                    mutations.push(MindmapMutation::SetNodeSupplement {
                        node_id,
                        before,
                        after: supplement,
                    });
                }
                MindmapCommand::SetNodeCollapsed { node_id, collapsed } => {
                    let node = node_mut(&mut candidate, &node_id)?;
                    let before = node.collapsed;
                    node.collapsed = collapsed;
                    inverses.push(MindmapInverse::RestoreCollapsed {
                        node_id: node_id.clone(),
                        collapsed: before,
                    });
                    changed_nodes.push(node_id.clone());
                    mutations.push(MindmapMutation::SetNodeCollapsed {
                        node_id,
                        before,
                        after: collapsed,
                    });
                }
                MindmapCommand::UpdateEdge {
                    edge_id,
                    source_id,
                    target_id,
                    label,
                    attrs,
                } => {
                    let before = edge(&candidate, &edge_id)?.clone();
                    let current = edge_mut(&mut candidate, &edge_id)?;
                    if let Some(source_id) = source_id {
                        ensure_id(&source_id)?;
                        current.source_id = source_id;
                    }
                    if let Some(target_id) = target_id {
                        ensure_id(&target_id)?;
                        current.target_id = target_id;
                    }
                    if current.source_id == current.target_id {
                        return Err(MindmapEngineError::SelfEdge(edge_id));
                    }
                    if let Some(label) = label {
                        current.label = label;
                    }
                    if let Some(attrs) = attrs {
                        current.attrs = attrs;
                    }
                    let after = current.clone();
                    // End the mutable edge borrow before checking node
                    // references against the candidate graph.
                    let source_id = after.source_id.clone();
                    let target_id = after.target_id.clone();
                    let _ = current;
                    node(&candidate, &source_id)?;
                    node(&candidate, &target_id)?;
                    inverses.push(MindmapInverse::RestoreExistingEdge(before.clone()));
                    changed_edges.push(edge_id.clone());
                    mutations.push(MindmapMutation::UpdateEdge {
                        edge_id,
                        before,
                        after,
                    });
                }
                MindmapCommand::SetEdgeStyle { edge_id, style } => {
                    let before_edge = edge(&candidate, &edge_id)?.clone();
                    let current = edge_mut(&mut candidate, &edge_id)?;
                    let before = std::mem::replace(&mut current.style, style.clone());
                    inverses.push(MindmapInverse::RestoreExistingEdge(before_edge));
                    changed_edges.push(edge_id.clone());
                    mutations.push(MindmapMutation::SetEdgeStyle {
                        edge_id,
                        before,
                        after: style,
                    });
                }
                MindmapCommand::DeleteEdge { edge_id } => {
                    let index = candidate
                        .edges
                        .iter()
                        .position(|item| item.id == edge_id)
                        .ok_or_else(|| MindmapEngineError::MissingEdge(edge_id.clone()))?;
                    let removed = candidate.edges.remove(index);
                    inverses.push(MindmapInverse::RestoreEdge {
                        edge: removed.clone(),
                        index,
                    });
                    changed_edges.push(edge_id);
                    mutations.push(MindmapMutation::DeleteEdge { edge: removed });
                    structure_changed = true;
                }
                MindmapCommand::AddSummary { summary } => {
                    ensure_new_entity_id(&candidate, &summary.id)?;
                    let id = summary.id.clone();
                    candidate.summaries.push(summary.clone());
                    inverses.push(MindmapInverse::RemoveSummary(id.clone()));
                    mutations.push(MindmapMutation::AddSummary { summary });
                    changed_advanced.push(("mindmap.summary", id.clone()));
                    changed_nodes.extend(summary_node_ids(&candidate, &id));
                    structure_changed = true;
                }
                MindmapCommand::UpdateSummary {
                    summary_id,
                    start_node_id,
                    end_node_id,
                    content,
                } => {
                    let current = summary_mut(&mut candidate, &summary_id)?;
                    let before = current.clone();
                    if let Some(value) = start_node_id {
                        current.start_node_id = value;
                    }
                    if let Some(value) = end_node_id {
                        current.end_node_id = value;
                    }
                    if let Some(value) = content {
                        current.content = value;
                    }
                    let after = current.clone();
                    let range_changed = before.start_node_id != after.start_node_id
                        || before.end_node_id != after.end_node_id;
                    structure_changed |= range_changed;
                    inverses.push(MindmapInverse::RestoreExistingSummary(before.clone()));
                    if range_changed {
                        changed_nodes.extend([
                            before.start_node_id.clone(),
                            before.end_node_id.clone(),
                            after.start_node_id.clone(),
                            after.end_node_id.clone(),
                        ]);
                    }
                    mutations.push(MindmapMutation::UpdateSummary {
                        summary_id: summary_id.clone(),
                        before,
                        after,
                    });
                    changed_advanced.push(("mindmap.summary", summary_id));
                }
                MindmapCommand::DeleteSummary { summary_id } => {
                    let index = candidate
                        .summaries
                        .iter()
                        .position(|item| item.id == summary_id)
                        .ok_or_else(|| MindmapEngineError::MissingSummary(summary_id.clone()))?;
                    let summary = candidate.summaries.remove(index);
                    changed_nodes
                        .extend([summary.start_node_id.clone(), summary.end_node_id.clone()]);
                    inverses.push(MindmapInverse::RestoreSummary {
                        summary: summary.clone(),
                        index,
                    });
                    mutations.push(MindmapMutation::DeleteSummary { summary });
                    changed_advanced.push(("mindmap.summary", summary_id));
                    structure_changed = true;
                }
                MindmapCommand::AddBoundary { boundary } => {
                    ensure_new_entity_id(&candidate, &boundary.id)?;
                    changed_nodes.push(boundary.root_node_id.clone());
                    inverses.push(MindmapInverse::RemoveBoundary(boundary.id.clone()));
                    candidate.boundaries.push(boundary.clone());
                    mutations.push(MindmapMutation::AddBoundary { boundary });
                    changed_advanced.push((
                        "mindmap.boundary",
                        candidate.boundaries.last().unwrap().id.clone(),
                    ));
                    structure_changed = true;
                }
                MindmapCommand::UpdateBoundary {
                    boundary_id,
                    root_node_id,
                    label,
                } => {
                    let current = boundary_mut(&mut candidate, &boundary_id)?;
                    let before = current.clone();
                    if let Some(value) = root_node_id {
                        current.root_node_id = value;
                    }
                    if let Some(value) = label {
                        current.label = value;
                    }
                    let after = current.clone();
                    let root_changed = before.root_node_id != after.root_node_id;
                    if root_changed {
                        changed_nodes
                            .extend([before.root_node_id.clone(), after.root_node_id.clone()]);
                    }
                    structure_changed |= root_changed;
                    inverses.push(MindmapInverse::RestoreExistingBoundary(before.clone()));
                    mutations.push(MindmapMutation::UpdateBoundary {
                        boundary_id: boundary_id.clone(),
                        before,
                        after,
                    });
                    changed_advanced.push(("mindmap.boundary", boundary_id));
                }
                MindmapCommand::DeleteBoundary { boundary_id } => {
                    let index = candidate
                        .boundaries
                        .iter()
                        .position(|item| item.id == boundary_id)
                        .ok_or_else(|| MindmapEngineError::MissingBoundary(boundary_id.clone()))?;
                    let boundary = candidate.boundaries.remove(index);
                    changed_nodes.push(boundary.root_node_id.clone());
                    inverses.push(MindmapInverse::RestoreBoundary {
                        boundary: boundary.clone(),
                        index,
                    });
                    mutations.push(MindmapMutation::DeleteBoundary { boundary });
                    changed_advanced.push(("mindmap.boundary", boundary_id));
                    structure_changed = true;
                }
                MindmapCommand::AddFormula { formula } => {
                    ensure_new_entity_id(&candidate, &formula.id)?;
                    changed_nodes.push(formula.node_id.clone());
                    inverses.push(MindmapInverse::RemoveFormula(formula.id.clone()));
                    candidate.formulas.push(formula.clone());
                    mutations.push(MindmapMutation::AddFormula { formula });
                    changed_advanced.push((
                        "mindmap.formula",
                        candidate.formulas.last().unwrap().id.clone(),
                    ));
                    structure_changed = true;
                }
                MindmapCommand::UpdateFormula {
                    formula_id,
                    node_id,
                    source,
                    display,
                } => {
                    let current = formula_mut(&mut candidate, &formula_id)?;
                    let before = current.clone();
                    if let Some(value) = node_id {
                        current.node_id = value;
                    }
                    if let Some(value) = source {
                        current.source = value;
                    }
                    if let Some(value) = display {
                        current.display = value;
                    }
                    let after = current.clone();
                    let node_changed = before.node_id != after.node_id;
                    if node_changed {
                        changed_nodes.extend([before.node_id.clone(), after.node_id.clone()]);
                    }
                    structure_changed |= node_changed;
                    inverses.push(MindmapInverse::RestoreExistingFormula(before.clone()));
                    mutations.push(MindmapMutation::UpdateFormula {
                        formula_id: formula_id.clone(),
                        before,
                        after,
                    });
                    changed_advanced.push(("mindmap.formula", formula_id));
                }
                MindmapCommand::DeleteFormula { formula_id } => {
                    let index = candidate
                        .formulas
                        .iter()
                        .position(|item| item.id == formula_id)
                        .ok_or_else(|| MindmapEngineError::MissingFormula(formula_id.clone()))?;
                    let formula = candidate.formulas.remove(index);
                    changed_nodes.push(formula.node_id.clone());
                    inverses.push(MindmapInverse::RestoreFormula {
                        formula: formula.clone(),
                        index,
                    });
                    mutations.push(MindmapMutation::DeleteFormula { formula });
                    changed_advanced.push(("mindmap.formula", formula_id));
                    structure_changed = true;
                }
                MindmapCommand::MoveNode {
                    node_id,
                    new_parent_id,
                    index,
                } => {
                    let to_parent_id = new_parent_id.clone();
                    let source = node(&candidate, &node_id)?.clone();
                    let from_parent_id = source.parent_id.clone();
                    let from_index = children(&candidate, from_parent_id.as_deref())
                        .iter()
                        .position(|item| item.id == node_id)
                        .ok_or_else(|| MindmapEngineError::MissingNode(node_id.clone()))?;
                    validate_parent(&candidate, new_parent_id.as_deref(), Some(&node_id))?;
                    if let Some(parent_id) = &new_parent_id {
                        if parent_id == &node_id
                            || subtree_contains(&candidate, &node_id, parent_id)?
                        {
                            return Err(MindmapEngineError::Cycle(node_id));
                        }
                    }
                    if source.parent_id.is_none() && new_parent_id.is_some() {
                        return Err(MindmapEngineError::RootCannotMoveUnderNode);
                    }
                    let subtree_ids = collect_subtree(&candidate, &node_id)?;
                    let subtree_nodes = subtree_ids
                        .iter()
                        .filter_map(|id| candidate.nodes.iter().find(|node| node.id == *id))
                        .cloned()
                        .collect::<Vec<_>>();
                    candidate
                        .nodes
                        .retain(|node| !subtree_ids.contains(&node.id));
                    let insertion = insertion_index(&candidate, new_parent_id.as_deref(), index)?;
                    candidate.nodes.insert(
                        insertion,
                        MindmapNode {
                            parent_id: to_parent_id.clone(),
                            ..source
                        },
                    );
                    // The remaining descendants retain their original parent IDs and are
                    // reinserted directly after the moved root as one contiguous subtree.
                    let descendants = subtree_ids
                        .iter()
                        .skip(1)
                        .filter_map(|id| subtree_nodes.iter().find(|node| node.id == *id))
                        .cloned()
                        .collect::<Vec<_>>();
                    candidate
                        .nodes
                        .splice(insertion + 1..insertion + 1, descendants);
                    let invalid_summaries = candidate
                        .summaries
                        .iter()
                        .enumerate()
                        .filter(|(_, summary)| !summary_references_valid(&candidate, summary))
                        .map(|(position, summary)| (position, summary.clone()))
                        .collect::<Vec<_>>();
                    let invalid_summary_ids = invalid_summaries
                        .iter()
                        .map(|(_, summary)| summary.id.as_str())
                        .collect::<HashSet<_>>();
                    candidate
                        .summaries
                        .retain(|summary| !invalid_summary_ids.contains(summary.id.as_str()));
                    for (position, summary) in invalid_summaries {
                        inverses.push(MindmapInverse::RestoreSummary {
                            summary: summary.clone(),
                            index: position,
                        });
                        changed_nodes
                            .extend([summary.start_node_id.clone(), summary.end_node_id.clone()]);
                        changed_advanced.push(("mindmap.summary", summary.id.clone()));
                        mutations.push(MindmapMutation::DeleteSummary { summary });
                    }
                    inverses.push(MindmapInverse::MoveNode {
                        node_id: node_id.clone(),
                        parent_id: from_parent_id.clone(),
                        index: from_index,
                    });
                    changed_nodes.extend(subtree_ids);
                    mutations.push(MindmapMutation::MoveNode {
                        node_id,
                        from_parent_id,
                        from_index,
                        to_parent_id,
                        to_index: index,
                    });
                    structure_changed = true;
                }
                MindmapCommand::DeleteNode { node_id } => {
                    let root_node = node(&candidate, &node_id)?.clone();
                    let parent_id = root_node.parent_id.clone();
                    let index = children(&candidate, parent_id.as_deref())
                        .iter()
                        .position(|item| item.id == node_id)
                        .unwrap_or(0);
                    let subtree = collect_subtree(&candidate, &node_id)?;
                    let subtree_set = subtree.iter().cloned().collect::<HashSet<_>>();
                    let removed_with_positions = candidate
                        .nodes
                        .iter()
                        .enumerate()
                        .filter(|(_, item)| subtree_set.contains(&item.id))
                        .map(|(position, item)| (position, item.clone()))
                        .collect::<Vec<_>>();
                    let removed = removed_with_positions
                        .iter()
                        .map(|(_, item)| item.clone())
                        .collect();
                    let removed_edges_with_positions = candidate
                        .edges
                        .iter()
                        .enumerate()
                        .filter(|(_, edge)| {
                            subtree_set.contains(&edge.source_id)
                                || subtree_set.contains(&edge.target_id)
                        })
                        .map(|(position, edge)| (position, edge.clone()))
                        .collect::<Vec<_>>();
                    let removed_edges = removed_edges_with_positions
                        .iter()
                        .map(|(_, edge)| edge.clone())
                        .collect::<Vec<_>>();
                    let removed_summaries_with_positions = candidate
                        .summaries
                        .iter()
                        .enumerate()
                        .filter(|(_, item)| {
                            subtree_set.contains(&item.start_node_id)
                                || subtree_set.contains(&item.end_node_id)
                        })
                        .map(|(position, item)| (position, item.clone()))
                        .collect::<Vec<_>>();
                    let removed_boundaries_with_positions = candidate
                        .boundaries
                        .iter()
                        .enumerate()
                        .filter(|(_, item)| subtree_set.contains(&item.root_node_id))
                        .map(|(position, item)| (position, item.clone()))
                        .collect::<Vec<_>>();
                    let removed_formulas_with_positions = candidate
                        .formulas
                        .iter()
                        .enumerate()
                        .filter(|(_, item)| subtree_set.contains(&item.node_id))
                        .map(|(position, item)| (position, item.clone()))
                        .collect::<Vec<_>>();
                    let removed_summaries = removed_summaries_with_positions
                        .iter()
                        .map(|(_, item)| item.clone())
                        .collect::<Vec<_>>();
                    let removed_boundaries = removed_boundaries_with_positions
                        .iter()
                        .map(|(_, item)| item.clone())
                        .collect::<Vec<_>>();
                    let removed_formulas = removed_formulas_with_positions
                        .iter()
                        .map(|(_, item)| item.clone())
                        .collect::<Vec<_>>();
                    inverses.push(MindmapInverse::RestoreDeleted {
                        root: candidate.root.clone(),
                        nodes: removed_with_positions,
                        edges: removed_edges_with_positions,
                        summaries: removed_summaries_with_positions,
                        boundaries: removed_boundaries_with_positions,
                        formulas: removed_formulas_with_positions,
                    });
                    if candidate.root.as_deref() == Some(node_id.as_str()) {
                        candidate.root = None;
                    }
                    candidate
                        .nodes
                        .retain(|node| !subtree_set.contains(&node.id));
                    candidate.edges.retain(|edge| {
                        !subtree_set.contains(&edge.source_id)
                            && !subtree_set.contains(&edge.target_id)
                    });
                    candidate.summaries.retain(|item| {
                        !subtree_set.contains(&item.start_node_id)
                            && !subtree_set.contains(&item.end_node_id)
                    });
                    candidate
                        .boundaries
                        .retain(|item| !subtree_set.contains(&item.root_node_id));
                    candidate
                        .formulas
                        .retain(|item| !subtree_set.contains(&item.node_id));
                    changed_nodes.extend(subtree);
                    mutations.push(MindmapMutation::DeleteNodes {
                        nodes: removed,
                        edges: removed_edges.clone(),
                        summaries: removed_summaries.clone(),
                        boundaries: removed_boundaries.clone(),
                        formulas: removed_formulas.clone(),
                        parent_id,
                        index,
                    });
                    changed_edges.extend(removed_edges.into_iter().map(|edge| edge.id));
                    changed_advanced.extend(
                        removed_summaries
                            .into_iter()
                            .map(|item| ("mindmap.summary", item.id)),
                    );
                    changed_advanced.extend(
                        removed_boundaries
                            .into_iter()
                            .map(|item| ("mindmap.boundary", item.id)),
                    );
                    changed_advanced.extend(
                        removed_formulas
                            .into_iter()
                            .map(|item| ("mindmap.formula", item.id)),
                    );
                    structure_changed = true;
                }
            }
        }
        validate(&candidate)?;
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(MindmapEngineError::RevisionOverflow)?;
        changed_nodes.sort();
        changed_nodes.dedup();
        changed_edges.sort();
        changed_edges.dedup();
        changed_advanced.sort();
        changed_advanced.dedup();
        self.model = candidate;
        self.revision = revision;
        self.redo.clear();
        let invalidation = Invalidation {
            changed_entities: changed_nodes
                .iter()
                .map(|id| EntityRef {
                    entity_type: "mindmap.node".into(),
                    entity_id: id.clone(),
                })
                .chain(changed_edges.iter().map(|id| EntityRef {
                    entity_type: "mindmap.edge".into(),
                    entity_id: id.clone(),
                }))
                .chain(changed_advanced.iter().map(|(entity_type, id)| EntityRef {
                    entity_type: (*entity_type).into(),
                    entity_id: id.clone(),
                }))
                .collect(),
            changed_containers: vec![EntityRef {
                entity_type: "mindmap.graph".into(),
                entity_id: "root".into(),
            }],
            structure_changed,
        };
        let change_set = MindmapChangeSet {
            revision,
            invalidation,
            mutations: mutations.clone(),
        };
        self.undo.push(MindmapJournalEntry {
            commands,
            inverses,
            mutations,
        });
        Ok(change_set)
    }

    /// Undo is a normal revision-advancing graph transition. The caller owns
    /// persistence; clients never provide inverse nodes or snapshots.
    pub fn undo(&mut self, base_revision: u64) -> Result<MindmapChangeSet, MindmapEngineError> {
        if base_revision != self.revision {
            return Err(MindmapEngineError::RevisionConflict {
                expected: self.revision,
                actual: base_revision,
            });
        }
        let entry = self.undo.pop().ok_or(MindmapEngineError::NothingToUndo)?;
        let mut candidate = self.model.clone();
        for inverse in entry.inverses.iter().rev().cloned() {
            if let Err(error) = apply_inverse(&mut candidate, inverse) {
                self.undo.push(entry);
                return Err(error);
            }
        }
        if let Err(error) = validate(&candidate) {
            self.undo.push(entry);
            return Err(error);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(MindmapEngineError::RevisionOverflow)?;
        let mutations = reverse_mutations(&entry.mutations);
        self.model = candidate;
        self.revision = revision;
        self.redo.push(entry);
        Ok(change_set_from_mutations(revision, mutations))
    }

    /// Redo reapplies the original typed semantic commands. Remaining redo
    /// entries survive, matching normal editor history behavior.
    pub fn redo(&mut self, base_revision: u64) -> Result<MindmapChangeSet, MindmapEngineError> {
        if base_revision != self.revision {
            return Err(MindmapEngineError::RevisionConflict {
                expected: self.revision,
                actual: base_revision,
            });
        }
        let mut remaining = std::mem::take(&mut self.redo);
        let entry = remaining.pop().ok_or(MindmapEngineError::NothingToRedo)?;
        let commands = entry.commands.clone();
        match self.execute(MindmapCommandBatch {
            base_revision,
            commands,
        }) {
            Ok(change_set) => {
                self.redo = remaining;
                Ok(change_set)
            }
            Err(error) => {
                remaining.push(entry);
                self.redo = remaining;
                Err(error)
            }
        }
    }
}

fn apply_inverse(
    model: &mut MindmapModel,
    inverse: MindmapInverse,
) -> Result<(), MindmapEngineError> {
    match inverse {
        MindmapInverse::RestoreSettings(settings) => model.settings = settings,
        MindmapInverse::RemoveNode { node_id } => {
            let position = model
                .nodes
                .iter()
                .position(|node| node.id == node_id)
                .ok_or_else(|| MindmapEngineError::MissingNode(node_id.clone()))?;
            model.nodes.remove(position);
            if model.root.as_deref() == Some(node_id.as_str()) {
                model.root = None;
            }
        }
        MindmapInverse::RemoveEdge { edge_id } => {
            let position = model
                .edges
                .iter()
                .position(|edge| edge.id == edge_id)
                .ok_or_else(|| MindmapEngineError::MissingEdge(edge_id.clone()))?;
            model.edges.remove(position);
        }
        MindmapInverse::RestoreNode(restored) => {
            let current = node_mut(model, &restored.id)?;
            *current = restored;
        }
        MindmapInverse::RestoreCollapsed { node_id, collapsed } => {
            node_mut(model, &node_id)?.collapsed = collapsed;
        }
        MindmapInverse::RestoreExistingEdge(restored) => {
            let current = edge_mut(model, &restored.id)?;
            *current = restored;
        }
        MindmapInverse::RestoreEdge { edge, index } => {
            if model.edges.iter().any(|current| current.id == edge.id) {
                return Err(MindmapEngineError::DuplicateEdge(edge.id));
            }
            model.edges.insert(index.min(model.edges.len()), edge);
        }
        MindmapInverse::RemoveSummary(id) => {
            let index = model
                .summaries
                .iter()
                .position(|item| item.id == id)
                .ok_or_else(|| MindmapEngineError::MissingSummary(id.clone()))?;
            model.summaries.remove(index);
        }
        MindmapInverse::RestoreSummary { summary, index } => {
            ensure_new_entity_id(model, &summary.id)?;
            model
                .summaries
                .insert(index.min(model.summaries.len()), summary);
        }
        MindmapInverse::RestoreExistingSummary(restored) => {
            let id = restored.id.clone();
            *summary_mut(model, &id)? = restored;
        }
        MindmapInverse::RemoveBoundary(id) => {
            let index = model
                .boundaries
                .iter()
                .position(|item| item.id == id)
                .ok_or_else(|| MindmapEngineError::MissingBoundary(id.clone()))?;
            model.boundaries.remove(index);
        }
        MindmapInverse::RestoreBoundary { boundary, index } => {
            ensure_new_entity_id(model, &boundary.id)?;
            model
                .boundaries
                .insert(index.min(model.boundaries.len()), boundary);
        }
        MindmapInverse::RestoreExistingBoundary(restored) => {
            let id = restored.id.clone();
            *boundary_mut(model, &id)? = restored;
        }
        MindmapInverse::RemoveFormula(id) => {
            let index = model
                .formulas
                .iter()
                .position(|item| item.id == id)
                .ok_or_else(|| MindmapEngineError::MissingFormula(id.clone()))?;
            model.formulas.remove(index);
        }
        MindmapInverse::RestoreFormula { formula, index } => {
            ensure_new_entity_id(model, &formula.id)?;
            model
                .formulas
                .insert(index.min(model.formulas.len()), formula);
        }
        MindmapInverse::RestoreExistingFormula(restored) => {
            let id = restored.id.clone();
            *formula_mut(model, &id)? = restored;
        }
        MindmapInverse::MoveNode {
            node_id,
            parent_id,
            index,
        } => move_node_in_model(model, &node_id, parent_id, index)?,
        MindmapInverse::RestoreDeleted {
            root,
            nodes,
            edges,
            summaries,
            boundaries,
            formulas,
        } => {
            model.root = root;
            for (position, node) in nodes {
                if model.nodes.iter().any(|current| current.id == node.id) {
                    return Err(MindmapEngineError::DuplicateNode(node.id));
                }
                model.nodes.insert(position.min(model.nodes.len()), node);
            }
            for (position, edge) in edges {
                if model.edges.iter().any(|current| current.id == edge.id) {
                    return Err(MindmapEngineError::DuplicateEdge(edge.id));
                }
                model.edges.insert(position.min(model.edges.len()), edge);
            }
            for (position, summary) in summaries {
                model
                    .summaries
                    .insert(position.min(model.summaries.len()), summary);
            }
            for (position, boundary) in boundaries {
                model
                    .boundaries
                    .insert(position.min(model.boundaries.len()), boundary);
            }
            for (position, formula) in formulas {
                model
                    .formulas
                    .insert(position.min(model.formulas.len()), formula);
            }
        }
    }
    Ok(())
}

fn move_node_in_model(
    model: &mut MindmapModel,
    node_id: &str,
    new_parent_id: Option<String>,
    index: usize,
) -> Result<(), MindmapEngineError> {
    let source = node(model, node_id)?.clone();
    validate_parent(model, new_parent_id.as_deref(), Some(node_id))?;
    if let Some(parent_id) = &new_parent_id {
        if parent_id == node_id || subtree_contains(model, node_id, parent_id)? {
            return Err(MindmapEngineError::Cycle(node_id.into()));
        }
    }
    if source.parent_id.is_none() && new_parent_id.is_some() {
        return Err(MindmapEngineError::RootCannotMoveUnderNode);
    }
    let subtree_ids = collect_subtree(model, node_id)?;
    let subtree_set = subtree_ids.iter().cloned().collect::<HashSet<_>>();
    let subtree_nodes = subtree_ids
        .iter()
        .filter_map(|id| model.nodes.iter().find(|node| node.id == *id))
        .cloned()
        .collect::<Vec<_>>();
    model.nodes.retain(|node| !subtree_set.contains(&node.id));
    let insertion = insertion_index(model, new_parent_id.as_deref(), index)?;
    model.nodes.insert(
        insertion,
        MindmapNode {
            parent_id: new_parent_id,
            ..source
        },
    );
    let descendants = subtree_ids
        .iter()
        .skip(1)
        .filter_map(|id| subtree_nodes.iter().find(|node| node.id == *id))
        .cloned()
        .collect::<Vec<_>>();
    model
        .nodes
        .splice(insertion + 1..insertion + 1, descendants);
    Ok(())
}

fn reverse_mutations(mutations: &[MindmapMutation]) -> Vec<MindmapMutation> {
    mutations
        .iter()
        .rev()
        .map(|mutation| match mutation {
            MindmapMutation::SettingsChanged { before, after } => {
                MindmapMutation::SettingsChanged {
                    before: after.clone(),
                    after: before.clone(),
                }
            }
            MindmapMutation::AddNode { node, index } => MindmapMutation::DeleteNodes {
                nodes: vec![node.clone()],
                edges: Vec::new(),
                summaries: Vec::new(),
                boundaries: Vec::new(),
                formulas: Vec::new(),
                parent_id: node.parent_id.clone(),
                index: *index,
            },
            MindmapMutation::AddEdge { edge } => MindmapMutation::DeleteEdge { edge: edge.clone() },
            MindmapMutation::UpdateNode {
                node_id,
                before,
                after,
            } => MindmapMutation::UpdateNode {
                node_id: node_id.clone(),
                before: after.clone(),
                after: before.clone(),
            },
            MindmapMutation::ReplaceNodeText {
                node_id,
                before,
                after,
            } => MindmapMutation::ReplaceNodeText {
                node_id: node_id.clone(),
                before: after.clone(),
                after: before.clone(),
            },
            MindmapMutation::PatchNodeTextRange {
                node_id,
                range,
                before,
                after,
            } => MindmapMutation::PatchNodeTextRange {
                node_id: node_id.clone(),
                range: *range,
                before: after.clone(),
                after: before.clone(),
            },
            MindmapMutation::SetNodeStyle {
                node_id,
                before,
                after,
            } => MindmapMutation::SetNodeStyle {
                node_id: node_id.clone(),
                before: after.clone(),
                after: before.clone(),
            },
            MindmapMutation::SetNodeSupplement {
                node_id,
                before,
                after,
            } => MindmapMutation::SetNodeSupplement {
                node_id: node_id.clone(),
                before: after.clone(),
                after: before.clone(),
            },
            MindmapMutation::SetNodeCollapsed {
                node_id,
                before,
                after,
            } => MindmapMutation::SetNodeCollapsed {
                node_id: node_id.clone(),
                before: *after,
                after: *before,
            },
            MindmapMutation::UpdateEdge {
                edge_id,
                before,
                after,
            } => MindmapMutation::UpdateEdge {
                edge_id: edge_id.clone(),
                before: after.clone(),
                after: before.clone(),
            },
            MindmapMutation::SetEdgeStyle {
                edge_id,
                before,
                after,
            } => MindmapMutation::SetEdgeStyle {
                edge_id: edge_id.clone(),
                before: after.clone(),
                after: before.clone(),
            },
            MindmapMutation::DeleteEdge { edge } => MindmapMutation::AddEdge { edge: edge.clone() },
            MindmapMutation::AddSummary { summary } => MindmapMutation::DeleteSummary {
                summary: summary.clone(),
            },
            MindmapMutation::UpdateSummary {
                summary_id,
                before,
                after,
            } => MindmapMutation::UpdateSummary {
                summary_id: summary_id.clone(),
                before: after.clone(),
                after: before.clone(),
            },
            MindmapMutation::DeleteSummary { summary } => MindmapMutation::AddSummary {
                summary: summary.clone(),
            },
            MindmapMutation::AddBoundary { boundary } => MindmapMutation::DeleteBoundary {
                boundary: boundary.clone(),
            },
            MindmapMutation::UpdateBoundary {
                boundary_id,
                before,
                after,
            } => MindmapMutation::UpdateBoundary {
                boundary_id: boundary_id.clone(),
                before: after.clone(),
                after: before.clone(),
            },
            MindmapMutation::DeleteBoundary { boundary } => MindmapMutation::AddBoundary {
                boundary: boundary.clone(),
            },
            MindmapMutation::AddFormula { formula } => MindmapMutation::DeleteFormula {
                formula: formula.clone(),
            },
            MindmapMutation::UpdateFormula {
                formula_id,
                before,
                after,
            } => MindmapMutation::UpdateFormula {
                formula_id: formula_id.clone(),
                before: after.clone(),
                after: before.clone(),
            },
            MindmapMutation::DeleteFormula { formula } => MindmapMutation::AddFormula {
                formula: formula.clone(),
            },
            MindmapMutation::MoveNode {
                node_id,
                from_parent_id,
                from_index,
                to_parent_id,
                to_index,
            } => MindmapMutation::MoveNode {
                node_id: node_id.clone(),
                from_parent_id: to_parent_id.clone(),
                from_index: *to_index,
                to_parent_id: from_parent_id.clone(),
                to_index: *from_index,
            },
            MindmapMutation::DeleteNodes {
                nodes,
                edges,
                summaries,
                boundaries,
                formulas,
                parent_id,
                index,
            } => MindmapMutation::RestoreNodes {
                nodes: nodes.clone(),
                edges: edges.clone(),
                summaries: summaries.clone(),
                boundaries: boundaries.clone(),
                formulas: formulas.clone(),
                parent_id: parent_id.clone(),
                index: *index,
            },
            MindmapMutation::RestoreNodes {
                nodes,
                edges,
                summaries,
                boundaries,
                formulas,
                parent_id,
                index,
            } => MindmapMutation::DeleteNodes {
                nodes: nodes.clone(),
                edges: edges.clone(),
                summaries: summaries.clone(),
                boundaries: boundaries.clone(),
                formulas: formulas.clone(),
                parent_id: parent_id.clone(),
                index: *index,
            },
        })
        .collect()
}

fn change_set_from_mutations(revision: u64, mutations: Vec<MindmapMutation>) -> MindmapChangeSet {
    let mut node_ids = Vec::new();
    let mut edge_ids = Vec::new();
    let mut advanced_ids = Vec::<(&'static str, String)>::new();
    let mut structure_changed = false;
    for mutation in &mutations {
        match mutation {
            MindmapMutation::SettingsChanged { .. } => {}
            MindmapMutation::AddNode { node, .. } => {
                node_ids.push(node.id.clone());
                structure_changed = true;
            }
            MindmapMutation::AddEdge { edge } => {
                edge_ids.push(edge.id.clone());
                structure_changed = true;
            }
            MindmapMutation::UpdateNode { node_id, .. }
            | MindmapMutation::ReplaceNodeText { node_id, .. }
            | MindmapMutation::PatchNodeTextRange { node_id, .. }
            | MindmapMutation::SetNodeStyle { node_id, .. }
            | MindmapMutation::SetNodeSupplement { node_id, .. }
            | MindmapMutation::SetNodeCollapsed { node_id, .. }
            | MindmapMutation::MoveNode { node_id, .. } => {
                node_ids.push(node_id.clone());
                structure_changed |= matches!(mutation, MindmapMutation::MoveNode { .. });
            }
            MindmapMutation::UpdateEdge { edge_id, .. }
            | MindmapMutation::SetEdgeStyle { edge_id, .. } => edge_ids.push(edge_id.clone()),
            MindmapMutation::DeleteEdge { edge } => {
                edge_ids.push(edge.id.clone());
                structure_changed = true;
            }
            MindmapMutation::AddSummary { summary }
            | MindmapMutation::DeleteSummary { summary } => {
                advanced_ids.push(("mindmap.summary", summary.id.clone()));
                node_ids.extend([summary.start_node_id.clone(), summary.end_node_id.clone()]);
                structure_changed = true;
            }
            MindmapMutation::UpdateSummary {
                summary_id,
                before,
                after,
            } => {
                advanced_ids.push(("mindmap.summary", summary_id.clone()));
                node_ids.extend([
                    before.start_node_id.clone(),
                    before.end_node_id.clone(),
                    after.start_node_id.clone(),
                    after.end_node_id.clone(),
                ]);
                structure_changed |= before.start_node_id != after.start_node_id
                    || before.end_node_id != after.end_node_id;
            }
            MindmapMutation::AddBoundary { boundary }
            | MindmapMutation::DeleteBoundary { boundary } => {
                advanced_ids.push(("mindmap.boundary", boundary.id.clone()));
                node_ids.push(boundary.root_node_id.clone());
                structure_changed = true;
            }
            MindmapMutation::UpdateBoundary {
                boundary_id,
                before,
                after,
            } => {
                advanced_ids.push(("mindmap.boundary", boundary_id.clone()));
                node_ids.extend([before.root_node_id.clone(), after.root_node_id.clone()]);
                structure_changed |= before.root_node_id != after.root_node_id;
            }
            MindmapMutation::AddFormula { formula }
            | MindmapMutation::DeleteFormula { formula } => {
                advanced_ids.push(("mindmap.formula", formula.id.clone()));
                node_ids.push(formula.node_id.clone());
                structure_changed = true;
            }
            MindmapMutation::UpdateFormula {
                formula_id,
                before,
                after,
            } => {
                advanced_ids.push(("mindmap.formula", formula_id.clone()));
                node_ids.extend([before.node_id.clone(), after.node_id.clone()]);
                structure_changed |= before.node_id != after.node_id;
            }
            MindmapMutation::DeleteNodes {
                nodes,
                edges,
                summaries,
                boundaries,
                formulas,
                ..
            }
            | MindmapMutation::RestoreNodes {
                nodes,
                edges,
                summaries,
                boundaries,
                formulas,
                ..
            } => {
                node_ids.extend(nodes.iter().map(|node| node.id.clone()));
                edge_ids.extend(edges.iter().map(|edge| edge.id.clone()));
                advanced_ids.extend(
                    summaries
                        .iter()
                        .map(|item| ("mindmap.summary", item.id.clone())),
                );
                advanced_ids.extend(
                    boundaries
                        .iter()
                        .map(|item| ("mindmap.boundary", item.id.clone())),
                );
                advanced_ids.extend(
                    formulas
                        .iter()
                        .map(|item| ("mindmap.formula", item.id.clone())),
                );
                structure_changed = true;
            }
        }
    }
    node_ids.sort();
    node_ids.dedup();
    edge_ids.sort();
    edge_ids.dedup();
    advanced_ids.sort();
    advanced_ids.dedup();
    MindmapChangeSet {
        revision,
        invalidation: Invalidation {
            changed_entities: node_ids
                .into_iter()
                .map(|entity_id| EntityRef {
                    entity_type: "mindmap.node".into(),
                    entity_id,
                })
                .chain(edge_ids.into_iter().map(|entity_id| EntityRef {
                    entity_type: "mindmap.edge".into(),
                    entity_id,
                }))
                .chain(
                    advanced_ids
                        .into_iter()
                        .map(|(entity_type, entity_id)| EntityRef {
                            entity_type: entity_type.into(),
                            entity_id,
                        }),
                )
                .collect(),
            changed_containers: vec![EntityRef {
                entity_type: "mindmap.graph".into(),
                entity_id: "root".into(),
            }],
            structure_changed,
        },
        mutations,
    }
}

fn validate(model: &MindmapModel) -> Result<(), MindmapEngineError> {
    model.validate().map_err(MindmapEngineError::Schema)
}

fn ensure_id(id: &str) -> Result<(), MindmapEngineError> {
    if id.trim().is_empty() {
        Err(MindmapEngineError::EmptyId)
    } else {
        Ok(())
    }
}

fn ensure_new_entity_id(model: &MindmapModel, id: &str) -> Result<(), MindmapEngineError> {
    ensure_id(id)?;
    let exists = model.nodes.iter().any(|item| item.id == id)
        || model.edges.iter().any(|item| item.id == id)
        || model.summaries.iter().any(|item| item.id == id)
        || model.boundaries.iter().any(|item| item.id == id)
        || model.formulas.iter().any(|item| item.id == id);
    if exists {
        return Err(MindmapEngineError::DuplicateEntity(id.into()));
    }
    Ok(())
}

fn summary_mut<'a>(
    model: &'a mut MindmapModel,
    id: &str,
) -> Result<&'a mut MindmapSummary, MindmapEngineError> {
    model
        .summaries
        .iter_mut()
        .find(|item| item.id == id)
        .ok_or_else(|| MindmapEngineError::MissingSummary(id.into()))
}

fn boundary_mut<'a>(
    model: &'a mut MindmapModel,
    id: &str,
) -> Result<&'a mut MindmapBoundary, MindmapEngineError> {
    model
        .boundaries
        .iter_mut()
        .find(|item| item.id == id)
        .ok_or_else(|| MindmapEngineError::MissingBoundary(id.into()))
}

fn formula_mut<'a>(
    model: &'a mut MindmapModel,
    id: &str,
) -> Result<&'a mut MindmapFormula, MindmapEngineError> {
    model
        .formulas
        .iter_mut()
        .find(|item| item.id == id)
        .ok_or_else(|| MindmapEngineError::MissingFormula(id.into()))
}

fn summary_node_ids(model: &MindmapModel, id: &str) -> Vec<String> {
    model
        .summaries
        .iter()
        .find(|item| item.id == id)
        .map(|item| vec![item.start_node_id.clone(), item.end_node_id.clone()])
        .unwrap_or_default()
}

fn summary_references_valid(model: &MindmapModel, summary: &MindmapSummary) -> bool {
    let Some(start) = model
        .nodes
        .iter()
        .find(|node| node.id == summary.start_node_id)
    else {
        return false;
    };
    let Some(end) = model
        .nodes
        .iter()
        .find(|node| node.id == summary.end_node_id)
    else {
        return false;
    };
    if start.id == end.id || start.parent_id.is_none() || start.parent_id != end.parent_id {
        return false;
    }
    let siblings = model
        .nodes
        .iter()
        .filter(|node| node.parent_id == start.parent_id)
        .map(|node| node.id.as_str())
        .collect::<Vec<_>>();
    let start_index = siblings.iter().position(|id| *id == start.id);
    let end_index = siblings.iter().position(|id| *id == end.id);
    matches!((start_index, end_index), (Some(start), Some(end)) if start < end)
}

fn validate_text_range(range: MindmapTextRange, text_len: usize) -> Result<(), MindmapEngineError> {
    if range.start >= range.end || range.end > text_len {
        return Err(MindmapEngineError::InvalidTextRange {
            start: range.start,
            end: range.end,
            text_len,
        });
    }
    Ok(())
}

fn patch_rich_text_runs(
    content: &mut RichText,
    range: MindmapTextRange,
    patch: &MindmapInlineStylePatch,
) {
    let text_len = content.text.chars().count();
    let source_runs = if content.runs.is_empty() {
        vec![InlineRun {
            start: 0,
            end: text_len,
            style: InlineStyle::default(),
        }]
    } else {
        content.runs.clone()
    };
    let mut next_runs = Vec::with_capacity(source_runs.len() + 2);
    for run in source_runs {
        if run.end <= range.start || run.start >= range.end {
            push_inline_run(&mut next_runs, run.start, run.end, run.style);
            continue;
        }
        if run.start < range.start {
            push_inline_run(&mut next_runs, run.start, range.start, run.style.clone());
        }
        let mut selected_style = run.style.clone();
        apply_inline_style_patch(&mut selected_style, patch);
        push_inline_run(
            &mut next_runs,
            run.start.max(range.start),
            run.end.min(range.end),
            selected_style,
        );
        if run.end > range.end {
            push_inline_run(&mut next_runs, range.end, run.end, run.style);
        }
    }
    content.runs = next_runs;
}

fn push_inline_run(runs: &mut Vec<InlineRun>, start: usize, end: usize, style: InlineStyle) {
    if start >= end {
        return;
    }
    if let Some(previous) = runs
        .last_mut()
        .filter(|previous| previous.end == start && previous.style == style)
    {
        previous.end = end;
    } else {
        runs.push(InlineRun { start, end, style });
    }
}

fn apply_inline_style_patch(style: &mut InlineStyle, patch: &MindmapInlineStylePatch) {
    apply_defaulted(&mut style.bold, patch.bold);
    apply_defaulted(&mut style.italic, patch.italic);
    apply_defaulted(&mut style.underline, patch.underline);
    apply_defaulted(&mut style.strikethrough, patch.strikethrough);
    apply_optional(&mut style.font_family, patch.font_family.clone());
    apply_optional(&mut style.font_size, patch.font_size);
    apply_optional(&mut style.color, patch.color.clone());
    apply_optional(&mut style.highlight, patch.highlight.clone());
    apply_optional(&mut style.vertical_align, patch.vertical_align.clone());
}

fn apply_defaulted<T: Default>(target: &mut T, patch: Option<Option<T>>) {
    if let Some(value) = patch {
        *target = value.unwrap_or_default();
    }
}

fn apply_optional<T>(target: &mut Option<T>, patch: Option<Option<T>>) {
    if let Some(value) = patch {
        *target = value;
    }
}

fn node<'a>(model: &'a MindmapModel, id: &str) -> Result<&'a MindmapNode, MindmapEngineError> {
    model
        .nodes
        .iter()
        .find(|node| node.id == id)
        .ok_or_else(|| MindmapEngineError::MissingNode(id.into()))
}

fn node_mut<'a>(
    model: &'a mut MindmapModel,
    id: &str,
) -> Result<&'a mut MindmapNode, MindmapEngineError> {
    model
        .nodes
        .iter_mut()
        .find(|node| node.id == id)
        .ok_or_else(|| MindmapEngineError::MissingNode(id.into()))
}

fn edge<'a>(model: &'a MindmapModel, id: &str) -> Result<&'a MindmapEdge, MindmapEngineError> {
    model
        .edges
        .iter()
        .find(|edge| edge.id == id)
        .ok_or_else(|| MindmapEngineError::MissingEdge(id.into()))
}

fn edge_mut<'a>(
    model: &'a mut MindmapModel,
    id: &str,
) -> Result<&'a mut MindmapEdge, MindmapEngineError> {
    model
        .edges
        .iter_mut()
        .find(|edge| edge.id == id)
        .ok_or_else(|| MindmapEngineError::MissingEdge(id.into()))
}

fn validate_parent(
    model: &MindmapModel,
    parent_id: Option<&str>,
    moving_id: Option<&str>,
) -> Result<(), MindmapEngineError> {
    match parent_id {
        Some(parent_id) => {
            node(model, parent_id)?;
            if moving_id == Some(parent_id) {
                return Err(MindmapEngineError::Cycle(parent_id.into()));
            }
        }
        None if !model.nodes.is_empty() && moving_id.is_none() => {
            return Err(MindmapEngineError::RootAlreadyExists)
        }
        None => {}
    }
    Ok(())
}

fn children<'a>(model: &'a MindmapModel, parent_id: Option<&str>) -> Vec<&'a MindmapNode> {
    model
        .nodes
        .iter()
        .filter(|node| node.parent_id.as_deref() == parent_id)
        .collect()
}

/// Read-only O(n) graph index shared by layout, routing and large-map query
/// paths. It contains no editable state and can be rebuilt from any immutable
/// snapshot without changing its identity.
#[derive(Debug)]
pub struct MindmapIndex<'a> {
    model: &'a MindmapModel,
    node_positions: HashMap<&'a str, usize>,
    edge_positions: HashMap<&'a str, usize>,
    children: HashMap<Option<&'a str>, Vec<usize>>,
}

impl<'a> MindmapIndex<'a> {
    pub fn new(model: &'a MindmapModel) -> Result<Self, MindmapEngineError> {
        validate(model)?;
        Ok(Self::from_model(model))
    }

    /// Internal command paths may briefly remove a subtree before reinserting
    /// it while cross-links still point at the stable ids. Indexing that
    /// transient candidate must not run final schema validation mid-command.
    fn from_model(model: &'a MindmapModel) -> Self {
        let node_positions = model
            .nodes
            .iter()
            .enumerate()
            .map(|(position, node)| (node.id.as_str(), position))
            .collect();
        let edge_positions = model
            .edges
            .iter()
            .enumerate()
            .map(|(position, edge)| (edge.id.as_str(), position))
            .collect();
        let mut children: HashMap<Option<&str>, Vec<usize>> = HashMap::new();
        for (position, node) in model.nodes.iter().enumerate() {
            children
                .entry(node.parent_id.as_deref())
                .or_default()
                .push(position);
        }
        Self {
            model,
            node_positions,
            edge_positions,
            children,
        }
    }

    pub fn node(&self, id: &str) -> Option<&'a MindmapNode> {
        self.node_positions
            .get(id)
            .map(|position| &self.model.nodes[*position])
    }

    pub fn edge(&self, id: &str) -> Option<&'a MindmapEdge> {
        self.edge_positions
            .get(id)
            .map(|position| &self.model.edges[*position])
    }

    pub fn children(&self, parent_id: Option<&str>) -> Vec<&'a MindmapNode> {
        self.children
            .get(&parent_id)
            .into_iter()
            .flatten()
            .map(|position| &self.model.nodes[*position])
            .collect()
    }

    pub fn collect_subtree(&self, root: &str) -> Result<Vec<String>, MindmapEngineError> {
        if self.node(root).is_none() {
            return Err(MindmapEngineError::MissingNode(root.into()));
        }
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut stack = vec![root.to_string()];
        while let Some(id) = stack.pop() {
            if !visited.insert(id.clone()) {
                continue;
            }
            result.push(id.clone());
            if let Some(children) = self.children.get(&Some(id.as_str())) {
                for position in children.iter().rev() {
                    stack.push(self.model.nodes[*position].id.clone());
                }
            }
        }
        Ok(result)
    }
}

fn collect_subtree(model: &MindmapModel, root: &str) -> Result<Vec<String>, MindmapEngineError> {
    MindmapIndex::from_model(model).collect_subtree(root)
}

fn subtree_contains(
    model: &MindmapModel,
    root: &str,
    target: &str,
) -> Result<bool, MindmapEngineError> {
    Ok(collect_subtree(model, root)?.iter().any(|id| id == target))
}

fn insertion_index(
    model: &MindmapModel,
    parent_id: Option<&str>,
    sibling_index: usize,
) -> Result<usize, MindmapEngineError> {
    let graph = MindmapIndex::from_model(model);
    let siblings = graph.children(parent_id);
    if sibling_index > siblings.len() {
        return Err(MindmapEngineError::InvalidIndex {
            index: sibling_index,
            len: siblings.len(),
        });
    }
    if let Some(previous) = siblings.get(sibling_index.saturating_sub(1)) {
        let subtree = graph.collect_subtree(&previous.id)?;
        let last = subtree
            .iter()
            .filter_map(|id| graph.node_positions.get(id.as_str()).copied())
            .max()
            .unwrap_or(0);
        return Ok(last + 1);
    }
    if let Some(parent_id) = parent_id {
        let parent = *graph.node_positions.get(parent_id).unwrap();
        return Ok(parent + 1);
    }
    Ok(model.nodes.len())
}

/// Immutable layout projection for a renderer. The projection is derived from the graph and
/// never written into `MindmapModel`; DOM/SVG/Canvas renderers only consume this value.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MindmapLayoutOptions {
    pub horizontal_gap: f32,
    pub vertical_gap: f32,
    pub node_width: f32,
    pub node_height: f32,
}

impl Default for MindmapLayoutOptions {
    fn default() -> Self {
        Self {
            horizontal_gap: 240.0,
            vertical_gap: 96.0,
            node_width: 160.0,
            node_height: 40.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MindmapLayoutNode {
    pub id: String,
    pub depth: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MindmapLayoutProjection {
    pub nodes: Vec<MindmapLayoutNode>,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MindmapPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MindmapEdgeRoute {
    /// `None` denotes the implicit parent/child route; explicit cross-links
    /// carry their stable edge id so renderers can select and update them.
    pub edge_id: Option<String>,
    pub parent_id: String,
    pub child_id: String,
    pub points: Vec<MindmapPoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MindmapEdgeProjection {
    pub routes: Vec<MindmapEdgeRoute>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MindmapRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MindmapSummaryProjection {
    pub summary_id: String,
    pub node_ids: Vec<String>,
    pub points: Vec<MindmapPoint>,
    pub label_anchor: MindmapPoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MindmapBoundaryProjection {
    pub boundary_id: String,
    pub node_ids: Vec<String>,
    pub rect: MindmapRect,
    pub label_anchor: MindmapPoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MindmapFormulaProjection {
    pub formula_id: String,
    pub node_id: String,
    pub anchor: MindmapPoint,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MindmapAdvancedProjection {
    pub summaries: Vec<MindmapSummaryProjection>,
    pub boundaries: Vec<MindmapBoundaryProjection>,
    pub formulas: Vec<MindmapFormulaProjection>,
}

/// Renderer theme is a projection concern. The graph model has no CSS or
/// palette fields, allowing the same snapshot to render in light/dark/product
/// themes without a transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum MindmapTheme {
    #[default]
    Light,
    Dark,
    HighContrast,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MindmapProjection {
    pub theme: MindmapTheme,
    pub layout: MindmapLayoutProjection,
    pub edges: MindmapEdgeProjection,
    pub advanced: MindmapAdvancedProjection,
}

impl MindmapProjection {
    pub fn build(
        model: &MindmapModel,
        options: MindmapLayoutOptions,
        theme: MindmapTheme,
    ) -> Result<Self, MindmapLayoutError> {
        Self::build_with_measurements(model, options, theme, &HashMap::new())
    }

    /// Renderer text measurements affect only the derived layout, never the graph snapshot.
    pub fn build_with_measurements(
        model: &MindmapModel,
        options: MindmapLayoutOptions,
        theme: MindmapTheme,
        measurements: &HashMap<String, MindmapNodeMeasurement>,
    ) -> Result<Self, MindmapLayoutError> {
        let mut layout = layout_with_measurements(model, options, measurements)?;
        let edges = route_edges(model, &layout)?;
        let advanced = project_advanced_entities(model, &layout)?;
        for summary in &advanced.summaries {
            layout.width = layout.width.max(summary.label_anchor.x + 120.0);
            layout.height = layout.height.max(summary.label_anchor.y + 32.0);
        }
        for boundary in &advanced.boundaries {
            layout.width = layout
                .width
                .max(boundary.rect.x + boundary.rect.width + 16.0);
            layout.height = layout
                .height
                .max(boundary.rect.y + boundary.rect.height + 16.0);
        }
        for formula in &advanced.formulas {
            layout.width = layout.width.max(formula.anchor.x + 80.0);
            layout.height = layout.height.max(formula.anchor.y + 32.0);
        }
        Ok(Self {
            theme,
            layout,
            edges,
            advanced,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MindmapProjectionUpdate {
    pub projection: MindmapProjection,
    /// Number of layout nodes whose geometry was recomputed. Renderer-only
    /// label/style changes deliberately keep this at zero.
    pub recomputed_layout_nodes: usize,
    pub recomputed_routes: usize,
    pub recomputed_advanced_entities: usize,
}

/// Applies a semantic invalidation to a previous immutable projection. This
/// cache never becomes domain state: callers may discard it and rebuild, and
/// the result is required to equal a full projection for the same model.
pub fn update_projection(
    previous: &MindmapProjection,
    model: &MindmapModel,
    invalidation: &Invalidation,
    options: MindmapLayoutOptions,
    theme: MindmapTheme,
) -> Result<MindmapProjectionUpdate, MindmapLayoutError> {
    let types = invalidation
        .changed_entities
        .iter()
        .map(|entity| entity.entity_type.as_str())
        .collect::<HashSet<_>>();
    let requires_layout = invalidation.structure_changed
        || previous.theme != theme
        || types.is_empty()
        || types.iter().any(|entity_type| {
            !matches!(
                *entity_type,
                "mindmap.edge" | "mindmap.summary" | "mindmap.boundary" | "mindmap.formula"
            )
        });
    if requires_layout {
        let projection = MindmapProjection::build(model, options, theme)?;
        return Ok(MindmapProjectionUpdate {
            recomputed_layout_nodes: projection.layout.nodes.len(),
            recomputed_routes: projection.edges.routes.len(),
            recomputed_advanced_entities: projection.advanced.summaries.len()
                + projection.advanced.boundaries.len()
                + projection.advanced.formulas.len(),
            projection,
        });
    }

    let mut projection = previous.clone();
    projection.theme = theme;
    let mut recomputed_routes = 0;
    let mut recomputed_advanced_entities = 0;
    if types.contains("mindmap.edge") {
        projection.edges = route_edges(model, &projection.layout)?;
        recomputed_routes = projection.edges.routes.len();
    }
    if types.iter().any(|entity_type| {
        matches!(
            *entity_type,
            "mindmap.summary" | "mindmap.boundary" | "mindmap.formula"
        )
    }) {
        projection.advanced = project_advanced_entities(model, &projection.layout)?;
        recomputed_advanced_entities = projection.advanced.summaries.len()
            + projection.advanced.boundaries.len()
            + projection.advanced.formulas.len();
    }
    Ok(MindmapProjectionUpdate {
        projection,
        recomputed_layout_nodes: 0,
        recomputed_routes,
        recomputed_advanced_entities,
    })
}

pub fn project_advanced_entities(
    model: &MindmapModel,
    layout: &MindmapLayoutProjection,
) -> Result<MindmapAdvancedProjection, MindmapLayoutError> {
    validate(model).map_err(MindmapLayoutError::Engine)?;
    let positions = layout
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<HashMap<_, _>>();
    let mut summaries = Vec::new();
    for summary in &model.summaries {
        let start = node(model, &summary.start_node_id).map_err(MindmapLayoutError::Engine)?;
        let siblings = model
            .nodes
            .iter()
            .filter(|item| item.parent_id == start.parent_id)
            .collect::<Vec<_>>();
        let start_index = siblings
            .iter()
            .position(|item| item.id == summary.start_node_id)
            .unwrap();
        let end_index = siblings
            .iter()
            .position(|item| item.id == summary.end_node_id)
            .unwrap();
        let node_ids = siblings[start_index..=end_index]
            .iter()
            .map(|item| item.id.clone())
            .collect::<Vec<_>>();
        let visible = node_ids
            .iter()
            .filter_map(|id| positions.get(id.as_str()).copied())
            .collect::<Vec<_>>();
        if let Some(rect) = bounding_rect(&visible, 0.0) {
            let x = rect.x + rect.width + 18.0;
            summaries.push(MindmapSummaryProjection {
                summary_id: summary.id.clone(),
                node_ids,
                points: vec![
                    MindmapPoint {
                        x: x - 8.0,
                        y: rect.y,
                    },
                    MindmapPoint { x, y: rect.y },
                    MindmapPoint {
                        x,
                        y: rect.y + rect.height,
                    },
                    MindmapPoint {
                        x: x - 8.0,
                        y: rect.y + rect.height,
                    },
                ],
                label_anchor: MindmapPoint {
                    x: x + 8.0,
                    y: rect.y + rect.height / 2.0,
                },
            });
        }
    }
    let graph = MindmapIndex::from_model(model);
    let mut boundaries = Vec::new();
    for boundary in &model.boundaries {
        let node_ids = graph
            .collect_subtree(&boundary.root_node_id)
            .map_err(MindmapLayoutError::Engine)?;
        let visible = node_ids
            .iter()
            .filter_map(|id| positions.get(id.as_str()).copied())
            .collect::<Vec<_>>();
        if let Some(rect) = bounding_rect(&visible, 10.0) {
            boundaries.push(MindmapBoundaryProjection {
                boundary_id: boundary.id.clone(),
                node_ids,
                label_anchor: MindmapPoint {
                    x: rect.x + 10.0,
                    y: rect.y + 16.0,
                },
                rect,
            });
        }
    }
    let formulas = model
        .formulas
        .iter()
        .filter_map(|formula| {
            positions
                .get(formula.node_id.as_str())
                .map(|position| MindmapFormulaProjection {
                    formula_id: formula.id.clone(),
                    node_id: formula.node_id.clone(),
                    anchor: MindmapPoint {
                        x: position.x + position.width / 2.0,
                        y: position.y + position.height + 18.0,
                    },
                })
        })
        .collect();
    Ok(MindmapAdvancedProjection {
        summaries,
        boundaries,
        formulas,
    })
}

fn bounding_rect(nodes: &[&MindmapLayoutNode], padding: f32) -> Option<MindmapRect> {
    let first = nodes.first()?;
    let min_x = nodes.iter().map(|node| node.x).fold(first.x, f32::min);
    let min_y = nodes.iter().map(|node| node.y).fold(first.y, f32::min);
    let max_x = nodes
        .iter()
        .map(|node| node.x + node.width)
        .fold(first.x + first.width, f32::max);
    let max_y = nodes
        .iter()
        .map(|node| node.y + node.height)
        .fold(first.y + first.height, f32::max);
    Some(MindmapRect {
        x: (min_x - padding).max(0.0),
        y: (min_y - padding).max(0.0),
        width: max_x - min_x + padding * 2.0,
        height: max_y - min_y + padding * 2.0,
    })
}

/// Route graph edges from a layout projection without mutating the graph or layout. Anchors are
/// selected from the dominant axis, so the same route contract works for horizontal, vertical,
/// mirrored and bidirectional strategies.
pub fn route_edges(
    model: &MindmapModel,
    layout: &MindmapLayoutProjection,
) -> Result<MindmapEdgeProjection, MindmapLayoutError> {
    validate(model).map_err(MindmapLayoutError::Engine)?;
    let positions = layout
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<HashMap<_, _>>();
    let mut routes = Vec::new();
    for child in &model.nodes {
        let Some(parent_id) = child.parent_id.as_deref() else {
            continue;
        };
        // Nodes hidden beneath a collapsed ancestor are intentionally absent
        // from both positions, so no connector is emitted for that branch.
        let (Some(parent_layout), Some(child_layout)) =
            (positions.get(parent_id), positions.get(child.id.as_str()))
        else {
            continue;
        };
        let (start, end, elbows) = connector_points(parent_layout, child_layout);
        routes.push(MindmapEdgeRoute {
            edge_id: None,
            parent_id: parent_id.to_string(),
            child_id: child.id.clone(),
            points: vec![start, elbows.0, elbows.1, end],
        });
    }
    for edge in &model.edges {
        let Some(source) = positions.get(edge.source_id.as_str()) else {
            continue;
        };
        let Some(target) = positions.get(edge.target_id.as_str()) else {
            continue;
        };
        let (start, end, elbows) = connector_points(source, target);
        routes.push(MindmapEdgeRoute {
            edge_id: Some(edge.id.clone()),
            parent_id: edge.source_id.clone(),
            child_id: edge.target_id.clone(),
            points: vec![start, elbows.0, elbows.1, end],
        });
    }
    Ok(MindmapEdgeProjection { routes })
}

fn connector_points(
    source: &MindmapLayoutNode,
    target: &MindmapLayoutNode,
) -> (MindmapPoint, MindmapPoint, (MindmapPoint, MindmapPoint)) {
    let source_center = MindmapPoint {
        x: source.x + source.width / 2.0,
        y: source.y + source.height / 2.0,
    };
    let target_center = MindmapPoint {
        x: target.x + target.width / 2.0,
        y: target.y + target.height / 2.0,
    };
    let dx = target_center.x - source_center.x;
    let dy = target_center.y - source_center.y;
    if dx.abs() >= dy.abs() {
        let direction = dx.signum();
        let start = MindmapPoint {
            x: source_center.x + source.width / 2.0 * direction,
            y: source_center.y,
        };
        let end = MindmapPoint {
            x: target_center.x - target.width / 2.0 * direction,
            y: target_center.y,
        };
        let middle = (start.x + end.x) / 2.0;
        (
            start,
            end,
            (
                MindmapPoint {
                    x: middle,
                    y: start.y,
                },
                MindmapPoint {
                    x: middle,
                    y: end.y,
                },
            ),
        )
    } else {
        let direction = dy.signum();
        let start = MindmapPoint {
            x: source_center.x,
            y: source_center.y + source.height / 2.0 * direction,
        };
        let end = MindmapPoint {
            x: target_center.x,
            y: target_center.y - target.height / 2.0 * direction,
        };
        let middle = (start.y + end.y) / 2.0;
        (
            start,
            end,
            (
                MindmapPoint {
                    x: start.x,
                    y: middle,
                },
                MindmapPoint {
                    x: end.x,
                    y: middle,
                },
            ),
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MindmapNodeMeasurement {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy)]
struct MeasuredNode {
    width: f32,
    height: f32,
}

/// Compute a deterministic renderer-independent layout. The persisted strategy selects one of
/// eight pure transforms; content and typed width constraints drive node dimensions. Moving a
/// viewport or changing renderer zoom never creates a graph command.
pub fn layout(
    model: &MindmapModel,
    options: MindmapLayoutOptions,
) -> Result<MindmapLayoutProjection, MindmapLayoutError> {
    layout_with_measurements(model, options, &HashMap::new())
}

pub fn layout_with_measurements(
    model: &MindmapModel,
    options: MindmapLayoutOptions,
    measurements: &HashMap<String, MindmapNodeMeasurement>,
) -> Result<MindmapLayoutProjection, MindmapLayoutError> {
    validate_layout_options(options)?;
    if measurements.values().any(|size| {
        !size.width.is_finite()
            || !size.height.is_finite()
            || size.width <= 0.0
            || size.height <= 0.0
            || size.height > 1_000_000.0
    }) {
        return Err(MindmapLayoutError::InvalidOptions);
    }
    validate(model).map_err(MindmapLayoutError::Engine)?;
    let Some(root) = model.root.as_deref() else {
        return Ok(MindmapLayoutProjection {
            nodes: Vec::new(),
            width: 0.0,
            height: 0.0,
        });
    };
    let graph = MindmapIndex::from_model(model);
    let visible = visible_preorder(&graph, root)?;
    let sizes = visible
        .iter()
        .map(|(id, _)| {
            let node = graph
                .node(id)
                .ok_or_else(|| MindmapLayoutError::MissingLayoutNode(id.clone()))?;
            let size = measurements
                .get(id)
                .map(|size| MeasuredNode {
                    width: size.width.clamp(node.style.min_width, node.style.max_width),
                    height: size.height.max(options.node_height),
                })
                .unwrap_or_else(|| measure_node(node, options));
            Ok((id.clone(), size))
        })
        .collect::<Result<HashMap<_, _>, MindmapLayoutError>>()?;
    let mut nodes = match model.settings.layout {
        MindmapLayoutKind::LogicalRight
        | MindmapLayoutKind::LogicalLeft
        | MindmapLayoutKind::MindMap
        | MindmapLayoutKind::Organization => {
            horizontal_tree_layout(&graph, root, &visible, &sizes, options)?
        }
        MindmapLayoutKind::Catalog
        | MindmapLayoutKind::TimelineHorizontal
        | MindmapLayoutKind::TimelineVertical
        | MindmapLayoutKind::Fishbone => linear_layout(&graph, root, &visible, &sizes, options)?,
    };
    apply_layout_strategy(
        model.settings.layout,
        &graph,
        root,
        &visible,
        options,
        &mut nodes,
    )?;
    Ok(normalize_layout(nodes))
}

fn visible_preorder(
    graph: &MindmapIndex<'_>,
    root: &str,
) -> Result<Vec<(String, usize)>, MindmapLayoutError> {
    let mut output = Vec::new();
    let mut stack = vec![(root.to_string(), 0usize)];
    while let Some((id, depth)) = stack.pop() {
        let node = graph
            .node(&id)
            .ok_or_else(|| MindmapLayoutError::MissingLayoutNode(id.clone()))?;
        output.push((id.clone(), depth));
        if !node.collapsed {
            for child in graph.children(Some(&id)).into_iter().rev() {
                stack.push((child.id.clone(), depth + 1));
            }
        }
    }
    Ok(output)
}

fn measure_node(node: &MindmapNode, options: MindmapLayoutOptions) -> MeasuredNode {
    let text = node
        .content
        .as_ref()
        .map(|content| content.text.as_str())
        .unwrap_or("");
    let longest_line = text
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0) as f32;
    let explicit_lines = text.lines().count().max(1) as f32;
    let preferred_width = options.node_width.max(longest_line * 7.4 + 34.0);
    let width = preferred_width.clamp(node.style.min_width, node.style.max_width);
    let wrapped_lines = ((longest_line * 7.4 + 28.0) / width).ceil().max(1.0);
    let line_count = explicit_lines.max(wrapped_lines);
    let image_height = node
        .supplement
        .image
        .as_ref()
        .map(|image| image.height.unwrap_or(72.0).clamp(24.0, 160.0))
        .unwrap_or(0.0);
    let note_badge = if node.supplement.note.is_some() || node.supplement.hyperlink.is_some() {
        12.0
    } else {
        0.0
    };
    let height = options
        .node_height
        .max(20.0 + line_count * 20.0 + image_height + note_badge);
    MeasuredNode { width, height }
}

fn horizontal_tree_layout(
    graph: &MindmapIndex<'_>,
    root: &str,
    visible: &[(String, usize)],
    sizes: &HashMap<String, MeasuredNode>,
    options: MindmapLayoutOptions,
) -> Result<Vec<MindmapLayoutNode>, MindmapLayoutError> {
    let max_depth = visible.iter().map(|(_, depth)| *depth).max().unwrap_or(0);
    let mut widths = vec![options.node_width; max_depth + 1];
    for (id, depth) in visible {
        widths[*depth] = widths[*depth].max(sizes[id].width);
    }
    let mut x_by_depth = vec![0.0; max_depth + 1];
    for depth in 1..=max_depth {
        x_by_depth[depth] =
            x_by_depth[depth - 1] + options.horizontal_gap.max(widths[depth - 1] + 72.0);
    }
    let mut next_leaf_y: f32 = 0.0;
    let mut positions = HashMap::<String, f32>::new();
    for (id, _) in visible {
        let node = graph
            .node(id)
            .ok_or_else(|| MindmapLayoutError::MissingLayoutNode(id.clone()))?;
        if node.collapsed || graph.children(Some(id)).is_empty() {
            let size = sizes[id];
            positions.insert(id.clone(), next_leaf_y + size.height / 2.0);
            next_leaf_y += options.vertical_gap.max(size.height + 32.0);
        }
    }
    for (id, _) in visible.iter().rev() {
        if positions.contains_key(id) {
            continue;
        }
        let children = graph.children(Some(id));
        let first = children
            .first()
            .and_then(|child| positions.get(&child.id))
            .copied();
        let last = children
            .last()
            .and_then(|child| positions.get(&child.id))
            .copied();
        let center = match (first, last) {
            (Some(first), Some(last)) => (first + last) / 2.0,
            _ => return Err(MindmapLayoutError::MissingLayoutNode(id.clone())),
        };
        positions.insert(id.clone(), center);
    }
    debug_assert!(positions.contains_key(root));
    Ok(visible
        .iter()
        .map(|(id, depth)| {
            let size = sizes[id];
            MindmapLayoutNode {
                id: id.clone(),
                depth: *depth,
                x: x_by_depth[*depth],
                y: positions[id] - size.height / 2.0,
                width: size.width,
                height: size.height,
            }
        })
        .collect())
}

fn linear_layout(
    _graph: &MindmapIndex<'_>,
    _root: &str,
    visible: &[(String, usize)],
    sizes: &HashMap<String, MeasuredNode>,
    options: MindmapLayoutOptions,
) -> Result<Vec<MindmapLayoutNode>, MindmapLayoutError> {
    let mut cursor = 0.0;
    Ok(visible
        .iter()
        .map(|(id, depth)| {
            let size = sizes[id];
            let node = MindmapLayoutNode {
                id: id.clone(),
                depth: *depth,
                x: *depth as f32 * 68.0,
                y: cursor,
                width: size.width,
                height: size.height,
            };
            cursor += options.vertical_gap.max(size.height + 28.0);
            node
        })
        .collect())
}

fn apply_layout_strategy(
    strategy: MindmapLayoutKind,
    graph: &MindmapIndex<'_>,
    root: &str,
    visible: &[(String, usize)],
    options: MindmapLayoutOptions,
    nodes: &mut [MindmapLayoutNode],
) -> Result<(), MindmapLayoutError> {
    match strategy {
        MindmapLayoutKind::LogicalRight | MindmapLayoutKind::Catalog => {}
        MindmapLayoutKind::LogicalLeft => {
            for node in nodes.iter_mut() {
                node.x = -node.x - node.width;
            }
        }
        MindmapLayoutKind::Organization => {
            for node in nodes.iter_mut() {
                std::mem::swap(&mut node.x, &mut node.y);
            }
        }
        MindmapLayoutKind::MindMap => {
            let root_children = graph.children(Some(root));
            let mut sides = HashMap::<String, bool>::new();
            for (index, child) in root_children.iter().enumerate() {
                let left = index < root_children.len() / 2;
                let mut stack = vec![child.id.as_str()];
                while let Some(id) = stack.pop() {
                    sides.insert(id.to_string(), left);
                    let node = graph
                        .node(id)
                        .ok_or_else(|| MindmapLayoutError::MissingLayoutNode(id.into()))?;
                    if !node.collapsed {
                        stack.extend(
                            graph
                                .children(Some(id))
                                .into_iter()
                                .map(|child| child.id.as_str()),
                        );
                    }
                }
            }
            for node in nodes.iter_mut() {
                if sides.get(&node.id).copied().unwrap_or(false) {
                    node.x = -node.x - node.width;
                }
            }
        }
        MindmapLayoutKind::TimelineHorizontal => {
            for (index, node) in nodes.iter_mut().enumerate() {
                node.x = index as f32 * options.horizontal_gap;
                node.y = node.depth as f32
                    * options.vertical_gap
                    * if index % 2 == 0 { 1.0 } else { -1.0 };
            }
        }
        MindmapLayoutKind::TimelineVertical => {
            for node in nodes.iter_mut() {
                node.x = node.depth as f32 * 120.0;
            }
        }
        MindmapLayoutKind::Fishbone => {
            let mut branch_by_id = HashMap::<String, usize>::new();
            for (branch, child) in graph.children(Some(root)).iter().enumerate() {
                let mut stack = vec![child.id.as_str()];
                while let Some(id) = stack.pop() {
                    branch_by_id.insert(id.to_string(), branch);
                    let item = graph
                        .node(id)
                        .ok_or_else(|| MindmapLayoutError::MissingLayoutNode(id.into()))?;
                    if !item.collapsed {
                        stack.extend(
                            graph
                                .children(Some(id))
                                .into_iter()
                                .map(|child| child.id.as_str()),
                        );
                    }
                }
            }
            for node in nodes.iter_mut() {
                if node.id == root {
                    node.x = 0.0;
                    node.y = 0.0;
                    continue;
                }
                let branch = branch_by_id.get(&node.id).copied().unwrap_or(0);
                let sign = if branch % 2 == 0 { -1.0 } else { 1.0 };
                node.x = (branch as f32 + 1.0) * options.horizontal_gap
                    + node.depth.saturating_sub(1) as f32 * 72.0;
                node.y = sign
                    * (node.depth as f32 * options.vertical_gap
                        + (branch / 2) as f32 * options.vertical_gap);
            }
        }
    }
    debug_assert_eq!(visible.len(), nodes.len());
    Ok(())
}

fn normalize_layout(mut nodes: Vec<MindmapLayoutNode>) -> MindmapLayoutProjection {
    if nodes.is_empty() {
        return MindmapLayoutProjection {
            nodes,
            width: 0.0,
            height: 0.0,
        };
    }
    let min_x = nodes
        .iter()
        .map(|node| node.x)
        .fold(f32::INFINITY, f32::min);
    let min_y = nodes
        .iter()
        .map(|node| node.y)
        .fold(f32::INFINITY, f32::min);
    for node in &mut nodes {
        node.x -= min_x;
        node.y -= min_y;
    }
    let width = nodes
        .iter()
        .map(|node| node.x + node.width)
        .fold(0.0, f32::max);
    let height = nodes
        .iter()
        .map(|node| node.y + node.height)
        .fold(0.0, f32::max);
    MindmapLayoutProjection {
        nodes,
        width,
        height,
    }
}

/// Portable, deterministic Markdown export. Headings encode the first six tree levels and
/// indented list items encode deeper levels. Notes are emitted as blockquotes immediately after
/// their topic so a subsequent import can restore them.
pub fn export_markdown(model: &MindmapModel) -> Result<String, MindmapEngineError> {
    validate(model)?;
    let Some(root) = model.root.as_deref() else {
        return Ok(String::new());
    };
    let graph = MindmapIndex::new(model)?;
    let mut output = String::new();
    fn write_node(
        graph: &MindmapIndex<'_>,
        id: &str,
        depth: usize,
        output: &mut String,
    ) -> Result<(), MindmapEngineError> {
        let node = graph
            .node(id)
            .ok_or_else(|| MindmapEngineError::MissingNode(id.into()))?;
        let text = node
            .content
            .as_ref()
            .map(|content| content.text.trim())
            .filter(|text| !text.is_empty())
            .unwrap_or("未命名主题");
        if depth < 6 {
            output.push_str(&"#".repeat(depth + 1));
            output.push(' ');
        } else {
            output.push_str(&"  ".repeat(depth - 6));
            output.push_str("- ");
        }
        output.push_str(&text.replace('\n', " "));
        output.push_str("\n\n");
        if let Some(note) = &node.supplement.note {
            for line in note.text.lines() {
                output.push_str("> ");
                output.push_str(line);
                output.push('\n');
            }
            output.push('\n');
        }
        for child in graph.children(Some(id)) {
            write_node(graph, &child.id, depth + 1, output)?;
        }
        Ok(())
    }
    write_node(&graph, root, 0, &mut output)?;
    Ok(output)
}

/// Import the structural Markdown subset emitted by [`export_markdown`]. Unsupported prose is
/// retained as a note on the most recent topic instead of being silently discarded.
pub fn import_markdown(input: &str) -> Result<MindmapModel, MindmapImportError> {
    let mut model = MindmapModel::default();
    let mut ancestors: Vec<String> = Vec::new();
    let mut current_id: Option<String> = None;
    let mut note_lines: Vec<String> = Vec::new();
    let flush_note =
        |model: &mut MindmapModel, current_id: &Option<String>, lines: &mut Vec<String>| {
            if lines.is_empty() {
                return;
            }
            if let Some(id) = current_id {
                if let Some(node) = model.nodes.iter_mut().find(|node| &node.id == id) {
                    node.supplement.note = Some(RichText {
                        text: lines.join("\n"),
                        runs: Vec::new(),
                    });
                }
            }
            lines.clear();
        };
    for raw in input.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(note) = trimmed.strip_prefix('>') {
            note_lines.push(note.trim_start().to_string());
            continue;
        }
        let heading_marks = trimmed
            .chars()
            .take_while(|character| *character == '#')
            .count();
        let (depth, text) = if (1..=6).contains(&heading_marks)
            && trimmed
                .chars()
                .nth(heading_marks)
                .is_some_and(char::is_whitespace)
        {
            (heading_marks - 1, trimmed[heading_marks..].trim())
        } else if let Some(marker) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            let indentation = raw
                .chars()
                .take_while(|character| character.is_whitespace())
                .count();
            (6 + indentation / 2, marker.trim())
        } else {
            note_lines.push(trimmed.to_string());
            continue;
        };
        flush_note(&mut model, &current_id, &mut note_lines);
        if text.is_empty() {
            continue;
        }
        let effective_depth = if model.nodes.is_empty() {
            0
        } else {
            depth.max(1).min(ancestors.len())
        };
        let parent_id = if effective_depth == 0 {
            None
        } else {
            ancestors.get(effective_depth - 1).cloned()
        };
        let id = format!("node-{}", model.nodes.len() + 1);
        model.nodes.push(MindmapNode {
            id: id.clone(),
            parent_id,
            content: Some(RichText {
                text: text.into(),
                runs: Vec::new(),
            }),
            ..MindmapNode::default()
        });
        if model.root.is_none() {
            model.root = Some(id.clone());
        }
        ancestors.truncate(effective_depth);
        ancestors.push(id.clone());
        current_id = Some(id);
    }
    flush_note(&mut model, &current_id, &mut note_lines);
    if model.nodes.is_empty() {
        return Err(MindmapImportError::NoTopics);
    }
    validate(&model).map_err(MindmapImportError::Engine)?;
    Ok(model)
}

#[derive(Debug, thiserror::Error)]
pub enum MindmapImportError {
    #[error("Markdown 中没有可导入的主题")]
    NoTopics,
    #[error("导入后的 Mindmap 无效：{0}")]
    Engine(MindmapEngineError),
}

fn validate_layout_options(options: MindmapLayoutOptions) -> Result<(), MindmapLayoutError> {
    let values = [
        options.horizontal_gap,
        options.vertical_gap,
        options.node_width,
        options.node_height,
    ];
    if values
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return Err(MindmapLayoutError::InvalidOptions);
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum MindmapLayoutError {
    #[error("mindmap layout 参数必须是有限正数")]
    InvalidOptions,
    #[error("Mindmap layout 查询失败：{0}")]
    Engine(MindmapEngineError),
    #[error("Mindmap layout 缺少节点 {0}")]
    MissingLayoutNode(String),
}

#[derive(Debug, thiserror::Error)]
pub enum MindmapEngineError {
    #[error("command batch 不能没有 command")]
    EmptyBatch,
    #[error("没有可撤销的 mindmap 事务")]
    NothingToUndo,
    #[error("没有可重做的 mindmap 事务")]
    NothingToRedo,
    #[error("revision 冲突：服务端是 {expected}，事务基于 {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("mindmap 节点 id 不能为空")]
    EmptyId,
    #[error("节点 {0} 已存在")]
    DuplicateNode(String),
    #[error("边 {0} 已存在")]
    DuplicateEdge(String),
    #[error("graph entity {0} 已存在")]
    DuplicateEntity(String),
    #[error("找不到节点 {0}")]
    MissingNode(String),
    #[error("找不到边 {0}")]
    MissingEdge(String),
    #[error("找不到概要 {0}")]
    MissingSummary(String),
    #[error("找不到外框 {0}")]
    MissingBoundary(String),
    #[error("找不到公式 {0}")]
    MissingFormula(String),
    #[error("节点 {0} 没有可格式化的文字")]
    MissingNodeText(String),
    #[error("mindmap 文字范围 {start}..{end} 超出文本长度 {text_len}")]
    InvalidTextRange {
        start: usize,
        end: usize,
        text_len: usize,
    },
    #[error("mindmap 行内样式 patch 不能为空")]
    InvalidInlinePatch,
    #[error("边 {0} 不能连接自身")]
    SelfEdge(String),
    #[error("mindmap 已有 root，不能再添加 root")]
    RootAlreadyExists,
    #[error("mindmap root 不能移动到其它节点下")]
    RootCannotMoveUnderNode,
    #[error("移动节点会产生环：{0}")]
    Cycle(String),
    #[error("插入位置 {index} 超出同级节点数 {len}")]
    InvalidIndex { index: usize, len: usize },
    #[error("revision 溢出")]
    RevisionOverflow,
    #[error("Mindmap schema 校验失败：{0}")]
    Schema(#[from] SchemaValidationError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rich_text(text: &str) -> RichText {
        RichText {
            text: text.into(),
            runs: Vec::new(),
        }
    }

    fn engine() -> MindmapEngine {
        MindmapEngine::new(MindmapModel::default(), 0).unwrap()
    }

    #[test]
    fn renderer_measurements_change_projection_without_changing_snapshot() {
        let mut engine = engine();
        engine
            .execute(MindmapCommandBatch {
                base_revision: 0,
                commands: vec![MindmapCommand::AddNode {
                    node_id: "root".into(),
                    parent_id: None,
                    content: Some(rich_text("长中文主题")),
                    attrs: Map::new(),
                    index: 0,
                }],
            })
            .unwrap();
        let original = engine.model().clone();
        let measurements = HashMap::from([(
            "root".into(),
            MindmapNodeMeasurement {
                width: 250.0,
                height: 120.0,
            },
        )]);
        let projection = MindmapProjection::build_with_measurements(
            engine.model(),
            MindmapLayoutOptions::default(),
            MindmapTheme::Light,
            &measurements,
        )
        .unwrap();
        assert_eq!(projection.layout.nodes[0].width, 250.0);
        assert_eq!(projection.layout.nodes[0].height, 120.0);
        assert_eq!(engine.model(), &original);
        let invalid = HashMap::from([(
            "root".into(),
            MindmapNodeMeasurement {
                width: f32::NAN,
                height: 20.0,
            },
        )]);
        assert!(layout_with_measurements(
            engine.model(),
            MindmapLayoutOptions::default(),
            &invalid
        )
        .is_err());
    }

    #[test]
    fn graph_operations_preserve_atomicity_and_order() {
        let mut engine = engine();
        let result = engine
            .execute(MindmapCommandBatch {
                base_revision: 0,
                commands: vec![MindmapCommand::AddNode {
                    node_id: "root".into(),
                    parent_id: None,
                    content: None,
                    attrs: Map::new(),
                    index: 0,
                }],
            })
            .unwrap();
        assert!(result.invalidation.structure_changed);
        assert_eq!(
            result.invalidation.changed_entities[0].entity_type,
            "mindmap.node"
        );
        assert_eq!(
            result.mutations[0].to_record().unwrap().type_id,
            "mindmap.nodeInserted"
        );
        engine
            .execute(MindmapCommandBatch {
                base_revision: 1,
                commands: vec![MindmapCommand::AddNode {
                    node_id: "child".into(),
                    parent_id: Some("root".into()),
                    content: None,
                    attrs: Map::new(),
                    index: 0,
                }],
            })
            .unwrap();
        assert_eq!(engine.model().nodes[1].id, "child");
        let error = engine
            .execute(MindmapCommandBatch {
                base_revision: 2,
                commands: vec![
                    MindmapCommand::UpdateNode {
                        node_id: "child".into(),
                        content: None,
                        attrs: Some(Map::new()),
                    },
                    MindmapCommand::AddNode {
                        node_id: "other-root".into(),
                        parent_id: None,
                        content: None,
                        attrs: Map::new(),
                        index: 0,
                    },
                ],
            })
            .unwrap_err();
        assert!(matches!(error, MindmapEngineError::RootAlreadyExists));
        assert_eq!(engine.revision(), 2);
    }

    #[test]
    fn command_batch_uses_semantic_camel_case_wire_names() {
        let batch = MindmapCommandBatch {
            base_revision: 0,
            commands: vec![MindmapCommand::DeleteNode {
                node_id: "node-1".into(),
            }],
        };
        let json = serde_json::to_value(batch).unwrap();
        assert_eq!(json["commands"][0]["type"], "deleteNode");
        assert_eq!(json["commands"][0]["nodeId"], "node-1");
    }

    #[test]
    fn deleting_root_removes_the_whole_subtree() {
        let model = MindmapModel {
            root: Some("root".into()),
            nodes: vec![
                MindmapNode {
                    id: "root".into(),
                    ..MindmapNode::default()
                },
                MindmapNode {
                    id: "child".into(),
                    parent_id: Some("root".into()),
                    ..MindmapNode::default()
                },
            ],
            ..MindmapModel::default()
        };
        let mut engine = MindmapEngine::new(model, 0).unwrap();
        engine
            .execute(MindmapCommandBatch {
                base_revision: 0,
                commands: vec![MindmapCommand::DeleteNode {
                    node_id: "root".into(),
                }],
            })
            .unwrap();
        assert!(engine.model().nodes.is_empty());
        assert!(engine.model().root.is_none());
    }

    #[test]
    fn moving_under_descendant_is_rejected_without_mutation() {
        let model = MindmapModel {
            root: Some("root".into()),
            nodes: vec![
                MindmapNode {
                    id: "root".into(),
                    ..MindmapNode::default()
                },
                MindmapNode {
                    id: "child".into(),
                    parent_id: Some("root".into()),
                    ..MindmapNode::default()
                },
            ],
            ..MindmapModel::default()
        };
        let mut engine = MindmapEngine::new(model.clone(), 0).unwrap();
        let error = engine
            .execute(MindmapCommandBatch {
                base_revision: 0,
                commands: vec![MindmapCommand::MoveNode {
                    node_id: "root".into(),
                    new_parent_id: Some("child".into()),
                    index: 0,
                }],
            })
            .unwrap_err();
        assert!(matches!(error, MindmapEngineError::Cycle(_)));
        assert_eq!(engine.model(), &model);
    }

    #[test]
    fn layout_is_a_deterministic_projection_not_graph_state() {
        let model = MindmapModel {
            root: Some("root".into()),
            nodes: vec![
                MindmapNode {
                    id: "root".into(),
                    ..MindmapNode::default()
                },
                MindmapNode {
                    id: "left".into(),
                    parent_id: Some("root".into()),
                    ..MindmapNode::default()
                },
                MindmapNode {
                    id: "right".into(),
                    parent_id: Some("root".into()),
                    ..MindmapNode::default()
                },
            ],
            ..MindmapModel::default()
        };
        let projection = layout(&model, MindmapLayoutOptions::default()).unwrap();
        assert_eq!(projection.nodes.len(), 3);
        let root = projection
            .nodes
            .iter()
            .find(|node| node.id == "root")
            .unwrap();
        let left = projection
            .nodes
            .iter()
            .find(|node| node.id == "left")
            .unwrap();
        let right = projection
            .nodes
            .iter()
            .find(|node| node.id == "right")
            .unwrap();
        assert!(left.y < right.y);
        assert_eq!(root.x, 0.0);
        assert_eq!(left.x, right.x);
        assert_eq!(model.nodes[0].attrs.len(), 0);
    }

    #[test]
    fn all_persisted_layout_strategies_are_finite_and_content_drives_node_size() {
        let mut model = MindmapModel {
            root: Some("root".into()),
            nodes: vec![
                MindmapNode {
                    id: "root".into(),
                    content: Some(rich_text("root")),
                    ..MindmapNode::default()
                },
                MindmapNode {
                    id: "a".into(),
                    parent_id: Some("root".into()),
                    content: Some(rich_text("a topic with enough text to require a wider box")),
                    ..MindmapNode::default()
                },
                MindmapNode {
                    id: "b".into(),
                    parent_id: Some("root".into()),
                    content: Some(rich_text("b")),
                    ..MindmapNode::default()
                },
            ],
            ..MindmapModel::default()
        };
        let strategies = [
            MindmapLayoutKind::LogicalRight,
            MindmapLayoutKind::LogicalLeft,
            MindmapLayoutKind::MindMap,
            MindmapLayoutKind::Organization,
            MindmapLayoutKind::Catalog,
            MindmapLayoutKind::TimelineHorizontal,
            MindmapLayoutKind::TimelineVertical,
            MindmapLayoutKind::Fishbone,
        ];
        let mut signatures = HashSet::new();
        for strategy in strategies {
            model.settings.layout = strategy;
            let projection = layout(&model, MindmapLayoutOptions::default()).unwrap();
            assert_eq!(projection.nodes.len(), 3);
            assert!(projection.width.is_finite() && projection.width > 0.0);
            assert!(projection.height.is_finite() && projection.height > 0.0);
            assert!(projection.nodes.iter().all(|node| node.x >= 0.0
                && node.y >= 0.0
                && node.width > 0.0
                && node.height > 0.0));
            let wide = projection.nodes.iter().find(|node| node.id == "a").unwrap();
            let short = projection.nodes.iter().find(|node| node.id == "b").unwrap();
            assert!(wide.width > short.width);
            signatures.insert(
                projection
                    .nodes
                    .iter()
                    .map(|node| format!("{:.0}:{:.0}", node.x, node.y))
                    .collect::<Vec<_>>()
                    .join("|"),
            );
        }
        assert!(
            signatures.len() >= 6,
            "layout strategies should not collapse to one projection"
        );
    }

    #[test]
    fn markdown_roundtrip_preserves_tree_text_and_notes() {
        let mut root = MindmapNode {
            id: "root".into(),
            content: Some(rich_text("Roadmap")),
            ..MindmapNode::default()
        };
        root.supplement.note = Some(rich_text("Shared context"));
        let model = MindmapModel {
            root: Some("root".into()),
            nodes: vec![
                root,
                MindmapNode {
                    id: "child".into(),
                    parent_id: Some("root".into()),
                    content: Some(rich_text("Milestone")),
                    ..MindmapNode::default()
                },
            ],
            ..MindmapModel::default()
        };
        let markdown = export_markdown(&model).unwrap();
        let imported = import_markdown(&markdown).unwrap();
        assert_eq!(imported.nodes.len(), 2);
        assert_eq!(imported.nodes[0].content.as_ref().unwrap().text, "Roadmap");
        assert_eq!(
            imported.nodes[0].supplement.note.as_ref().unwrap().text,
            "Shared context"
        );
        assert_eq!(
            imported.nodes[1].parent_id.as_deref(),
            imported.root.as_deref()
        );
    }

    #[test]
    fn large_map_layout_and_routing_remain_linear_enough_for_interactive_use() {
        let mut nodes = Vec::with_capacity(10_001);
        nodes.push(MindmapNode {
            id: "root".into(),
            content: Some(rich_text("Root")),
            ..MindmapNode::default()
        });
        for index in 0..10_000 {
            nodes.push(MindmapNode {
                id: format!("node-{index}"),
                parent_id: Some("root".into()),
                content: Some(rich_text("Topic")),
                ..MindmapNode::default()
            });
        }
        let model = MindmapModel {
            root: Some("root".into()),
            nodes,
            summaries: vec![MindmapSummary {
                id: "summary".into(),
                start_node_id: "node-0".into(),
                end_node_id: "node-9999".into(),
                content: rich_text("All topics"),
            }],
            boundaries: vec![MindmapBoundary {
                id: "boundary".into(),
                root_node_id: "node-0".into(),
                label: Some(rich_text("First topic")),
            }],
            formulas: vec![MindmapFormula {
                id: "formula".into(),
                node_id: "node-9999".into(),
                source: "x^2".into(),
                display: MindmapFormulaDisplay::Inline,
            }],
            ..MindmapModel::default()
        };
        let started = std::time::Instant::now();
        let projection =
            MindmapProjection::build(&model, MindmapLayoutOptions::default(), MindmapTheme::Light)
                .unwrap();
        assert_eq!(projection.layout.nodes.len(), 10_001);
        assert_eq!(projection.edges.routes.len(), 10_000);
        assert_eq!(projection.advanced.summaries[0].node_ids.len(), 10_000);
        assert_eq!(projection.advanced.boundaries[0].node_ids, ["node-0"]);
        assert_eq!(projection.advanced.formulas[0].node_id, "node-9999");
        assert!(started.elapsed() < std::time::Duration::from_secs(5));

        let mut engine = MindmapEngine::new(model, 0).unwrap();
        let formula_update = engine
            .execute(MindmapCommandBatch {
                base_revision: 0,
                commands: vec![MindmapCommand::UpdateFormula {
                    formula_id: "formula".into(),
                    node_id: None,
                    source: Some("y^2".into()),
                    display: None,
                }],
            })
            .unwrap();
        assert!(!formula_update.invalidation.structure_changed);
        assert_eq!(formula_update.invalidation.changed_entities.len(), 1);

        let summary_label_update = engine
            .execute(MindmapCommandBatch {
                base_revision: 1,
                commands: vec![MindmapCommand::UpdateSummary {
                    summary_id: "summary".into(),
                    start_node_id: None,
                    end_node_id: None,
                    content: Some(rich_text("Updated")),
                }],
            })
            .unwrap();
        assert!(!summary_label_update.invalidation.structure_changed);

        let summary_range_update = engine
            .execute(MindmapCommandBatch {
                base_revision: 2,
                commands: vec![MindmapCommand::UpdateSummary {
                    summary_id: "summary".into(),
                    start_node_id: Some("node-1".into()),
                    end_node_id: None,
                    content: None,
                }],
            })
            .unwrap();
        assert!(summary_range_update.invalidation.structure_changed);
    }

    #[test]
    fn projection_cache_reuses_layout_for_local_advanced_changes_and_matches_full_build() {
        let model = MindmapModel {
            root: Some("root".into()),
            nodes: vec![MindmapNode {
                id: "root".into(),
                content: Some(rich_text("Root")),
                ..Default::default()
            }],
            formulas: vec![MindmapFormula {
                id: "formula".into(),
                node_id: "root".into(),
                source: "x".into(),
                display: MindmapFormulaDisplay::Inline,
            }],
            ..Default::default()
        };
        let options = MindmapLayoutOptions::default();
        let previous = MindmapProjection::build(&model, options, MindmapTheme::Light).unwrap();
        let mut engine = MindmapEngine::new(model, 0).unwrap();
        let changed = engine
            .execute(MindmapCommandBatch {
                base_revision: 0,
                commands: vec![MindmapCommand::UpdateFormula {
                    formula_id: "formula".into(),
                    node_id: None,
                    source: Some("x^2 + y^2".into()),
                    display: None,
                }],
            })
            .unwrap();
        let updated = update_projection(
            &previous,
            engine.model(),
            &changed.invalidation,
            options,
            MindmapTheme::Light,
        )
        .unwrap();
        let full = MindmapProjection::build(engine.model(), options, MindmapTheme::Light).unwrap();
        assert_eq!(updated.projection, full);
        assert_eq!(updated.recomputed_layout_nodes, 0);
        assert_eq!(updated.recomputed_routes, 0);
        assert_eq!(updated.recomputed_advanced_entities, 1);

        let changed = engine
            .execute(MindmapCommandBatch {
                base_revision: 1,
                commands: vec![MindmapCommand::ReplaceNodeText {
                    node_id: "root".into(),
                    content: Some(rich_text("A much wider root topic")),
                }],
            })
            .unwrap();
        let updated = update_projection(
            &updated.projection,
            engine.model(),
            &changed.invalidation,
            options,
            MindmapTheme::Light,
        )
        .unwrap();
        assert_eq!(updated.recomputed_layout_nodes, 1);
        assert_eq!(
            updated.projection,
            MindmapProjection::build(engine.model(), options, MindmapTheme::Light).unwrap()
        );
    }

    #[test]
    fn edge_routes_are_stable_projection_only() {
        let model = MindmapModel {
            root: Some("root".into()),
            nodes: vec![
                MindmapNode {
                    id: "root".into(),
                    ..MindmapNode::default()
                },
                MindmapNode {
                    id: "child".into(),
                    parent_id: Some("root".into()),
                    ..MindmapNode::default()
                },
            ],
            ..MindmapModel::default()
        };
        let layout = layout(&model, MindmapLayoutOptions::default()).unwrap();
        let routes = route_edges(&model, &layout).unwrap();
        assert_eq!(routes.routes.len(), 1);
        assert_eq!(routes.routes[0].parent_id, "root");
        assert_eq!(routes.routes[0].child_id, "child");
        assert_eq!(routes.routes[0].points.len(), 4);
        assert_eq!(routes.routes[0].points.first().unwrap().x, 160.0);
        assert_eq!(routes.routes[0].points.last().unwrap().x, 240.0);
        assert_eq!(model.nodes[0].attrs.len(), 0);
    }

    #[test]
    fn explicit_edges_and_collapse_are_semantic_and_projected() {
        let model = MindmapModel {
            root: Some("root".into()),
            nodes: vec![
                MindmapNode {
                    id: "root".into(),
                    ..MindmapNode::default()
                },
                MindmapNode {
                    id: "child".into(),
                    parent_id: Some("root".into()),
                    ..MindmapNode::default()
                },
                MindmapNode {
                    id: "leaf".into(),
                    parent_id: Some("child".into()),
                    ..MindmapNode::default()
                },
            ],
            ..MindmapModel::default()
        };
        let mut engine = MindmapEngine::new(model, 0).unwrap();
        let change = engine
            .execute(MindmapCommandBatch {
                base_revision: 0,
                commands: vec![MindmapCommand::AddEdge {
                    edge: MindmapEdge {
                        id: "cross-link".into(),
                        source_id: "root".into(),
                        target_id: "leaf".into(),
                        ..MindmapEdge::default()
                    },
                }],
            })
            .unwrap();
        assert!(change
            .invalidation
            .changed_entities
            .iter()
            .any(|entity| entity.entity_type == "mindmap.edge"));
        engine
            .execute(MindmapCommandBatch {
                base_revision: 1,
                commands: vec![MindmapCommand::UpdateEdge {
                    edge_id: "cross-link".into(),
                    source_id: None,
                    target_id: None,
                    label: Some(Some(RichText {
                        text: "依赖".into(),
                        runs: Vec::new(),
                    })),
                    attrs: None,
                }],
            })
            .unwrap();
        assert_eq!(engine.model().edges[0].label.as_ref().unwrap().text, "依赖");
        engine
            .execute(MindmapCommandBatch {
                base_revision: 2,
                commands: vec![MindmapCommand::SetNodeCollapsed {
                    node_id: "child".into(),
                    collapsed: true,
                }],
            })
            .unwrap();
        let projection = MindmapProjection::build(
            engine.model(),
            MindmapLayoutOptions::default(),
            MindmapTheme::Dark,
        )
        .unwrap();
        assert_eq!(projection.theme, MindmapTheme::Dark);
        assert_eq!(projection.layout.nodes.len(), 2);
        assert!(projection
            .edges
            .routes
            .iter()
            .all(|route| route.child_id != "leaf"));
    }

    #[test]
    fn edge_endpoint_and_label_updates_are_atomic_and_undoable() {
        let model = MindmapModel {
            root: Some("root".into()),
            nodes: vec![
                MindmapNode {
                    id: "root".into(),
                    ..MindmapNode::default()
                },
                MindmapNode {
                    id: "a".into(),
                    parent_id: Some("root".into()),
                    ..MindmapNode::default()
                },
                MindmapNode {
                    id: "b".into(),
                    parent_id: Some("root".into()),
                    ..MindmapNode::default()
                },
            ],
            edges: vec![MindmapEdge {
                id: "edge".into(),
                source_id: "a".into(),
                target_id: "b".into(),
                ..MindmapEdge::default()
            }],
            ..MindmapModel::default()
        };
        let original = model.clone();
        let mut engine = MindmapEngine::new(model, 7).unwrap();
        engine
            .execute(MindmapCommandBatch {
                base_revision: 7,
                commands: vec![MindmapCommand::UpdateEdge {
                    edge_id: "edge".into(),
                    source_id: Some("root".into()),
                    target_id: None,
                    label: Some(Some(RichText {
                        text: "依赖".into(),
                        runs: Vec::new(),
                    })),
                    attrs: None,
                }],
            })
            .unwrap();
        let updated = engine.model().clone();
        assert_eq!(updated.edges[0].source_id, "root");
        assert_eq!(updated.edges[0].label.as_ref().unwrap().text, "依赖");

        let failed = engine.execute(MindmapCommandBatch {
            base_revision: 8,
            commands: vec![MindmapCommand::UpdateEdge {
                edge_id: "edge".into(),
                source_id: None,
                target_id: Some("root".into()),
                label: Some(None),
                attrs: None,
            }],
        });
        assert!(matches!(failed, Err(MindmapEngineError::SelfEdge(_))));
        assert_eq!(engine.revision(), 8);
        assert_eq!(engine.model(), &updated);

        engine.undo(8).unwrap();
        assert_eq!(engine.model(), &original);
        engine.redo(9).unwrap();
        assert_eq!(engine.model(), &updated);
    }

    #[test]
    fn node_text_range_patch_uses_unicode_offsets_and_is_atomic_and_undoable() {
        let original_text = rich_text("A😀中Z");
        let model = MindmapModel {
            root: Some("root".into()),
            nodes: vec![MindmapNode {
                id: "root".into(),
                content: Some(original_text.clone()),
                ..MindmapNode::default()
            }],
            ..MindmapModel::default()
        };
        let mut engine = MindmapEngine::new(model, 3).unwrap();
        let change = engine
            .execute(MindmapCommandBatch {
                base_revision: 3,
                commands: vec![MindmapCommand::PatchNodeTextRange {
                    node_id: "root".into(),
                    range: MindmapTextRange { start: 1, end: 3 },
                    patch: MindmapInlineStylePatch {
                        bold: Some(Some(true)),
                        ..MindmapInlineStylePatch::default()
                    },
                }],
            })
            .unwrap();
        let content = engine.model().nodes[0].content.as_ref().unwrap();
        assert_eq!(content.runs.len(), 3);
        assert_eq!((content.runs[1].start, content.runs[1].end), (1, 3));
        assert!(content.runs[1].style.bold);
        assert!(!change.invalidation.structure_changed);
        assert_eq!(change.invalidation.changed_entities.len(), 1);
        assert_eq!(change.invalidation.changed_entities[0].entity_id, "root");

        let formatted = engine.model().clone();
        let failed = engine.execute(MindmapCommandBatch {
            base_revision: 4,
            commands: vec![MindmapCommand::PatchNodeTextRange {
                node_id: "root".into(),
                range: MindmapTextRange { start: 2, end: 5 },
                patch: MindmapInlineStylePatch {
                    italic: Some(Some(true)),
                    ..MindmapInlineStylePatch::default()
                },
            }],
        });
        assert!(matches!(
            failed,
            Err(MindmapEngineError::InvalidTextRange { .. })
        ));
        assert_eq!(engine.revision(), 4);
        assert_eq!(engine.model(), &formatted);

        engine.undo(4).unwrap();
        assert_eq!(engine.model().nodes[0].content, Some(original_text));
        engine.redo(5).unwrap();
        assert_eq!(engine.model(), &formatted);
    }

    #[test]
    fn replace_node_text_can_clear_content_and_json_null_clears_edge_label() {
        let command: MindmapCommand = serde_json::from_value(serde_json::json!({
            "type": "updateEdge",
            "edgeId": "edge",
            "label": null
        }))
        .unwrap();
        assert!(matches!(
            command,
            MindmapCommand::UpdateEdge {
                label: Some(None),
                ..
            }
        ));

        let model = MindmapModel {
            root: Some("root".into()),
            nodes: vec![MindmapNode {
                id: "root".into(),
                content: Some(rich_text("before")),
                ..MindmapNode::default()
            }],
            ..MindmapModel::default()
        };
        let mut engine = MindmapEngine::new(model, 0).unwrap();
        engine
            .execute(MindmapCommandBatch {
                base_revision: 0,
                commands: vec![MindmapCommand::ReplaceNodeText {
                    node_id: "root".into(),
                    content: None,
                }],
            })
            .unwrap();
        assert!(engine.model().nodes[0].content.is_none());
        engine.undo(1).unwrap();
        assert_eq!(
            engine.model().nodes[0].content.as_ref().unwrap().text,
            "before"
        );
    }

    #[test]
    fn advanced_entities_are_typed_reversible_and_cleaned_with_structure() {
        let node = |id: &str, parent: Option<&str>| MindmapNode {
            id: id.into(),
            parent_id: parent.map(str::to_owned),
            content: Some(rich_text(id)),
            ..MindmapNode::default()
        };
        let model = MindmapModel {
            root: Some("root".into()),
            nodes: vec![
                node("root", None),
                node("a", Some("root")),
                node("b", Some("root")),
                node("c", Some("root")),
            ],
            ..MindmapModel::default()
        };
        let mut engine = MindmapEngine::new(model, 0).unwrap();
        let inserted = engine
            .execute(MindmapCommandBatch {
                base_revision: 0,
                commands: vec![
                    MindmapCommand::AddSummary {
                        summary: MindmapSummary {
                            id: "summary".into(),
                            start_node_id: "a".into(),
                            end_node_id: "b".into(),
                            content: rich_text("结论"),
                        },
                    },
                    MindmapCommand::AddBoundary {
                        boundary: MindmapBoundary {
                            id: "boundary".into(),
                            root_node_id: "a".into(),
                            label: Some(rich_text("范围")),
                        },
                    },
                    MindmapCommand::AddFormula {
                        formula: MindmapFormula {
                            id: "formula".into(),
                            node_id: "a".into(),
                            source: "x^2".into(),
                            display: MindmapFormulaDisplay::Inline,
                        },
                    },
                ],
            })
            .unwrap();
        assert!(inserted.invalidation.structure_changed);
        assert_eq!(engine.model().summaries.len(), 1);
        assert_eq!(engine.model().boundaries.len(), 1);
        assert_eq!(engine.model().formulas.len(), 1);
        let projection = MindmapProjection::build(
            engine.model(),
            MindmapLayoutOptions::default(),
            MindmapTheme::Light,
        )
        .unwrap();
        assert_eq!(projection.advanced.summaries[0].node_ids, ["a", "b"]);
        assert_eq!(projection.advanced.boundaries[0].node_ids, ["a"]);
        assert_eq!(projection.advanced.formulas[0].node_id, "a");
        assert!(projection.advanced.summaries[0]
            .points
            .iter()
            .all(|point| point.x.is_finite() && point.y.is_finite()));

        engine
            .execute(MindmapCommandBatch {
                base_revision: 1,
                commands: vec![
                    MindmapCommand::UpdateSummary {
                        summary_id: "summary".into(),
                        start_node_id: None,
                        end_node_id: Some("c".into()),
                        content: Some(rich_text("更新结论")),
                    },
                    MindmapCommand::UpdateBoundary {
                        boundary_id: "boundary".into(),
                        root_node_id: Some("b".into()),
                        label: Some(None),
                    },
                    MindmapCommand::UpdateFormula {
                        formula_id: "formula".into(),
                        node_id: Some("b".into()),
                        source: Some("y^2".into()),
                        display: Some(MindmapFormulaDisplay::Block),
                    },
                ],
            })
            .unwrap();
        let updated = engine.model().clone();
        assert_eq!(updated.summaries[0].end_node_id, "c");
        assert!(updated.boundaries[0].label.is_none());
        assert_eq!(updated.formulas[0].source, "y^2");

        let failed = engine.execute(MindmapCommandBatch {
            base_revision: 2,
            commands: vec![MindmapCommand::UpdateSummary {
                summary_id: "summary".into(),
                start_node_id: Some("c".into()),
                end_node_id: Some("a".into()),
                content: None,
            }],
        });
        assert!(matches!(failed, Err(MindmapEngineError::Schema(_))));
        assert_eq!(engine.revision(), 2);
        assert_eq!(engine.model(), &updated);

        engine
            .execute(MindmapCommandBatch {
                base_revision: 2,
                commands: vec![MindmapCommand::MoveNode {
                    node_id: "c".into(),
                    new_parent_id: Some("b".into()),
                    index: 0,
                }],
            })
            .unwrap();
        assert!(engine.model().summaries.is_empty());
        assert_eq!(engine.model().boundaries.len(), 1);
        assert_eq!(engine.model().formulas.len(), 1);
        engine.undo(3).unwrap();
        assert_eq!(engine.model(), &updated);

        engine
            .execute(MindmapCommandBatch {
                base_revision: 4,
                commands: vec![MindmapCommand::DeleteNode {
                    node_id: "root".into(),
                }],
            })
            .unwrap();
        assert!(engine.model().summaries.is_empty());
        assert!(engine.model().boundaries.is_empty());
        assert!(engine.model().formulas.is_empty());
        engine.undo(5).unwrap();
        assert_eq!(engine.model(), &updated);
    }

    #[test]
    fn registry_is_unique_and_matches_real_wire_commands() {
        let registry = mindmap_command_registry();
        let ids = registry
            .iter()
            .map(|descriptor| descriptor.type_id)
            .collect::<HashSet<_>>();
        assert_eq!(ids.len(), registry.len());
        assert!(ids.contains("mindmap.setSettings"));
        assert!(ids.contains("mindmap.replaceNodeText"));
        assert!(ids.contains("mindmap.patchNodeTextRange"));
        assert!(ids.contains("mindmap.setNodeStyle"));
        assert!(ids.contains("mindmap.setEdgeStyle"));
        for descriptor in registry {
            assert!(descriptor.type_id.starts_with("mindmap."));
            assert!(descriptor.scope.starts_with("mindmap."));
        }
    }

    #[test]
    fn typed_inverse_history_roundtrips_batch_delete_and_move() {
        let mut engine = engine();
        engine
            .execute(MindmapCommandBatch {
                base_revision: 0,
                commands: vec![
                    MindmapCommand::AddNode {
                        node_id: "root".into(),
                        parent_id: None,
                        content: None,
                        attrs: Map::new(),
                        index: 0,
                    },
                    MindmapCommand::AddNode {
                        node_id: "a".into(),
                        parent_id: Some("root".into()),
                        content: None,
                        attrs: Map::new(),
                        index: 0,
                    },
                    MindmapCommand::AddNode {
                        node_id: "b".into(),
                        parent_id: Some("root".into()),
                        content: None,
                        attrs: Map::new(),
                        index: 1,
                    },
                    MindmapCommand::AddEdge {
                        edge: MindmapEdge {
                            id: "edge".into(),
                            source_id: "a".into(),
                            target_id: "b".into(),
                            ..MindmapEdge::default()
                        },
                    },
                    MindmapCommand::SetSettings {
                        settings: MindmapSettings {
                            layout: oo_schema::MindmapLayoutKind::MindMap,
                            theme_id: Some("ocean".into()),
                            ..MindmapSettings::default()
                        },
                    },
                ],
            })
            .unwrap();
        let populated = engine.model().clone();
        assert!(engine.can_undo());

        let undo = engine.undo(1).unwrap();
        assert_eq!(engine.model(), &MindmapModel::default());
        assert_eq!(undo.revision, 2);
        assert!(engine.can_redo());
        engine.redo(2).unwrap();
        assert_eq!(engine.model(), &populated);

        engine
            .execute(MindmapCommandBatch {
                base_revision: 3,
                commands: vec![MindmapCommand::MoveNode {
                    node_id: "b".into(),
                    new_parent_id: Some("a".into()),
                    index: 0,
                }],
            })
            .unwrap();
        assert_eq!(
            node(engine.model(), "b").unwrap().parent_id.as_deref(),
            Some("a")
        );
        engine.undo(4).unwrap();
        assert_eq!(engine.model(), &populated);

        engine
            .execute(MindmapCommandBatch {
                base_revision: 5,
                commands: vec![MindmapCommand::DeleteNode {
                    node_id: "a".into(),
                }],
            })
            .unwrap();
        assert!(engine.model().edges.is_empty());
        engine.undo(6).unwrap();
        assert_eq!(engine.model(), &populated);
    }

    #[test]
    fn index_preserves_sibling_order_and_subtree_in_linear_index() {
        let model = MindmapModel {
            root: Some("root".into()),
            nodes: vec![
                MindmapNode {
                    id: "root".into(),
                    ..MindmapNode::default()
                },
                MindmapNode {
                    id: "a".into(),
                    parent_id: Some("root".into()),
                    ..MindmapNode::default()
                },
                MindmapNode {
                    id: "a1".into(),
                    parent_id: Some("a".into()),
                    ..MindmapNode::default()
                },
                MindmapNode {
                    id: "b".into(),
                    parent_id: Some("root".into()),
                    ..MindmapNode::default()
                },
            ],
            ..MindmapModel::default()
        };
        let index = MindmapIndex::new(&model).unwrap();
        assert_eq!(
            index
                .children(Some("root"))
                .iter()
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        assert_eq!(index.collect_subtree("a").unwrap(), vec!["a", "a1"]);
    }
}
