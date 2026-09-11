//! Spreadsheet Artifact 的 canonical 稀疏 Grid runtime。
//!
//! Spreadsheet 不复用 Document Block，也不携带 DOM、Canvas、WASM 或 React 状态。所有
//! 持久化修改都从 [`SpreadsheetCommandBatch`] 进入 [`SpreadsheetEngine::execute`]，引擎
//! 产出可逆的 [`SpreadsheetMutation`] 与协议级局部失效摘要。失效摘要包含被写单元格
//! 及其公式依赖闭包（由引擎内维护的非持久化 [`FormulaDependencyIndex`] 缓存推导），
//! 因此渲染器无需重新扫描公式即可知道哪些派生值过期。

use oo_protocol::{EntityRef, Invalidation};
use oo_schema::{
    CalculationMode, CellModel, CellStyle, FilterSpec, FreezePane, GridRange,
    SchemaValidationError, SheetMetadata, SheetModel, SortKey, SpreadsheetModel,
    SpreadsheetNamedRange,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

mod calculation;
mod capabilities;
mod formatting;
mod formula;
use formatting::apply_style_fields;
pub use formatting::{CellStyleField, RangeRowPattern};
mod viewport;

pub use calculation::{
    calculate, calculate_targets_with_errors, calculate_with_errors, spreadsheet_function_catalog,
    CalculatedValue, CalculationResult, FormulaCalculationError, FormulaErrorCode,
    SpreadsheetFunctionCategory, SpreadsheetFunctionDescriptor,
};
pub use capabilities::{
    spreadsheet_capability_catalog, spreadsheet_command_registry, SpreadsheetCapability,
    SpreadsheetCapabilityCatalog, SpreadsheetCommandDescriptor,
};
pub use formula::{
    rewrite_formula_references, rewrite_sheet_lookup, translate_formula_for_copy,
    FormulaDependencyEdge, FormulaDependencyError, FormulaDependencyIndex,
    FormulaDependencySubgraph, RewriteAxis, RewriteOp,
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
    /// Update a row range without replacing unrelated worksheet metadata.
    SetRowLayout {
        sheet_id: String,
        start_row: u32,
        end_row: u32,
        #[serde(default)]
        height: Option<f64>,
        #[serde(default)]
        reset_height: bool,
        #[serde(default)]
        hidden: Option<bool>,
    },
    SetSheetMetadata {
        sheet_id: String,
        metadata: SheetMetadata,
    },
    ClearCell {
        sheet_id: String,
        row: u32,
        column: u32,
    },
    /// Inserts empty rows at a 0-based row boundary. All existing cells at or
    /// below `at` shift down by `count`; merged ranges and the frozen band are
    /// remapped so a structural edit never silently corrupts them.
    InsertRows {
        sheet_id: String,
        at: u32,
        count: u32,
    },
    DeleteRows {
        sheet_id: String,
        at: u32,
        count: u32,
    },
    InsertColumns {
        sheet_id: String,
        at: u32,
        count: u32,
    },
    DeleteColumns {
        sheet_id: String,
        at: u32,
        count: u32,
    },
    /// Merges an inclusive rectangular region into one anchor cell (its top-left
    /// corner). Content of every non-anchor cell inside the region is dropped,
    /// matching Excel; the whole edit stays reversible through the sheet
    /// mutation. Overlapping an existing merged range is rejected.
    MergeCells {
        sheet_id: String,
        range: GridRange,
    },
    /// Splits a previously merged region. `range` must exactly match an
    /// existing merged range so undo semantics stay unambiguous.
    UnmergeCells {
        sheet_id: String,
        range: GridRange,
    },
    /// Reorders the rows of an inclusive region by the given sort keys.
    /// Cells carrying formulas make the row order ambiguous (a moved formula
    /// silently changes meaning), so sorting a region with formulas is a typed
    /// error instead of a best-effort shuffle.
    SortRange {
        sheet_id: String,
        range: GridRange,
        keys: Vec<SortKey>,
    },
    /// Applies a complete style or only explicitly selected style fields to
    /// the inclusive range, optionally selecting rows by a pattern, as a
    /// single atomic range mutation (M1-S: one user
    /// action = one history entry = one journal mutation). Blank cells are
    /// materialized only when the region is small (see
    /// `MAX_RANGE_MATERIALIZE`), so a 100k-cell format never explodes the
    /// sparse grid.
    FormatRange {
        sheet_id: String,
        range: GridRange,
        style: CellStyle,
        /// Absent replaces the entire style (e.g. format painter). A field list
        /// modifies only those properties on each cell, preserving mixed styles.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fields: Option<Vec<CellStyleField>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        row_pattern: Option<RangeRowPattern>,
    },
    /// Clears one aspect of the inclusive range atomically.
    ClearRange {
        sheet_id: String,
        range: GridRange,
        mode: ClearRangeMode,
    },
    /// Replaces text inside string values across the inclusive range as one
    /// atomic operation. Formula cells are skipped (rewriting formulas is the
    /// structural-edit rewriter's job, not find-and-replace's).
    ReplaceRange {
        sheet_id: String,
        range: GridRange,
        search: String,
        replace: String,
        #[serde(default)]
        match_case: bool,
    },
    /// Pastes a sparse clipboard matrix into one destination range. The
    /// browser sends one command regardless of matrix size; the engine owns
    /// clearing, formula translation, validation and the reversible journal.
    PasteRange {
        sheet_id: String,
        start_row: u32,
        start_column: u32,
        row_count: u32,
        column_count: u32,
        cells: Vec<SpreadsheetClipboardCell>,
        mode: SpreadsheetPasteMode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_origin: Option<SpreadsheetClipboardOrigin>,
    },
    /// Repeats a canonical source range over the destination. Source cells
    /// are captured before mutation, so overlapping fills remain deterministic.
    FillRange {
        source_sheet_id: String,
        source_range: GridRange,
        destination_sheet_id: String,
        destination_range: GridRange,
        #[serde(default)]
        mode: SpreadsheetPasteMode,
    },
    /// Replaces only the frozen pane instead of the whole sheet metadata.
    SetFreezePane {
        sheet_id: String,
        rows: u32,
        columns: u32,
    },
    /// Replaces only the auto filter instead of the whole sheet metadata.
    /// `None` clears filtering; `Some(range)` enables filter mode with an
    /// empty predicate list (nothing is hidden until a column predicate is
    /// configured).
    SetAutoFilter {
        sheet_id: String,
        range: Option<GridRange>,
    },
    /// M2-S：为筛选区域内一列插入或替换谓词（按列号幂等）。要求 autoFilter
    /// 已启用且列号落在筛选范围之内。
    UpsertFilterColumn {
        sheet_id: String,
        column: u32,
        predicate: oo_schema::FilterPredicate,
    },
    /// M2-S：移除筛选列谓词。`None` 清空全部列谓词，`Some(column)` 只清一列。
    ClearFilterColumn {
        sheet_id: String,
        column: Option<u32>,
    },
    /// M2-S：计算模式（automatic/manual）是 workbook 级窄命令。
    SetCalculationMode {
        calculation_mode: CalculationMode,
    },
    /// M2-S：行数维度窄命令。`None` 清除自定义维度。
    SetRowDimensions {
        sheet_id: String,
        rows: Option<u32>,
    },
    /// M2-S：列数维度窄命令。
    SetColumnDimensions {
        sheet_id: String,
        columns: Option<u32>,
    },
    /// M2-S：条件格式规则按 `rule.id` 幂等插入或整体替换。
    UpsertConditionalFormat {
        sheet_id: String,
        rule: oo_schema::ConditionalFormatRule,
    },
    /// M2-S：按 id 删除条件格式规则。
    DeleteConditionalFormat {
        sheet_id: String,
        rule_id: String,
    },
    /// M2-S：数据校验规则按 `rule.id` 幂等插入或整体替换。
    UpsertDataValidation {
        sheet_id: String,
        rule: oo_schema::DataValidationRule,
    },
    /// M2-S：按 id 删除数据校验规则。
    DeleteDataValidation {
        sheet_id: String,
        rule_id: String,
    },
}

/// `ClearRange` 的作用域：内容 / 格式 / 全部。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClearRangeMode {
    #[default]
    Contents,
    Formats,
    All,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpreadsheetPasteMode {
    #[default]
    All,
    Values,
    Formats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpreadsheetClipboardOrigin {
    pub row: u32,
    pub column: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpreadsheetClipboardCell {
    pub row_offset: u32,
    pub column_offset: u32,
    #[serde(default)]
    pub value: Option<Value>,
    #[serde(default)]
    pub formula: Option<String>,
    #[serde(default)]
    pub attrs: Map<String, Value>,
    #[serde(default)]
    pub style: Option<CellStyle>,
}

/// 稀疏 Grid 的原子变更。before/after 使撤销和协同适配不依赖第二份 snapshot。
#[allow(clippy::large_enum_variant)]
// SheetChanged 携带整表快照是撤销可逆性的既定设计（见 emit_sheet_change 文档）
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
    /// Range 级原子变更（M1-S journal 压缩）：一次范围操作只产生一条 mutation，
    /// before/after 携带范围内受影响格子的完整快照，撤销 = 前后互换。
    RangeChanged {
        sheet_id: String,
        range: GridRange,
        #[serde(default)]
        before: Vec<CellModel>,
        #[serde(default)]
        after: Vec<CellModel>,
    },
    /// 冻结窗格的细粒度变更（避免整体覆盖 SheetMetadata 的协作冲突）。
    PaneChanged {
        sheet_id: String,
        before: FreezePane,
        after: FreezePane,
    },
    /// 筛选的细粒度变更。
    FilterChanged {
        sheet_id: String,
        before: Option<FilterSpec>,
        after: Option<FilterSpec>,
    },
    /// M2-S：筛选列谓词的细粒度变更（整份 columns 前后互换）。
    FilterColumnsChanged {
        sheet_id: String,
        before: Vec<oo_schema::FilterColumn>,
        after: Vec<oo_schema::FilterColumn>,
    },
    /// M2-S：计算模式是 workbook 级字段，与任何 sheet 无关。
    CalculationModeChanged {
        before: CalculationMode,
        after: CalculationMode,
    },
    /// Workbook-level named ranges affected by sheet deletion or structural grid edits.
    NamedRangesChanged {
        before: Vec<SpreadsheetNamedRange>,
        after: Vec<SpreadsheetNamedRange>,
    },
    ActiveSheetChanged {
        before: Option<String>,
        after: Option<String>,
    },
    /// M2-S：行/列维度窄变更（冻结范围校验在命令分支完成）。
    RowDimensionsChanged {
        sheet_id: String,
        before: Option<u32>,
        after: Option<u32>,
    },
    ColumnDimensionsChanged {
        sheet_id: String,
        before: Option<u32>,
        after: Option<u32>,
    },
    /// M2-S：条件格式/数据校验规则按 id 的细粒度变更。
    ConditionalFormatUpserted {
        sheet_id: String,
        rule: oo_schema::ConditionalFormatRule,
        /// 被覆盖的旧规则（若存在）；撤销时精确恢复。
        #[serde(default)]
        previous: Option<oo_schema::ConditionalFormatRule>,
    },
    ConditionalFormatRemoved {
        sheet_id: String,
        rule_id: String,
        removed: oo_schema::ConditionalFormatRule,
    },
    DataValidationUpserted {
        sheet_id: String,
        rule: oo_schema::DataValidationRule,
        /// 被覆盖的旧规则（若存在）；撤销时精确恢复。
        #[serde(default)]
        previous: Option<oo_schema::DataValidationRule>,
    },
    DataValidationRemoved {
        sheet_id: String,
        rule_id: String,
        removed: oo_schema::DataValidationRule,
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
            Self::RangeChanged {
                sheet_id,
                range,
                before,
                after,
            } => Self::RangeChanged {
                sheet_id: sheet_id.clone(),
                range: *range,
                before: after.clone(),
                after: before.clone(),
            },
            Self::PaneChanged {
                sheet_id,
                before,
                after,
            } => Self::PaneChanged {
                sheet_id: sheet_id.clone(),
                before: *after,
                after: *before,
            },
            Self::FilterChanged {
                sheet_id,
                before,
                after,
            } => Self::FilterChanged {
                sheet_id: sheet_id.clone(),
                before: after.clone(),
                after: before.clone(),
            },
            Self::FilterColumnsChanged {
                sheet_id,
                before,
                after,
            } => Self::FilterColumnsChanged {
                sheet_id: sheet_id.clone(),
                before: after.clone(),
                after: before.clone(),
            },
            Self::CalculationModeChanged { before, after } => Self::CalculationModeChanged {
                before: *after,
                after: *before,
            },
            Self::NamedRangesChanged { before, after } => Self::NamedRangesChanged {
                before: after.clone(),
                after: before.clone(),
            },
            Self::ActiveSheetChanged { before, after } => Self::ActiveSheetChanged {
                before: after.clone(),
                after: before.clone(),
            },
            Self::RowDimensionsChanged {
                sheet_id,
                before,
                after,
            } => Self::RowDimensionsChanged {
                sheet_id: sheet_id.clone(),
                before: *after,
                after: *before,
            },
            Self::ColumnDimensionsChanged {
                sheet_id,
                before,
                after,
            } => Self::ColumnDimensionsChanged {
                sheet_id: sheet_id.clone(),
                before: *after,
                after: *before,
            },
            // 实体级 upsert 的逆 = 删除该 id（remove 幂等）；remove 的逆 = 放回快照。
            // upsert 覆盖旧值的情况由引擎 inverse_with_model 补足旧值精度。
            Self::ConditionalFormatUpserted {
                sheet_id,
                rule,
                previous,
            } => match previous {
                Some(previous) => Self::ConditionalFormatUpserted {
                    sheet_id: sheet_id.clone(),
                    rule: previous.clone(),
                    previous: Some(rule.clone()),
                },
                None => Self::ConditionalFormatRemoved {
                    sheet_id: sheet_id.clone(),
                    rule_id: rule.id.clone(),
                    removed: rule.clone(),
                },
            },
            Self::ConditionalFormatRemoved {
                sheet_id,
                rule_id: _,
                removed,
            } => Self::ConditionalFormatUpserted {
                sheet_id: sheet_id.clone(),
                rule: removed.clone(),
                previous: None,
            },
            Self::DataValidationUpserted {
                sheet_id,
                rule,
                previous,
            } => match previous {
                Some(previous) => Self::DataValidationUpserted {
                    sheet_id: sheet_id.clone(),
                    rule: previous.clone(),
                    previous: Some(rule.clone()),
                },
                None => Self::DataValidationRemoved {
                    sheet_id: sheet_id.clone(),
                    rule_id: rule.id.clone(),
                    removed: rule.clone(),
                },
            },
            Self::DataValidationRemoved {
                sheet_id,
                rule_id: _,
                removed,
            } => Self::DataValidationUpserted {
                sheet_id: sheet_id.clone(),
                rule: removed.clone(),
                previous: None,
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

#[derive(Debug, Clone)]
pub struct SpreadsheetEngine {
    model: SpreadsheetModel,
    revision: u64,
    undo: Vec<Vec<SpreadsheetMutation>>,
    redo: Vec<Vec<SpreadsheetMutation>>,
    /// 非持久化的公式依赖图缓存，从不进入 snapshot 或 mutation。普通单元格写入走
    /// 增量 `update`，任何 SheetChanged（结构编辑、改名、建删表）之后惰性整体重建；
    /// 模型里出现索引无法解析的公式时该缓存被丢弃，依赖失效汇报退化为尽力而为。
    dependencies: Option<FormulaDependencyIndex>,
}

impl PartialEq for SpreadsheetEngine {
    /// 依赖图是纯派生缓存，不参与引擎相等性：同一模型同一历史 state 一定可重建出
    /// 同一索引，缓存命中与否不改变可观察行为。
    fn eq(&self, other: &Self) -> bool {
        self.model == other.model
            && self.revision == other.revision
            && self.undo == other.undo
            && self.redo == other.redo
    }
}

impl SpreadsheetEngine {
    pub fn new(model: SpreadsheetModel, revision: u64) -> Result<Self, SpreadsheetEngineError> {
        validate_model(&model)?;
        Ok(Self {
            model,
            revision,
            undo: Vec::new(),
            redo: Vec::new(),
            dependencies: None,
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
        let mut change_set = SpreadsheetChangeSet {
            revision,
            invalidation,
            mutations,
        };
        self.sync_dependency_invalidation(&change_set.mutations, &mut change_set.invalidation);
        Ok(change_set)
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
            .map(|mutation| self.inverse_with_model(mutation))
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
        let mut change_set = ChangeSetBuilder::from_mutations(revision, inverse).build();
        self.sync_dependency_invalidation(&change_set.mutations, &mut change_set.invalidation);
        Ok(change_set)
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
        let mut change_set = ChangeSetBuilder::from_mutations(revision, mutations).build();
        self.sync_dependency_invalidation(&change_set.mutations, &mut change_set.invalidation);
        Ok(change_set)
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
                let named_before = self.model.metadata.named_ranges.clone();
                let named_after: Vec<_> = named_before
                    .iter()
                    .filter(|named| {
                        named.sheet_id != sheet_id
                            && named.scope_sheet_id.as_deref() != Some(sheet_id.as_str())
                    })
                    .cloned()
                    .collect();
                if named_before != named_after {
                    let named_mutation = SpreadsheetMutation::NamedRangesChanged {
                        before: named_before,
                        after: named_after,
                    };
                    apply_mutation(&mut self.model, &named_mutation)?;
                    mutations.push(named_mutation);
                    invalidation.changed_containers.push(workbook_ref());
                }
                if self.model.metadata.active_sheet_id.as_deref() == Some(sheet_id.as_str()) {
                    let next_active = self
                        .model
                        .sheets
                        .iter()
                        .enumerate()
                        .filter(|(_, sheet)| sheet.id != sheet_id)
                        .min_by_key(|(position, _)| position.abs_diff(index))
                        .map(|(_, sheet)| sheet.id.clone());
                    let active_mutation = SpreadsheetMutation::ActiveSheetChanged {
                        before: self.model.metadata.active_sheet_id.clone(),
                        after: next_active,
                    };
                    apply_mutation(&mut self.model, &active_mutation)?;
                    mutations.push(active_mutation);
                    invalidation.changed_containers.push(workbook_ref());
                }
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
                let style = before.as_ref().and_then(|cell| cell.style.clone());
                let after = normalized_cell(row, column, value, formula, attrs, style);
                if before == after {
                    return Ok(());
                }
                // Data validation rules gate the written value, not the cell
                // identity; clearing a cell is always allowed (ClearCell).
                if let Some(after_cell) = after.as_ref() {
                    validate_against_rules(&self.model, &address, after_cell)?;
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
                let before_cell = find_cell(&self.model, &address).cloned();
                if let Some(existing) = before_cell.as_ref() {
                    if existing.style == style {
                        return Ok(());
                    }
                }
                // Formatting an empty cell is a first-class action: the sparse grid
                // materializes a blank, styled cell so a fill or border never errors
                // on a cell that has no value yet.
                let mut after_cell = before_cell.clone().unwrap_or_else(|| CellModel {
                    row,
                    column,
                    value: None,
                    formula: None,
                    attrs: Map::new(),
                    style: None,
                });
                after_cell.style = style;
                let mutation = SpreadsheetMutation::CellChanged {
                    address: address.clone(),
                    before: before_cell,
                    after: Some(after_cell),
                };
                apply_mutation(&mut self.model, &mutation)?;
                mutations.push(mutation);
                invalidation
                    .changed_containers
                    .push(sheet_ref(address.sheet_id.clone()));
                invalidation.changed_entities.push(cell_ref(&address));
            }
            SpreadsheetCommand::SetRowLayout {
                sheet_id,
                start_row,
                end_row,
                height,
                reset_height,
                hidden,
            } => {
                let index = sheet_index(&self.model, &sheet_id)?;
                let before = self.model.sheets[index].clone();
                if start_row > end_row
                    || end_row >= 1_048_576
                    || end_row - start_row >= 10_000
                    || height.is_some_and(|h| !h.is_finite() || h <= 0.0 || h > 409.5)
                    || (height.is_some() && reset_height)
                {
                    return Err(SpreadsheetEngineError::InvalidMutation(
                        "行范围或行高无效".into(),
                    ));
                }
                let mut after = before.clone();
                let mut layout: std::collections::BTreeMap<_, _> = after
                    .metadata
                    .row_layout
                    .into_iter()
                    .map(|entry| (entry.row, entry))
                    .collect();
                for row in start_row..=end_row {
                    let entry = layout.entry(row).or_insert(oo_schema::SheetRowLayout {
                        row,
                        height: None,
                        hidden: false,
                    });
                    if reset_height {
                        entry.height = None;
                    }
                    if let Some(value) = height {
                        entry.height = Some(value);
                    }
                    if let Some(value) = hidden {
                        entry.hidden = value;
                    }
                }
                after.metadata.row_layout = layout
                    .into_values()
                    .filter(|entry| entry.hidden || entry.height.is_some())
                    .collect();
                if let (Some(count), Some(last)) =
                    (after.metadata.row_count, after.metadata.row_layout.last())
                {
                    after.metadata.row_count = Some(count.max(last.row + 1));
                }
                if after == before {
                    return Ok(());
                }
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
            SpreadsheetCommand::InsertRows {
                sheet_id,
                at,
                count,
            } => {
                if count == 0 {
                    return Ok(());
                }
                self.structural_edit(
                    sheet_id,
                    SheetAxis::Row,
                    RemapOp::Insert(at, count),
                    mutations,
                    invalidation,
                )?;
            }
            SpreadsheetCommand::DeleteRows {
                sheet_id,
                at,
                count,
            } => {
                if count == 0 {
                    return Ok(());
                }
                self.structural_edit(
                    sheet_id,
                    SheetAxis::Row,
                    RemapOp::Delete(at, count),
                    mutations,
                    invalidation,
                )?;
            }
            SpreadsheetCommand::InsertColumns {
                sheet_id,
                at,
                count,
            } => {
                if count == 0 {
                    return Ok(());
                }
                self.structural_edit(
                    sheet_id,
                    SheetAxis::Column,
                    RemapOp::Insert(at, count),
                    mutations,
                    invalidation,
                )?;
            }
            SpreadsheetCommand::DeleteColumns {
                sheet_id,
                at,
                count,
            } => {
                if count == 0 {
                    return Ok(());
                }
                self.structural_edit(
                    sheet_id,
                    SheetAxis::Column,
                    RemapOp::Delete(at, count),
                    mutations,
                    invalidation,
                )?;
            }
            SpreadsheetCommand::MergeCells { sheet_id, range } => {
                if range.start_row > range.end_row || range.start_column > range.end_column {
                    return Err(SpreadsheetEngineError::InvalidMutation(format!(
                        "合并区域起点不得超过终点：{range:?}"
                    )));
                }
                let index = sheet_index(&self.model, &sheet_id)?;
                // Reject any overlap with an existing merged region; merged
                // regions must stay a disjoint partition of the sheet.
                if self.model.sheets[index]
                    .metadata
                    .merged_ranges
                    .iter()
                    .any(|existing| ranges_overlap(*existing, range))
                {
                    return Err(SpreadsheetEngineError::MergeOverlap);
                }
                let mut after = self.model.sheets[index].clone();
                // Excel drops the content of every non-anchor cell in the
                // region; the anchor keeps its value/formula/style.
                after.cells.retain(|cell| {
                    !inside_range(cell.row, cell.column, range)
                        || (cell.row == range.start_row && cell.column == range.start_column)
                });
                after.metadata.merged_ranges.push(range);
                after.metadata.merged_ranges.sort_by_key(|existing| {
                    (
                        existing.start_row,
                        existing.start_column,
                        existing.end_row,
                        existing.end_column,
                    )
                });
                self.emit_sheet_change(sheet_id, after, mutations, invalidation)?;
            }
            SpreadsheetCommand::UnmergeCells { sheet_id, range } => {
                let index = sheet_index(&self.model, &sheet_id)?;
                let Some(position) = self.model.sheets[index]
                    .metadata
                    .merged_ranges
                    .iter()
                    .position(|existing| *existing == range)
                else {
                    return Err(SpreadsheetEngineError::MergeUnknown);
                };
                let mut after = self.model.sheets[index].clone();
                after.metadata.merged_ranges.remove(position);
                self.emit_sheet_change(sheet_id, after, mutations, invalidation)?;
            }
            SpreadsheetCommand::SortRange {
                sheet_id,
                range,
                keys,
            } => {
                if keys.is_empty() {
                    return Err(SpreadsheetEngineError::InvalidSortKeys);
                }
                if range.start_row > range.end_row || range.start_column > range.end_column {
                    return Err(SpreadsheetEngineError::InvalidMutation(format!(
                        "排序区域起点不得超过终点：{range:?}"
                    )));
                }
                let index = sheet_index(&self.model, &sheet_id)?;
                let after = sort_sheet_region(&self.model.sheets[index], range, &keys)?;
                self.emit_sheet_change(sheet_id, after, mutations, invalidation)?;
            }
            SpreadsheetCommand::UpsertFilterColumn {
                sheet_id,
                column,
                predicate,
            } => {
                let index = sheet_index(&self.model, &sheet_id)?;
                let Some(filter) = self.model.sheets[index].metadata.auto_filter.as_ref() else {
                    return Err(SpreadsheetEngineError::InvalidMutation(format!(
                        "sheet {sheet_id} 未启用筛选，请先 setAutoFilter"
                    )));
                };
                if column < filter.range.start_column || column > filter.range.end_column {
                    return Err(SpreadsheetEngineError::InvalidMutation(format!(
                        "筛选列 {column} 不在筛选范围 [{}, {}] 内",
                        filter.range.start_column, filter.range.end_column
                    )));
                }
                let before = filter.columns.clone();
                let mut after = before.clone();
                match after.iter_mut().find(|entry| entry.column == column) {
                    Some(entry) => entry.predicate = predicate,
                    None => after.push(oo_schema::FilterColumn { column, predicate }),
                }
                if before == after {
                    return Ok(());
                }
                let mutation = SpreadsheetMutation::FilterColumnsChanged {
                    sheet_id: sheet_id.clone(),
                    before,
                    after,
                };
                apply_mutation(&mut self.model, &mutation)?;
                mutations.push(mutation);
                invalidation.changed_containers.push(sheet_ref(sheet_id));
            }
            SpreadsheetCommand::ClearFilterColumn { sheet_id, column } => {
                let index = sheet_index(&self.model, &sheet_id)?;
                let Some(filter) = self.model.sheets[index].metadata.auto_filter.as_ref() else {
                    return Ok(());
                };
                let before = filter.columns.clone();
                let after: Vec<oo_schema::FilterColumn> = match column {
                    Some(target) => before
                        .iter()
                        .filter(|entry| entry.column != target)
                        .cloned()
                        .collect(),
                    None => Vec::new(),
                };
                if before == after {
                    return Ok(());
                }
                let mutation = SpreadsheetMutation::FilterColumnsChanged {
                    sheet_id: sheet_id.clone(),
                    before,
                    after,
                };
                apply_mutation(&mut self.model, &mutation)?;
                mutations.push(mutation);
                invalidation.changed_containers.push(sheet_ref(sheet_id));
            }
            SpreadsheetCommand::SetCalculationMode { calculation_mode } => {
                let before = self.model.metadata.calculation_mode;
                if before == calculation_mode {
                    return Ok(());
                }
                let mutation = SpreadsheetMutation::CalculationModeChanged {
                    before,
                    after: calculation_mode,
                };
                apply_mutation(&mut self.model, &mutation)?;
                mutations.push(mutation);
                invalidation.changed_containers.push(EntityRef {
                    entity_type: "spreadsheet.workbook".into(),
                    entity_id: "workbook".into(),
                });
            }
            SpreadsheetCommand::SetRowDimensions { sheet_id, rows } => {
                let index = sheet_index(&self.model, &sheet_id)?;
                let sheet = &self.model.sheets[index];
                if let Some(count) = rows {
                    if sheet.metadata.freeze.rows > count {
                        return Err(SpreadsheetEngineError::InvalidMutation(format!(
                            "行数 {count} 小于冻结行数 {}",
                            sheet.metadata.freeze.rows
                        )));
                    }
                }
                let before = sheet.metadata.row_count;
                if before == rows {
                    return Ok(());
                }
                let mutation = SpreadsheetMutation::RowDimensionsChanged {
                    sheet_id: sheet_id.clone(),
                    before,
                    after: rows,
                };
                apply_mutation(&mut self.model, &mutation)?;
                mutations.push(mutation);
                invalidation.changed_containers.push(sheet_ref(sheet_id));
            }
            SpreadsheetCommand::SetColumnDimensions { sheet_id, columns } => {
                let index = sheet_index(&self.model, &sheet_id)?;
                let sheet = &self.model.sheets[index];
                if let Some(count) = columns {
                    if sheet.metadata.freeze.columns > count {
                        return Err(SpreadsheetEngineError::InvalidMutation(format!(
                            "列数 {count} 小于冻结列数 {}",
                            sheet.metadata.freeze.columns
                        )));
                    }
                }
                let before = sheet.metadata.column_count;
                if before == columns {
                    return Ok(());
                }
                let mutation = SpreadsheetMutation::ColumnDimensionsChanged {
                    sheet_id: sheet_id.clone(),
                    before,
                    after: columns,
                };
                apply_mutation(&mut self.model, &mutation)?;
                mutations.push(mutation);
                invalidation.changed_containers.push(sheet_ref(sheet_id));
            }
            SpreadsheetCommand::UpsertConditionalFormat { sheet_id, rule } => {
                if rule.id.trim().is_empty() {
                    return Err(SpreadsheetEngineError::InvalidMutation(
                        "条件格式规则 id 不能为空".into(),
                    ));
                }
                let index = sheet_index(&self.model, &sheet_id)?;
                let previous = self.model.sheets[index]
                    .metadata
                    .conditional_formats
                    .iter()
                    .find(|existing| existing.id == rule.id)
                    .cloned();
                let mutation = SpreadsheetMutation::ConditionalFormatUpserted {
                    sheet_id: sheet_id.clone(),
                    rule,
                    previous,
                };
                apply_mutation(&mut self.model, &mutation)?;
                mutations.push(mutation);
                invalidation.changed_containers.push(sheet_ref(sheet_id));
            }
            SpreadsheetCommand::DeleteConditionalFormat { sheet_id, rule_id } => {
                let index = sheet_index(&self.model, &sheet_id)?;
                let Some(removed) = self.model.sheets[index]
                    .metadata
                    .conditional_formats
                    .iter()
                    .find(|rule| rule.id == rule_id)
                    .cloned()
                else {
                    return Ok(());
                };
                let mutation = SpreadsheetMutation::ConditionalFormatRemoved {
                    sheet_id: sheet_id.clone(),
                    rule_id,
                    removed,
                };
                apply_mutation(&mut self.model, &mutation)?;
                mutations.push(mutation);
                invalidation.changed_containers.push(sheet_ref(sheet_id));
            }
            SpreadsheetCommand::UpsertDataValidation { sheet_id, rule } => {
                if rule.id.trim().is_empty() {
                    return Err(SpreadsheetEngineError::InvalidMutation(
                        "数据校验规则 id 不能为空".into(),
                    ));
                }
                let index = sheet_index(&self.model, &sheet_id)?;
                let previous = self.model.sheets[index]
                    .metadata
                    .data_validations
                    .iter()
                    .find(|existing| existing.id == rule.id)
                    .cloned();
                let mutation = SpreadsheetMutation::DataValidationUpserted {
                    sheet_id: sheet_id.clone(),
                    rule,
                    previous,
                };
                apply_mutation(&mut self.model, &mutation)?;
                mutations.push(mutation);
                invalidation.changed_containers.push(sheet_ref(sheet_id));
            }
            SpreadsheetCommand::DeleteDataValidation { sheet_id, rule_id } => {
                let index = sheet_index(&self.model, &sheet_id)?;
                let Some(removed) = self.model.sheets[index]
                    .metadata
                    .data_validations
                    .iter()
                    .find(|rule| rule.id == rule_id)
                    .cloned()
                else {
                    return Ok(());
                };
                let mutation = SpreadsheetMutation::DataValidationRemoved {
                    sheet_id: sheet_id.clone(),
                    rule_id,
                    removed,
                };
                apply_mutation(&mut self.model, &mutation)?;
                mutations.push(mutation);
                invalidation.changed_containers.push(sheet_ref(sheet_id));
            }
            SpreadsheetCommand::FormatRange {
                sheet_id,
                range,
                style,
                fields,
                row_pattern,
            } => {
                if range.start_row > range.end_row || range.start_column > range.end_column {
                    return Err(SpreadsheetEngineError::InvalidMutation(format!(
                        "格式化区域起点不得超过终点：{range:?}"
                    )));
                }
                let index = sheet_index(&self.model, &sheet_id)?;
                let before = range_cells(&self.model.sheets[index], range);
                let materialize_blanks = range_cell_count(range) <= MAX_RANGE_MATERIALIZE;
                let mut after: Vec<CellModel> = before
                    .iter()
                    .map(|cell| {
                        let mut styled = cell.clone();
                        if row_pattern.is_some_and(|pattern| !pattern.matches(range, cell.row)) {
                            return styled;
                        }
                        let base = cell.style.clone().unwrap_or_default();
                        let next = apply_style_fields(
                            &base,
                            &style,
                            fields.as_deref(),
                            range,
                            cell.row,
                            cell.column,
                        );
                        if next != base || fields.is_none() {
                            styled.style = Some(next);
                        }
                        styled
                    })
                    .collect();
                if materialize_blanks {
                    let existing = before
                        .iter()
                        .map(|cell| (cell.row, cell.column))
                        .collect::<std::collections::HashSet<_>>();
                    for row in range.start_row..=range.end_row {
                        if row_pattern.is_some_and(|pattern| !pattern.matches(range, row)) {
                            continue;
                        }
                        for column in range.start_column..=range.end_column {
                            if existing.contains(&(row, column)) {
                                continue;
                            }
                            let next = apply_style_fields(
                                &CellStyle::default(),
                                &style,
                                fields.as_deref(),
                                range,
                                row,
                                column,
                            );
                            if fields.is_some() && next == CellStyle::default() {
                                continue;
                            }
                            after.push(CellModel {
                                row,
                                column,
                                value: None,
                                formula: None,
                                attrs: Map::new(),
                                style: Some(next),
                            });
                        }
                    }
                }
                self.emit_range_change(sheet_id, range, before, after, mutations, invalidation)?;
            }
            SpreadsheetCommand::ClearRange {
                sheet_id,
                range,
                mode,
            } => {
                if range.start_row > range.end_row || range.start_column > range.end_column {
                    return Err(SpreadsheetEngineError::InvalidMutation(format!(
                        "清除区域起点不得超过终点：{range:?}"
                    )));
                }
                let index = sheet_index(&self.model, &sheet_id)?;
                let before = range_cells(&self.model.sheets[index], range);
                let after: Vec<CellModel> = match mode {
                    ClearRangeMode::All => Vec::new(),
                    // 只清内容：无样式的格子直接消失，有样式的保留样式壳。
                    ClearRangeMode::Contents => before
                        .iter()
                        .filter_map(|cell| {
                            let styled = cell.style.is_some();
                            if !styled {
                                return None;
                            }
                            let mut cleared = cell.clone();
                            cleared.value = None;
                            cleared.formula = None;
                            Some(cleared)
                        })
                        .collect(),
                    // 只清格式：保留值/公式的格子，剥掉样式壳后无内容的格子消失。
                    ClearRangeMode::Formats => before
                        .iter()
                        .filter_map(|cell| {
                            let mut stripped = cell.clone();
                            stripped.style = None;
                            let has_content = stripped.value.is_some()
                                || stripped.formula.is_some()
                                || !stripped.attrs.is_empty();
                            has_content.then_some(stripped)
                        })
                        .collect(),
                };
                self.emit_range_change(sheet_id, range, before, after, mutations, invalidation)?;
            }
            SpreadsheetCommand::ReplaceRange {
                sheet_id,
                range,
                search,
                replace,
                match_case,
            } => {
                if search.is_empty() {
                    return Err(SpreadsheetEngineError::InvalidMutation(
                        "替换的查找文本不能为空".into(),
                    ));
                }
                if range.start_row > range.end_row || range.start_column > range.end_column {
                    return Err(SpreadsheetEngineError::InvalidMutation(format!(
                        "替换区域起点不得超过终点：{range:?}"
                    )));
                }
                let index = sheet_index(&self.model, &sheet_id)?;
                let before = range_cells(&self.model.sheets[index], range);
                // after 必须是范围内完整新状态：未匹配/非文本的格子原样保留，
                // 否则 RangeChanged 的"清空范围再写入"会把它们误删。
                let after: Vec<CellModel> = before
                    .iter()
                    .map(|cell| {
                        let Some(text) = cell.value.as_ref().and_then(Value::as_str) else {
                            return cell.clone();
                        };
                        let updated = if match_case {
                            text.contains(&search)
                                .then(|| text.replace(&search, &replace))
                        } else {
                            replace_case_insensitive(text, &search, &replace)
                        };
                        updated
                            .map(|text| {
                                let mut updated = cell.clone();
                                updated.value = Some(Value::String(text));
                                updated
                            })
                            .unwrap_or_else(|| cell.clone())
                    })
                    .collect();
                self.emit_range_change(sheet_id, range, before, after, mutations, invalidation)?;
            }
            SpreadsheetCommand::PasteRange {
                sheet_id,
                start_row,
                start_column,
                row_count,
                column_count,
                cells,
                mode,
                source_origin,
            } => {
                let range = checked_paste_range(
                    &self.model,
                    &sheet_id,
                    start_row,
                    start_column,
                    row_count,
                    column_count,
                )?;
                validate_clipboard_cells(&cells, row_count, column_count)?;
                if source_origin.is_some_and(|origin| {
                    origin.row.checked_add(row_count - 1).is_none()
                        || origin.column.checked_add(column_count - 1).is_none()
                }) {
                    return Err(SpreadsheetEngineError::InvalidMutation(
                        "剪贴板源坐标溢出".into(),
                    ));
                }
                let prepared = cells
                    .into_iter()
                    .map(|cell| {
                        let source = source_origin.map(|origin| SpreadsheetClipboardOrigin {
                            row: origin.row + cell.row_offset,
                            column: origin.column + cell.column_offset,
                        });
                        (cell, source)
                    })
                    .collect();
                let (before, after) =
                    build_paste_after(&self.model, &sheet_id, range, prepared, mode)?;
                self.emit_range_change(sheet_id, range, before, after, mutations, invalidation)?;
            }
            SpreadsheetCommand::FillRange {
                source_sheet_id,
                source_range,
                destination_sheet_id,
                destination_range,
                mode,
            } => {
                validate_grid_range(source_range, "填充源区域")?;
                validate_grid_range(destination_range, "填充目标区域")?;
                if range_cell_count(destination_range) > MAX_PASTE_CELLS {
                    return Err(SpreadsheetEngineError::InvalidMutation(format!(
                        "填充目标超过 {MAX_PASTE_CELLS} 个单元格"
                    )));
                }
                let source_sheet = sheet_at(&self.model, &source_sheet_id)?;
                let source_rows = source_range.end_row - source_range.start_row + 1;
                let source_columns = source_range.end_column - source_range.start_column + 1;
                let source_cells: std::collections::HashMap<_, _> =
                    range_cells(source_sheet, source_range)
                        .into_iter()
                        .map(|cell| {
                            (
                                (
                                    cell.row - source_range.start_row,
                                    cell.column - source_range.start_column,
                                ),
                                cell,
                            )
                        })
                        .collect();
                let mut prepared = Vec::new();
                for row in destination_range.start_row..=destination_range.end_row {
                    for column in destination_range.start_column..=destination_range.end_column {
                        let source_offset = (
                            (row - destination_range.start_row) % source_rows,
                            (column - destination_range.start_column) % source_columns,
                        );
                        let Some(source) = source_cells.get(&source_offset) else {
                            continue;
                        };
                        prepared.push((
                            SpreadsheetClipboardCell {
                                row_offset: row - destination_range.start_row,
                                column_offset: column - destination_range.start_column,
                                value: source.value.clone(),
                                formula: source.formula.clone(),
                                attrs: source.attrs.clone(),
                                style: source.style.clone(),
                            },
                            Some(SpreadsheetClipboardOrigin {
                                row: source.row,
                                column: source.column,
                            }),
                        ));
                    }
                }
                checked_paste_range(
                    &self.model,
                    &destination_sheet_id,
                    destination_range.start_row,
                    destination_range.start_column,
                    destination_range.end_row - destination_range.start_row + 1,
                    destination_range.end_column - destination_range.start_column + 1,
                )?;
                let (before, after) = build_paste_after(
                    &self.model,
                    &destination_sheet_id,
                    destination_range,
                    prepared,
                    mode,
                )?;
                self.emit_range_change(
                    destination_sheet_id,
                    destination_range,
                    before,
                    after,
                    mutations,
                    invalidation,
                )?;
            }
            SpreadsheetCommand::SetFreezePane {
                sheet_id,
                rows,
                columns,
            } => {
                let index = sheet_index(&self.model, &sheet_id)?;
                let sheet = &self.model.sheets[index];
                if let Some(row_count) = sheet.metadata.row_count {
                    if rows > row_count {
                        return Err(SpreadsheetEngineError::InvalidMutation(format!(
                            "冻结行数 {rows} 超出工作表行数 {row_count}"
                        )));
                    }
                }
                if let Some(column_count) = sheet.metadata.column_count {
                    if columns > column_count {
                        return Err(SpreadsheetEngineError::InvalidMutation(format!(
                            "冻结列数 {columns} 超出工作表列数 {column_count}"
                        )));
                    }
                }
                let before = sheet.metadata.freeze;
                let after = FreezePane { rows, columns };
                if before == after {
                    return Ok(());
                }
                let mutation = SpreadsheetMutation::PaneChanged {
                    sheet_id: sheet_id.clone(),
                    before,
                    after,
                };
                apply_mutation(&mut self.model, &mutation)?;
                mutations.push(mutation);
                invalidation.changed_containers.push(sheet_ref(sheet_id));
            }
            SpreadsheetCommand::SetAutoFilter { sheet_id, range } => {
                let index = sheet_index(&self.model, &sheet_id)?;
                let before = self.model.sheets[index].metadata.auto_filter.clone();
                let after = range.map(|range| FilterSpec {
                    range,
                    columns: Vec::new(),
                });
                if before == after {
                    return Ok(());
                }
                let mutation = SpreadsheetMutation::FilterChanged {
                    sheet_id: sheet_id.clone(),
                    before,
                    after,
                };
                apply_mutation(&mut self.model, &mutation)?;
                mutations.push(mutation);
                invalidation.changed_containers.push(sheet_ref(sheet_id));
            }
        }
        normalize_invalidation(invalidation);
        Ok(())
    }

    /// Submits a structural sheet replacement as one reversible `SheetChanged`
    /// mutation. The whole sheet snapshot travels in the mutation so undo and a
    /// rebase never need to re-derive the removed rows/columns.
    fn emit_sheet_change(
        &mut self,
        sheet_id: String,
        after: SheetModel,
        mutations: &mut Vec<SpreadsheetMutation>,
        invalidation: &mut Invalidation,
    ) -> Result<(), SpreadsheetEngineError> {
        let index = sheet_index(&self.model, &sheet_id)?;
        let before = self.model.sheets[index].clone();
        if before == after {
            return Ok(());
        }
        let mutation = SpreadsheetMutation::SheetChanged {
            sheet_id: sheet_id.clone(),
            index,
            before: Some(before),
            after: Some(after),
        };
        apply_mutation(&mut self.model, &mutation)?;
        mutations.push(mutation);
        invalidation.changed_containers.push(sheet_ref(sheet_id));
        invalidation.structure_changed = true;
        Ok(())
    }

    /// Submits one range-level mutation (M1-S journal compression). A no-op
    /// range (before == after) is swallowed here so it never becomes an
    /// eventless revision.
    fn emit_range_change(
        &mut self,
        sheet_id: String,
        range: GridRange,
        before: Vec<CellModel>,
        after: Vec<CellModel>,
        mutations: &mut Vec<SpreadsheetMutation>,
        invalidation: &mut Invalidation,
    ) -> Result<(), SpreadsheetEngineError> {
        if before == after {
            return Ok(());
        }
        let mutation = SpreadsheetMutation::RangeChanged {
            sheet_id: sheet_id.clone(),
            range,
            before,
            after,
        };
        apply_mutation(&mut self.model, &mutation)?;
        mutations.push(mutation);
        invalidation
            .changed_containers
            .push(sheet_ref(sheet_id.clone()));
        // 范围整体作为一个失效实体，避免逐格生成几十万个 EntityRef。
        invalidation.changed_entities.push(EntityRef {
            entity_type: "spreadsheet.range".into(),
            entity_id: format!(
                "{}:{}:{}:{}:{}",
                sheet_id, range.start_row, range.start_column, range.end_row, range.end_column
            ),
        });
        Ok(())
    }

    fn rollback(&mut self, mutations: &[SpreadsheetMutation]) {
        for mutation in mutations.iter().rev() {
            let inverse = self.inverse_with_model(mutation);
            let _ = apply_mutation(&mut self.model, &inverse);
        }
    }

    /// 求逆辅助：实体级 upsert 的 previous 已内嵌于 mutation（apply 时捕获），
    /// 这里保留入口以备未来需要模型感知的变体；其余直接走 `inverse`。
    fn inverse_with_model(&self, mutation: &SpreadsheetMutation) -> SpreadsheetMutation {
        let _ = &self.model;
        mutation.inverse()
    }

    /// One atomic row/column insert or delete on `sheet_id`.
    ///
    /// Emits at most one `SheetChanged` mutation per affected sheet:
    /// - the edited sheet shifts cells, merged ranges, the frozen band, the
    ///   four rule ranges (filter/sort/conditional format/data validation)
    ///   and rewrites every formula reference that points into the moved band;
    /// - every other sheet whose formulas reference the edited sheet gets its
    ///   formula text rewritten (cross-sheet references shift with the grid).
    fn structural_edit(
        &mut self,
        sheet_id: String,
        axis: SheetAxis,
        op: RemapOp,
        mutations: &mut Vec<SpreadsheetMutation>,
        invalidation: &mut Invalidation,
    ) -> Result<(), SpreadsheetEngineError> {
        let lookup = rewrite_sheet_lookup(&self.model);
        let sheet = sheet_at(&self.model, &sheet_id)?.clone();
        let after = remap_sheet(&sheet, axis, op, &sheet_id, &lookup)?;
        self.emit_sheet_change(sheet_id.clone(), after, mutations, invalidation)?;

        let rewrite_axis = match axis {
            SheetAxis::Row => RewriteAxis::Row,
            SheetAxis::Column => RewriteAxis::Column,
        };
        let rewrite_op = match op {
            RemapOp::Insert(at, count) => RewriteOp::Insert { at, count },
            RemapOp::Delete(at, count) => RewriteOp::Delete { at, count },
        };
        let other_sheet_ids: Vec<String> = self
            .model
            .sheets
            .iter()
            .filter(|sheet| sheet.id != sheet_id)
            .map(|sheet| sheet.id.clone())
            .collect();
        for other_id in other_sheet_ids {
            let Some(current) = self.model.sheets.iter().find(|sheet| sheet.id == other_id) else {
                continue;
            };
            let Some(rewritten) = rewrite_foreign_sheet_formulas(
                current,
                &sheet_id,
                &lookup,
                rewrite_axis,
                rewrite_op,
            ) else {
                continue;
            };
            self.emit_sheet_change(other_id, rewritten, mutations, invalidation)?;
        }
        let named_before = self.model.metadata.named_ranges.clone();
        let named_after: Vec<_> = named_before
            .iter()
            .filter_map(|named| {
                if named.sheet_id != sheet_id {
                    return Some(named.clone());
                }
                remap_grid_range(named.range, axis, op).map(|range| {
                    let mut named = named.clone();
                    named.range = range;
                    named
                })
            })
            .collect();
        if named_before != named_after {
            let mutation = SpreadsheetMutation::NamedRangesChanged {
                before: named_before,
                after: named_after,
            };
            apply_mutation(&mut self.model, &mutation)?;
            mutations.push(mutation);
            invalidation.changed_containers.push(workbook_ref());
        }
        Ok(())
    }

    /// Keeps the derived dependency cache consistent with the freshly applied
    /// `applied` mutations and appends the reverse-dependent closure to the
    /// invalidation so consumers learn which formula cells must re-render.
    ///
    /// - Any `SheetChanged` mutation (structural edit, sheet create/rename/
    ///   delete, metadata replace) shifts addresses en masse: the cache is
    ///   dropped and lazily rebuilt on the next need.
    /// - Pure `CellChanged` batches update the cache incrementally; the
    ///   affected topology then names every dependent formula cell.
    ///
    /// Enrichment is best-effort: a model containing formulas the index cannot
    /// parse keeps a working engine but no dependent reporting.
    fn sync_dependency_invalidation(
        &mut self,
        applied: &[SpreadsheetMutation],
        invalidation: &mut Invalidation,
    ) {
        let structural = applied.iter().any(|mutation| {
            matches!(
                mutation,
                SpreadsheetMutation::SheetChanged { .. }
                    // 范围清除可能删掉公式；范围替换不改公式但依赖图增量代价
                    // 高于整建，统一走重建以保正确。
                    | SpreadsheetMutation::RangeChanged { .. }
            )
        });
        let changed: Vec<CellAddress> = if structural {
            self.dependencies = None;
            Vec::new()
        } else {
            let mut changed: Vec<CellAddress> = applied
                .iter()
                .filter_map(|mutation| match mutation {
                    SpreadsheetMutation::CellChanged { address, .. } => Some(address.clone()),
                    SpreadsheetMutation::SheetChanged { .. }
                    | SpreadsheetMutation::RangeChanged { .. }
                    | SpreadsheetMutation::PaneChanged { .. }
                    | SpreadsheetMutation::FilterChanged { .. }
                    | SpreadsheetMutation::FilterColumnsChanged { .. }
                    | SpreadsheetMutation::CalculationModeChanged { .. }
                    | SpreadsheetMutation::NamedRangesChanged { .. }
                    | SpreadsheetMutation::ActiveSheetChanged { .. }
                    | SpreadsheetMutation::RowDimensionsChanged { .. }
                    | SpreadsheetMutation::ColumnDimensionsChanged { .. }
                    | SpreadsheetMutation::ConditionalFormatUpserted { .. }
                    | SpreadsheetMutation::ConditionalFormatRemoved { .. }
                    | SpreadsheetMutation::DataValidationUpserted { .. }
                    | SpreadsheetMutation::DataValidationRemoved { .. } => None,
                })
                .collect();
            changed.sort();
            changed.dedup();
            changed
        };
        if changed.is_empty() {
            return;
        }
        let Some(subgraph) = self.refresh_dependency_index(&changed) else {
            return;
        };
        for node in subgraph.nodes {
            invalidation.changed_entities.push(cell_ref(&node));
        }
        normalize_invalidation(invalidation);
    }

    fn refresh_dependency_index(
        &mut self,
        changed: &[CellAddress],
    ) -> Option<FormulaDependencySubgraph> {
        if let Some(index) = self.dependencies.as_mut() {
            return match index.update(&self.model, changed) {
                Ok(subgraph) => Some(subgraph),
                // The cache saw a formula it cannot parse; drop it instead of
                // keeping a partially mutated graph.
                Err(_) => {
                    self.dependencies = None;
                    None
                }
            };
        }
        let index = FormulaDependencyIndex::from_validated_model(&self.model).ok()?;
        let subgraph = index.affected_topology(changed);
        self.dependencies = Some(index);
        Some(subgraph)
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
        SpreadsheetMutation::RangeChanged {
            sheet_id,
            range,
            before: _,
            after,
        } => {
            let sheet = sheet_mut(model, sheet_id)?;
            // 先移除范围内全部已物化格子，再写入 after 快照。
            sheet
                .cells
                .retain(|cell| !inside_range(cell.row, cell.column, *range));
            sheet.cells.extend(after.iter().cloned());
            sheet.cells.sort_by_key(|cell| (cell.row, cell.column));
        }
        SpreadsheetMutation::PaneChanged {
            sheet_id,
            before: _,
            after,
        } => {
            let sheet = sheet_mut(model, sheet_id)?;
            sheet.metadata.freeze = *after;
        }
        SpreadsheetMutation::FilterChanged {
            sheet_id,
            before: _,
            after,
        } => {
            let sheet = sheet_mut(model, sheet_id)?;
            sheet.metadata.auto_filter = after.clone();
        }
        SpreadsheetMutation::FilterColumnsChanged {
            sheet_id,
            before: _,
            after,
        } => {
            let sheet = sheet_mut(model, sheet_id)?;
            if let Some(filter) = sheet.metadata.auto_filter.as_mut() {
                filter.columns = after.clone();
            } else {
                return Err(SpreadsheetEngineError::InvalidMutation(format!(
                    "sheet {sheet_id} 未启用筛选，无法应用筛选列变更"
                )));
            }
        }
        SpreadsheetMutation::CalculationModeChanged { before: _, after } => {
            model.metadata.calculation_mode = *after;
        }
        SpreadsheetMutation::NamedRangesChanged { before: _, after } => {
            model.metadata.named_ranges.clone_from(after);
        }
        SpreadsheetMutation::ActiveSheetChanged { before: _, after } => {
            model.metadata.active_sheet_id.clone_from(after);
        }
        SpreadsheetMutation::RowDimensionsChanged {
            sheet_id,
            before: _,
            after,
        } => {
            let sheet = sheet_mut(model, sheet_id)?;
            sheet.metadata.row_count = *after;
        }
        SpreadsheetMutation::ColumnDimensionsChanged {
            sheet_id,
            before: _,
            after,
        } => {
            let sheet = sheet_mut(model, sheet_id)?;
            sheet.metadata.column_count = *after;
        }
        SpreadsheetMutation::ConditionalFormatUpserted { sheet_id, rule, .. } => {
            let sheet = sheet_mut(model, sheet_id)?;
            match sheet
                .metadata
                .conditional_formats
                .iter_mut()
                .find(|existing| existing.id == rule.id)
            {
                Some(existing) => *existing = rule.clone(),
                None => sheet.metadata.conditional_formats.push(rule.clone()),
            }
        }
        SpreadsheetMutation::ConditionalFormatRemoved {
            sheet_id,
            rule_id,
            removed: _,
        } => {
            let sheet = sheet_mut(model, sheet_id)?;
            sheet
                .metadata
                .conditional_formats
                .retain(|rule| rule.id != *rule_id);
        }
        SpreadsheetMutation::DataValidationUpserted { sheet_id, rule, .. } => {
            let sheet = sheet_mut(model, sheet_id)?;
            match sheet
                .metadata
                .data_validations
                .iter_mut()
                .find(|existing| existing.id == rule.id)
            {
                Some(existing) => *existing = rule.clone(),
                None => sheet.metadata.data_validations.push(rule.clone()),
            }
        }
        SpreadsheetMutation::DataValidationRemoved {
            sheet_id,
            rule_id,
            removed: _,
        } => {
            let sheet = sheet_mut(model, sheet_id)?;
            sheet
                .metadata
                .data_validations
                .retain(|rule| rule.id != *rule_id);
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
    style: Option<CellStyle>,
) -> Option<CellModel> {
    let formula = formula.filter(|formula| !formula.trim().is_empty());
    if value.is_none() && formula.is_none() && attrs.is_empty() && style.is_none() {
        None
    } else {
        Some(CellModel {
            row,
            column,
            value,
            formula,
            attrs,
            style,
        })
    }
}

fn ranges_overlap(left: GridRange, right: GridRange) -> bool {
    left.start_row <= right.end_row
        && right.start_row <= left.end_row
        && left.start_column <= right.end_column
        && right.start_column <= left.end_column
}

fn inside_range(row: u32, column: u32, range: GridRange) -> bool {
    row >= range.start_row
        && row <= range.end_row
        && column >= range.start_column
        && column <= range.end_column
}

/// 范围内格子总数（含空格）；用于决定是否为格式化物化空白格。
fn range_cell_count(range: GridRange) -> u64 {
    (u64::from(range.end_row - range.start_row) + 1)
        * (u64::from(range.end_column - range.start_column) + 1)
}

/// 大范围格式化不物化空白格的阈值：超过它只格式化已物化格子，
/// 避免一次大区格式把稀疏 Grid 爆成稠密。
const MAX_RANGE_MATERIALIZE: u64 = 4_096;
const MAX_PASTE_CELLS: u64 = 100_000;
const MAX_SHEET_ROWS: u32 = 1_048_576;
const MAX_SHEET_COLUMNS: u32 = 16_384;

/// 收集范围内已物化格子的快照（engine 内部按坐标排序存储）。
fn range_cells(sheet: &SheetModel, range: GridRange) -> Vec<CellModel> {
    sheet
        .cells
        .iter()
        .filter(|cell| inside_range(cell.row, cell.column, range))
        .cloned()
        .collect()
}

fn validate_grid_range(range: GridRange, label: &str) -> Result<(), SpreadsheetEngineError> {
    if range.start_row > range.end_row || range.start_column > range.end_column {
        return Err(SpreadsheetEngineError::InvalidMutation(format!(
            "{label}起点不得超过终点"
        )));
    }
    Ok(())
}

fn checked_paste_range(
    model: &SpreadsheetModel,
    sheet_id: &str,
    start_row: u32,
    start_column: u32,
    row_count: u32,
    column_count: u32,
) -> Result<GridRange, SpreadsheetEngineError> {
    if row_count == 0 || column_count == 0 {
        return Err(SpreadsheetEngineError::InvalidMutation(
            "粘贴区域行列数必须大于零".into(),
        ));
    }
    let count = u64::from(row_count) * u64::from(column_count);
    if count > MAX_PASTE_CELLS {
        return Err(SpreadsheetEngineError::InvalidMutation(format!(
            "粘贴区域超过 {MAX_PASTE_CELLS} 个单元格"
        )));
    }
    let end_row = start_row
        .checked_add(row_count - 1)
        .ok_or_else(|| SpreadsheetEngineError::InvalidMutation("粘贴行坐标溢出".into()))?;
    let end_column = start_column
        .checked_add(column_count - 1)
        .ok_or_else(|| SpreadsheetEngineError::InvalidMutation("粘贴列坐标溢出".into()))?;
    let sheet = sheet_at(model, sheet_id)?;
    let row_limit = sheet
        .metadata
        .row_count
        .unwrap_or(MAX_SHEET_ROWS)
        .min(MAX_SHEET_ROWS);
    let column_limit = sheet
        .metadata
        .column_count
        .unwrap_or(MAX_SHEET_COLUMNS)
        .min(MAX_SHEET_COLUMNS);
    if end_row >= row_limit || end_column >= column_limit {
        return Err(SpreadsheetEngineError::InvalidMutation(
            "粘贴区域超出工作表边界".into(),
        ));
    }
    Ok(GridRange {
        start_row,
        start_column,
        end_row,
        end_column,
    })
}

fn validate_clipboard_cells(
    cells: &[SpreadsheetClipboardCell],
    row_count: u32,
    column_count: u32,
) -> Result<(), SpreadsheetEngineError> {
    let mut coordinates = std::collections::HashSet::new();
    for cell in cells {
        if cell.row_offset >= row_count
            || cell.column_offset >= column_count
            || !coordinates.insert((cell.row_offset, cell.column_offset))
        {
            return Err(SpreadsheetEngineError::InvalidMutation(
                "剪贴板单元格坐标越界或重复".into(),
            ));
        }
    }
    Ok(())
}

fn build_paste_after(
    model: &SpreadsheetModel,
    sheet_id: &str,
    range: GridRange,
    cells: Vec<(SpreadsheetClipboardCell, Option<SpreadsheetClipboardOrigin>)>,
    mode: SpreadsheetPasteMode,
) -> Result<(Vec<CellModel>, Vec<CellModel>), SpreadsheetEngineError> {
    let before = range_cells(sheet_at(model, sheet_id)?, range);
    let mut after: std::collections::BTreeMap<(u32, u32), CellModel> = before
        .iter()
        .filter_map(|cell| {
            let retained = match mode {
                SpreadsheetPasteMode::All => None,
                SpreadsheetPasteMode::Values => cell.style.clone().map(|style| CellModel {
                    row: cell.row,
                    column: cell.column,
                    value: None,
                    formula: None,
                    attrs: Map::new(),
                    style: Some(style),
                }),
                SpreadsheetPasteMode::Formats => {
                    let mut retained = cell.clone();
                    retained.style = None;
                    (retained.value.is_some()
                        || retained.formula.is_some()
                        || !retained.attrs.is_empty())
                    .then_some(retained)
                }
            };
            retained.map(|cell| ((cell.row, cell.column), cell))
        })
        .collect();

    for (source, source_coordinate) in cells {
        let row = range.start_row + source.row_offset;
        let column = range.start_column + source.column_offset;
        let existing_style = after
            .get(&(row, column))
            .and_then(|cell| cell.style.clone());
        let next = match mode {
            SpreadsheetPasteMode::All => {
                let formula = match (source.formula, source_coordinate) {
                    (Some(formula), Some(origin)) => Some(translate_formula_for_copy(
                        &formula,
                        i64::from(row) - i64::from(origin.row),
                        i64::from(column) - i64::from(origin.column),
                    )),
                    (formula, None) => formula,
                    (None, Some(_)) => None,
                };
                normalized_cell(
                    row,
                    column,
                    source.value,
                    formula,
                    source.attrs,
                    source.style,
                )
            }
            SpreadsheetPasteMode::Values => normalized_cell(
                row,
                column,
                source.value,
                None,
                source.attrs,
                existing_style,
            ),
            SpreadsheetPasteMode::Formats => {
                let existing = after.remove(&(row, column));
                normalized_cell(
                    row,
                    column,
                    existing.as_ref().and_then(|cell| cell.value.clone()),
                    existing.as_ref().and_then(|cell| cell.formula.clone()),
                    existing.map(|cell| cell.attrs).unwrap_or_default(),
                    source.style,
                )
            }
        };
        if let Some(cell) = next {
            let address = CellAddress {
                sheet_id: sheet_id.into(),
                row,
                column,
            };
            validate_against_rules(model, &address, &cell)?;
            after.insert((row, column), cell);
        } else {
            after.remove(&(row, column));
        }
    }
    Ok((before, after.into_values().collect()))
}

/// 大小写不敏感的查找替换，保留源文本的其余部分。
fn replace_case_insensitive(text: &str, search: &str, replace: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let needle = search.to_lowercase();
    if !lower.contains(&needle) {
        return None;
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut consumed = 0usize;
    while let Some(position) = lower[consumed..].find(&needle) {
        let absolute = consumed + position;
        out.push_str(&rest[..position]);
        out.push_str(replace);
        rest = &rest[position + search.len()..];
        consumed = absolute + search.len();
        if search.is_empty() {
            break;
        }
    }
    out.push_str(rest);
    Some(out)
}

/// Applies every data-validation rule covering the target address to the
/// cell about to be written. `CustomFormula` rules are deliberately skipped:
/// evaluating an arbitrary validation formula needs the full calculator and
/// is tracked separately; skipping is honest because the rule was never
/// evaluated anywhere else either.
fn validate_against_rules(
    model: &SpreadsheetModel,
    address: &CellAddress,
    cell: &CellModel,
) -> Result<(), SpreadsheetEngineError> {
    let Some(sheet) = model
        .sheets
        .iter()
        .find(|sheet| sheet.id == address.sheet_id)
    else {
        return Ok(());
    };
    for rule in &sheet.metadata.data_validations {
        if !inside_range(address.row, address.column, rule.range) {
            continue;
        }
        if let Some(message) =
            validation_violation(&rule.kind, rule.allow_blank, cell.value.as_ref())
        {
            return Err(SpreadsheetEngineError::DataValidationRejected {
                cell: address.clone(),
                rule_id: rule.id.clone(),
                message: rule.error_message.clone().unwrap_or(message),
            });
        }
    }
    Ok(())
}

/// Returns `Some(default_message)` when the value violates the rule.
fn validation_violation(
    kind: &oo_schema::DataValidationKind,
    allow_blank: bool,
    value: Option<&Value>,
) -> Option<String> {
    use oo_schema::DataValidationKind as Kind;
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return (!allow_blank).then(|| "该单元格不允许为空".into());
    };
    match kind {
        Kind::List(allowed) => {
            let text = value.as_str();
            if text.is_some_and(|text| allowed.contains(&text.to_string())) {
                None
            } else {
                Some("值不在允许的列表内".into())
            }
        }
        Kind::WholeNumber { min, max } => {
            let number = as_finite(value)?;
            if number.fract() != 0.0 {
                Some("必须输入整数".into())
            } else if number < *min as f64 || number > *max as f64 {
                Some(format!("整数必须在 {min} 到 {max} 之间"))
            } else {
                None
            }
        }
        Kind::Decimal { min, max } => {
            let number = as_finite(value)?;
            if number < *min || number > *max {
                Some(format!("数值必须在 {min} 到 {max} 之间"))
            } else {
                None
            }
        }
        Kind::Date {
            min_serial,
            max_serial,
        } => {
            let number = as_finite(value)?;
            if number < *min_serial || number > *max_serial {
                Some("日期超出允许范围".into())
            } else {
                None
            }
        }
        // A custom validation formula is never evaluated anywhere in the
        // pipeline today; enforcing it would fabricate semantics.
        Kind::CustomFormula(_) => None,
    }
}

fn as_finite(value: &Value) -> Option<f64> {
    value.as_f64().filter(|number| number.is_finite())
}

/// Reorders the rows that carry cells inside `range` by `keys`. Only occupied
/// rows participate (the permutation keeps their row positions), which keeps
/// the sort bounded by materialized data instead of the declared region size.
fn sort_sheet_region(
    sheet: &SheetModel,
    range: GridRange,
    keys: &[SortKey],
) -> Result<SheetModel, SpreadsheetEngineError> {
    let mut region_rows: Vec<u32> = sheet
        .cells
        .iter()
        .filter(|cell| inside_range(cell.row, cell.column, range))
        .map(|cell| cell.row)
        .collect();
    region_rows.sort_unstable();
    region_rows.dedup();
    if region_rows.len() <= 1 {
        // Nothing to permute; the caller reports NoChanges so no eventless
        // revision is created.
        return Ok(sheet.clone());
    }
    for row in &region_rows {
        if sheet.cells.iter().any(|cell| {
            cell.row == *row && inside_range(cell.row, cell.column, range) && cell.formula.is_some()
        }) {
            return Err(SpreadsheetEngineError::SortWithFormulas(CellAddress {
                sheet_id: sheet.id.clone(),
                row: *row,
                column: range.start_column,
            }));
        }
    }

    let value_rank = |row: u32, column: u32| -> (u8, f64, String, bool) {
        let value = sheet
            .cells
            .iter()
            .find(|cell| cell.row == row && cell.column == column)
            .and_then(|cell| cell.value.as_ref());
        match value {
            None | Some(Value::Null) => (3, 0.0, String::new(), false),
            Some(Value::Number(number)) => {
                (0, number.as_f64().unwrap_or(f64::NAN), String::new(), false)
            }
            Some(Value::String(text)) => (1, 0.0, text.clone(), false),
            Some(Value::Bool(flag)) => (2, 0.0, String::new(), *flag),
            Some(_) => (4, 0.0, String::new(), false),
        }
    };
    let compare_rows = |left: &u32, right: &u32| -> std::cmp::Ordering {
        for key in keys {
            let (left_rank, left_number, left_text, left_flag) = value_rank(*left, key.column);
            let (right_rank, right_number, right_text, right_flag) = value_rank(*right, key.column);
            // Blanks always sort last, independent of the key direction.
            let ordering = match (left_rank, right_rank) {
                (3, 3) => std::cmp::Ordering::Equal,
                (3, _) => std::cmp::Ordering::Greater,
                (_, 3) => std::cmp::Ordering::Less,
                _ => {
                    let base = match left_rank.cmp(&right_rank) {
                        std::cmp::Ordering::Equal => match left_rank {
                            0 => left_number
                                .partial_cmp(&right_number)
                                .unwrap_or(std::cmp::Ordering::Equal),
                            1 => left_text.cmp(&right_text),
                            2 => left_flag.cmp(&right_flag),
                            _ => std::cmp::Ordering::Equal,
                        },
                        other => other,
                    };
                    match key.direction {
                        oo_schema::SortDirection::Ascending => base,
                        oo_schema::SortDirection::Descending => base.reverse(),
                    }
                }
            };
            if ordering != std::cmp::Ordering::Equal {
                return ordering;
            }
        }
        left.cmp(right)
    };

    let mut ordered = region_rows.clone();
    ordered.sort_by(compare_rows);
    // Preserve the sort's stability: `sort_by` is stable, and `ordered` is a
    // permutation of the occupied row indices.
    let mut new_row_of: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    for (position, original_row) in ordered.iter().enumerate() {
        new_row_of.insert(*original_row, region_rows[position]);
    }
    let mut cells = sheet.cells.clone();
    for cell in &mut cells {
        if let Some(new_row) = new_row_of.get(&cell.row) {
            if inside_range(cell.row, cell.column, range) {
                cell.row = *new_row;
            }
        }
    }
    cells.sort_by_key(|cell| (cell.row, cell.column));
    Ok(SheetModel {
        id: sheet.id.clone(),
        name: sheet.name.clone(),
        cells,
        metadata: sheet.metadata.clone(),
    })
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

fn sheet_at<'a>(
    model: &'a SpreadsheetModel,
    id: &str,
) -> Result<&'a SheetModel, SpreadsheetEngineError> {
    let index = sheet_index(model, id)?;
    Ok(&model.sheets[index])
}

/// Which axis a structural edit shifts along.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SheetAxis {
    Row,
    Column,
}

/// The structural edit to apply along an axis.
#[derive(Debug, Clone, Copy)]
enum RemapOp {
    /// Insert `count` empty slots at 0-based position `at`.
    Insert(u32, u32),
    /// Delete the `count` slots in `[at, at + count)`.
    Delete(u32, u32),
}

/// Applies a row/column insert or delete to a sheet clone and returns the new
/// model. Cells and merged ranges are shifted/dropped, the frozen band is
/// enlarged or shrunk, the row/column dimension count follows the edit, the
/// filter/sort/conditional-format/validation ranges follow the same shift, and
/// every formula reference that points into the moved band is rewritten
/// (Excel semantics: swallowed references degrade to the `#REF!` literal).
fn remap_sheet(
    sheet: &SheetModel,
    axis: SheetAxis,
    op: RemapOp,
    edited_sheet_id: &str,
    lookup: &std::collections::HashMap<String, String>,
) -> Result<SheetModel, SpreadsheetEngineError> {
    let rewrite_axis = match axis {
        SheetAxis::Row => RewriteAxis::Row,
        SheetAxis::Column => RewriteAxis::Column,
    };
    let rewrite_op = match op {
        RemapOp::Insert(at, count) => RewriteOp::Insert { at, count },
        RemapOp::Delete(at, count) => RewriteOp::Delete { at, count },
    };
    let mut cells: Vec<CellModel> = Vec::with_capacity(sheet.cells.len());
    for cell in &sheet.cells {
        let position = match axis {
            SheetAxis::Row => cell.row,
            SheetAxis::Column => cell.column,
        };
        if let Some(position) = remap_position(position, op) {
            let mut remapped = cell.clone();
            match axis {
                SheetAxis::Row => remapped.row = position,
                SheetAxis::Column => remapped.column = position,
            }
            if let Some(formula) = remapped.formula.as_deref() {
                if let Some(rewritten) = rewrite_formula_references(
                    formula,
                    edited_sheet_id,
                    edited_sheet_id,
                    lookup,
                    rewrite_axis,
                    rewrite_op,
                ) {
                    remapped.formula = Some(rewritten);
                }
            }
            cells.push(remapped);
        }
    }
    cells.sort_by_key(|cell| (cell.row, cell.column));

    let mut metadata = sheet.metadata.clone();
    let mut merged_ranges = Vec::new();
    for range in &sheet.metadata.merged_ranges {
        if let Some(range) = remap_grid_range(*range, axis, op) {
            merged_ranges.push(range);
        }
    }
    metadata.merged_ranges = merged_ranges;

    // The four rule ranges follow the same shift as merged ranges; a rule
    // fully swallowed by a delete is dropped instead of keeping a dangling
    // range that no longer covers any cell.
    metadata.conditional_formats = metadata
        .conditional_formats
        .into_iter()
        .filter_map(|mut rule| {
            remap_grid_range(rule.range, axis, op).map(|range| {
                rule.range = range;
                rule
            })
        })
        .collect();
    metadata.data_validations = metadata
        .data_validations
        .into_iter()
        .filter_map(|mut rule| {
            remap_grid_range(rule.range, axis, op).map(|range| {
                rule.range = range;
                rule
            })
        })
        .collect();
    metadata.auto_filter = metadata.auto_filter.and_then(|mut filter| {
        remap_grid_range(filter.range, axis, op).map(|range| {
            filter.range = range;
            if axis == SheetAxis::Column {
                filter.columns = filter
                    .columns
                    .into_iter()
                    .filter_map(|mut column| match remap_position(column.column, op) {
                        Some(position) => {
                            column.column = position;
                            Some(column)
                        }
                        None => None,
                    })
                    .collect();
            }
            filter
        })
    });
    metadata.sort = metadata.sort.and_then(|mut sort| {
        remap_grid_range(sort.range, axis, op).map(|range| {
            sort.range = range;
            if axis == SheetAxis::Column {
                sort.keys = sort
                    .keys
                    .into_iter()
                    .filter_map(|mut key| match remap_position(key.column, op) {
                        Some(position) => {
                            key.column = position;
                            Some(key)
                        }
                        None => None,
                    })
                    .collect();
            }
            sort
        })
    });

    match axis {
        SheetAxis::Row => {
            metadata.freeze.rows = remap_freeze(metadata.freeze.rows, op);
            metadata.row_layout = metadata
                .row_layout
                .into_iter()
                .filter_map(|mut entry| {
                    entry.row = remap_position(entry.row, op)?;
                    Some(entry)
                })
                .collect();
        }
        SheetAxis::Column => metadata.freeze.columns = remap_freeze(metadata.freeze.columns, op),
    }
    match axis {
        SheetAxis::Row => {
            if let Some(count) = metadata.row_count {
                metadata.row_count = Some(remap_dimension(count, op));
            }
            // The frozen band must never exceed the live row dimension.
            if let Some(count) = metadata.row_count {
                metadata.freeze.rows = metadata.freeze.rows.min(count);
            }
        }
        SheetAxis::Column => {
            if let Some(count) = metadata.column_count {
                metadata.column_count = Some(remap_dimension(count, op));
            }
            if let Some(count) = metadata.column_count {
                metadata.freeze.columns = metadata.freeze.columns.min(count);
            }
        }
    }

    Ok(SheetModel {
        id: sheet.id.clone(),
        name: sheet.name.clone(),
        cells,
        metadata,
    })
}

/// Shifts a 0-based coordinate along an axis; `None` when the coordinate lives
/// inside a deleted band.
fn remap_position(position: u32, op: RemapOp) -> Option<u32> {
    match op {
        RemapOp::Insert(at, count) => Some(if position >= at {
            position + count
        } else {
            position
        }),
        RemapOp::Delete(at, count) => {
            if position >= at + count {
                Some(position - count)
            } else if position >= at {
                None
            } else {
                Some(position)
            }
        }
    }
}

/// Shifts/contracts an inclusive range along an axis; `None` when the range is
/// fully swallowed by a delete.
fn remap_grid_range(range: GridRange, axis: SheetAxis, op: RemapOp) -> Option<GridRange> {
    let (start, end) = match axis {
        SheetAxis::Row => (range.start_row, range.end_row),
        SheetAxis::Column => (range.start_column, range.end_column),
    };
    let (start, end) = remap_range_bounds(start, end, op)?;
    let (start, end) = clamp_range_bounds(start, end, op);
    match axis {
        SheetAxis::Row => Some(GridRange {
            start_row: start,
            end_row: end,
            start_column: range.start_column,
            end_column: range.end_column,
        }),
        SheetAxis::Column => Some(GridRange {
            start_row: range.start_row,
            end_row: range.end_row,
            start_column: start,
            end_column: end,
        }),
    }
}

/// Pure inclusive-range shift for one axis, using signed math so a delete can
/// contract a range that spans the removed band.
fn remap_range_bounds(start: u32, end: u32, op: RemapOp) -> Option<(u32, u32)> {
    let start = i64::from(start);
    let end = i64::from(end);
    match op {
        RemapOp::Insert(at, count) => {
            let at = i64::from(at);
            let count = i64::from(count);
            if start >= at {
                Some(((start + count) as u32, (end + count) as u32))
            } else if end >= at {
                Some((start as u32, (end + count) as u32))
            } else {
                Some((start as u32, end as u32))
            }
        }
        RemapOp::Delete(at, count) => {
            let at = i64::from(at);
            let count = i64::from(count);
            let band_start = at;
            let band_end = at + count;
            if start >= band_end {
                Some(((start - count) as u32, (end - count) as u32))
            } else if end < band_start {
                Some((start as u32, end as u32))
            } else if start >= band_start && end < band_end {
                None
            } else if start >= band_start {
                Some((band_start as u32, (end - count) as u32))
            } else if end >= band_end {
                Some((start as u32, (end - count) as u32))
            } else {
                Some((start as u32, (band_start - 1) as u32))
            }
        }
    }
}

/// Clamps a delete-contracted range so `start <= end` and never underflows.
fn clamp_range_bounds(start: u32, end: u32, _op: RemapOp) -> (u32, u32) {
    (start.min(end), end.max(start))
}

/// Grows/shrinks the frozen-band size for a structural edit.
fn remap_freeze(frozen: u32, op: RemapOp) -> u32 {
    match op {
        RemapOp::Insert(at, count) => {
            if at < frozen {
                frozen + count
            } else {
                frozen
            }
        }
        RemapOp::Delete(at, count) => {
            let removed_in_band = if at < frozen {
                (at + count).min(frozen) - at
            } else {
                0
            };
            frozen.saturating_sub(removed_in_band)
        }
    }
}

/// Adjusts the used-range dimension count for a structural edit.
fn remap_dimension(dimension: u32, op: RemapOp) -> u32 {
    match op {
        RemapOp::Insert(_, count) => dimension + count,
        RemapOp::Delete(_, count) => dimension.saturating_sub(count),
    }
}

/// Rewrites formulas in a sheet that is *not* the one being structurally
/// edited but whose cells reference the edited sheet (by id or by name).
/// Returns `None` when nothing in this sheet needs to change.
fn rewrite_foreign_sheet_formulas(
    sheet: &SheetModel,
    edited_sheet_id: &str,
    lookup: &std::collections::HashMap<String, String>,
    axis: RewriteAxis,
    op: RewriteOp,
) -> Option<SheetModel> {
    let mut changed = false;
    let mut cells = sheet.cells.clone();
    for cell in &mut cells {
        let Some(formula) = cell.formula.as_deref() else {
            continue;
        };
        if let Some(rewritten) =
            rewrite_formula_references(formula, &sheet.id, edited_sheet_id, lookup, axis, op)
        {
            cell.formula = Some(rewritten);
            changed = true;
        }
    }
    changed.then(|| SheetModel {
        id: sheet.id.clone(),
        name: sheet.name.clone(),
        cells,
        metadata: sheet.metadata.clone(),
    })
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

fn workbook_ref() -> EntityRef {
    EntityRef {
        entity_type: "spreadsheet.workbook".into(),
        entity_id: "workbook".into(),
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
    #[error("合并区域与现有合并单元格重叠")]
    MergeOverlap,
    #[error("指定区域不是现有合并单元格")]
    MergeUnknown,
    #[error("排序键不能为空")]
    InvalidSortKeys,
    #[error("排序区域包含公式单元格 {0:?}，移动公式会改变其语义")]
    SortWithFormulas(CellAddress),
    #[error("单元格 {cell:?} 违反数据校验规则 {rule_id}：{message}")]
    DataValidationRejected {
        cell: CellAddress,
        rule_id: String,
        message: String,
    },
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
                SpreadsheetMutation::RangeChanged {
                    sheet_id, range, ..
                } => {
                    builder
                        .invalidation
                        .changed_containers
                        .push(sheet_ref(sheet_id.clone()));
                    builder.invalidation.changed_entities.push(EntityRef {
                        entity_type: "spreadsheet.range".into(),
                        entity_id: format!(
                            "{}:{}:{}:{}:{}",
                            sheet_id,
                            range.start_row,
                            range.start_column,
                            range.end_row,
                            range.end_column
                        ),
                    });
                }
                SpreadsheetMutation::PaneChanged { sheet_id, .. }
                | SpreadsheetMutation::FilterChanged { sheet_id, .. }
                | SpreadsheetMutation::FilterColumnsChanged { sheet_id, .. }
                | SpreadsheetMutation::RowDimensionsChanged { sheet_id, .. }
                | SpreadsheetMutation::ColumnDimensionsChanged { sheet_id, .. }
                | SpreadsheetMutation::ConditionalFormatUpserted { sheet_id, .. }
                | SpreadsheetMutation::ConditionalFormatRemoved { sheet_id, .. }
                | SpreadsheetMutation::DataValidationUpserted { sheet_id, .. }
                | SpreadsheetMutation::DataValidationRemoved { sheet_id, .. } => {
                    builder
                        .invalidation
                        .changed_containers
                        .push(sheet_ref(sheet_id.clone()));
                }
                SpreadsheetMutation::CalculationModeChanged { .. }
                | SpreadsheetMutation::NamedRangesChanged { .. }
                | SpreadsheetMutation::ActiveSheetChanged { .. } => {
                    builder.invalidation.changed_containers.push(EntityRef {
                        entity_type: "spreadsheet.workbook".into(),
                        entity_id: "workbook".into(),
                    });
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
    use oo_schema::FreezePane;

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
    fn row_layout_can_extend_an_imported_used_range() {
        let mut engine = engine();
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::SetRowDimensions {
                    sheet_id: "sheet-1".into(),
                    rows: Some(3),
                }],
            })
            .unwrap();
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: engine.revision(),
                commands: vec![SpreadsheetCommand::SetRowLayout {
                    sheet_id: "sheet-1".into(),
                    start_row: 6,
                    end_row: 6,
                    height: Some(42.0),
                    reset_height: false,
                    hidden: None,
                }],
            })
            .unwrap();
        assert_eq!(engine.model().sheets[0].metadata.row_count, Some(7));
        engine.undo().unwrap();
        assert_eq!(engine.model().sheets[0].metadata.row_count, Some(3));
    }

    #[test]
    fn row_layout_is_undoable_validated_and_follows_structural_edits() {
        let mut engine = engine();
        let layout = SpreadsheetCommand::SetRowLayout {
            sheet_id: "sheet-1".into(),
            start_row: 1,
            end_row: 2,
            height: Some(42.0),
            reset_height: false,
            hidden: Some(true),
        };
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![layout],
            })
            .unwrap();
        assert_eq!(engine.model().sheets[0].metadata.row_layout.len(), 2);
        engine.undo().unwrap();
        assert!(engine.model().sheets[0].metadata.row_layout.is_empty());
        engine.redo().unwrap();
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: engine.revision(),
                commands: vec![SpreadsheetCommand::InsertRows {
                    sheet_id: "sheet-1".into(),
                    at: 1,
                    count: 3,
                }],
            })
            .unwrap();
        assert_eq!(engine.model().sheets[0].metadata.row_layout[0].row, 4);
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: engine.revision(),
                commands: vec![SpreadsheetCommand::DeleteRows {
                    sheet_id: "sheet-1".into(),
                    at: 4,
                    count: 1,
                }],
            })
            .unwrap();
        assert_eq!(engine.model().sheets[0].metadata.row_layout.len(), 1);
        assert_eq!(engine.model().sheets[0].metadata.row_layout[0].row, 4);
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: engine.revision(),
                commands: vec![SpreadsheetCommand::SetRowLayout {
                    sheet_id: "sheet-1".into(),
                    start_row: 4,
                    end_row: 4,
                    height: None,
                    reset_height: true,
                    hidden: Some(false),
                }],
            })
            .unwrap();
        assert!(engine.model().sheets[0].metadata.row_layout.is_empty());
        let revision = engine.revision();
        assert!(engine
            .execute(SpreadsheetCommandBatch {
                base_revision: revision,
                commands: vec![SpreadsheetCommand::SetRowLayout {
                    sheet_id: "sheet-1".into(),
                    start_row: 0,
                    end_row: 0,
                    height: Some(-1.0),
                    reset_height: false,
                    hidden: None
                }]
            })
            .is_err());
        assert_eq!(engine.revision(), revision);
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
    fn styling_an_empty_cell_materializes_a_blank_styled_cell() {
        let mut engine = engine();
        let style = CellStyle {
            number_format: Some("0.00".into()),
            font: None,
            fill: None,
            alignment: None,
            borders: None,
        };
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::SetCellStyle {
                    sheet_id: "sheet-1".into(),
                    row: 0,
                    column: 1,
                    style: Some(style.clone()),
                }],
            })
            .unwrap();
        let cell = &engine.model().sheets[0].cells[0];
        assert_eq!(cell.row, 0);
        assert_eq!(cell.column, 1);
        assert!(cell.value.is_none());
        assert!(cell.formula.is_none());
        assert_eq!(cell.style, Some(style));
    }

    #[test]
    fn insert_rows_shifts_cells_and_grows_merge_range() {
        let mut engine = SpreadsheetEngine::new(
            SpreadsheetModel {
                sheets: vec![SheetModel {
                    id: "sheet-1".into(),
                    name: "Sheet 1".into(),
                    cells: vec![
                        CellModel {
                            row: 0,
                            column: 0,
                            value: Some("A".into()),
                            ..CellModel::default()
                        },
                        CellModel {
                            row: 2,
                            column: 0,
                            value: Some("C".into()),
                            ..CellModel::default()
                        },
                        CellModel {
                            row: 5,
                            column: 0,
                            value: Some("F".into()),
                            ..CellModel::default()
                        },
                    ],
                    metadata: SheetMetadata {
                        freeze: FreezePane {
                            rows: 1,
                            columns: 0,
                        },
                        row_count: Some(6),
                        merged_ranges: vec![GridRange {
                            start_row: 1,
                            start_column: 0,
                            end_row: 1,
                            end_column: 1,
                        }],
                        ..SheetMetadata::default()
                    },
                }],
                ..SpreadsheetModel::default()
            },
            0,
        )
        .unwrap();

        let result = engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::InsertRows {
                    sheet_id: "sheet-1".into(),
                    at: 1,
                    count: 2,
                }],
            })
            .unwrap();
        assert!(result.invalidation.structure_changed);
        let sheet = &engine.model().sheets[0];
        assert_eq!(sheet.cells[0].row, 0);
        assert_eq!(sheet.cells[1].row, 4);
        assert_eq!(sheet.cells[2].row, 7);
        assert_eq!(
            sheet.metadata.merged_ranges[0],
            GridRange {
                start_row: 3,
                start_column: 0,
                end_row: 3,
                end_column: 1
            }
        );
        assert_eq!(sheet.metadata.freeze.rows, 1);
        assert_eq!(sheet.metadata.row_count, Some(8));
        // Undo restores the original coordinates.
        engine.undo().unwrap();
        assert_eq!(engine.model().sheets[0].cells.len(), 3);
        assert_eq!(engine.model().sheets[0].cells[0].row, 0);
    }

    #[test]
    fn delete_rows_drops_cells_and_contracts_merge_and_freeze() {
        let mut engine = SpreadsheetEngine::new(
            SpreadsheetModel {
                sheets: vec![SheetModel {
                    id: "sheet-1".into(),
                    name: "Sheet 1".into(),
                    cells: vec![
                        CellModel {
                            row: 0,
                            column: 0,
                            value: Some("A".into()),
                            ..CellModel::default()
                        },
                        CellModel {
                            row: 1,
                            column: 0,
                            value: Some("B".into()),
                            ..CellModel::default()
                        },
                        CellModel {
                            row: 2,
                            column: 0,
                            value: Some("C".into()),
                            ..CellModel::default()
                        },
                        CellModel {
                            row: 4,
                            column: 0,
                            value: Some("E".into()),
                            ..CellModel::default()
                        },
                    ],
                    metadata: SheetMetadata {
                        freeze: FreezePane {
                            rows: 2,
                            columns: 0,
                        },
                        row_count: Some(6),
                        merged_ranges: vec![GridRange {
                            start_row: 1,
                            start_column: 0,
                            end_row: 2,
                            end_column: 0,
                        }],
                        ..SheetMetadata::default()
                    },
                }],
                ..SpreadsheetModel::default()
            },
            0,
        )
        .unwrap();

        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::DeleteRows {
                    sheet_id: "sheet-1".into(),
                    at: 1,
                    count: 2,
                }],
            })
            .unwrap();
        let sheet = &engine.model().sheets[0];
        assert_eq!(sheet.cells.len(), 2);
        assert_eq!(sheet.cells[0].row, 0);
        assert_eq!(sheet.cells[1].row, 2);
        assert!(sheet.metadata.merged_ranges.is_empty());
        assert_eq!(sheet.metadata.freeze.rows, 1);
        assert_eq!(sheet.metadata.row_count, Some(4));
    }

    #[test]
    fn insert_then_delete_columns_remaps_cells_and_merged_ranges() {
        let mut engine = SpreadsheetEngine::new(
            SpreadsheetModel {
                sheets: vec![SheetModel {
                    id: "sheet-1".into(),
                    name: "Sheet 1".into(),
                    cells: vec![CellModel {
                        row: 0,
                        column: 1,
                        value: Some("B".into()),
                        ..CellModel::default()
                    }],
                    metadata: SheetMetadata {
                        column_count: Some(4),
                        merged_ranges: vec![GridRange {
                            start_row: 0,
                            start_column: 0,
                            end_row: 0,
                            end_column: 1,
                        }],
                        ..SheetMetadata::default()
                    },
                }],
                ..SpreadsheetModel::default()
            },
            0,
        )
        .unwrap();

        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::InsertColumns {
                    sheet_id: "sheet-1".into(),
                    at: 1,
                    count: 1,
                }],
            })
            .unwrap();
        let sheet = &engine.model().sheets[0];
        assert_eq!(sheet.cells[0].column, 2);
        assert_eq!(
            sheet.metadata.merged_ranges[0],
            GridRange {
                start_row: 0,
                start_column: 0,
                end_row: 0,
                end_column: 2
            }
        );
        assert_eq!(sheet.metadata.column_count, Some(5));

        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 1,
                commands: vec![SpreadsheetCommand::DeleteColumns {
                    sheet_id: "sheet-1".into(),
                    at: 0,
                    count: 1,
                }],
            })
            .unwrap();
        let sheet = &engine.model().sheets[0];
        assert_eq!(sheet.cells[0].column, 1);
        assert_eq!(
            sheet.metadata.merged_ranges[0],
            GridRange {
                start_row: 0,
                start_column: 0,
                end_row: 0,
                end_column: 1
            }
        );
        assert_eq!(sheet.metadata.column_count, Some(4));
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

    fn entity_ids(result: &SpreadsheetChangeSet) -> Vec<String> {
        result
            .invalidation
            .changed_entities
            .iter()
            .map(|entity| entity.entity_id.clone())
            .collect()
    }

    #[test]
    fn set_cell_invalidation_includes_dependent_formula_cells() {
        let mut engine = engine();
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::SetCell {
                    sheet_id: "sheet-1".into(),
                    row: 0,
                    column: 1,
                    value: None,
                    formula: Some("=A1 + 1".into()),
                    attrs: Map::new(),
                }],
            })
            .unwrap();
        let result = engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 1,
                commands: vec![SpreadsheetCommand::SetCell {
                    sheet_id: "sheet-1".into(),
                    row: 0,
                    column: 0,
                    value: Some(5.into()),
                    formula: None,
                    attrs: Map::new(),
                }],
            })
            .unwrap();
        let ids = entity_ids(&result);
        assert!(ids.contains(&"sheet-1:0:0".to_string()), "written cell");
        assert!(
            ids.contains(&"sheet-1:0:1".to_string()),
            "dependent formula"
        );

        // Undo reports the same reverse closure: B1's derived value changed too.
        let undo = engine.undo().unwrap();
        let undo_ids = entity_ids(&undo);
        assert!(undo_ids.contains(&"sheet-1:0:0".to_string()));
        assert!(undo_ids.contains(&"sheet-1:0:1".to_string()));
    }

    #[test]
    fn structural_edit_rebuilds_dependency_cache_for_followup_writes() {
        let mut engine = SpreadsheetEngine::new(
            SpreadsheetModel {
                sheets: vec![SheetModel {
                    id: "sheet-1".into(),
                    name: "Sheet 1".into(),
                    cells: vec![
                        CellModel {
                            row: 0,
                            column: 0,
                            value: Some(1.into()),
                            ..CellModel::default()
                        },
                        CellModel {
                            row: 0,
                            column: 1,
                            formula: Some("=A1 + 1".into()),
                            ..CellModel::default()
                        },
                    ],
                    metadata: SheetMetadata {
                        row_count: Some(5),
                        ..SheetMetadata::default()
                    },
                }],
                ..SpreadsheetModel::default()
            },
            0,
        )
        .unwrap();
        // Warm the cache, then force a structural rebuild via a row insert that
        // leaves row 0 (and therefore the A1 reference) untouched.
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::SetCell {
                    sheet_id: "sheet-1".into(),
                    row: 0,
                    column: 0,
                    value: Some(2.into()),
                    formula: None,
                    attrs: Map::new(),
                }],
            })
            .unwrap();
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 1,
                commands: vec![SpreadsheetCommand::InsertRows {
                    sheet_id: "sheet-1".into(),
                    at: 1,
                    count: 3,
                }],
            })
            .unwrap();
        let result = engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 2,
                commands: vec![SpreadsheetCommand::SetCell {
                    sheet_id: "sheet-1".into(),
                    row: 0,
                    column: 0,
                    value: Some(9.into()),
                    formula: None,
                    attrs: Map::new(),
                }],
            })
            .unwrap();
        let ids = entity_ids(&result);
        assert!(
            ids.contains(&"sheet-1:0:1".to_string()),
            "dependents survive the rebuild"
        );
    }

    #[test]
    fn unparseable_formula_disables_dependent_reporting_without_breaking_writes() {
        let mut engine = engine();
        // A range the dependency index refuses to expand must not poison the
        // engine: writes keep working, enrichment is simply skipped.
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::SetCell {
                    sheet_id: "sheet-1".into(),
                    row: 0,
                    column: 0,
                    value: None,
                    formula: Some("=A1:XFD100000".into()),
                    attrs: Map::new(),
                }],
            })
            .unwrap();
        let result = engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 1,
                commands: vec![SpreadsheetCommand::SetCell {
                    sheet_id: "sheet-1".into(),
                    row: 1,
                    column: 0,
                    value: Some(1.into()),
                    formula: None,
                    attrs: Map::new(),
                }],
            })
            .unwrap();
        assert_eq!(result.revision, 2);
        assert!(entity_ids(&result).contains(&"sheet-1:1:0".to_string()));
    }

    fn formula_engine(cells: Vec<CellModel>) -> SpreadsheetEngine {
        SpreadsheetEngine::new(
            SpreadsheetModel {
                sheets: vec![SheetModel {
                    id: "sheet-1".into(),
                    name: "Sheet 1".into(),
                    cells,
                    metadata: SheetMetadata {
                        row_count: Some(10),
                        column_count: Some(10),
                        ..SheetMetadata::default()
                    },
                }],
                ..SpreadsheetModel::default()
            },
            0,
        )
        .unwrap()
    }

    fn cell_formula(engine: &SpreadsheetEngine, row: u32, column: u32) -> Option<String> {
        engine.model().sheets[0]
            .cells
            .iter()
            .find(|cell| cell.row == row && cell.column == column)
            .and_then(|cell| cell.formula.clone())
    }

    #[test]
    fn insert_rows_rewrites_formula_references_below_the_insertion() {
        // A3 reads A1; A4 reads A3. Inserting two rows at index 1 shifts the
        // references that point at row >= 1 but leaves `=A1` untouched.
        let mut engine = formula_engine(vec![
            CellModel {
                row: 0,
                column: 0,
                value: Some(1.into()),
                ..CellModel::default()
            },
            CellModel {
                row: 2,
                column: 0,
                formula: Some("=A1 + 10".into()),
                ..CellModel::default()
            },
            CellModel {
                row: 3,
                column: 0,
                formula: Some("=A3 * 2".into()),
                ..CellModel::default()
            },
            CellModel {
                row: 4,
                column: 0,
                formula: Some("=SUM(A1:A3)".into()),
                ..CellModel::default()
            },
        ]);
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::InsertRows {
                    sheet_id: "sheet-1".into(),
                    at: 1,
                    count: 2,
                }],
            })
            .unwrap();
        assert_eq!(
            cell_formula(&engine, 4, 0).as_deref(),
            Some("=A1 + 10"),
            "引用 A1 不动"
        );
        assert_eq!(
            cell_formula(&engine, 5, 0).as_deref(),
            Some("=A5 * 2"),
            "跟随单元格下移"
        );
        assert_eq!(
            cell_formula(&engine, 6, 0).as_deref(),
            Some("=SUM(A1:A5)"),
            "范围扩展"
        );
    }

    #[test]
    fn delete_rows_turns_swallowed_references_into_ref_error() {
        let mut engine = formula_engine(vec![
            CellModel {
                row: 4,
                column: 0,
                formula: Some("=A2 + 1".into()),
                ..CellModel::default()
            },
            CellModel {
                row: 5,
                column: 0,
                formula: Some("=SUM(A2:A4)".into()),
                ..CellModel::default()
            },
            CellModel {
                row: 6,
                column: 0,
                formula: Some("=SUM(A1:A5)".into()),
                ..CellModel::default()
            },
        ]);
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::DeleteRows {
                    sheet_id: "sheet-1".into(),
                    at: 1,
                    count: 2,
                }],
            })
            .unwrap();
        assert_eq!(cell_formula(&engine, 2, 0).as_deref(), Some("=#REF! + 1"));
        // The range loses its two deleted rows; Excel contracts it to the
        // surviving boundary cell.
        assert_eq!(cell_formula(&engine, 3, 0).as_deref(), Some("=SUM(A2)"));
        assert_eq!(
            cell_formula(&engine, 4, 0).as_deref(),
            Some("=SUM(A1:A3)"),
            "部分覆盖收缩"
        );
    }

    #[test]
    fn insert_columns_rewrites_column_references_and_keeps_absolute_markers() {
        let mut engine = formula_engine(vec![CellModel {
            row: 0,
            column: 3,
            formula: Some("=$B1 + C1".into()),
            ..CellModel::default()
        }]);
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::InsertColumns {
                    sheet_id: "sheet-1".into(),
                    at: 0,
                    count: 1,
                }],
            })
            .unwrap();
        assert_eq!(cell_formula(&engine, 0, 4).as_deref(), Some("=$C1 + D1"));
    }

    #[test]
    fn structural_edits_remap_the_four_rule_ranges() {
        let mut engine = SpreadsheetEngine::new(
            SpreadsheetModel {
                sheets: vec![SheetModel {
                    id: "sheet-1".into(),
                    name: "Sheet 1".into(),
                    cells: Vec::new(),
                    metadata: SheetMetadata {
                        row_count: Some(20),
                        column_count: Some(10),
                        auto_filter: Some(oo_schema::FilterSpec {
                            range: GridRange {
                                start_row: 0,
                                start_column: 0,
                                end_row: 9,
                                end_column: 3,
                            },
                            columns: vec![oo_schema::FilterColumn {
                                column: 1,
                                predicate: oo_schema::FilterPredicate::Equals("x".into()),
                            }],
                        }),
                        sort: Some(oo_schema::SortSpec {
                            range: GridRange {
                                start_row: 0,
                                start_column: 0,
                                end_row: 9,
                                end_column: 3,
                            },
                            keys: vec![oo_schema::SortKey {
                                column: 1,
                                direction: oo_schema::SortDirection::Ascending,
                            }],
                        }),
                        conditional_formats: vec![oo_schema::ConditionalFormatRule {
                            id: "cf-1".into(),
                            range: GridRange {
                                start_row: 2,
                                start_column: 0,
                                end_row: 5,
                                end_column: 2,
                            },
                            predicate: oo_schema::ConditionalPredicate::CellIs {
                                operator: oo_schema::ComparisonOperator::GreaterThan,
                                value: serde_json::json!(0),
                            },
                            style: CellStyle::default(),
                        }],
                        data_validations: vec![oo_schema::DataValidationRule {
                            id: "dv-1".into(),
                            range: GridRange {
                                start_row: 2,
                                start_column: 1,
                                end_row: 5,
                                end_column: 1,
                            },
                            kind: oo_schema::DataValidationKind::WholeNumber { min: 0, max: 100 },
                            allow_blank: true,
                            error_message: None,
                        }],
                        ..SheetMetadata::default()
                    },
                }],
                ..SpreadsheetModel::default()
            },
            0,
        )
        .unwrap();
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::InsertRows {
                    sheet_id: "sheet-1".into(),
                    at: 3,
                    count: 2,
                }],
            })
            .unwrap();
        let metadata = &engine.model().sheets[0].metadata;
        let range_of = |start: u32, end: u32| GridRange {
            start_row: start,
            start_column: 0,
            end_row: end,
            end_column: 2,
        };
        assert_eq!(
            metadata.conditional_formats[0].range,
            range_of(2, 7),
            "条件格式扩展"
        );
        assert_eq!(
            metadata.data_validations[0].range,
            GridRange {
                start_row: 2,
                start_column: 1,
                end_row: 7,
                end_column: 1
            },
            "校验扩展"
        );
        assert_eq!(
            metadata.auto_filter.as_ref().unwrap().range,
            GridRange {
                start_row: 0,
                start_column: 0,
                end_row: 11,
                end_column: 3
            },
            "筛选扩展"
        );
        assert_eq!(metadata.auto_filter.as_ref().unwrap().columns[0].column, 1);
        assert_eq!(
            metadata.sort.as_ref().unwrap().range,
            GridRange {
                start_row: 0,
                start_column: 0,
                end_row: 11,
                end_column: 3
            },
            "排序扩展"
        );
        assert_eq!(metadata.sort.as_ref().unwrap().keys[0].column, 1);

        // Deleting the column that carries the filter/sort keys drops the
        // keys but keeps the range alive.
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 1,
                commands: vec![SpreadsheetCommand::DeleteColumns {
                    sheet_id: "sheet-1".into(),
                    at: 1,
                    count: 1,
                }],
            })
            .unwrap();
        let metadata = &engine.model().sheets[0].metadata;
        assert!(
            metadata.auto_filter.as_ref().unwrap().columns.is_empty(),
            "被删列的筛选键丢弃"
        );
        assert!(
            metadata.sort.as_ref().unwrap().keys.is_empty(),
            "被删列的排序键丢弃"
        );
        assert_eq!(metadata.conditional_formats[0].range.start_column, 0);
        assert_eq!(metadata.conditional_formats[0].range.end_column, 1);
    }

    #[test]
    fn structural_edits_and_sheet_deletion_keep_named_ranges_valid_and_reversible() {
        let mut engine = SpreadsheetEngine::new(
            SpreadsheetModel {
                metadata: oo_schema::SpreadsheetMetadata {
                    active_sheet_id: Some("data".into()),
                    named_ranges: vec![oo_schema::SpreadsheetNamedRange {
                        name: "Revenue".into(),
                        scope_sheet_id: Some("summary".into()),
                        sheet_id: "data".into(),
                        range: GridRange {
                            start_row: 1,
                            start_column: 0,
                            end_row: 3,
                            end_column: 0,
                        },
                    }],
                    ..oo_schema::SpreadsheetMetadata::default()
                },
                sheets: vec![
                    SheetModel {
                        id: "data".into(),
                        name: "Data".into(),
                        ..SheetModel::default()
                    },
                    SheetModel {
                        id: "summary".into(),
                        name: "Summary".into(),
                        ..SheetModel::default()
                    },
                ],
            },
            0,
        )
        .unwrap();

        let changes = engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::InsertRows {
                    sheet_id: "data".into(),
                    at: 2,
                    count: 2,
                }],
            })
            .unwrap();
        assert!(changes
            .mutations
            .iter()
            .any(|mutation| matches!(mutation, SpreadsheetMutation::NamedRangesChanged { .. })));
        assert_eq!(engine.model().metadata.named_ranges[0].range.end_row, 5);
        engine.undo().unwrap();
        assert_eq!(engine.model().metadata.named_ranges[0].range.end_row, 3);
        engine.redo().unwrap();
        assert_eq!(engine.model().metadata.named_ranges[0].range.end_row, 5);

        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: engine.revision(),
                commands: vec![SpreadsheetCommand::DeleteSheet {
                    sheet_id: "data".into(),
                }],
            })
            .unwrap();
        assert!(engine.model().metadata.named_ranges.is_empty());
        assert_eq!(
            engine.model().metadata.active_sheet_id.as_deref(),
            Some("summary")
        );
        engine.undo().unwrap();
        assert_eq!(engine.model().metadata.named_ranges[0].name, "Revenue");
        assert_eq!(
            engine.model().metadata.active_sheet_id.as_deref(),
            Some("data")
        );
    }

    #[test]
    fn cross_sheet_references_follow_the_structural_edit() {
        let mut engine = SpreadsheetEngine::new(
            SpreadsheetModel {
                sheets: vec![
                    SheetModel {
                        id: "data".into(),
                        name: "Data".into(),
                        cells: vec![
                            CellModel {
                                row: 0,
                                column: 0,
                                value: Some(1.into()),
                                ..CellModel::default()
                            },
                            CellModel {
                                row: 1,
                                column: 0,
                                value: Some(2.into()),
                                ..CellModel::default()
                            },
                        ],
                        metadata: SheetMetadata {
                            row_count: Some(10),
                            ..SheetMetadata::default()
                        },
                    },
                    SheetModel {
                        id: "report".into(),
                        name: "Report".into(),
                        cells: vec![
                            CellModel {
                                row: 0,
                                column: 0,
                                formula: Some("=Data!A2 + 1".into()),
                                ..CellModel::default()
                            },
                            CellModel {
                                row: 0,
                                column: 1,
                                formula: Some("=SUM(Data!A1:A2)".into()),
                                ..CellModel::default()
                            },
                        ],
                        ..SheetModel::default()
                    },
                ],
                ..SpreadsheetModel::default()
            },
            0,
        )
        .unwrap();
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::InsertRows {
                    sheet_id: "data".into(),
                    at: 1,
                    count: 1,
                }],
            })
            .unwrap();
        let report = engine
            .model()
            .sheets
            .iter()
            .find(|sheet| sheet.id == "report")
            .unwrap();
        assert_eq!(
            report.cells[0].formula.as_deref(),
            Some("=Data!A3 + 1"),
            "跨表引用跟随平移"
        );
        assert_eq!(
            report.cells[1].formula.as_deref(),
            Some("=SUM(Data!A1:A3)"),
            "跨表范围扩展"
        );
        // The edited sheet's own data moved too.
        let data = engine
            .model()
            .sheets
            .iter()
            .find(|sheet| sheet.id == "data")
            .unwrap();
        assert_eq!(data.cells[1].row, 2);
    }

    #[test]
    fn merge_cells_drops_non_anchor_content_and_is_reversible() {
        let mut engine = formula_engine(vec![
            CellModel {
                row: 0,
                column: 0,
                value: Some("anchor".into()),
                ..CellModel::default()
            },
            CellModel {
                row: 0,
                column: 1,
                value: Some("drop".into()),
                ..CellModel::default()
            },
            CellModel {
                row: 1,
                column: 0,
                value: Some("drop".into()),
                ..CellModel::default()
            },
            CellModel {
                row: 1,
                column: 1,
                value: Some("drop".into()),
                ..CellModel::default()
            },
        ]);
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::MergeCells {
                    sheet_id: "sheet-1".into(),
                    range: GridRange {
                        start_row: 0,
                        start_column: 0,
                        end_row: 1,
                        end_column: 1,
                    },
                }],
            })
            .unwrap();
        let sheet = &engine.model().sheets[0];
        assert_eq!(sheet.cells.len(), 1, "非锚点内容按 Excel 语义丢弃");
        assert_eq!(sheet.cells[0].value, Some(Value::String("anchor".into())));
        assert_eq!(sheet.metadata.merged_ranges.len(), 1);

        engine.undo().unwrap();
        let sheet = &engine.model().sheets[0];
        assert_eq!(sheet.cells.len(), 4, "undo 恢复全部内容");
        assert!(sheet.metadata.merged_ranges.is_empty());
    }

    #[test]
    fn overlapping_merge_is_rejected_and_unmerge_requires_exact_match() {
        let mut engine = engine();
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::MergeCells {
                    sheet_id: "sheet-1".into(),
                    range: GridRange {
                        start_row: 0,
                        start_column: 0,
                        end_row: 1,
                        end_column: 1,
                    },
                }],
            })
            .unwrap();
        let overlap = engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 1,
                commands: vec![SpreadsheetCommand::MergeCells {
                    sheet_id: "sheet-1".into(),
                    range: GridRange {
                        start_row: 1,
                        start_column: 1,
                        end_row: 2,
                        end_column: 2,
                    },
                }],
            })
            .unwrap_err();
        assert!(matches!(overlap, SpreadsheetEngineError::MergeOverlap));

        let exact = SpreadsheetCommand::UnmergeCells {
            sheet_id: "sheet-1".into(),
            range: GridRange {
                start_row: 0,
                start_column: 0,
                end_row: 1,
                end_column: 1,
            },
        };
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 1,
                commands: vec![exact],
            })
            .unwrap();
        assert!(engine.model().sheets[0].metadata.merged_ranges.is_empty());
    }

    #[test]
    fn sort_range_reorders_rows_and_refuses_formulas() {
        let mut engine = formula_engine(vec![
            CellModel {
                row: 0,
                column: 0,
                value: Some(3.into()),
                ..CellModel::default()
            },
            CellModel {
                row: 1,
                column: 0,
                value: Some(1.into()),
                ..CellModel::default()
            },
            CellModel {
                row: 2,
                column: 0,
                value: Some(2.into()),
                ..CellModel::default()
            },
        ]);
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::SortRange {
                    sheet_id: "sheet-1".into(),
                    range: GridRange {
                        start_row: 0,
                        start_column: 0,
                        end_row: 2,
                        end_column: 0,
                    },
                    keys: vec![SortKey {
                        column: 0,
                        direction: oo_schema::SortDirection::Ascending,
                    }],
                }],
            })
            .unwrap();
        let values: Vec<_> = engine.model().sheets[0]
            .cells
            .iter()
            .map(|cell| cell.value.clone())
            .collect();
        assert_eq!(values, vec![Some(1.into()), Some(2.into()), Some(3.into())]);

        // A formula inside the region makes the sort a typed error.
        let mut engine = formula_engine(vec![
            CellModel {
                row: 0,
                column: 0,
                value: Some(3.into()),
                ..CellModel::default()
            },
            CellModel {
                row: 1,
                column: 0,
                formula: Some("=A1 + 1".into()),
                ..CellModel::default()
            },
        ]);
        let error = engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::SortRange {
                    sheet_id: "sheet-1".into(),
                    range: GridRange {
                        start_row: 0,
                        start_column: 0,
                        end_row: 1,
                        end_column: 0,
                    },
                    keys: vec![SortKey {
                        column: 0,
                        direction: oo_schema::SortDirection::Ascending,
                    }],
                }],
            })
            .unwrap_err();
        assert!(matches!(error, SpreadsheetEngineError::SortWithFormulas(_)));
    }

    #[test]
    fn data_validation_rejects_bad_writes_but_allows_blank_policy() {
        let mut engine = SpreadsheetEngine::new(
            SpreadsheetModel {
                sheets: vec![SheetModel {
                    id: "sheet-1".into(),
                    name: "Sheet 1".into(),
                    cells: Vec::new(),
                    metadata: SheetMetadata {
                        data_validations: vec![oo_schema::DataValidationRule {
                            id: "dv-list".into(),
                            range: GridRange {
                                start_row: 0,
                                start_column: 0,
                                end_row: 9,
                                end_column: 0,
                            },
                            kind: oo_schema::DataValidationKind::List(vec![
                                "red".into(),
                                "blue".into(),
                            ]),
                            allow_blank: true,
                            error_message: Some("只允许 red/blue".into()),
                        }],
                        ..SheetMetadata::default()
                    },
                }],
                ..SpreadsheetModel::default()
            },
            0,
        )
        .unwrap();
        let mut write = |value: Value| {
            engine
                .execute(SpreadsheetCommandBatch {
                    base_revision: engine.revision(),
                    commands: vec![SpreadsheetCommand::SetCell {
                        sheet_id: "sheet-1".into(),
                        row: 0,
                        column: 0,
                        value: Some(value),
                        formula: None,
                        attrs: Map::new(),
                    }],
                })
                .map(|result| result.revision)
        };
        assert_eq!(write(Value::String("red".into())).unwrap(), 1);
        let rejected = write(Value::String("green".into())).unwrap_err();
        assert!(matches!(
            rejected,
            SpreadsheetEngineError::DataValidationRejected { .. }
        ));
        // allow_blank = true means clearing the cell through SetCell(null) works.
        assert_eq!(write(Value::Null).unwrap(), 2);
    }

    #[test]
    fn range_style_fields_preserve_mixed_cells_and_undo_atomically() {
        let first = CellStyle {
            font: Some(oo_schema::FontStyle {
                bold: true,
                size: Some(18.0),
                ..Default::default()
            }),
            ..Default::default()
        };
        let second = CellStyle {
            number_format: Some("0.00%".into()),
            font: Some(oo_schema::FontStyle {
                italic: true,
                color: Some("#165dff".into()),
                size: Some(10.0),
                ..Default::default()
            }),
            fill: Some(oo_schema::FillStyle {
                background: Some("#fadc19".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut engine = formula_engine(vec![
            CellModel {
                row: 0,
                column: 0,
                value: Some(1.into()),
                style: Some(first.clone()),
                ..Default::default()
            },
            CellModel {
                row: 0,
                column: 1,
                value: Some(2.into()),
                style: Some(second.clone()),
                ..Default::default()
            },
        ]);
        let before = engine.model().clone();
        let result = engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::FormatRange {
                    sheet_id: "sheet-1".into(),
                    range: GridRange {
                        start_row: 0,
                        end_row: 0,
                        start_column: 0,
                        end_column: 1,
                    },
                    style: first.clone(),
                    fields: Some(vec![CellStyleField::FontBold]),
                    row_pattern: None,
                }],
            })
            .unwrap();
        let mut expected = second;
        expected.font.as_mut().unwrap().bold = true;
        assert_eq!(engine.model().sheets[0].cells[0].style, Some(first));
        assert_eq!(engine.model().sheets[0].cells[1].style, Some(expected));
        assert_eq!(engine.model().sheets[0].cells[1].value, Some(2.into()));
        assert_eq!(result.mutations.len(), 1);
        let after = engine.model().clone();
        engine.undo().unwrap();
        assert_eq!(engine.model(), &before);
        engine.redo().unwrap();
        assert_eq!(engine.model(), &after);
    }

    #[test]
    fn outer_range_borders_only_touch_perimeter_and_keep_inner_edges() {
        let edge = oo_schema::CellBorderEdge {
            style: Some("thin".into()),
            color: Some("#165dff".into()),
        };
        let inner = oo_schema::CellBorderEdge {
            style: Some("dashed".into()),
            color: Some("#f53f3f".into()),
        };
        let original = CellStyle {
            borders: Some(oo_schema::CellBorders {
                right: Some(inner.clone()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut engine = formula_engine(vec![CellModel {
            row: 1,
            column: 1,
            value: Some(7.into()),
            style: Some(original.clone()),
            ..Default::default()
        }]);
        let before = engine.model().clone();
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::FormatRange {
                    sheet_id: "sheet-1".into(),
                    range: GridRange {
                        start_row: 0,
                        end_row: 2,
                        start_column: 0,
                        end_column: 2,
                    },
                    style: CellStyle {
                        borders: Some(oo_schema::CellBorders {
                            top: Some(edge.clone()),
                            bottom: Some(edge.clone()),
                            left: Some(edge.clone()),
                            right: Some(edge.clone()),
                        }),
                        ..Default::default()
                    },
                    fields: Some(vec![CellStyleField::OuterBorders]),
                    row_pattern: None,
                }],
            })
            .unwrap();
        let cells = &engine.model().sheets[0].cells;
        assert_eq!(cells.len(), 9);
        for cell in cells {
            if (cell.row, cell.column) == (1, 1) {
                assert_eq!(cell.style, Some(original.clone()));
                continue;
            }
            let borders = cell.style.as_ref().unwrap().borders.as_ref().unwrap();
            assert_eq!(borders.top, (cell.row == 0).then(|| edge.clone()));
            assert_eq!(borders.bottom, (cell.row == 2).then(|| edge.clone()));
            assert_eq!(borders.left, (cell.column == 0).then(|| edge.clone()));
            assert_eq!(borders.right, (cell.column == 2).then(|| edge.clone()));
        }
        engine.undo().unwrap();
        assert_eq!(engine.model(), &before);
    }

    #[test]
    fn editing_values_formulas_and_blank_content_preserves_cell_style() {
        let style = CellStyle {
            number_format: Some("0.00%".into()),
            ..Default::default()
        };
        let mut engine = formula_engine(vec![CellModel {
            row: 0,
            column: 0,
            value: Some(0.1.into()),
            style: Some(style.clone()),
            ..Default::default()
        }]);
        for (revision, value, formula) in [
            (0, Some(0.2.into()), None),
            (1, None, Some("=SUM(1,2)".to_string())),
            (2, None, None),
        ] {
            engine
                .execute(SpreadsheetCommandBatch {
                    base_revision: revision,
                    commands: vec![SpreadsheetCommand::SetCell {
                        sheet_id: "sheet-1".into(),
                        row: 0,
                        column: 0,
                        value: value.clone(),
                        formula: formula.clone(),
                        attrs: Map::new(),
                    }],
                })
                .unwrap();
            let cell = &engine.model().sheets[0].cells[0];
            assert_eq!(cell.style, Some(style.clone()));
            assert_eq!(cell.value, value);
            assert_eq!(cell.formula, formula);
        }
        engine.undo().unwrap();
        assert_eq!(
            engine.model().sheets[0].cells[0].formula.as_deref(),
            Some("=SUM(1,2)")
        );
        assert_eq!(engine.model().sheets[0].cells[0].style, Some(style));
    }

    #[test]
    fn table_appearance_batch_patterns_are_relative_and_undo_together() {
        let original = CellStyle {
            number_format: Some("0.00%".into()),
            font: Some(oo_schema::FontStyle {
                italic: true,
                size: Some(18.0),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut engine = formula_engine(vec![CellModel {
            row: 5,
            column: 2,
            value: Some(0.1.into()),
            style: Some(original.clone()),
            ..Default::default()
        }]);
        let before = engine.model().clone();
        let range = GridRange {
            start_row: 4,
            end_row: 7,
            start_column: 2,
            end_column: 3,
        };
        let fill = |color: &str, row_pattern| SpreadsheetCommand::FormatRange {
            sheet_id: "sheet-1".into(),
            range,
            style: CellStyle {
                fill: Some(oo_schema::FillStyle {
                    background: Some(color.into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            fields: Some(vec![CellStyleField::FillBackground]),
            row_pattern,
        };
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![
                    fill("#ffffff", None),
                    fill("#e8f3ff", Some(RangeRowPattern::AlternatingRows)),
                    fill("#165dff", Some(RangeRowPattern::FirstRow)),
                ],
            })
            .unwrap();
        assert_eq!(engine.model().sheets[0].cells.len(), 8);
        for cell in &engine.model().sheets[0].cells {
            let expected = match cell.row {
                4 => "#165dff",
                5 | 7 => "#e8f3ff",
                _ => "#ffffff",
            };
            assert_eq!(
                cell.style
                    .as_ref()
                    .unwrap()
                    .fill
                    .as_ref()
                    .unwrap()
                    .background
                    .as_deref(),
                Some(expected)
            );
            if (cell.row, cell.column) == (5, 2) {
                assert_eq!(
                    cell.style.as_ref().unwrap().number_format,
                    original.number_format
                );
                assert_eq!(cell.style.as_ref().unwrap().font, original.font);
                assert_eq!(cell.value, Some(0.1.into()));
            }
        }
        let after = engine.model().clone();
        engine.undo().unwrap();
        assert_eq!(engine.model(), &before);
        engine.redo().unwrap();
        assert_eq!(engine.model(), &after);
    }

    #[test]
    fn format_fields_reject_unknown_wire_properties() {
        let command = serde_json::json!({ "type": "formatRange", "sheetId": "sheet-1", "range": { "startRow": 0, "endRow": 0, "startColumn": 0, "endColumn": 0 }, "style": {}, "fields": ["fontBoldd"] });
        assert!(serde_json::from_value::<SpreadsheetCommand>(command).is_err());
    }

    #[test]
    fn format_range_is_one_atomic_history_entry() {
        let mut engine = formula_engine(vec![
            CellModel {
                row: 0,
                column: 0,
                value: Some(1.into()),
                ..CellModel::default()
            },
            CellModel {
                row: 0,
                column: 1,
                value: Some(2.into()),
                ..CellModel::default()
            },
        ]);
        let style = CellStyle {
            number_format: None,
            font: Some(oo_schema::FontStyle {
                bold: true,
                ..oo_schema::FontStyle::default()
            }),
            fill: None,
            alignment: None,
            borders: None,
        };
        let result = engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::FormatRange {
                    sheet_id: "sheet-1".into(),
                    range: GridRange {
                        start_row: 0,
                        start_column: 0,
                        end_row: 0,
                        end_column: 1,
                    },
                    style: style.clone(),
                    fields: None,
                    row_pattern: None,
                }],
            })
            .unwrap();
        // Journal 压缩：一次范围格式化 = 一条 RangeChanged mutation = 一个历史项。
        assert_eq!(result.mutations.len(), 1);
        assert!(matches!(
            result.mutations[0],
            SpreadsheetMutation::RangeChanged { .. }
        ));
        let cells = &engine.model().sheets[0].cells;
        assert!(cells.iter().all(|cell| cell.style == Some(style.clone())));
        // 撤销恢复原状（不含样式）。
        engine.undo().unwrap();
        assert!(engine.model().sheets[0]
            .cells
            .iter()
            .all(|cell| cell.style.is_none()));
    }

    #[test]
    fn format_range_skips_blank_materialization_on_large_regions() {
        let mut engine = engine();
        let style = CellStyle::default();
        // 200 x 100 = 20k 格 > MAX_RANGE_MATERIALIZE：只格式化已物化格子（0 个），不物化。
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::SetCell {
                    sheet_id: "sheet-1".into(),
                    row: 150,
                    column: 50,
                    value: Some(1.into()),
                    formula: None,
                    attrs: Map::new(),
                }],
            })
            .unwrap();
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 1,
                commands: vec![SpreadsheetCommand::FormatRange {
                    sheet_id: "sheet-1".into(),
                    range: GridRange {
                        start_row: 0,
                        start_column: 0,
                        end_row: 199,
                        end_column: 99,
                    },
                    style: style.clone(),
                    fields: None,
                    row_pattern: None,
                }],
            })
            .unwrap();
        let cells = &engine.model().sheets[0].cells;
        assert_eq!(cells.len(), 1, "大范围不物化空白格");
        assert_eq!(cells[0].style, Some(style));
    }

    #[test]
    fn clear_range_modes_split_contents_and_formats() {
        let style = CellStyle {
            number_format: Some("0.00".into()),
            font: None,
            fill: None,
            alignment: None,
            borders: None,
        };
        let mut engine = formula_engine(vec![
            CellModel {
                row: 0,
                column: 0,
                value: Some(1.into()),
                style: Some(style.clone()),
                ..CellModel::default()
            },
            CellModel {
                row: 1,
                column: 0,
                value: Some(2.into()),
                ..CellModel::default()
            },
        ]);
        // Formats 模式：剥样式，保留值。
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::ClearRange {
                    sheet_id: "sheet-1".into(),
                    range: GridRange {
                        start_row: 0,
                        start_column: 0,
                        end_row: 1,
                        end_column: 0,
                    },
                    mode: ClearRangeMode::Formats,
                }],
            })
            .unwrap();
        let cells = &engine.model().sheets[0].cells;
        assert_eq!(cells.len(), 2);
        assert!(cells
            .iter()
            .all(|cell| cell.style.is_none() && cell.value.is_some()));

        // Contents 模式：清值；无样式格子直接消失，原样式壳保留。
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 1,
                commands: vec![SpreadsheetCommand::ClearRange {
                    sheet_id: "sheet-1".into(),
                    range: GridRange {
                        start_row: 0,
                        start_column: 0,
                        end_row: 1,
                        end_column: 0,
                    },
                    mode: ClearRangeMode::Contents,
                }],
            })
            .unwrap();
        assert!(engine.model().sheets[0].cells.is_empty());
    }

    #[test]
    fn replace_range_rewrites_string_values_case_insensitively_by_default() {
        let mut engine = formula_engine(vec![
            CellModel {
                row: 0,
                column: 0,
                value: Some("Hello world".into()),
                ..CellModel::default()
            },
            CellModel {
                row: 1,
                column: 0,
                value: Some("hello again".into()),
                ..CellModel::default()
            },
            CellModel {
                row: 2,
                column: 0,
                value: Some(42.into()),
                ..CellModel::default()
            },
        ]);
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::ReplaceRange {
                    sheet_id: "sheet-1".into(),
                    range: GridRange {
                        start_row: 0,
                        start_column: 0,
                        end_row: 2,
                        end_column: 0,
                    },
                    search: "hello".into(),
                    replace: "hi".into(),
                    match_case: false,
                }],
            })
            .unwrap();
        let cells = &engine.model().sheets[0].cells;
        assert_eq!(cells[0].value, Some(Value::String("hi world".into())));
        assert_eq!(cells[1].value, Some(Value::String("hi again".into())));
        assert_eq!(cells[2].value, Some(42.into()), "数字不参与替换");

        // match_case = true 时不匹配小写源文本。
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 1,
                commands: vec![SpreadsheetCommand::ReplaceRange {
                    sheet_id: "sheet-1".into(),
                    range: GridRange {
                        start_row: 0,
                        start_column: 0,
                        end_row: 2,
                        end_column: 0,
                    },
                    search: "HI".into(),
                    replace: "yo".into(),
                    match_case: true,
                }],
            })
            .unwrap_err();
    }

    #[test]
    fn freeze_and_filter_commands_replace_metadata_fields_atomically() {
        let mut engine = engine();
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![
                    SpreadsheetCommand::SetFreezePane {
                        sheet_id: "sheet-1".into(),
                        rows: 2,
                        columns: 1,
                    },
                    SpreadsheetCommand::SetAutoFilter {
                        sheet_id: "sheet-1".into(),
                        range: Some(GridRange {
                            start_row: 0,
                            start_column: 0,
                            end_row: 9,
                            end_column: 3,
                        }),
                    },
                ],
            })
            .unwrap();
        let metadata = &engine.model().sheets[0].metadata;
        assert_eq!(
            metadata.freeze,
            FreezePane {
                rows: 2,
                columns: 1
            }
        );
        assert!(metadata.auto_filter.is_some());
        // 每个细粒度命令 = 一条专用 mutation（不产生 SheetChanged）。

        // 单独撤筛选：冻结不受影响。
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 1,
                commands: vec![SpreadsheetCommand::SetAutoFilter {
                    sheet_id: "sheet-1".into(),
                    range: None,
                }],
            })
            .unwrap();
        let metadata = &engine.model().sheets[0].metadata;
        assert!(metadata.auto_filter.is_none());
        assert_eq!(
            metadata.freeze,
            FreezePane {
                rows: 2,
                columns: 1
            }
        );

        // undo 撤销"清筛选"，redo 冻结状态始终未被牵连。
        engine.undo().unwrap();
        let metadata = &engine.model().sheets[0].metadata;
        assert!(metadata.auto_filter.is_some());
        assert_eq!(
            metadata.freeze,
            FreezePane {
                rows: 2,
                columns: 1
            }
        );
    }
    #[test]
    fn filter_column_upsert_is_idempotent_by_column_and_validated_against_range() {
        let mut engine = engine();
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::SetAutoFilter {
                    sheet_id: "sheet-1".into(),
                    range: Some(GridRange {
                        start_row: 0,
                        start_column: 0,
                        end_row: 9,
                        end_column: 3,
                    }),
                }],
            })
            .unwrap();
        // 范围外的列被拒绝。
        let out_of_range = engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 1,
                commands: vec![SpreadsheetCommand::UpsertFilterColumn {
                    sheet_id: "sheet-1".into(),
                    column: 5,
                    predicate: oo_schema::FilterPredicate::Contains("x".into()),
                }],
            })
            .unwrap_err();
        assert!(matches!(
            out_of_range,
            SpreadsheetEngineError::InvalidMutation(_)
        ));
        // 首次 upsert 插入。
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 1,
                commands: vec![SpreadsheetCommand::UpsertFilterColumn {
                    sheet_id: "sheet-1".into(),
                    column: 1,
                    predicate: oo_schema::FilterPredicate::Contains("a".into()),
                }],
            })
            .unwrap();
        // 再次 upsert 同列 = 替换（幂等）。
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 2,
                commands: vec![SpreadsheetCommand::UpsertFilterColumn {
                    sheet_id: "sheet-1".into(),
                    column: 1,
                    predicate: oo_schema::FilterPredicate::Contains("b".into()),
                }],
            })
            .unwrap();
        let filter = engine.model().sheets[0]
            .metadata
            .auto_filter
            .as_ref()
            .unwrap();
        assert_eq!(filter.columns.len(), 1);
        assert_eq!(
            filter.columns[0].predicate,
            oo_schema::FilterPredicate::Contains("b".into())
        );
        // undo 回到上一个谓词（精确恢复）。
        engine.undo().unwrap();
        let filter = engine.model().sheets[0]
            .metadata
            .auto_filter
            .as_ref()
            .unwrap();
        assert_eq!(
            filter.columns[0].predicate,
            oo_schema::FilterPredicate::Contains("a".into())
        );
        // 清空一列（revision 用引擎当前值：NoChanges/undo 不会线性推号）。
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: engine.revision(),
                commands: vec![SpreadsheetCommand::ClearFilterColumn {
                    sheet_id: "sheet-1".into(),
                    column: Some(1),
                }],
            })
            .unwrap();
        assert!(engine.model().sheets[0]
            .metadata
            .auto_filter
            .as_ref()
            .unwrap()
            .columns
            .is_empty());
    }

    #[test]
    fn dimension_commands_reject_freeze_conflicts_and_support_none_reset() {
        let mut engine = engine();
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![
                    SpreadsheetCommand::SetFreezePane {
                        sheet_id: "sheet-1".into(),
                        rows: 3,
                        columns: 2,
                    },
                    SpreadsheetCommand::SetRowDimensions {
                        sheet_id: "sheet-1".into(),
                        rows: Some(50),
                    },
                    SpreadsheetCommand::SetColumnDimensions {
                        sheet_id: "sheet-1".into(),
                        columns: Some(20),
                    },
                ],
            })
            .unwrap();
        // 行数 2 < 冻结行 3：拒绝。
        let conflict = engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 1,
                commands: vec![SpreadsheetCommand::SetRowDimensions {
                    sheet_id: "sheet-1".into(),
                    rows: Some(2),
                }],
            })
            .unwrap_err();
        assert!(matches!(
            conflict,
            SpreadsheetEngineError::InvalidMutation(_)
        ));
        let conflict = engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 1,
                commands: vec![SpreadsheetCommand::SetColumnDimensions {
                    sheet_id: "sheet-1".into(),
                    columns: Some(1),
                }],
            })
            .unwrap_err();
        assert!(matches!(
            conflict,
            SpreadsheetEngineError::InvalidMutation(_)
        ));
        assert_eq!(engine.model().sheets[0].metadata.row_count, Some(50));
        // None 重置维度。
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 1,
                commands: vec![SpreadsheetCommand::SetRowDimensions {
                    sheet_id: "sheet-1".into(),
                    rows: None,
                }],
            })
            .unwrap();
        assert_eq!(engine.model().sheets[0].metadata.row_count, None);
    }

    #[test]
    fn rule_crud_is_idempotent_by_id_and_undo_restores_previous_rule() {
        let mut engine = engine();
        let rule_v1 = oo_schema::ConditionalFormatRule {
            id: "cf-1".into(),
            range: GridRange {
                start_row: 0,
                start_column: 0,
                end_row: 5,
                end_column: 1,
            },
            predicate: oo_schema::ConditionalPredicate::CellIs {
                operator: oo_schema::ComparisonOperator::GreaterThan,
                value: serde_json::json!(0),
            },
            style: CellStyle::default(),
        };
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::UpsertConditionalFormat {
                    sheet_id: "sheet-1".into(),
                    rule: rule_v1.clone(),
                }],
            })
            .unwrap();
        // 相同 id 再次 upsert = 整体替换。
        let mut rule_v2 = rule_v1.clone();
        rule_v2.range = GridRange {
            start_row: 0,
            start_column: 0,
            end_row: 9,
            end_column: 2,
        };
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 1,
                commands: vec![SpreadsheetCommand::UpsertConditionalFormat {
                    sheet_id: "sheet-1".into(),
                    rule: rule_v2.clone(),
                }],
            })
            .unwrap();
        let rules = &engine.model().sheets[0].metadata.conditional_formats;
        assert_eq!(rules.len(), 1, "按 id 幂等");
        assert_eq!(rules[0].range.end_row, 9);
        // undo 恢复被覆盖的 v1（精确恢复，不是删除）。
        engine.undo().unwrap();
        let rules = &engine.model().sheets[0].metadata.conditional_formats;
        assert_eq!(rules[0].range.end_row, 5);
        // redo 回到 v2，再删除。
        engine.redo().unwrap();
        assert_eq!(
            engine.model().sheets[0].metadata.conditional_formats[0]
                .range
                .end_row,
            9
        );
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: engine.revision(),
                commands: vec![SpreadsheetCommand::DeleteConditionalFormat {
                    sheet_id: "sheet-1".into(),
                    rule_id: "cf-1".into(),
                }],
            })
            .unwrap();
        assert!(engine.model().sheets[0]
            .metadata
            .conditional_formats
            .is_empty());
        engine.undo().unwrap();
        assert_eq!(
            engine.model().sheets[0].metadata.conditional_formats.len(),
            1
        );

        // 数据校验同构。
        let dv = oo_schema::DataValidationRule {
            id: "dv-1".into(),
            range: GridRange {
                start_row: 0,
                start_column: 0,
                end_row: 5,
                end_column: 0,
            },
            kind: oo_schema::DataValidationKind::WholeNumber { min: 0, max: 10 },
            allow_blank: true,
            error_message: None,
        };
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: engine.revision(),
                commands: vec![SpreadsheetCommand::UpsertDataValidation {
                    sheet_id: "sheet-1".into(),
                    rule: dv.clone(),
                }],
            })
            .unwrap();
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: engine.revision(),
                commands: vec![SpreadsheetCommand::DeleteDataValidation {
                    sheet_id: "sheet-1".into(),
                    rule_id: "dv-1".into(),
                }],
            })
            .unwrap();
        assert!(engine.model().sheets[0]
            .metadata
            .data_validations
            .is_empty());
        engine.undo().unwrap();
        assert_eq!(engine.model().sheets[0].metadata.data_validations.len(), 1);
    }

    #[test]
    fn calculation_mode_is_a_workbook_level_atomic_toggle() {
        let mut engine = engine();
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::SetCalculationMode {
                    calculation_mode: CalculationMode::Manual,
                }],
            })
            .unwrap();
        assert_eq!(
            engine.model().metadata.calculation_mode,
            CalculationMode::Manual
        );
        engine.undo().unwrap();
        assert_eq!(
            engine.model().metadata.calculation_mode,
            CalculationMode::Automatic
        );
    }

    #[test]
    fn paste_range_is_one_reversible_mutation_and_translates_formulas() {
        let mut engine = engine();
        let style = CellStyle {
            number_format: Some("0.00".into()),
            ..CellStyle::default()
        };
        let change = engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::PasteRange {
                    sheet_id: "sheet-1".into(),
                    start_row: 1,
                    start_column: 2,
                    row_count: 100,
                    column_count: 100,
                    cells: vec![SpreadsheetClipboardCell {
                        row_offset: 0,
                        column_offset: 0,
                        value: None,
                        formula: Some("=A1+$B1+C$1+$D$1+'Other Sheet'!E2".into()),
                        attrs: Map::new(),
                        style: Some(style),
                    }],
                    mode: SpreadsheetPasteMode::All,
                    source_origin: Some(SpreadsheetClipboardOrigin { row: 0, column: 0 }),
                }],
            })
            .unwrap();
        assert_eq!(change.mutations.len(), 1);
        assert!(matches!(
            change.mutations[0],
            SpreadsheetMutation::RangeChanged { .. }
        ));
        let pasted = &engine.model().sheets[0].cells[0];
        assert_eq!(
            pasted.formula.as_deref(),
            Some("=C2+$B2+E$1+$D$1+'Other Sheet'!G3")
        );
        assert_eq!(
            pasted
                .style
                .as_ref()
                .and_then(|style| style.number_format.as_deref()),
            Some("0.00")
        );
        engine.undo().unwrap();
        assert!(engine.model().sheets[0].cells.is_empty());
        engine.redo().unwrap();
        assert_eq!(engine.model().sheets[0].cells.len(), 1);
    }

    #[test]
    fn fill_range_repeats_sparse_source_as_one_history_entry() {
        let mut engine = engine();
        engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 0,
                commands: vec![SpreadsheetCommand::SetCell {
                    sheet_id: "sheet-1".into(),
                    row: 0,
                    column: 0,
                    value: Some(serde_json::json!(1)),
                    formula: Some("=B1".into()),
                    attrs: Map::new(),
                }],
            })
            .unwrap();
        let change = engine
            .execute(SpreadsheetCommandBatch {
                base_revision: 1,
                commands: vec![SpreadsheetCommand::FillRange {
                    source_sheet_id: "sheet-1".into(),
                    source_range: GridRange {
                        start_row: 0,
                        start_column: 0,
                        end_row: 0,
                        end_column: 0,
                    },
                    destination_sheet_id: "sheet-1".into(),
                    destination_range: GridRange {
                        start_row: 1,
                        start_column: 0,
                        end_row: 10_000,
                        end_column: 0,
                    },
                    mode: SpreadsheetPasteMode::All,
                }],
            })
            .unwrap();
        assert_eq!(change.mutations.len(), 1);
        assert_eq!(engine.model().sheets[0].cells.len(), 10_001);
        assert_eq!(
            engine.model().sheets[0].cells[1].formula.as_deref(),
            Some("=B2")
        );
        assert_eq!(
            engine.model().sheets[0].cells[10_000].formula.as_deref(),
            Some("=B10001")
        );
        engine.undo().unwrap();
        assert_eq!(engine.model().sheets[0].cells.len(), 1);
    }
}
