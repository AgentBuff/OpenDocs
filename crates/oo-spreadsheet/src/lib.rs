//! Spreadsheet Artifact 的 canonical 稀疏 Grid runtime。
//!
//! Spreadsheet 不复用 Document Block，也不携带 DOM、Canvas、WASM 或 React 状态。所有
//! 持久化修改都从 [`SpreadsheetCommandBatch`] 进入 [`SpreadsheetEngine::execute`]，引擎
//! 产出可逆的 [`SpreadsheetMutation`] 与协议级局部失效摘要。上层渲染器只
//! 消费 snapshot/change set；公式依赖图、虚拟 viewport 和 XLSX adapter 都应在此边界之外
//! 订阅这些明确的变更。

use oo_protocol::{EntityRef, Invalidation};
use oo_schema::{
    CellModel, CellStyle, SchemaValidationError, SheetMetadata, SheetModel, SpreadsheetModel,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

mod calculation;
mod capabilities;
mod formula;
mod viewport;

pub use calculation::{
    calculate, calculate_with_errors, CalculatedValue, CalculationResult, FormulaCalculationError,
    FormulaErrorCode,
};
pub use capabilities::{
    spreadsheet_capability_catalog, SpreadsheetCapability, SpreadsheetCapabilityCatalog,
};
pub use formula::{
    FormulaDependencyEdge, FormulaDependencyError, FormulaDependencyIndex,
    FormulaDependencySubgraph,
};
pub use viewport::{GridViewport, SparseGridViewport, ViewportCell, ViewportError};

/// Spreadsheet 的一次原子语义命令批次。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpreadsheetCommandBatch {
    pub base_revision: u64,
    pub commands: Vec<SpreadsheetCommand>,
}

/// Grid 命令与 Document Block 命令保持独立；这里没有 Block/children 字段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SpreadsheetCommand {
    CreateSheet {
        id: String,
        name: String,
    },
    RenameSheet {
        sheet_id: String,
        name: String,
    },
    DeleteSheet {
        sheet_id: String,
    },
    SetCell {
        sheet_id: String,
        row: u32,
        column: u32,
        #[serde(default)]
        value: Option<Value>,
        #[serde(default)]
        formula: Option<String>,
        #[serde(default)]
        attrs: Map<String, Value>,
    },
    /// Formats one cell without touching its value/formula, so toolbar formatting remains a
    /// semantic command and can be batched with range operations by a higher-level adapter.
    SetCellStyle {
        sheet_id: String,
        row: u32,
        column: u32,
        #[serde(default)]
        style: Option<CellStyle>,
    },
    /// Freeze/filter/sort/validation metadata is persisted on the worksheet, not in viewport
    /// state or an opaque cell attrs map.
    SetSheetMetadata {
        sheet_id: String,
        metadata: SheetMetadata,
    },
    ClearCell {
        sheet_id: String,
        row: u32,
        column: u32,
    },
}

/// 稀疏 Grid 的原子变更。before/after 使撤销和协同适配不依赖第二份 snapshot。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SpreadsheetMutation {
    SheetChanged {
        sheet_id: String,
        index: usize,
        #[serde(default)]
        before: Option<SheetModel>,
        #[serde(default)]
        after: Option<SheetModel>,
    },
    CellChanged {
        address: CellAddress,
        #[serde(default)]
        before: Option<CellModel>,
        #[serde(default)]
        after: Option<CellModel>,
    },
}

impl SpreadsheetMutation {
    /// Mutation 自带求逆所需的全部内容，撤销不需要读取旧文档或重新计算 diff。
    pub fn inverse(&self) -> Self {
        match self {
            Self::SheetChanged {
                sheet_id,
                index,
                before,
                after,
            } => Self::SheetChanged {
                sheet_id: sheet_id.clone(),
                index: *index,
                before: after.clone(),
                after: before.clone(),
            },
            Self::CellChanged {
                address,
                before,
                after,
            } => Self::CellChanged {
                address: address.clone(),
                before: after.clone(),
                after: before.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellAddress {
    pub sheet_id: String,
    pub row: u32,
    pub column: u32,
}

/// 执行结果是新的 revision、可逆 mutations 和局部失效摘要。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpreadsheetChangeSet {
    pub revision: u64,
    /// 使用跨 Artifact 共用的失效协议；cell 坐标编码为 `sheetId:row:column` 的 entityId，
    /// 避免 Spreadsheet 自己再维护一套不可被其他 Artifact 消费的 Invalidation 类型。
    pub invalidation: Invalidation,
    pub mutations: Vec<SpreadsheetMutation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpreadsheetEngine {
    model: SpreadsheetModel,
    revision: u64,
    undo: Vec<Vec<SpreadsheetMutation>>,
    redo: Vec<Vec<SpreadsheetMutation>>,
}

impl SpreadsheetEngine {
    pub fn new(model: SpreadsheetModel, revision: u64) -> Result<Self, SpreadsheetEngineError> {
        validate_model(&model)?;
        Ok(Self {
            model,
            revision,
            undo: Vec::new(),
            redo: Vec::new(),
        })
    }

    pub fn model(&self) -> &SpreadsheetModel {
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

    /// 唯一的 Spreadsheet 写入口。失败时反向应用已产生的 mutation，模型保持不变。
    pub fn execute(
        &mut self,
        batch: SpreadsheetCommandBatch,
    ) -> Result<SpreadsheetChangeSet, SpreadsheetEngineError> {
        self.ensure_batch(&batch)?;
        let revision = self.next_revision()?;
        let mut mutations = Vec::new();
        let mut invalidation = Invalidation::default();

        for command in batch.commands {
            if let Err(error) = self.apply_command(command, &mut mutations, &mut invalidation) {
                self.rollback(&mutations);
                return Err(error);
            }
        }

        if let Err(error) = validate_model(&self.model) {
            self.rollback(&mutations);
            return Err(error);
        }
        // Revision is a commit clock, not an input counter.  A command that leaves the sparse
        // Grid unchanged must not create an eventless revision which consumers cannot project.
        if mutations.is_empty() {
            return Err(SpreadsheetEngineError::NoChanges);
        }

        self.revision = revision;
        self.undo.push(mutations.clone());
        self.redo.clear();
        Ok(SpreadsheetChangeSet {
            revision,
            invalidation,
            mutations,
        })
    }

    pub fn undo(&mut self) -> Result<SpreadsheetChangeSet, SpreadsheetEngineError> {
        let revision = self.next_revision()?;
        let mutations = self
            .undo
            .pop()
            .ok_or(SpreadsheetEngineError::NothingToUndo)?;
        let inverse: Vec<_> = mutations
            .iter()
            .rev()
            .map(SpreadsheetMutation::inverse)
            .collect();
        let mut applied = Vec::with_capacity(inverse.len());
        for mutation in &inverse {
            if let Err(error) = apply_mutation(&mut self.model, mutation) {
                self.rollback(&applied);
                self.undo.push(mutations);
                return Err(error);
            }
            applied.push(mutation.clone());
        }
        if let Err(error) = validate_model(&self.model) {
            self.rollback(&applied);
            self.undo.push(mutations);
            return Err(error);
        }
        self.redo.push(mutations);
        self.revision = revision;
        Ok(ChangeSetBuilder::from_mutations(revision, inverse).build())
    }

    pub fn redo(&mut self) -> Result<SpreadsheetChangeSet, SpreadsheetEngineError> {
        let revision = self.next_revision()?;
        let mutations = self
            .redo
            .pop()
            .ok_or(SpreadsheetEngineError::NothingToRedo)?;
        let mut applied = Vec::with_capacity(mutations.len());
        for mutation in &mutations {
            if let Err(error) = apply_mutation(&mut self.model, mutation) {
                self.rollback(&applied);
                self.redo.push(mutations);
                return Err(error);
            }
            applied.push(mutation.clone());
        }
        if let Err(error) = validate_model(&self.model) {
            self.rollback(&applied);
            self.redo.push(mutations);
            return Err(error);
        }
        self.undo.push(mutations.clone());
        self.revision = revision;
        Ok(ChangeSetBuilder::from_mutations(revision, mutations).build())
    }

    fn ensure_batch(&self, batch: &SpreadsheetCommandBatch) -> Result<(), SpreadsheetEngineError> {
        if batch.commands.is_empty() {
            return Err(SpreadsheetEngineError::EmptyCommandBatch);
        }
        if batch.base_revision != self.revision {
            return Err(SpreadsheetEngineError::RevisionConflict {
                expected: self.revision,
                actual: batch.base_revision,
            });
        }
        Ok(())
    }

    fn next_revision(&self) -> Result<u64, SpreadsheetEngineError> {
        self.revision
            .checked_add(1)
            .ok_or(SpreadsheetEngineError::RevisionOverflow)
    }

    fn apply_command(
        &mut self,
        command: SpreadsheetCommand,
        mutations: &mut Vec<SpreadsheetMutation>,
        invalidation: &mut Invalidation,
    ) -> Result<(), SpreadsheetEngineError> {
        match command {
            SpreadsheetCommand::CreateSheet { id, name } => {
                if id.trim().is_empty() || name.trim().is_empty() {
                    return Err(SpreadsheetEngineError::EmptySheetIdentity);
                }
                if self.model.sheets.iter().any(|sheet| sheet.id == id) {
                    return Err(SpreadsheetEngineError::DuplicateSheet(id));
                }
                let index = self.model.sheets.len();
                let sheet = SheetModel {
                    id: id.clone(),
                    name,
                    cells: Vec::new(),
                    metadata: Default::default(),
                };
                let mutation = SpreadsheetMutation::SheetChanged {
                    sheet_id: id.clone(),
                    index,
                    before: None,
                    after: Some(sheet),
                };
                apply_mutation(&mut self.model, &mutation)?;
                mutations.push(mutation);
                invalidation.changed_containers.push(sheet_ref(id));
                invalidation.structure_changed = true;
            }
            SpreadsheetCommand::RenameSheet { sheet_id, name } => {
                if name.trim().is_empty() {
                    return Err(SpreadsheetEngineError::EmptySheetIdentity);
                }
                let index = sheet_index(&self.model, &sheet_id)?;
                let before = self.model.sheets[index].clone();
                let mut after = before.clone();
                after.name = name;
                let mutation = SpreadsheetMutation::SheetChanged {
                    sheet_id: sheet_id.clone(),
                    index,
                    before: Some(before),
                    after: Some(after),
                };
                apply_mutation(&mut self.model, &mutation)?;
                mutations.push(mutation);
                invalidation.changed_containers.push(sheet_ref(sheet_id));
            }
            SpreadsheetCommand::DeleteSheet { sheet_id } => {
                let index = sheet_index(&self.model, &sheet_id)?;
                let before = self.model.sheets[index].clone();
                let mutation = SpreadsheetMutation::SheetChanged {
                    sheet_id: sheet_id.clone(),
                    index,
                    before: Some(before),
                    after: None,
                };
                apply_mutation(&mut self.model, &mutation)?;
                mutations.push(mutation);
                invalidation.changed_containers.push(sheet_ref(sheet_id));
                invalidation.structure_changed = true;
            }
            SpreadsheetCommand::SetCell {
                sheet_id,
                row,
                column,
                value,
                formula,
                attrs,
            } => {
                let address = CellAddress {
                    sheet_id,
                    row,
                    column,
                };
                let before = find_cell(&self.model, &address).cloned();
                let after = normalized_cell(row, column, value, formula, attrs);
                if before == after {
                    return Ok(());
                }
                let mutation = SpreadsheetMutation::CellChanged {
                    address: address.clone(),
                    before,
                    after,
                };
                apply_mutation(&mut self.model, &mutation)?;
                mutations.push(mutation);
                invalidation
                    .changed_containers
                    .push(sheet_ref(address.sheet_id.clone()));
                invalidation.changed_entities.push(cell_ref(&address));
            }
            SpreadsheetCommand::SetCellStyle {
                sheet_id,
                row,
                column,
                style,
            } => {
                let address = CellAddress {
                    sheet_id,
                    row,
                    column,
                };
                let Some(before_cell) = find_cell(&self.model, &address).cloned() else {
                    return Err(SpreadsheetEngineError::MissingCell(address));
                };
                if before_cell.style == style {
                    return Ok(());
                }
                let mut after_cell = before_cell.clone();
                after_cell.style = style;
                let mutation = SpreadsheetMutation::CellChanged {
                    address: address.clone(),
                    before: Some(before_cell),
                    after: Some(after_cell),
                };
                apply_mutation(&mut self.model, &mutation)?;
                mutations.push(mutation);
                invalidation
                    .changed_containers
                    .push(sheet_ref(address.sheet_id.clone()));
                invalidation.changed_entities.push(cell_ref(&address));
            }
            SpreadsheetCommand::SetSheetMetadata { sheet_id, metadata } => {
                let index = sheet_index(&self.model, &sheet_id)?;
                let before = self.model.sheets[index].clone();
                if before.metadata == metadata {
                    return Ok(());
                }
                let mut after = before.clone();
                after.metadata = metadata;
                let mutation = SpreadsheetMutation::SheetChanged {
                    sheet_id: sheet_id.clone(),
                    index,
                    before: Some(before),
                    after: Some(after),
                };
                apply_mutation(&mut self.model, &mutation)?;
                mutations.push(mutation);
                invalidation.changed_containers.push(sheet_ref(sheet_id));
            }
            SpreadsheetCommand::ClearCell {
                sheet_id,
                row,
                column,
            } => {
                let address = CellAddress {
                    sheet_id,
                    row,
                    column,
                };
                let Some(before) = find_cell(&self.model, &address).cloned() else {
                    return Ok(());
                };
                let mutation = SpreadsheetMutation::CellChanged {
                    address: address.clone(),
                    before: Some(before),
                    after: None,
                };
                apply_mutation(&mut self.model, &mutation)?;
                mutations.push(mutation);
                invalidation
                    .changed_containers
                    .push(sheet_ref(address.sheet_id.clone()));
                invalidation.changed_entities.push(cell_ref(&address));
            }
        }
        normalize_invalidation(invalidation);
        Ok(())
    }

    fn rollback(&mut self, mutations: &[SpreadsheetMutation]) {
        for mutation in mutations.iter().rev() {
            let _ = apply_mutation(&mut self.model, &mutation.inverse());
        }
    }
}

/// Applies one canonical mutation. This is intentionally public for a future XLSX import/export
/// adapter and server-side event consumer; adapters must not mutate `SpreadsheetModel` directly.
pub fn apply_mutation(
    model: &mut SpreadsheetModel,
    mutation: &SpreadsheetMutation,
) -> Result<(), SpreadsheetEngineError> {
    match mutation {
        SpreadsheetMutation::SheetChanged {
            sheet_id,
            index,
            before: _,
            after,
        } => {
            if let Some(sheet) = after {
                if let Some(existing) = model.sheets.iter_mut().find(|sheet| sheet.id == *sheet_id)
                {
                    *existing = sheet.clone();
                } else if *index <= model.sheets.len() {
                    model.sheets.insert(*index, sheet.clone());
                } else {
                    return Err(SpreadsheetEngineError::InvalidMutation(format!(
                        "sheet {} insertion index {} 越界",
                        sheet_id, index
                    )));
                }
            } else if let Some(index) = model.sheets.iter().position(|sheet| sheet.id == *sheet_id)
            {
                model.sheets.remove(index);
            }
        }
        SpreadsheetMutation::CellChanged {
            address,
            before: _,
            after,
        } => {
            let sheet = sheet_mut(model, &address.sheet_id)?;
            if let Some(cell) = after {
                if let Some(existing) = sheet
                    .cells
                    .iter_mut()
                    .find(|cell| cell.row == address.row && cell.column == address.column)
                {
                    *existing = cell.clone();
                } else {
                    sheet.cells.push(cell.clone());
                }
                sheet.cells.sort_by_key(|cell| (cell.row, cell.column));
            } else if let Some(index) = sheet
                .cells
                .iter()
                .position(|cell| cell.row == address.row && cell.column == address.column)
            {
                sheet.cells.remove(index);
            }
        }
    }
    Ok(())
}

fn normalized_cell(
    row: u32,
    column: u32,
    value: Option<Value>,
    formula: Option<String>,
    attrs: Map<String, Value>,
) -> Option<CellModel> {
    let formula = formula.filter(|formula| !formula.trim().is_empty());
    if value.is_none() && formula.is_none() && attrs.is_empty() {
        None
    } else {
        Some(CellModel {
            row,
            column,
            value,
            formula,
            attrs,
            style: None,
        })
    }
}

fn validate_model(model: &SpreadsheetModel) -> Result<(), SpreadsheetEngineError> {
    oo_schema::ArtifactEnvelope::new(
        "spreadsheet-engine",
        oo_schema::ArtifactPayload::Spreadsheet(model.clone()),
    )
    .validate()
    .map_err(SpreadsheetEngineError::Schema)
}

fn sheet_index(model: &SpreadsheetModel, id: &str) -> Result<usize, SpreadsheetEngineError> {
    model
        .sheets
        .iter()
        .position(|sheet| sheet.id == id)
        .ok_or_else(|| SpreadsheetEngineError::MissingSheet(id.to_string()))
}

fn sheet_mut<'a>(
    model: &'a mut SpreadsheetModel,
    id: &str,
) -> Result<&'a mut SheetModel, SpreadsheetEngineError> {
    let index = sheet_index(model, id)?;
    Ok(&mut model.sheets[index])
}

fn find_cell<'a>(model: &'a SpreadsheetModel, address: &CellAddress) -> Option<&'a CellModel> {
    model
        .sheets
        .iter()
        .find(|sheet| sheet.id == address.sheet_id)
        .and_then(|sheet| {
            sheet
                .cells
                .iter()
                .find(|cell| cell.row == address.row && cell.column == address.column)
        })
}

fn sheet_ref(sheet_id: String) -> EntityRef {
    EntityRef {
        entity_type: "spreadsheet.sheet".into(),
        entity_id: sheet_id,
    }
}

fn cell_ref(address: &CellAddress) -> EntityRef {
    EntityRef {
        entity_type: "spreadsheet.cell".into(),
        entity_id: format!("{}:{}:{}", address.sheet_id, address.row, address.column),
    }
}

fn normalize_invalidation(invalidation: &mut Invalidation) {
    invalidation.changed_entities.sort_by(|left, right| {
        (&left.entity_type, &left.entity_id).cmp(&(&right.entity_type, &right.entity_id))
    });
    invalidation.changed_entities.dedup();
    invalidation.changed_containers.sort_by(|left, right| {
        (&left.entity_type, &left.entity_id).cmp(&(&right.entity_type, &right.entity_id))
    });
    invalidation.changed_containers.dedup();
}

#[derive(Debug, thiserror::Error)]
pub enum SpreadsheetEngineError {
    #[error("命令批次不能为空")]
    EmptyCommandBatch,
    #[error("revision 冲突：服务端是 {expected}，批次基于 {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("sheet {0} 不存在")]
    MissingSheet(String),
    #[error("sheet {0} 已存在")]
    DuplicateSheet(String),
    #[error("sheet id 或名称不能为空")]
    EmptySheetIdentity,
    #[error("cell {0:?} 不存在")]
    MissingCell(CellAddress),
    #[error("revision 溢出")]
    RevisionOverflow,
    #[error("命令没有产生 Spreadsheet 变更")]
    NoChanges,
    #[error("没有可撤销的 Spreadsheet 命令")]
    NothingToUndo,
    #[error("没有可重做的 Spreadsheet 命令")]
    NothingToRedo,
    #[error("无效的 Spreadsheet mutation：{0}")]
    InvalidMutation(String),
    #[error("Spreadsheet schema 校验失败：{0}")]
    Schema(#[from] SchemaValidationError),
}

struct ChangeSetBuilder {
    revision: u64,
    mutations: Vec<SpreadsheetMutation>,
    invalidation: Invalidation,
}

impl ChangeSetBuilder {
    fn from_mutations(revision: u64, mutations: Vec<SpreadsheetMutation>) -> Self {
        let mut builder = Self {
            revision,
            mutations,
            invalidation: Invalidation::default(),
        };
        for mutation in &builder.mutations {
            match mutation {
                SpreadsheetMutation::SheetChanged {
                    sheet_id,
                    before,
                    after,
                    ..
                } => {
                    builder
                        .invalidation
                        .changed_containers
                        .push(sheet_ref(sheet_id.clone()));
                    builder.invalidation.structure_changed |= before.is_none() || after.is_none();
                }
                SpreadsheetMutation::CellChanged { address, .. } => {
                    builder
                        .invalidation
                        .changed_containers
                        .push(sheet_ref(address.sheet_id.clone()));
                    builder
                        .invalidation
                        .changed_entities
                        .push(cell_ref(address));
                }
            }
        }
        normalize_invalidation(&mut builder.invalidation);
        builder
    }

    fn build(self) -> SpreadsheetChangeSet {
        SpreadsheetChangeSet {
            revision: self.revision,
            invalidation: self.invalidation,
            mutations: self.mutations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> SpreadsheetEngine {
        SpreadsheetEngine::new(
            SpreadsheetModel {
                sheets: vec![SheetModel {
                    id: "sheet-1".into(),
                    name: "Sheet 1".into(),
                    cells: Vec::new(),
                    ..SheetModel::default()
                }],
                ..SpreadsheetModel::default()
            },
            0,
        )
        .unwrap()
    }

    #[test]
    fn sparse_grid_returns_local_invalidation_and_reversible_mutation() {
        let mut engine = engine();
        let result = engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::SetCell {
                    sheet_id: "sheet-1".into(),
                    row: 4,
                    column: 2,
                    value: Some(Value::String("hello".into())),
                    formula: None,
                    attrs: Map::new(),
                }],
            })
            .unwrap();
        assert_eq!(result.revision, 1);
        assert_eq!(result.invalidation.changed_entities.len(), 1);
        assert_eq!(
            result.invalidation.changed_entities[0].entity_type,
            "spreadsheet.cell"
        );
        assert_eq!(engine.model().sheets[0].cells.len(), 1);
        assert_eq!(result.mutations.len(), 1);

        let undo = engine.undo().unwrap();
        assert_eq!(undo.revision, 2);
        assert!(engine.model().sheets[0].cells.is_empty());
        assert!(engine.can_redo());
        engine.redo().unwrap();
        assert_eq!(engine.model().sheets[0].cells[0].row, 4);
    }

    #[test]
    fn failed_batch_rolls_back_without_snapshot_clone_api() {
        let mut engine = engine();
        let error = engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![
                    SpreadsheetCommand::CreateSheet {
                        id: "sheet-2".into(),
                        name: "Sheet 2".into(),
                    },
                    SpreadsheetCommand::RenameSheet {
                        sheet_id: "missing".into(),
                        name: "bad".into(),
                    },
                ],
            })
            .unwrap_err();
        assert!(matches!(error, SpreadsheetEngineError::MissingSheet(_)));
        assert_eq!(engine.revision(), 0);
        assert_eq!(engine.model().sheets.len(), 1);
    }

    #[test]
    fn empty_cell_is_not_materialized_in_sparse_grid() {
        let mut engine = engine();
        let result = engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::SetCell {
                    sheet_id: "sheet-1".into(),
                    row: 0,
                    column: 0,
                    value: None,
                    formula: Some("   ".into()),
                    attrs: Map::new(),
                }],
            })
            .unwrap_err();
        assert!(matches!(result, SpreadsheetEngineError::NoChanges));
        assert!(engine.model().sheets[0].cells.is_empty());
    }

    #[test]
    fn mutation_json_is_stable_and_does_not_use_document_block_names() {
        let command = SpreadsheetCommand::SetCell {
            sheet_id: "sheet-1".into(),
            row: 2,
            column: 3,
            value: Some(Value::Number(42.into())),
            formula: Some("=A1".into()),
            attrs: Map::new(),
        };
        let json = serde_json::to_value(command).unwrap();
        assert_eq!(json["type"], "setCell");
        assert_eq!(json["sheetId"], "sheet-1");
        assert!(json.get("blockId").is_none());
    }

    #[test]
    fn duplicate_coordinates_are_rejected_by_schema() {
        let model = SpreadsheetModel {
            sheets: vec![SheetModel {
                id: "sheet-1".into(),
                name: "Sheet 1".into(),
                cells: vec![
                    CellModel {
                        row: 0,
                        column: 0,
                        ..CellModel::default()
                    },
                    CellModel {
                        row: 0,
                        column: 0,
                        ..CellModel::default()
                    },
                ],
                ..SheetModel::default()
            }],
            ..SpreadsheetModel::default()
        };
        assert!(matches!(
            SpreadsheetEngine::new(model, 0),
            Err(SpreadsheetEngineError::Schema(_))
        ));
    }
}
