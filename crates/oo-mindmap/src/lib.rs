//! Mindmap Artifact 的 Graph engine。
//!
//! 思维导图不是 Document Block：节点通过 parentId 组成有根图，文本是节点属性。本 crate
//! 负责结构事务、原子性、schema 不变量，以及只读布局 projection；边路由和最终绘制仍由
//! 上层 renderer 负责。

use std::collections::HashMap;

use oo_protocol::{EntityRef, Invalidation, MutationRecord};
use oo_schema::{MindmapEdge, MindmapModel, MindmapNode, RichText, SchemaValidationError};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MindmapCommandBatch {
    pub base_revision: u64,
    pub commands: Vec<MindmapCommand>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MindmapCommand {
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
        #[serde(default)]
        attrs: Option<Map<String, Value>>,
    },
    DeleteEdge {
        edge_id: String,
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
    AddNode {
        node: MindmapNode,
        index: usize,
    },
    AddEdge {
        edge: MindmapEdge,
    },
    UpdateNode {
        node_id: String,
        before: MindmapNode,
        after: MindmapNode,
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
    DeleteEdge {
        edge: MindmapEdge,
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
        parent_id: Option<String>,
        index: usize,
    },
}

impl MindmapMutation {
    pub fn type_id(&self) -> &'static str {
        match self {
            Self::AddNode { .. } => "mindmap.nodeInserted",
            Self::AddEdge { .. } => "mindmap.edgeInserted",
            Self::UpdateNode { .. } => "mindmap.nodeUpdated",
            Self::SetNodeCollapsed { .. } => "mindmap.nodeCollapsedChanged",
            Self::UpdateEdge { .. } => "mindmap.edgeUpdated",
            Self::DeleteEdge { .. } => "mindmap.edgeDeleted",
            Self::MoveNode { .. } => "mindmap.nodeMoved",
            Self::DeleteNodes { .. } => "mindmap.nodeDeleted",
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
pub struct MindmapEngine {
    model: MindmapModel,
    revision: u64,
}

impl MindmapEngine {
    pub fn new(model: MindmapModel, revision: u64) -> Result<Self, MindmapEngineError> {
        validate(&model)?;
        Ok(Self { model, revision })
    }

    pub fn model(&self) -> &MindmapModel {
        &self.model
    }

    pub fn revision(&self) -> u64 {
        self.revision
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

        let mut candidate = self.model.clone();
        let mut changed_nodes = Vec::new();
        let mut changed_edges = Vec::new();
        let mut mutations = Vec::new();
        let mut structure_changed = false;
        for command in batch.commands {
            match command {
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
                        attrs,
                        collapsed: false,
                    };
                    let insertion = insertion_index(&candidate, parent_id.as_deref(), index)?;
                    if parent_id.is_none() {
                        candidate.root = Some(node_id.clone());
                    }
                    candidate.nodes.insert(insertion, node);
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
                    changed_nodes.push(node_id.clone());
                    mutations.push(MindmapMutation::UpdateNode {
                        node_id,
                        before,
                        after,
                    });
                }
                MindmapCommand::SetNodeCollapsed { node_id, collapsed } => {
                    let node = node_mut(&mut candidate, &node_id)?;
                    let before = node.collapsed;
                    node.collapsed = collapsed;
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
                    changed_edges.push(edge_id.clone());
                    mutations.push(MindmapMutation::UpdateEdge {
                        edge_id,
                        before,
                        after,
                    });
                }
                MindmapCommand::DeleteEdge { edge_id } => {
                    let index = candidate
                        .edges
                        .iter()
                        .position(|item| item.id == edge_id)
                        .ok_or_else(|| MindmapEngineError::MissingEdge(edge_id.clone()))?;
                    let removed = candidate.edges.remove(index);
                    changed_edges.push(edge_id);
                    mutations.push(MindmapMutation::DeleteEdge { edge: removed });
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
                    let removed = subtree
                        .iter()
                        .filter_map(|id| candidate.nodes.iter().find(|item| item.id == *id))
                        .cloned()
                        .collect();
                    let removed_edges = candidate
                        .edges
                        .iter()
                        .filter(|edge| {
                            subtree.contains(&edge.source_id) || subtree.contains(&edge.target_id)
                        })
                        .cloned()
                        .collect::<Vec<_>>();
                    if candidate.root.as_deref() == Some(node_id.as_str()) {
                        candidate.root = None;
                    }
                    candidate.nodes.retain(|node| !subtree.contains(&node.id));
                    candidate.edges.retain(|edge| {
                        !subtree.contains(&edge.source_id) && !subtree.contains(&edge.target_id)
                    });
                    changed_nodes.extend(subtree);
                    mutations.push(MindmapMutation::DeleteNodes {
                        nodes: removed,
                        edges: removed_edges.clone(),
                        parent_id,
                        index,
                    });
                    changed_edges.extend(removed_edges.into_iter().map(|edge| edge.id));
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
        self.model = candidate;
        self.revision = revision;
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
                .collect(),
            changed_containers: vec![EntityRef {
                entity_type: "mindmap.graph".into(),
                entity_id: "root".into(),
            }],
            structure_changed,
        };
        Ok(MindmapChangeSet {
            revision,
            invalidation,
            mutations,
        })
    }
}

fn validate(model: &MindmapModel) -> Result<(), MindmapEngineError> {
    oo_schema::ArtifactEnvelope::new(
        "mindmap-engine",
        oo_schema::ArtifactPayload::Mindmap(model.clone()),
    )
    .validate()
    .map_err(MindmapEngineError::Schema)
}

fn ensure_id(id: &str) -> Result<(), MindmapEngineError> {
    if id.trim().is_empty() {
        Err(MindmapEngineError::EmptyId)
    } else {
        Ok(())
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

fn collect_subtree(model: &MindmapModel, root: &str) -> Result<Vec<String>, MindmapEngineError> {
    node(model, root)?;
    let mut result = Vec::new();
    let mut stack = vec![root.to_string()];
    while let Some(id) = stack.pop() {
        result.push(id.clone());
        for child in children(model, Some(&id)) {
            stack.push(child.id.clone());
        }
    }
    Ok(result)
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
    let siblings = children(model, parent_id);
    if sibling_index > siblings.len() {
        return Err(MindmapEngineError::InvalidIndex {
            index: sibling_index,
            len: siblings.len(),
        });
    }
    if let Some(previous) = siblings.get(sibling_index.saturating_sub(1)) {
        let subtree = collect_subtree(model, &previous.id)?;
        let last = subtree
            .iter()
            .filter_map(|id| model.nodes.iter().position(|node| node.id == *id))
            .max()
            .unwrap_or(0);
        return Ok(last + 1);
    }
    if let Some(parent_id) = parent_id {
        let parent = model
            .nodes
            .iter()
            .position(|node| node.id == parent_id)
            .unwrap();
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
}

impl MindmapProjection {
    pub fn build(
        model: &MindmapModel,
        options: MindmapLayoutOptions,
        theme: MindmapTheme,
    ) -> Result<Self, MindmapLayoutError> {
        let layout = layout(model, options)?;
        let edges = route_edges(model, &layout)?;
        Ok(Self {
            theme,
            layout,
            edges,
        })
    }
}

/// Route graph edges from a layout projection without mutating the graph or layout. The
/// orthogonal elbow path keeps connectors readable while renderers remain free to choose stroke
/// style and animation.
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
    for parent in &model.nodes {
        let Some(parent_layout) = positions.get(parent.id.as_str()) else {
            // Nodes hidden beneath a collapsed ancestor are intentionally not
            // routable in this projection.
            continue;
        };
        for child in model
            .nodes
            .iter()
            .filter(|node| node.parent_id.as_deref() == Some(parent.id.as_str()))
        {
            // Collapsed descendants are intentionally absent from the layout
            // projection; their hierarchy remains in the model and can be
            // expanded without creating a new layout identity.
            let Some(child_layout) = positions.get(child.id.as_str()) else {
                continue;
            };
            let start = MindmapPoint {
                x: parent_layout.x + parent_layout.width,
                y: parent_layout.y + parent_layout.height / 2.0,
            };
            let end = MindmapPoint {
                x: child_layout.x,
                y: child_layout.y + child_layout.height / 2.0,
            };
            let mid_x = (start.x + end.x) / 2.0;
            routes.push(MindmapEdgeRoute {
                edge_id: None,
                parent_id: parent.id.clone(),
                child_id: child.id.clone(),
                points: vec![
                    start,
                    MindmapPoint {
                        x: mid_x,
                        y: start.y,
                    },
                    MindmapPoint { x: mid_x, y: end.y },
                    end,
                ],
            });
        }
    }
    for edge in &model.edges {
        let Some(source) = positions.get(edge.source_id.as_str()) else {
            continue;
        };
        let Some(target) = positions.get(edge.target_id.as_str()) else {
            continue;
        };
        let start = MindmapPoint {
            x: source.x + source.width,
            y: source.y + source.height / 2.0,
        };
        let end = MindmapPoint {
            x: target.x,
            y: target.y + target.height / 2.0,
        };
        let mid_x = (start.x + end.x) / 2.0;
        routes.push(MindmapEdgeRoute {
            edge_id: Some(edge.id.clone()),
            parent_id: edge.source_id.clone(),
            child_id: edge.target_id.clone(),
            points: vec![
                start,
                MindmapPoint {
                    x: mid_x,
                    y: start.y,
                },
                MindmapPoint { x: mid_x, y: end.y },
                end,
            ],
        });
    }
    Ok(MindmapEdgeProjection { routes })
}

/// Compute a stable top-down tree layout. This is intentionally a pure query: moving a viewport
/// or changing renderer zoom must not create a graph command or mutate the persisted artifact.
pub fn layout(
    model: &MindmapModel,
    options: MindmapLayoutOptions,
) -> Result<MindmapLayoutProjection, MindmapLayoutError> {
    validate_layout_options(options)?;
    validate(model).map_err(MindmapLayoutError::Engine)?;
    let Some(root) = model.root.as_deref() else {
        return Ok(MindmapLayoutProjection {
            nodes: Vec::new(),
            width: 0.0,
            height: 0.0,
        });
    };
    let mut nodes = Vec::with_capacity(model.nodes.len());
    let mut leaf_index = 0usize;
    let root_y = place_node(model, root, 0, options, &mut leaf_index, &mut nodes)?;
    let max_depth = nodes.iter().map(|node| node.depth).max().unwrap_or(0);
    let width = (max_depth as f32 * options.horizontal_gap) + options.node_width;
    let height = (leaf_index.max(1) as f32 - 1.0) * options.vertical_gap + options.node_height;
    debug_assert!(root_y.is_finite());
    Ok(MindmapLayoutProjection {
        nodes,
        width,
        height,
    })
}

fn place_node(
    model: &MindmapModel,
    id: &str,
    depth: usize,
    options: MindmapLayoutOptions,
    leaf_index: &mut usize,
    output: &mut Vec<MindmapLayoutNode>,
) -> Result<f32, MindmapLayoutError> {
    let node = node(model, id).map_err(MindmapLayoutError::Engine)?;
    let children = if node.collapsed {
        Vec::new()
    } else {
        children(model, Some(id))
    };
    let y = if children.is_empty() {
        let y = *leaf_index as f32 * options.vertical_gap;
        *leaf_index += 1;
        y
    } else {
        let mut positions = Vec::with_capacity(children.len());
        for child in children {
            positions.push(place_node(
                model,
                &child.id,
                depth + 1,
                options,
                leaf_index,
                output,
            )?);
        }
        positions.iter().sum::<f32>() / positions.len() as f32
    };
    output.push(MindmapLayoutNode {
        id: id.to_string(),
        depth,
        x: depth as f32 * options.horizontal_gap,
        y,
        width: options.node_width,
        height: options.node_height,
    });
    Ok(y)
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
    #[error("revision 冲突：服务端是 {expected}，事务基于 {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("mindmap 节点 id 不能为空")]
    EmptyId,
    #[error("节点 {0} 已存在")]
    DuplicateNode(String),
    #[error("边 {0} 已存在")]
    DuplicateEdge(String),
    #[error("找不到节点 {0}")]
    MissingNode(String),
    #[error("找不到边 {0}")]
    MissingEdge(String),
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

    fn engine() -> MindmapEngine {
        MindmapEngine::new(MindmapModel::default(), 0).unwrap()
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
            edges: Vec::new(),
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
            edges: Vec::new(),
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
            edges: Vec::new(),
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
            edges: Vec::new(),
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
}
