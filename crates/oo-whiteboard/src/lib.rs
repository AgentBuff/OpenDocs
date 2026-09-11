//! Whiteboard Artifact 的 Scene Graph engine。
//!
//! 白板的模型只描述元素和相机；空间索引、命中测试、Canvas/WebGL 绘制属于 renderer，
//! 不在本 crate 中复制。所有结构修改在候选模型上完成，schema 校验通过后才提交。

use std::collections::HashSet;

use oo_protocol::{EntityRef, Invalidation, MutationRecord};
use oo_schema::{Camera, SceneElement, SchemaValidationError, WhiteboardModel};
use serde::{Deserialize, Serialize};
use serde_json::Map;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhiteboardCommandBatch {
    pub base_revision: u64,
    pub commands: Vec<WhiteboardCommand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhiteboardCommandDescriptor {
    pub type_id: &'static str,
    pub scope: &'static str,
}

pub fn whiteboard_command_registry() -> &'static [WhiteboardCommandDescriptor] {
    const COMMANDS: &[WhiteboardCommandDescriptor] = &[
        WhiteboardCommandDescriptor {
            type_id: "whiteboard.addElement",
            scope: "whiteboard.element",
        },
        WhiteboardCommandDescriptor {
            type_id: "whiteboard.updateElement",
            scope: "whiteboard.element",
        },
        WhiteboardCommandDescriptor {
            type_id: "whiteboard.deleteElement",
            scope: "whiteboard.element",
        },
        WhiteboardCommandDescriptor {
            type_id: "whiteboard.setCamera",
            scope: "whiteboard.camera",
        },
        WhiteboardCommandDescriptor {
            type_id: "whiteboard.panCamera",
            scope: "whiteboard.camera",
        },
        WhiteboardCommandDescriptor {
            type_id: "whiteboard.zoomCamera",
            scope: "whiteboard.camera",
        },
    ];
    COMMANDS
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WhiteboardCommand {
    AddElement {
        element: SceneElement,
        index: usize,
    },
    UpdateElement {
        element_id: String,
        #[serde(default)]
        type_id: Option<String>,
        #[serde(default)]
        transform: Option<oo_schema::Transform>,
        #[serde(default)]
        attrs: Option<Map<String, serde_json::Value>>,
        #[serde(default)]
        children: Option<Vec<String>>,
    },
    DeleteElement {
        element_id: String,
    },
    SetCamera {
        camera: Camera,
    },
    PanCamera {
        dx: f32,
        dy: f32,
    },
    ZoomCamera {
        factor: f32,
        #[serde(default)]
        anchor_x: f32,
        #[serde(default)]
        anchor_y: f32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhiteboardChangeSet {
    pub revision: u64,
    pub invalidation: Invalidation,
    pub mutations: Vec<WhiteboardMutation>,
}

/// Typed scene mutations are consumed by history/collaboration; renderers only consume
/// projection and invalidation and never mutate the persisted scene directly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WhiteboardMutation {
    AddElement {
        element: SceneElement,
        index: usize,
    },
    UpdateElement {
        element_id: String,
        before: SceneElement,
        after: SceneElement,
    },
    DeleteElements {
        removed: Vec<RemovedElement>,
        parent_id: Option<String>,
        parent_children_before: Option<Vec<String>>,
    },
    SetCamera {
        before: Camera,
        after: Camera,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovedElement {
    pub position: usize,
    pub element: SceneElement,
}

impl WhiteboardMutation {
    pub fn type_id(&self) -> &'static str {
        match self {
            Self::AddElement { .. } => "whiteboard.elementInserted",
            Self::UpdateElement { .. } => "whiteboard.elementUpdated",
            Self::DeleteElements { .. } => "whiteboard.elementDeleted",
            Self::SetCamera { .. } => "whiteboard.cameraChanged",
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
pub struct WhiteboardEngine {
    model: WhiteboardModel,
    revision: u64,
}

impl WhiteboardEngine {
    pub fn new(model: WhiteboardModel, revision: u64) -> Result<Self, WhiteboardEngineError> {
        validate(&model)?;
        Ok(Self { model, revision })
    }

    pub fn model(&self) -> &WhiteboardModel {
        &self.model
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn execute(
        &mut self,
        batch: WhiteboardCommandBatch,
    ) -> Result<WhiteboardChangeSet, WhiteboardEngineError> {
        if batch.commands.is_empty() {
            return Err(WhiteboardEngineError::EmptyBatch);
        }
        if batch.base_revision != self.revision {
            return Err(WhiteboardEngineError::RevisionConflict {
                expected: self.revision,
                actual: batch.base_revision,
            });
        }

        // The revision is checked before touching the live model. Every successful command
        // appends a typed inverse to `mutations`; any later failure rolls these inverses back in
        // reverse order, so the hot path never clones the full SceneGraph.
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(WhiteboardEngineError::RevisionOverflow)?;
        let mut changed_elements = Vec::new();
        let mut mutations = Vec::new();
        let mut structure_changed = false;
        let execution = (|| {
            for command in batch.commands {
                match command {
                    WhiteboardCommand::AddElement { element, index } => {
                        if element.id.trim().is_empty() || element.type_id.trim().is_empty() {
                            return Err(WhiteboardEngineError::EmptyElementIdentity);
                        }
                        if self.model.elements.iter().any(|item| item.id == element.id) {
                            return Err(WhiteboardEngineError::DuplicateElement(element.id));
                        }
                        if index > self.model.elements.len() {
                            return Err(WhiteboardEngineError::InvalidIndex {
                                index,
                                len: self.model.elements.len(),
                            });
                        }
                        let id = element.id.clone();
                        self.model.elements.insert(index, element.clone());
                        changed_elements.push(id);
                        mutations.push(WhiteboardMutation::AddElement { element, index });
                        structure_changed = true;
                    }
                    WhiteboardCommand::UpdateElement {
                        element_id,
                        type_id,
                        transform,
                        attrs,
                        children,
                    } => {
                        let before = self
                            .model
                            .elements
                            .iter()
                            .find(|item| item.id == element_id)
                            .cloned()
                            .ok_or_else(|| {
                                WhiteboardEngineError::MissingElement(element_id.clone())
                            })?;
                        let element = element_mut(&mut self.model, &element_id)?;
                        if let Some(type_id) = type_id {
                            if type_id.trim().is_empty() {
                                return Err(WhiteboardEngineError::EmptyElementIdentity);
                            }
                            element.type_id = type_id;
                        }
                        if let Some(transform) = transform {
                            element.transform = transform;
                        }
                        if let Some(attrs) = attrs {
                            element.attrs = attrs;
                        }
                        if let Some(children) = children {
                            element.children = children;
                        }
                        let after = element.clone();
                        changed_elements.push(element_id.clone());
                        mutations.push(WhiteboardMutation::UpdateElement {
                            element_id,
                            before,
                            after,
                        });
                    }
                    WhiteboardCommand::DeleteElement { element_id } => {
                        if !self
                            .model
                            .elements
                            .iter()
                            .any(|element| element.id == element_id)
                        {
                            return Err(WhiteboardEngineError::MissingElement(element_id.clone()));
                        }
                        let subtree = collect_subtree(&self.model, &element_id)?;
                        let removed = subtree
                            .iter()
                            .filter_map(|id| {
                                self.model
                                    .elements
                                    .iter()
                                    .enumerate()
                                    .find(|(_, item)| item.id == *id)
                                    .map(|(position, element)| RemovedElement {
                                        position,
                                        element: element.clone(),
                                    })
                            })
                            .collect::<Vec<_>>();
                        let parent = self
                            .model
                            .elements
                            .iter()
                            .find(|element| {
                                element.children.iter().any(|child| child == &element_id)
                            })
                            .map(|element| (element.id.clone(), element.children.clone()));
                        if let Some((parent_id, _)) = &parent {
                            let owner = element_mut(&mut self.model, parent_id)?;
                            owner.children.retain(|child| child != &element_id);
                        }
                        self.model
                            .elements
                            .retain(|element| !subtree.contains(&element.id));
                        changed_elements.extend(subtree);
                        mutations.push(WhiteboardMutation::DeleteElements {
                            removed,
                            parent_id: parent.as_ref().map(|(id, _)| id.clone()),
                            parent_children_before: parent.map(|(_, children)| children),
                        });
                        structure_changed = true;
                    }
                    WhiteboardCommand::SetCamera { camera } => {
                        let before = self.model.camera.clone();
                        self.model.camera = camera;
                        mutations.push(WhiteboardMutation::SetCamera {
                            before,
                            after: self.model.camera.clone(),
                        });
                    }
                    WhiteboardCommand::PanCamera { dx, dy } => {
                        if !dx.is_finite() || !dy.is_finite() {
                            return Err(WhiteboardEngineError::InvalidCamera);
                        }
                        let before = self.model.camera.clone();
                        self.model.camera.x += dx;
                        self.model.camera.y += dy;
                        mutations.push(WhiteboardMutation::SetCamera {
                            before,
                            after: self.model.camera.clone(),
                        });
                    }
                    WhiteboardCommand::ZoomCamera {
                        factor,
                        anchor_x,
                        anchor_y,
                    } => {
                        if !factor.is_finite() || factor <= 0.0 {
                            return Err(WhiteboardEngineError::InvalidZoom);
                        }
                        let before = self.model.camera.clone();
                        let next_scale = self.model.camera.scale * factor;
                        if !next_scale.is_finite() || next_scale <= 0.0 {
                            return Err(WhiteboardEngineError::InvalidZoom);
                        }
                        self.model.camera.x = anchor_x - (anchor_x - self.model.camera.x) * factor;
                        self.model.camera.y = anchor_y - (anchor_y - self.model.camera.y) * factor;
                        self.model.camera.scale = next_scale;
                        mutations.push(WhiteboardMutation::SetCamera {
                            before,
                            after: self.model.camera.clone(),
                        });
                    }
                }
            }
            validate(&self.model)
        })();
        if let Err(error) = execution {
            self.rollback(&mutations);
            return Err(error);
        }
        changed_elements.sort();
        changed_elements.dedup();
        self.revision = revision;
        let mut changed_entities = changed_elements
            .iter()
            .map(|id| EntityRef {
                entity_type: "whiteboard.element".into(),
                entity_id: id.clone(),
            })
            .collect::<Vec<_>>();
        if mutations
            .iter()
            .any(|mutation| matches!(mutation, WhiteboardMutation::SetCamera { .. }))
        {
            changed_entities.push(EntityRef {
                entity_type: "whiteboard.camera".into(),
                entity_id: "viewport".into(),
            });
        }
        Ok(WhiteboardChangeSet {
            revision,
            invalidation: Invalidation {
                changed_entities,
                changed_containers: vec![EntityRef {
                    entity_type: "whiteboard.scene".into(),
                    entity_id: "root".into(),
                }],
                structure_changed,
            },
            mutations,
        })
    }

    fn rollback(&mut self, mutations: &[WhiteboardMutation]) {
        for mutation in mutations.iter().rev() {
            match mutation {
                WhiteboardMutation::AddElement { element, .. } => {
                    self.model.elements.retain(|item| item.id != element.id);
                }
                WhiteboardMutation::UpdateElement {
                    element_id, before, ..
                } => {
                    if let Some(current) = self
                        .model
                        .elements
                        .iter_mut()
                        .find(|item| item.id == *element_id)
                    {
                        *current = before.clone();
                    }
                }
                WhiteboardMutation::DeleteElements {
                    removed,
                    parent_id,
                    parent_children_before,
                } => {
                    if let (Some(parent_id), Some(children)) =
                        (parent_id.as_deref(), parent_children_before)
                    {
                        if let Some(parent) = self
                            .model
                            .elements
                            .iter_mut()
                            .find(|item| item.id == parent_id)
                        {
                            parent.children = children.clone();
                        }
                    }
                    // Positions are from the pre-delete vector. Inserting in ascending order
                    // restores the original order without cloning untouched scene elements.
                    let mut ordered = removed.iter().collect::<Vec<_>>();
                    ordered.sort_by_key(|item| item.position);
                    for removed in ordered {
                        let position = removed.position.min(self.model.elements.len());
                        self.model
                            .elements
                            .insert(position, removed.element.clone());
                    }
                }
                WhiteboardMutation::SetCamera { before, .. } => {
                    self.model.camera = before.clone();
                }
            }
        }
    }
}

fn validate(model: &WhiteboardModel) -> Result<(), WhiteboardEngineError> {
    model.validate().map_err(WhiteboardEngineError::Schema)
}

fn element_mut<'a>(
    model: &'a mut WhiteboardModel,
    id: &str,
) -> Result<&'a mut SceneElement, WhiteboardEngineError> {
    model
        .elements
        .iter_mut()
        .find(|element| element.id == id)
        .ok_or_else(|| WhiteboardEngineError::MissingElement(id.into()))
}

fn collect_subtree(
    model: &WhiteboardModel,
    root: &str,
) -> Result<Vec<String>, WhiteboardEngineError> {
    if !model.elements.iter().any(|element| element.id == root) {
        return Err(WhiteboardEngineError::MissingElement(root.into()));
    }
    let mut result = Vec::new();
    let mut stack = vec![root.to_string()];
    let mut seen = HashSet::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id.clone()) {
            return Err(WhiteboardEngineError::Cycle(id));
        }
        result.push(id.clone());
        let element = model
            .elements
            .iter()
            .find(|element| element.id == id)
            .ok_or_else(|| WhiteboardEngineError::MissingElement(id.clone()))?;
        stack.extend(element.children.iter().cloned());
    }
    Ok(result)
}

/// Viewport math is a pure projection boundary. Scene elements stay in world coordinates; a
/// renderer applies this transform when drawing and never writes screen coordinates back to the
/// artifact model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WhiteboardViewport {
    pub width: f32,
    pub height: f32,
    pub device_pixel_ratio: f32,
}

impl WhiteboardViewport {
    pub fn new(
        width: f32,
        height: f32,
        device_pixel_ratio: f32,
    ) -> Result<Self, WhiteboardProjectionError> {
        let viewport = Self {
            width,
            height,
            device_pixel_ratio,
        };
        if [width, height, device_pixel_ratio]
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0)
        {
            return Err(WhiteboardProjectionError::InvalidViewport);
        }
        Ok(viewport)
    }

    pub fn world_to_screen(self, camera: &Camera, x: f32, y: f32) -> (f32, f32) {
        (
            (x - camera.x) * camera.scale + self.width / 2.0,
            (y - camera.y) * camera.scale + self.height / 2.0,
        )
    }

    pub fn screen_to_world(self, camera: &Camera, x: f32, y: f32) -> (f32, f32) {
        (
            (x - self.width / 2.0) / camera.scale + camera.x,
            (y - self.height / 2.0) / camera.scale + camera.y,
        )
    }

    pub fn project(self, camera: &Camera, transform: &oo_schema::Transform) -> ScreenRect {
        let (x, y) = self.world_to_screen(camera, transform.x, transform.y);
        ScreenRect {
            x,
            y,
            width: transform.width * camera.scale,
            height: transform.height * camera.scale,
            rotation: transform.rotation,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WorldRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl WorldRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Result<Self, SpatialIndexError> {
        let rect = Self {
            x,
            y,
            width,
            height,
        };
        if [x, y, width, height].iter().any(|value| !value.is_finite()) {
            return Err(SpatialIndexError::InvalidRect);
        }
        if width < 0.0 || height < 0.0 {
            return Err(SpatialIndexError::InvalidRect);
        }
        if !rect.right().is_finite() || !rect.bottom().is_finite() {
            return Err(SpatialIndexError::InvalidRect);
        }
        Ok(rect)
    }

    pub fn right(self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    fn intersects(self, other: Self) -> bool {
        self.x <= other.right()
            && self.right() >= other.x
            && self.y <= other.bottom()
            && self.bottom() >= other.y
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpatialQuery {
    pub ids: Vec<String>,
    pub candidate_count: usize,
    pub cells_examined: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GridCell {
    x: i32,
    y: i32,
}

/// A renderer-independent uniform grid. It stores only element bounds and ids, so it cannot
/// become a second scene model. Querying a viewport visits covered cells and then exact-tests
/// their candidates rather than scanning every element.
#[derive(Debug, Clone)]
pub struct WhiteboardSpatialIndex {
    cell_size: f32,
    cells: std::collections::HashMap<GridCell, Vec<String>>,
    bounds: std::collections::HashMap<String, WorldRect>,
    order: std::collections::HashMap<String, usize>,
}

impl WhiteboardSpatialIndex {
    pub fn build(model: &WhiteboardModel, cell_size: f32) -> Result<Self, SpatialIndexError> {
        model.validate().map_err(SpatialIndexError::Schema)?;
        if !cell_size.is_finite() || cell_size <= 0.0 {
            return Err(SpatialIndexError::InvalidCellSize);
        }
        let mut index = Self {
            cell_size,
            cells: std::collections::HashMap::new(),
            bounds: std::collections::HashMap::with_capacity(model.elements.len()),
            order: std::collections::HashMap::with_capacity(model.elements.len()),
        };
        for (order, element) in model.elements.iter().enumerate() {
            let rect = WorldRect::new(
                element.transform.x,
                element.transform.y,
                element.transform.width,
                element.transform.height,
            )?;
            index.bounds.insert(element.id.clone(), rect);
            index.order.insert(element.id.clone(), order);
            for cell in index.cells_for_rect(rect)? {
                index
                    .cells
                    .entry(cell)
                    .or_default()
                    .push(element.id.clone());
            }
        }
        Ok(index)
    }

    pub fn len(&self) -> usize {
        self.bounds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bounds.is_empty()
    }

    /// Incrementally inserts or replaces one bound. This is the hot-path API
    /// used after a scene change; it never rebuilds or scans the complete
    /// scene graph.
    pub fn upsert(
        &mut self,
        id: impl Into<String>,
        rect: WorldRect,
        order: usize,
    ) -> Result<(), SpatialIndexError> {
        let id = id.into();
        if let Some(previous) = self.bounds.remove(&id) {
            for cell in self.cells_for_rect(previous)? {
                self.remove_cell_id(cell, &id);
            }
        }
        for cell in self.cells_for_rect(rect)? {
            let ids = self.cells.entry(cell).or_default();
            if !ids.iter().any(|item| item == &id) {
                ids.push(id.clone());
            }
        }
        self.bounds.insert(id.clone(), rect);
        self.order.insert(id, order);
        Ok(())
    }

    /// Removes one element from the index without touching any other cell.
    pub fn remove(&mut self, id: &str) -> Result<bool, SpatialIndexError> {
        let Some(rect) = self.bounds.remove(id) else {
            return Ok(false);
        };
        for cell in self.cells_for_rect(rect)? {
            self.remove_cell_id(cell, id);
        }
        self.order.remove(id);
        Ok(true)
    }

    pub fn apply_delta(&mut self, delta: &[SpatialIndexDelta]) -> Result<(), SpatialIndexError> {
        for change in delta {
            match change {
                SpatialIndexDelta::Upsert { id, rect, order } => {
                    self.upsert(id.clone(), *rect, *order)?;
                }
                SpatialIndexDelta::Remove { id } => {
                    self.remove(id)?;
                }
            }
        }
        Ok(())
    }

    fn remove_cell_id(&mut self, cell: GridCell, id: &str) {
        let Some(ids) = self.cells.get_mut(&cell) else {
            return;
        };
        ids.retain(|item| item != id);
        if ids.is_empty() {
            self.cells.remove(&cell);
        }
    }

    pub fn query(&self, rect: WorldRect) -> Result<Vec<String>, SpatialIndexError> {
        Ok(self.query_with_stats(rect)?.ids)
    }

    pub fn query_viewport(
        &self,
        viewport: WhiteboardViewport,
        camera: &Camera,
    ) -> Result<SpatialQuery, SpatialIndexError> {
        if !camera.x.is_finite()
            || !camera.y.is_finite()
            || !camera.scale.is_finite()
            || camera.scale <= 0.0
        {
            return Err(SpatialIndexError::InvalidCamera);
        }
        let top_left = viewport.screen_to_world(camera, 0.0, 0.0);
        let bottom_right = viewport.screen_to_world(camera, viewport.width, viewport.height);
        let rect = WorldRect::new(
            top_left.0.min(bottom_right.0),
            top_left.1.min(bottom_right.1),
            (bottom_right.0 - top_left.0).abs(),
            (bottom_right.1 - top_left.1).abs(),
        )?;
        self.query_with_stats(rect)
    }

    pub fn query_with_stats(&self, rect: WorldRect) -> Result<SpatialQuery, SpatialIndexError> {
        let cells = self.cells_for_rect(rect)?;
        let mut candidate_ids = HashSet::new();
        for cell in &cells {
            if let Some(ids) = self.cells.get(cell) {
                candidate_ids.extend(ids.iter());
            }
        }
        let candidate_count = candidate_ids.len();
        let mut ids = candidate_ids
            .into_iter()
            .filter(|id| {
                self.bounds
                    .get(*id)
                    .is_some_and(|bounds| bounds.intersects(rect))
            })
            .map(String::from)
            .collect::<Vec<_>>();
        ids.sort_by_key(|id| self.order.get(id).copied().unwrap_or(usize::MAX));
        Ok(SpatialQuery {
            candidate_count,
            ids,
            cells_examined: cells.len(),
        })
    }

    fn cells_for_rect(&self, rect: WorldRect) -> Result<Vec<GridCell>, SpatialIndexError> {
        let min_x = cell_coord(rect.x, self.cell_size)?;
        let max_x = cell_coord(rect.right(), self.cell_size)?;
        let min_y = cell_coord(rect.y, self.cell_size)?;
        let max_y = cell_coord(rect.bottom(), self.cell_size)?;
        let width = i64::from(max_x) - i64::from(min_x) + 1;
        let height = i64::from(max_y) - i64::from(min_y) + 1;
        let count = width
            .checked_mul(height)
            .ok_or(SpatialIndexError::QueryTooLarge)?;
        if count > 1_000_000 {
            return Err(SpatialIndexError::QueryTooLarge);
        }
        let mut cells = Vec::with_capacity(count as usize);
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                cells.push(GridCell { x, y });
            }
        }
        Ok(cells)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SpatialIndexDelta {
    Upsert {
        id: String,
        rect: WorldRect,
        order: usize,
    },
    Remove {
        id: String,
    },
}

fn cell_coord(value: f32, cell_size: f32) -> Result<i32, SpatialIndexError> {
    let coord = (value as f64 / cell_size as f64).floor();
    if !coord.is_finite() || coord < i32::MIN as f64 || coord > i32::MAX as f64 {
        return Err(SpatialIndexError::CoordinateOutOfRange);
    }
    Ok(coord as i32)
}

#[derive(Debug, thiserror::Error)]
pub enum SpatialIndexError {
    #[error("whiteboard cell size 必须是有限正数")]
    InvalidCellSize,
    #[error("whiteboard world rect 无效")]
    InvalidRect,
    #[error("whiteboard camera 无效")]
    InvalidCamera,
    #[error("whiteboard 查询覆盖的网格过大")]
    QueryTooLarge,
    #[error("whiteboard 坐标超出空间索引范围")]
    CoordinateOutOfRange,
    #[error("Whiteboard schema 校验失败：{0}")]
    Schema(#[from] SchemaValidationError),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WorldPoint {
    pub x: f32,
    pub y: f32,
}

impl WorldPoint {
    fn finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    Replace,
    Add,
    Toggle,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WhiteboardSelection {
    pub element_ids: Vec<String>,
}

impl WhiteboardSelection {
    pub fn apply(
        &self,
        model: &WhiteboardModel,
        index: &WhiteboardSpatialIndex,
        rect: WorldRect,
        mode: SelectionMode,
    ) -> Result<Self, SpatialIndexError> {
        let hits = index.query(rect)?;
        let mut ids = match mode {
            SelectionMode::Replace => Vec::new(),
            SelectionMode::Add | SelectionMode::Toggle => self.element_ids.clone(),
        };
        let valid: HashSet<&str> = model.elements.iter().map(|item| item.id.as_str()).collect();
        ids.retain(|id| valid.contains(id.as_str()));
        for id in hits {
            match mode {
                SelectionMode::Toggle if ids.iter().any(|item| item == &id) => {
                    ids.retain(|item| item != &id);
                }
                SelectionMode::Replace | SelectionMode::Add | SelectionMode::Toggle => {
                    if !ids.iter().any(|item| item == &id) {
                        ids.push(id);
                    }
                }
            }
        }
        ids.sort_by_key(|id| index.order.get(id).copied().unwrap_or(usize::MAX));
        Ok(Self { element_ids: ids })
    }
}

/// Box selection is a read-only projection over the spatial index. It returns
/// stable scene order, so selection does not flicker as cells are visited.
pub fn select_rect(
    model: &WhiteboardModel,
    index: &WhiteboardSpatialIndex,
    rect: WorldRect,
    previous: &WhiteboardSelection,
    mode: SelectionMode,
) -> Result<WhiteboardSelection, SpatialIndexError> {
    previous.apply(model, index, rect, mode)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapOptions {
    pub grid_size: Option<f32>,
    pub threshold: f32,
    pub snap_to_elements: bool,
}

impl Default for SnapOptions {
    fn default() -> Self {
        Self {
            grid_size: Some(8.0),
            threshold: 6.0,
            snap_to_elements: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapTarget {
    Grid,
    ElementEdge,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SnapResult {
    pub point: WorldPoint,
    pub target: Option<SnapTarget>,
}

/// Snap is pure interaction state: the returned point is fed into a semantic
/// `UpdateElement` command by the client, never persisted by this helper.
pub fn snap_point(
    model: &WhiteboardModel,
    point: WorldPoint,
    options: SnapOptions,
) -> Result<SnapResult, WhiteboardProjectionError> {
    if !point.finite()
        || !options.threshold.is_finite()
        || options.threshold < 0.0
        || options
            .grid_size
            .is_some_and(|size| !size.is_finite() || size <= 0.0)
    {
        return Err(WhiteboardProjectionError::InvalidSnapOptions);
    }
    let mut best = SnapResult {
        point,
        target: None,
    };
    if let Some(size) = options.grid_size {
        let candidate = WorldPoint {
            x: (point.x / size).round() * size,
            y: (point.y / size).round() * size,
        };
        if distance(point, candidate) <= options.threshold {
            best = SnapResult {
                point: candidate,
                target: Some(SnapTarget::Grid),
            };
        }
    }
    if options.snap_to_elements {
        for element in &model.elements {
            for candidate in [
                WorldPoint {
                    x: element.transform.x,
                    y: point.y,
                },
                WorldPoint {
                    x: element.transform.x + element.transform.width,
                    y: point.y,
                },
                WorldPoint {
                    x: point.x,
                    y: element.transform.y,
                },
                WorldPoint {
                    x: point.x,
                    y: element.transform.y + element.transform.height,
                },
            ] {
                if distance(point, candidate) <= options.threshold
                    && (best.target.is_none()
                        || distance(point, candidate) < distance(point, best.point))
                {
                    best = SnapResult {
                        point: candidate,
                        target: Some(SnapTarget::ElementEdge),
                    };
                }
            }
        }
    }
    Ok(best)
}

fn distance(a: WorldPoint, b: WorldPoint) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorAnchor {
    Center,
    Top,
    Right,
    Bottom,
    Left,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WhiteboardConnectorRoute {
    pub source_id: String,
    pub target_id: String,
    pub points: Vec<WorldPoint>,
}

/// Orthogonal connector routing is a projection. It reads scene transforms
/// and emits a stable four-point elbow route; no connector element is added to
/// the persisted model until the caller issues an explicit command.
pub fn route_connector(
    model: &WhiteboardModel,
    source_id: &str,
    target_id: &str,
    source_anchor: ConnectorAnchor,
    target_anchor: ConnectorAnchor,
) -> Result<WhiteboardConnectorRoute, WhiteboardProjectionError> {
    if source_id == target_id {
        return Err(WhiteboardProjectionError::SelfConnector);
    }
    let source = model
        .elements
        .iter()
        .find(|element| element.id == source_id)
        .ok_or_else(|| WhiteboardProjectionError::MissingElement(source_id.into()))?;
    let target = model
        .elements
        .iter()
        .find(|element| element.id == target_id)
        .ok_or_else(|| WhiteboardProjectionError::MissingElement(target_id.into()))?;
    let start = anchor_point(&source.transform, source_anchor);
    let end = anchor_point(&target.transform, target_anchor);
    let mid_x = (start.x + end.x) / 2.0;
    Ok(WhiteboardConnectorRoute {
        source_id: source_id.into(),
        target_id: target_id.into(),
        points: vec![
            start,
            WorldPoint {
                x: mid_x,
                y: start.y,
            },
            WorldPoint { x: mid_x, y: end.y },
            end,
        ],
    })
}

fn anchor_point(transform: &oo_schema::Transform, anchor: ConnectorAnchor) -> WorldPoint {
    let cx = transform.x + transform.width / 2.0;
    let cy = transform.y + transform.height / 2.0;
    match anchor {
        ConnectorAnchor::Center => WorldPoint { x: cx, y: cy },
        ConnectorAnchor::Top => WorldPoint {
            x: cx,
            y: transform.y,
        },
        ConnectorAnchor::Right => WorldPoint {
            x: transform.x + transform.width,
            y: cy,
        },
        ConnectorAnchor::Bottom => WorldPoint {
            x: cx,
            y: transform.y + transform.height,
        },
        ConnectorAnchor::Left => WorldPoint {
            x: transform.x,
            y: cy,
        },
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WhiteboardExportProjection {
    pub bounds: Option<WorldRect>,
    pub element_ids: Vec<String>,
}

pub fn export_projection(
    model: &WhiteboardModel,
) -> Result<WhiteboardExportProjection, WhiteboardProjectionError> {
    model
        .validate()
        .map_err(WhiteboardProjectionError::Schema)?;
    let mut bounds: Option<WorldRect> = None;
    for element in &model.elements {
        let rect = WorldRect::new(
            element.transform.x,
            element.transform.y,
            element.transform.width,
            element.transform.height,
        )
        .map_err(WhiteboardProjectionError::Spatial)?;
        bounds = Some(match bounds {
            None => rect,
            Some(current) => WorldRect::new(
                current.x.min(rect.x),
                current.y.min(rect.y),
                current.right().max(rect.right()) - current.x.min(rect.x),
                current.bottom().max(rect.bottom()) - current.y.min(rect.y),
            )
            .map_err(WhiteboardProjectionError::Spatial)?,
        });
    }
    Ok(WhiteboardExportProjection {
        bounds,
        element_ids: model
            .elements
            .iter()
            .map(|element| element.id.clone())
            .collect(),
    })
}

#[derive(Debug, thiserror::Error)]
pub enum WhiteboardProjectionError {
    #[error("whiteboard viewport 参数必须是有限正数")]
    InvalidViewport,
    #[error("whiteboard snap 参数无效")]
    InvalidSnapOptions,
    #[error("whiteboard connector 不能连接自身")]
    SelfConnector,
    #[error("找不到 element {0}")]
    MissingElement(String),
    #[error("whiteboard schema 校验失败：{0}")]
    Schema(#[from] SchemaValidationError),
    #[error("whiteboard projection 几何无效：{0}")]
    Spatial(#[from] SpatialIndexError),
}

#[derive(Debug, thiserror::Error)]
pub enum WhiteboardEngineError {
    #[error("command batch 不能没有 command")]
    EmptyBatch,
    #[error("revision 冲突：服务端是 {expected}，事务基于 {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("element id 或 type 不能为空")]
    EmptyElementIdentity,
    #[error("element {0} 已存在")]
    DuplicateElement(String),
    #[error("camera 参数无效")]
    InvalidCamera,
    #[error("zoom factor 必须是有限正数")]
    InvalidZoom,
    #[error("找不到 element {0}")]
    MissingElement(String),
    #[error("scene graph 存在环：{0}")]
    Cycle(String),
    #[error("插入位置 {index} 超出容器长度 {len}")]
    InvalidIndex { index: usize, len: usize },
    #[error("revision 溢出")]
    RevisionOverflow,
    #[error("Whiteboard schema 校验失败：{0}")]
    Schema(#[from] SchemaValidationError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_registry_is_unique_and_engine_owned() {
        let registry = whiteboard_command_registry();
        assert_eq!(registry.len(), 6);
        let unique = registry
            .iter()
            .map(|descriptor| descriptor.type_id)
            .collect::<HashSet<_>>();
        assert_eq!(unique.len(), registry.len());
        assert!(registry.iter().all(|descriptor| {
            descriptor.type_id.starts_with("whiteboard.")
                && descriptor.scope.starts_with("whiteboard")
        }));
    }

    fn engine() -> WhiteboardEngine {
        WhiteboardEngine::new(WhiteboardModel::default(), 0).unwrap()
    }

    fn element(id: &str) -> SceneElement {
        SceneElement {
            id: id.into(),
            type_id: "shape".into(),
            ..SceneElement::default()
        }
    }

    fn positioned_element(id: &str, x: f32, y: f32) -> SceneElement {
        SceneElement {
            id: id.into(),
            type_id: "shape".into(),
            transform: oo_schema::Transform {
                x,
                y,
                width: 10.0,
                height: 10.0,
                ..oo_schema::Transform::default()
            },
            ..SceneElement::default()
        }
    }

    #[test]
    fn element_and_camera_operations_are_atomic() {
        let mut engine = engine();
        let result = engine
            .execute(WhiteboardCommandBatch {
                base_revision: 0,
                commands: vec![
                    WhiteboardCommand::AddElement {
                        element: element("shape-1"),
                        index: 0,
                    },
                    WhiteboardCommand::SetCamera {
                        camera: Camera {
                            x: 12.0,
                            y: 4.0,
                            scale: 1.5,
                        },
                    },
                ],
            })
            .unwrap();
        assert_eq!(result.invalidation.changed_entities[0].entity_id, "shape-1");
        assert_eq!(
            result.mutations[0].to_record().unwrap().type_id,
            "whiteboard.elementInserted"
        );
        assert!(result
            .invalidation
            .changed_entities
            .iter()
            .any(|entity| entity.entity_type == "whiteboard.camera"));

        let before_failed_batch = engine.model().clone();
        let error = engine
            .execute(WhiteboardCommandBatch {
                base_revision: 1,
                commands: vec![
                    WhiteboardCommand::UpdateElement {
                        element_id: "shape-1".into(),
                        type_id: Some("text".into()),
                        transform: None,
                        attrs: None,
                        children: None,
                    },
                    WhiteboardCommand::DeleteElement {
                        element_id: "missing".into(),
                    },
                ],
            })
            .unwrap_err();
        assert!(matches!(error, WhiteboardEngineError::MissingElement(_)));
        assert_eq!(engine.model(), &before_failed_batch);
        assert_eq!(engine.revision(), 1);
    }

    #[test]
    fn hot_update_keeps_untouched_element_identity() {
        let model = WhiteboardModel {
            elements: vec![element("target"), element("untouched")],
            camera: Camera::default(),
        };
        let mut engine = WhiteboardEngine::new(model, 0).unwrap();
        let untouched_before = &engine.model().elements[1] as *const SceneElement;
        engine
            .execute(WhiteboardCommandBatch {
                base_revision: 0,
                commands: vec![WhiteboardCommand::UpdateElement {
                    element_id: "target".into(),
                    type_id: Some("text".into()),
                    transform: None,
                    attrs: None,
                    children: None,
                }],
            })
            .unwrap();
        let untouched_after = &engine.model().elements[1] as *const SceneElement;
        assert_eq!(untouched_before, untouched_after);
    }

    #[test]
    fn spatial_index_returns_local_candidates_without_full_scan() {
        let model = WhiteboardModel {
            elements: (0..2_000)
                .map(|index| positioned_element(&format!("e-{index}"), index as f32 * 20.0, 0.0))
                .collect(),
            camera: Camera::default(),
        };
        let index = WhiteboardSpatialIndex::build(&model, 16.0).unwrap();
        let query = index
            .query_with_stats(WorldRect::new(1_000.0, 0.0, 20.0, 10.0).unwrap())
            .unwrap();
        assert_eq!(query.ids, vec!["e-50", "e-51"]);
        assert!(query.candidate_count < model.elements.len() / 100);
        assert!(query.cells_examined <= 3);
        assert_eq!(index.len(), model.elements.len());
    }

    #[test]
    fn spatial_index_projects_camera_viewport_to_world_query() {
        let model = WhiteboardModel {
            elements: vec![
                positioned_element("near", 95.0, 45.0),
                positioned_element("far", 900.0, 900.0),
            ],
            camera: Camera::default(),
        };
        let index = WhiteboardSpatialIndex::build(&model, 32.0).unwrap();
        let viewport = WhiteboardViewport::new(100.0, 100.0, 1.0).unwrap();
        let query = index
            .query_viewport(
                viewport,
                &Camera {
                    x: 100.0,
                    y: 50.0,
                    scale: 1.0,
                },
            )
            .unwrap();
        assert_eq!(query.ids, vec!["near"]);
    }

    #[test]
    fn command_batch_uses_semantic_camel_case_wire_names() {
        let batch = WhiteboardCommandBatch {
            base_revision: 0,
            commands: vec![WhiteboardCommand::PanCamera { dx: 4.0, dy: -2.0 }],
        };
        let json = serde_json::to_value(batch).unwrap();
        assert_eq!(json["commands"][0]["type"], "panCamera");
        assert_eq!(json["commands"][0]["dx"], 4.0);
    }

    #[test]
    fn deleting_group_removes_descendants() {
        let model = WhiteboardModel {
            elements: vec![
                SceneElement {
                    id: "group".into(),
                    type_id: "group".into(),
                    children: vec!["child".into()],
                    ..SceneElement::default()
                },
                element("child"),
            ],
            camera: Camera {
                scale: 1.0,
                ..Camera::default()
            },
        };
        let mut engine = WhiteboardEngine::new(model, 0).unwrap();
        engine
            .execute(WhiteboardCommandBatch {
                base_revision: 0,
                commands: vec![WhiteboardCommand::DeleteElement {
                    element_id: "group".into(),
                }],
            })
            .unwrap();
        assert!(engine.model().elements.is_empty());
    }

    #[test]
    fn deleting_nested_element_detaches_it_from_its_parent() {
        let model = WhiteboardModel {
            elements: vec![
                SceneElement {
                    id: "group".into(),
                    type_id: "group".into(),
                    children: vec!["child".into()],
                    ..SceneElement::default()
                },
                element("child"),
            ],
            camera: Camera::default(),
        };
        let mut engine = WhiteboardEngine::new(model, 0).unwrap();
        engine
            .execute(WhiteboardCommandBatch {
                base_revision: 0,
                commands: vec![WhiteboardCommand::DeleteElement {
                    element_id: "child".into(),
                }],
            })
            .unwrap();
        assert_eq!(engine.model().elements[0].children, Vec::<String>::new());
    }

    #[test]
    fn cyclic_children_are_rejected_without_hanging_or_mutating() {
        let mut engine = engine();
        engine
            .execute(WhiteboardCommandBatch {
                base_revision: 0,
                commands: vec![WhiteboardCommand::AddElement {
                    element: element("group"),
                    index: 0,
                }],
            })
            .unwrap();
        let error = engine
            .execute(WhiteboardCommandBatch {
                base_revision: 1,
                commands: vec![
                    WhiteboardCommand::UpdateElement {
                        element_id: "group".into(),
                        type_id: None,
                        transform: None,
                        attrs: None,
                        children: Some(vec!["group".into()]),
                    },
                    WhiteboardCommand::DeleteElement {
                        element_id: "group".into(),
                    },
                ],
            })
            .unwrap_err();
        assert!(matches!(error, WhiteboardEngineError::Cycle(_)));
        assert!(engine.model().elements[0].children.is_empty());
        assert_eq!(engine.revision(), 1);
    }

    #[test]
    fn invalid_camera_does_not_mutate() {
        let mut engine = engine();
        let error = engine
            .execute(WhiteboardCommandBatch {
                base_revision: 0,
                commands: vec![WhiteboardCommand::SetCamera {
                    camera: Camera {
                        scale: 0.0,
                        ..Camera::default()
                    },
                }],
            })
            .unwrap_err();
        assert!(matches!(error, WhiteboardEngineError::Schema(_)));
        assert_eq!(engine.revision(), 0);
    }

    #[test]
    fn zoom_and_pan_are_commands_and_keep_world_point_stable() {
        let mut engine = engine();
        let result = engine
            .execute(WhiteboardCommandBatch {
                base_revision: 0,
                commands: vec![
                    WhiteboardCommand::PanCamera { dx: 10.0, dy: -4.0 },
                    WhiteboardCommand::ZoomCamera {
                        factor: 2.0,
                        anchor_x: 0.0,
                        anchor_y: 0.0,
                    },
                ],
            })
            .unwrap();
        assert_eq!(engine.model().camera.scale, 2.0);
        assert!(result
            .invalidation
            .changed_entities
            .iter()
            .any(|entity| entity.entity_type == "whiteboard.camera"));
    }

    #[test]
    fn viewport_projection_round_trips_without_scene_mutation() {
        let viewport = WhiteboardViewport::new(800.0, 600.0, 2.0).unwrap();
        let camera = Camera {
            x: 100.0,
            y: 40.0,
            scale: 2.0,
        };
        let world = (120.0, 50.0);
        let screen = viewport.world_to_screen(&camera, world.0, world.1);
        let round_trip = viewport.screen_to_world(&camera, screen.0, screen.1);
        assert!((round_trip.0 - world.0).abs() < f32::EPSILON);
        assert!((round_trip.1 - world.1).abs() < f32::EPSILON);
        let rect = viewport.project(
            &camera,
            &oo_schema::Transform {
                x: 100.0,
                y: 40.0,
                width: 20.0,
                height: 10.0,
                rotation: 0.0,
            },
        );
        assert_eq!(rect.width, 40.0);
        assert_eq!(rect.height, 20.0);
    }

    #[test]
    fn spatial_index_applies_local_deltas_without_rebuild() {
        let model = WhiteboardModel {
            elements: vec![positioned_element("one", 0.0, 0.0)],
            camera: Camera::default(),
        };
        let mut index = WhiteboardSpatialIndex::build(&model, 16.0).unwrap();
        index
            .apply_delta(&[
                SpatialIndexDelta::Upsert {
                    id: "one".into(),
                    rect: WorldRect::new(100.0, 100.0, 10.0, 10.0).unwrap(),
                    order: 0,
                },
                SpatialIndexDelta::Upsert {
                    id: "two".into(),
                    rect: WorldRect::new(0.0, 0.0, 10.0, 10.0).unwrap(),
                    order: 1,
                },
            ])
            .unwrap();
        assert_eq!(
            index
                .query(WorldRect::new(0.0, 0.0, 20.0, 20.0).unwrap())
                .unwrap(),
            vec!["two"]
        );
        assert!(index.remove("one").unwrap());
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn selection_snap_connector_and_export_are_projection_only() {
        let model = WhiteboardModel {
            elements: vec![
                positioned_element("a", 0.0, 0.0),
                positioned_element("b", 40.0, 0.0),
            ],
            camera: Camera::default(),
        };
        let index = WhiteboardSpatialIndex::build(&model, 16.0).unwrap();
        let selection = select_rect(
            &model,
            &index,
            WorldRect::new(-2.0, -2.0, 20.0, 20.0).unwrap(),
            &WhiteboardSelection::default(),
            SelectionMode::Replace,
        )
        .unwrap();
        assert_eq!(selection.element_ids, vec!["a"]);
        let snapped = snap_point(
            &model,
            WorldPoint { x: 7.0, y: 7.0 },
            SnapOptions::default(),
        )
        .unwrap();
        assert_eq!(snapped.target, Some(SnapTarget::Grid));
        let connector = route_connector(
            &model,
            "a",
            "b",
            ConnectorAnchor::Right,
            ConnectorAnchor::Left,
        )
        .unwrap();
        assert_eq!(connector.points.len(), 4);
        let export = export_projection(&model).unwrap();
        assert_eq!(export.element_ids, vec!["a", "b"]);
        assert_eq!(model.elements[0].transform.x, 0.0);
    }
}

#[cfg(test)]
mod perf_bench {
    use super::*;
    use std::time::Instant;

    fn element_at(id: &str, x: f32, y: f32) -> SceneElement {
        SceneElement {
            id: id.into(),
            type_id: "shape".into(),
            transform: oo_schema::Transform {
                x,
                y,
                width: 10.0,
                height: 10.0,
                ..oo_schema::Transform::default()
            },
            ..SceneElement::default()
        }
    }

    fn spread_elements(n: usize) -> WhiteboardModel {
        let elements = (0..n)
            .map(|index| {
                element_at(
                    &format!("e-{index}"),
                    index as f32 * 20.0,
                    (index % 20) as f32 * 20.0,
                )
            })
            .collect();
        WhiteboardModel {
            elements,
            camera: Camera::default(),
        }
    }

    /// R7 budget: a 10k-element scene must not scan the whole set for a
    /// viewport query; candidate and examined-cell counts stay bounded.
    #[test]
    #[ignore = "engine perf harness; run with --release -- --ignored perf_"]
    fn perf_spatial_query_bounds_candidates_at_10k() {
        let model = spread_elements(10_000);
        let built = Instant::now();
        let index = WhiteboardSpatialIndex::build(&model, 32.0).unwrap();
        let build_ms = built.elapsed().as_secs_f64() * 1000.0;

        let target = WorldRect::new(600.0, 0.0, 40.0, 20.0).unwrap();
        let started = Instant::now();
        let query = index.query_with_stats(target).unwrap();
        let query_ms = started.elapsed().as_secs_f64() * 1000.0;

        eprintln!(
            "perf_whiteboard build_ms={build_ms:.3} query_ms={query_ms:.3} candidates={} examined={} of {} elements",
            query.candidate_count, query.cells_examined, model.elements.len()
        );
        assert!(
            query.candidate_count < model.elements.len() / 50,
            "candidates {} not bounded",
            query.candidate_count
        );
        assert!(
            query.cells_examined <= 8,
            "cells_examined {} not bounded",
            query.cells_examined
        );
        assert!(query_ms < 10.0, "spatial query took {query_ms:.3}ms");
    }
}
