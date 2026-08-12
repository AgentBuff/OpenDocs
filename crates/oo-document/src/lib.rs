//! Document Artifact 的 Block Tree engine。
//!
//! 这个 crate 只处理文档结构和事务，不知道 React、DOM、Canvas 或 WASM。UI、协同层和
//! 服务端都通过 [`DocumentCommandBatch`] 进入同一条原子路径；排版层只消费提交后的
//! [`oo_schema::DocumentModel`]，不会在渲染器里复制一套 block 业务状态。

use std::collections::HashSet;

use oo_schema::{
    BlockAlignment, BlockData, BlockId, BlockPresentation, CodeBlockConfig, Color, DocumentBlock,
    DocumentBlockKind, DocumentModel, ImageBlock, ImageTransform, InlineRun, InlineStyle,
    LinkBlock, ListPresentation, PageSetup, ParagraphStyleRef, RichText, SchemaValidationError,
    TableBlock, TableBorder, TableCell, TableColumn, TableRange, TableRow, TodoBlock,
    VerticalAlign,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

mod change;
mod mutation;
mod store;
pub mod table_grid;

pub use change::{ChangeSet, MutationJournal};
pub use mutation::{Mutation, RemovedBlock};
use store::BlockStore;
pub use table_grid::{
    GridBounds, GridCellTarget, TableCellSelection, TableGridProjection, TableGridQueryError,
};

/// 当前文档 engine 的一次原子事务。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentCommandBatch {
    pub base_revision: u64,
    pub commands: Vec<DocumentCommand>,
}

/// 结构命令与样式/内容命令分开，避免用一个「万能 update」绕过 Block Tree 不变量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DocumentCommand {
    InsertBlock {
        block: DocumentBlock,
        #[serde(default)]
        parent_id: Option<BlockId>,
        index: usize,
    },
    /// Insert a quote block with typed text content. This keeps common Word insertion
    /// intents discoverable without making clients construct the full BlockData envelope.
    InsertQuote {
        block_id: BlockId,
        content: RichText,
        #[serde(default)]
        parent_id: Option<BlockId>,
        index: usize,
    },
    /// Insert a todo block with its persisted completion state.
    InsertTodo {
        block_id: BlockId,
        content: RichText,
        #[serde(default)]
        checked: bool,
        #[serde(default)]
        parent_id: Option<BlockId>,
        index: usize,
    },
    /// Insert a link block with a typed target URL.
    InsertLink {
        block_id: BlockId,
        content: RichText,
        url: String,
        #[serde(default)]
        parent_id: Option<BlockId>,
        index: usize,
    },
    /// Insert a structural divider. Divider blocks intentionally have no RichText content.
    InsertDivider {
        block_id: BlockId,
        #[serde(default)]
        parent_id: Option<BlockId>,
        index: usize,
    },
    /// Patch only the well-known block presentation fields.  The generic
    /// block update command was intentionally removed; editor and API clients
    /// use semantic commands for each mutation surface.
    SetBlockPresentation {
        block_id: BlockId,
        patch: BlockPresentationPatch,
    },
    /// Patch only the selected RichText runs. The range is expressed in
    /// Unicode scalar offsets, matching the persisted RichText contract.
    PatchInlineRange {
        block_id: BlockId,
        range: TextRange,
        patch: InlineStylePatch,
    },
    DeleteBlock {
        block_id: BlockId,
    },
    /// Reset a block to an empty paragraph. This is used by document-level
    /// clear actions and is deliberately narrower than the retired generic
    /// update command.
    ResetBlock {
        block_id: BlockId,
    },
    MoveBlock {
        block_id: BlockId,
        #[serde(default)]
        parent_id: Option<BlockId>,
        index: usize,
    },
    SetPageSetup {
        page_setup: Option<PageSetup>,
    },
    FormatTableCells {
        block_id: BlockId,
        selection: TableCellSelection,
        patch: TableCellFormatPatch,
    },
    SetTableBorders {
        block_id: BlockId,
        selection: TableCellSelection,
        patch: TableBorderPatch,
    },
    /// Apply a grid-aware border preset to a stable-id selection. The engine
    /// resolves outer and internal edges against the canonical table grid;
    /// callers never emulate this by issuing per-cell commands.
    ApplyTableBorderPreset {
        block_id: BlockId,
        selection: TableCellSelection,
        preset: TableBorderPreset,
        #[serde(default)]
        border: Option<TableBorder>,
    },
    SetTodoChecked {
        block_id: BlockId,
        checked: bool,
    },
    ConvertToLink {
        block_id: BlockId,
        url: String,
    },
    SetLinkTarget {
        block_id: BlockId,
        url: String,
    },
    SetCodeConfig {
        block_id: BlockId,
        config: CodeBlockConfig,
    },
    /// Patch persistent image state. Image transformations, derived assets and
    /// captions are image-domain data, never a renderer-local style map.
    SetImageConfig {
        block_id: BlockId,
        patch: ImageBlockPatch,
    },
    ReplaceBlockText {
        block_id: BlockId,
        content: RichText,
    },
    ConvertBlock {
        block_id: BlockId,
        kind: DocumentBlockKind,
    },
    ReplaceTableCellText {
        block_id: BlockId,
        row_id: String,
        cell_id: String,
        content: RichText,
    },
    /// Patch only the selected RichText runs inside one stable table cell.
    /// Cell-level formatting remains FormatTableCells; this command exists so
    /// a native text selection never degrades into formatting the whole cell.
    PatchTableCellInlineRange {
        block_id: BlockId,
        row_id: String,
        cell_id: String,
        range: TextRange,
        patch: InlineStylePatch,
    },
    InsertTableRow {
        block_id: BlockId,
        index: usize,
        row: TableRow,
    },
    InsertTableColumn {
        block_id: BlockId,
        index: usize,
        column: TableColumn,
        cells: Vec<TableCell>,
    },
    DeleteTableRow {
        block_id: BlockId,
        row_id: String,
    },
    DeleteTableColumn {
        block_id: BlockId,
        column_id: String,
    },
    SetTableColumnWidth {
        block_id: BlockId,
        column_id: String,
        width: f32,
    },
    SetTableRowHeight {
        block_id: BlockId,
        row_id: String,
        height: f32,
    },
    MergeTableCells {
        block_id: BlockId,
        range: TableRange,
    },
    SplitTableCells {
        block_id: BlockId,
        range: TableRange,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageBlockPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    /// `Some(None)` clears the restore target after the original is restored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_asset_id: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<ImageTransform>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableCellFormatPatch {
    /// Inline attributes are applied to every rich-text run in the targeted
    /// cells. This is intentionally a patch rather than a second text model.
    #[serde(default)]
    pub text_attrs: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_color: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horizontal_align: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical_align: Option<String>,
}

/// Border writes are edge-level patches. An omitted edge is untouched, while
/// an explicit `null` clears the edge. This keeps toolbar actions composable
/// for arbitrary stable-id cell/range selections.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableBorderPatch {
    #[serde(default)]
    pub top: Option<Option<TableBorder>>,
    #[serde(default)]
    pub right: Option<Option<TableBorder>>,
    #[serde(default)]
    pub bottom: Option<Option<TableBorder>>,
    #[serde(default)]
    pub left: Option<Option<TableBorder>>,
    #[serde(default)]
    pub diagonal_down: Option<Option<TableBorder>>,
    #[serde(default)]
    pub diagonal_up: Option<Option<TableBorder>>,
}

impl TableBorderPatch {
    fn is_empty(&self) -> bool {
        self.top.is_none()
            && self.right.is_none()
            && self.bottom.is_none()
            && self.left.is_none()
            && self.diagonal_down.is_none()
            && self.diagonal_up.is_none()
    }

    fn validate(&self) -> Result<(), DocumentEngineError> {
        if self.is_empty() {
            return Err(DocumentEngineError::InvalidTableBorderPatch(
                "至少设置或清除一条边框".into(),
            ));
        }
        for border in [
            &self.top,
            &self.right,
            &self.bottom,
            &self.left,
            &self.diagonal_down,
            &self.diagonal_up,
        ]
        .into_iter()
        .filter_map(|edge| edge.as_ref().and_then(Option::as_ref))
        {
            if !is_hex_color(&border.color) {
                return Err(DocumentEngineError::InvalidTableBorderPatch(
                    "边框颜色必须是 #RRGGBB 或 #RRGGBBAA".into(),
                ));
            }
            if !border.width.is_finite() || border.width <= 0.0 || border.width > 32.0 {
                return Err(DocumentEngineError::InvalidTableBorderPatch(
                    "边框宽度必须大于 0 且不超过 32".into(),
                ));
            }
        }
        Ok(())
    }
}

/// Table border gallery vocabulary. These values are semantic operations,
/// not UI labels: the engine calculates which physical cell edges belong to
/// the selected grid bounds in one atomic document transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TableBorderPreset {
    Top,
    Right,
    Bottom,
    Left,
    None,
    All,
    Outer,
    Inner,
    InnerHorizontal,
    InnerVertical,
    DiagonalDown,
    DiagonalUp,
}

/// Incremental paragraph presentation patch. `Some(Some(value))` sets a
/// field, `Some(None)` clears it, and `None` leaves it unchanged.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockPresentationPatch {
    #[serde(default)]
    pub align: Option<Option<BlockAlignment>>,
    #[serde(default)]
    pub list: Option<Option<ListPresentation>>,
    #[serde(default)]
    pub indent_start: Option<Option<u8>>,
    #[serde(default)]
    pub indent_end: Option<Option<f32>>,
    #[serde(default)]
    pub spacing_before: Option<Option<f32>>,
    #[serde(default)]
    pub spacing_after: Option<Option<f32>>,
    #[serde(default)]
    pub line_height: Option<Option<f32>>,
    /// Named paragraph style reference; this is separate from direct formatting so style
    /// selection can be cleared without mutating the other presentation fields.
    #[serde(default)]
    pub named_style: Option<Option<ParagraphStyleRef>>,
}

/// A half-open Unicode scalar range in a block's RichText.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextRange {
    pub start: usize,
    pub end: usize,
}

/// Canonical target for a text-range style patch inside one stable table
/// cell. Keeping the target as one value prevents the engine's private write
/// boundary from drifting into an unstructured list of coordinates.
#[derive(Debug, Clone, PartialEq)]
struct TableCellInlineRangePatch {
    block_id: BlockId,
    row_id: String,
    cell_id: String,
    range: TextRange,
    patch: InlineStylePatch,
}

impl TextRange {
    fn validate(&self, text_len: usize) -> Result<(), DocumentEngineError> {
        if self.start >= self.end || self.end > text_len {
            return Err(DocumentEngineError::InvalidInlineRange {
                start: self.start,
                end: self.end,
                text_len,
            });
        }
        Ok(())
    }
}

/// Strict, tri-state patch for the supported inline presentation fields.
/// Omitted fields are untouched; `null` removes the field from selected runs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineStylePatch {
    #[serde(default)]
    pub bold: Option<Option<bool>>,
    #[serde(default)]
    pub italic: Option<Option<bool>>,
    #[serde(default)]
    pub underline: Option<Option<bool>>,
    #[serde(default)]
    pub strikethrough: Option<Option<bool>>,
    #[serde(default)]
    pub font_family: Option<Option<String>>,
    #[serde(default)]
    pub font_size: Option<Option<f32>>,
    #[serde(default)]
    pub color: Option<Option<String>>,
    #[serde(default)]
    pub highlight: Option<Option<String>>,
    #[serde(default)]
    pub vertical_align: Option<Option<VerticalAlign>>,
}

impl InlineStylePatch {
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

    fn validate(&self) -> Result<(), DocumentEngineError> {
        if self.is_empty() {
            return Err(DocumentEngineError::InvalidInlinePatch(
                "至少设置或清除一个行内样式字段".into(),
            ));
        }
        for (name, value) in [
            ("fontFamily", &self.font_family),
            ("color", &self.color),
            ("highlight", &self.highlight),
        ] {
            if value
                .as_ref()
                .and_then(|item| item.as_ref())
                .is_some_and(|text| text.trim().is_empty())
            {
                return Err(DocumentEngineError::InvalidInlinePatch(format!(
                    "{name} 不能是空字符串"
                )));
            }
            if value
                .as_ref()
                .and_then(|item| item.as_ref())
                .is_some_and(|text| Color::new(text.clone()).is_err())
            {
                return Err(DocumentEngineError::InvalidInlinePatch(format!(
                    "{name} 必须是合法的 hex/rgb 颜色"
                )));
            }
        }
        if self
            .font_size
            .as_ref()
            .and_then(|value| value.as_ref())
            .is_some_and(|number| !number.is_finite() || *number <= 0.0 || *number > 512.0)
        {
            return Err(DocumentEngineError::InvalidInlinePatch(
                "fontSize 必须大于 0 且不超过 512".into(),
            ));
        }
        Ok(())
    }
}

impl BlockPresentationPatch {
    fn validate(&self) -> Result<(), DocumentEngineError> {
        if self
            .indent_start
            .as_ref()
            .and_then(|value| value.as_ref())
            .is_some_and(|value| *value > 20)
        {
            return Err(DocumentEngineError::InvalidBlockPresentation(
                "indentStart 必须在 0 到 20 之间".into(),
            ));
        }
        for (name, value) in [
            ("indentEnd", &self.indent_end),
            ("spacingBefore", &self.spacing_before),
            ("spacingAfter", &self.spacing_after),
        ] {
            if value
                .as_ref()
                .and_then(|item| item.as_ref())
                .is_some_and(|number| !number.is_finite() || *number < 0.0)
            {
                return Err(DocumentEngineError::InvalidBlockPresentation(format!(
                    "{name} 必须是非负有限数"
                )));
            }
        }
        if self
            .line_height
            .as_ref()
            .and_then(|value| value.as_ref())
            .is_some_and(|number| !number.is_finite() || *number <= 0.0 || *number > 10.0)
        {
            return Err(DocumentEngineError::InvalidBlockPresentation(
                "lineHeight 必须大于 0 且不超过 10".into(),
            ));
        }
        if self
            .named_style
            .as_ref()
            .and_then(|value| value.as_ref())
            .is_some_and(|style| style.name.trim().is_empty() || style.name.chars().count() > 256)
        {
            return Err(DocumentEngineError::InvalidBlockPresentation(
                "namedStyle.name 必须是 1 到 256 个字符".into(),
            ));
        }
        Ok(())
    }
}

/// 持有已校验模型和服务端/engine revision 的纯结构状态。
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentEngine {
    store: BlockStore,
    revision: u64,
    journal: MutationJournal,
}

#[derive(Debug, Default)]
struct ChangeTracker {
    changed_blocks: HashSet<BlockId>,
    changed_containers: HashSet<BlockId>,
    structure_changed: bool,
}

impl DocumentEngine {
    pub fn new(model: DocumentModel, revision: u64) -> Result<Self, DocumentEngineError> {
        Ok(Self {
            store: BlockStore::new(model)?,
            revision,
            journal: MutationJournal::default(),
        })
    }

    pub fn model(&self) -> &DocumentModel {
        self.store.model()
    }

    /// Reads one canonical block through the indexed BlockId lookup.
    ///
    /// Browser callers use this after a ChangeSet has identified an invalidated block. Keeping
    /// the lookup in the engine avoids rebuilding a second BlockId index in the WASM adapter.
    pub fn read_block(&self, block_id: &str) -> Result<&DocumentBlock, DocumentEngineError> {
        self.store.block(block_id)
    }

    /// Reads changed blocks in the requested order through one engine-side batch.
    ///
    /// The batch is intentionally read-only and does not return a snapshot. It exists to keep
    /// the WASM boundary at one JSON crossing for a ChangeSet with multiple invalidated blocks.
    pub fn read_blocks<'a>(
        &'a self,
        block_ids: &[BlockId],
    ) -> Result<Vec<&'a DocumentBlock>, DocumentEngineError> {
        block_ids.iter().map(|id| self.read_block(id)).collect()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn journal(&self) -> &MutationJournal {
        &self.journal
    }

    /// Executes a semantic command batch directly against the live BlockStore and records inverses
    /// in a local journal. Any failed command (including final schema validation) replays that journal in
    /// reverse order, so the live model and indexes remain unchanged without cloning the model.
    pub fn execute(
        &mut self,
        batch: DocumentCommandBatch,
    ) -> Result<ChangeSet, DocumentEngineError> {
        if batch.commands.is_empty() {
            return Err(DocumentEngineError::EmptyTransaction);
        }
        if batch.base_revision != self.revision {
            return Err(DocumentEngineError::RevisionConflict {
                expected: self.revision,
                actual: batch.base_revision,
            });
        }
        // Avoid mutating and then rolling back a transaction that can never receive a revision.
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(DocumentEngineError::RevisionOverflow)?;

        let mut journal = Vec::new();
        let mut changes = ChangeTracker::default();
        for command in batch.commands {
            let result = self.apply_command(command, &mut journal, &mut changes);
            if let Err(error) = result {
                self.rollback(&journal);
                return Err(error);
            }
        }

        if let Err(error) = self.store.model.validate() {
            self.rollback(&journal);
            return Err(error.into());
        }
        let mut mutations = journal;
        mutations.shrink_to_fit();
        self.journal.push(mutations.clone());
        self.revision = revision;
        let mut changed_blocks: Vec<_> = changes.changed_blocks.into_iter().collect();
        let mut changed_containers: Vec<_> = changes.changed_containers.into_iter().collect();
        changed_blocks.sort();
        changed_containers.sort();
        Ok(ChangeSet {
            revision,
            changed_blocks,
            changed_containers,
            structure_changed: changes.structure_changed,
            mutations,
        })
    }

    /// Reverses the latest committed transaction. Undo is an engine operation rather than a
    /// frontend-side synthetic transaction, so the exact mutation journal (including nested
    /// subtree records) remains the source of truth.
    pub fn undo(&mut self) -> Result<ChangeSet, DocumentEngineError> {
        let revision = self.next_revision()?;
        let entry = self
            .journal
            .pop_undo()
            .ok_or(DocumentEngineError::NothingToUndo)?;
        let inverse: Vec<Mutation> = entry.iter().rev().map(Mutation::inverse).collect();
        for mutation in entry.iter().rev() {
            self.apply_inverse(mutation);
        }
        if let Err(error) = self.store.model.validate() {
            // A valid committed journal should never fail here. Replaying forward restores the
            // pre-undo state without cloning the full document, and the history entry is put back.
            for mutation in &entry {
                self.apply_forward(mutation);
            }
            self.journal.restore_undo(entry);
            return Err(error.into());
        }
        self.journal.push_redo(entry);
        self.revision = revision;
        Ok(self.change_set(revision, inverse))
    }

    /// Reapplies the latest undone transaction, generating a fresh mutation sequence against the
    /// current model so positions and deleted subtree snapshots are accurate.
    pub fn redo(&mut self) -> Result<ChangeSet, DocumentEngineError> {
        let revision = self.next_revision()?;
        let entry = self
            .journal
            .pop_redo()
            .ok_or(DocumentEngineError::NothingToRedo)?;
        let mut applied = Vec::new();
        let mut changes = ChangeTracker::default();
        for mutation in &entry {
            if let Mutation::Update {
                block_id, after, ..
            } = mutation
            {
                let before = self.store.block(block_id)?.clone();
                *self.store.block_mut(block_id)? = (**after).clone();
                applied.push(Mutation::Update {
                    block_id: block_id.clone(),
                    before: Box::new(before),
                    after: Box::new((**after).clone()),
                });
                changes.changed_blocks.insert(block_id.clone());
                continue;
            }
            let Some(command) = mutation_to_command(mutation) else {
                self.rollback(&applied);
                self.journal.restore_redo(entry);
                return Err(DocumentEngineError::InvalidHistoryMutation);
            };
            if let Err(error) = self.apply_command(command, &mut applied, &mut changes) {
                self.rollback(&applied);
                self.journal.restore_redo(entry);
                return Err(error);
            }
        }
        if let Err(error) = self.store.model.validate() {
            self.rollback(&applied);
            self.journal.restore_redo(entry);
            return Err(error.into());
        }
        self.journal.push_undo(applied.clone());
        self.revision = revision;
        Ok(self.change_set(revision, applied))
    }

    fn next_revision(&self) -> Result<u64, DocumentEngineError> {
        self.revision
            .checked_add(1)
            .ok_or(DocumentEngineError::RevisionOverflow)
    }

    fn change_set(&self, revision: u64, mutations: Vec<Mutation>) -> ChangeSet {
        let mut changes = ChangeTracker::default();
        summarize_mutations(&mut changes, &mutations);
        let mut changed_blocks: Vec<_> = changes.changed_blocks.into_iter().collect();
        let mut changed_containers: Vec<_> = changes.changed_containers.into_iter().collect();
        changed_blocks.sort();
        changed_containers.sort();
        ChangeSet {
            revision,
            changed_blocks,
            changed_containers,
            structure_changed: changes.structure_changed,
            mutations,
        }
    }

    fn apply_command(
        &mut self,
        command: DocumentCommand,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        match command {
            DocumentCommand::InsertBlock {
                block,
                parent_id,
                index,
            } => self.insert_block(block, parent_id, index, journal, changes),
            DocumentCommand::InsertQuote {
                block_id,
                content,
                parent_id,
                index,
            } => self.insert_semantic_block(
                DocumentBlock {
                    id: block_id,
                    kind: DocumentBlockKind::Quote,
                    presentation: BlockPresentation::default(),
                    content: Some(content),
                    children: Vec::new(),
                    data: BlockData::None,
                },
                parent_id,
                index,
                journal,
                changes,
            ),
            DocumentCommand::InsertTodo {
                block_id,
                content,
                checked,
                parent_id,
                index,
            } => self.insert_semantic_block(
                DocumentBlock {
                    id: block_id,
                    kind: DocumentBlockKind::Todo,
                    presentation: BlockPresentation::default(),
                    content: Some(content),
                    children: Vec::new(),
                    data: BlockData::Todo(TodoBlock { checked }),
                },
                parent_id,
                index,
                journal,
                changes,
            ),
            DocumentCommand::InsertLink {
                block_id,
                content,
                url,
                parent_id,
                index,
            } => {
                validate_link_url(&url)?;
                self.insert_semantic_block(
                    DocumentBlock {
                        id: block_id,
                        kind: DocumentBlockKind::Link,
                        presentation: BlockPresentation::default(),
                        content: Some(content),
                        children: Vec::new(),
                        data: BlockData::Link(LinkBlock { url }),
                    },
                    parent_id,
                    index,
                    journal,
                    changes,
                )
            }
            DocumentCommand::InsertDivider {
                block_id,
                parent_id,
                index,
            } => self.insert_semantic_block(
                DocumentBlock {
                    id: block_id,
                    kind: DocumentBlockKind::Divider,
                    presentation: BlockPresentation::default(),
                    content: None,
                    children: Vec::new(),
                    data: BlockData::None,
                },
                parent_id,
                index,
                journal,
                changes,
            ),
            DocumentCommand::SetBlockPresentation { block_id, patch } => {
                self.set_block_presentation(block_id, patch, journal, changes)
            }
            DocumentCommand::PatchInlineRange {
                block_id,
                range,
                patch,
            } => self.patch_inline_range(block_id, range, patch, journal, changes),
            DocumentCommand::DeleteBlock { block_id } => {
                self.delete_block(&block_id, journal, changes)
            }
            DocumentCommand::ResetBlock { block_id } => {
                self.reset_block(block_id, journal, changes)
            }
            DocumentCommand::MoveBlock {
                block_id,
                parent_id,
                index,
            } => self.move_block(block_id, parent_id, index, journal, changes),
            DocumentCommand::SetPageSetup { page_setup } => {
                let before = self.store.model.page_setup.clone();
                self.store.model.page_setup = page_setup.clone();
                journal.push(Mutation::SetPageSetup {
                    before,
                    after: page_setup,
                });
                changes.structure_changed = true;
                Ok(())
            }
            DocumentCommand::FormatTableCells {
                block_id,
                selection,
                patch,
            } => self.format_table_cells(block_id, selection, patch, journal, changes),
            DocumentCommand::SetTableBorders {
                block_id,
                selection,
                patch,
            } => self.set_table_borders(block_id, selection, patch, journal, changes),
            DocumentCommand::ApplyTableBorderPreset {
                block_id,
                selection,
                preset,
                border,
            } => self
                .apply_table_border_preset(block_id, selection, preset, border, journal, changes),
            DocumentCommand::SetTodoChecked { block_id, checked } => {
                self.set_todo_checked(block_id, checked, journal, changes)
            }
            DocumentCommand::ConvertToLink { block_id, url } => {
                self.convert_to_link(block_id, url, journal, changes)
            }
            DocumentCommand::SetLinkTarget { block_id, url } => {
                self.set_link_target(block_id, url, journal, changes)
            }
            DocumentCommand::SetCodeConfig { block_id, config } => {
                self.set_code_config(block_id, config, journal, changes)
            }
            DocumentCommand::SetImageConfig { block_id, patch } => {
                self.set_image_config(block_id, patch, journal, changes)
            }
            DocumentCommand::ReplaceBlockText { block_id, content } => {
                self.replace_block_text(block_id, content, journal, changes)
            }
            DocumentCommand::ConvertBlock { block_id, kind } => {
                self.convert_block(block_id, kind, journal, changes)
            }
            DocumentCommand::ReplaceTableCellText {
                block_id,
                row_id,
                cell_id,
                content,
            } => self.replace_table_cell_text(block_id, row_id, cell_id, content, journal, changes),
            DocumentCommand::PatchTableCellInlineRange {
                block_id,
                row_id,
                cell_id,
                range,
                patch,
            } => self.patch_table_cell_inline_range(
                TableCellInlineRangePatch {
                    block_id,
                    row_id,
                    cell_id,
                    range,
                    patch,
                },
                journal,
                changes,
            ),
            DocumentCommand::InsertTableRow {
                block_id,
                index,
                row,
            } => self.insert_table_row(block_id, index, row, journal, changes),
            DocumentCommand::InsertTableColumn {
                block_id,
                index,
                column,
                cells,
            } => self.insert_table_column(block_id, index, column, cells, journal, changes),
            DocumentCommand::DeleteTableRow { block_id, row_id } => {
                self.delete_table_row(block_id, row_id, journal, changes)
            }
            DocumentCommand::DeleteTableColumn {
                block_id,
                column_id,
            } => self.delete_table_column(block_id, column_id, journal, changes),
            DocumentCommand::SetTableColumnWidth {
                block_id,
                column_id,
                width,
            } => self.set_table_column_width(block_id, column_id, width, journal, changes),
            DocumentCommand::SetTableRowHeight {
                block_id,
                row_id,
                height,
            } => self.set_table_row_height(block_id, row_id, height, journal, changes),
            DocumentCommand::MergeTableCells { block_id, range } => {
                self.merge_table_cells(block_id, range, journal, changes)
            }
            DocumentCommand::SplitTableCells { block_id, range } => {
                self.split_table_cells(block_id, range, journal, changes)
            }
        }
    }

    fn set_todo_checked(
        &mut self,
        block_id: BlockId,
        checked: bool,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        if current.kind != DocumentBlockKind::Todo {
            return Err(DocumentEngineError::NotTodoBlock(block_id));
        }
        let BlockData::Todo(todo) = &mut current.data else {
            return Err(DocumentEngineError::NotTodoBlock(block_id));
        };
        todo.checked = checked;
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn insert_semantic_block(
        &mut self,
        block: DocumentBlock,
        parent_id: Option<BlockId>,
        index: usize,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        self.insert_block(block, parent_id, index, journal, changes)
    }

    fn set_block_presentation(
        &mut self,
        block_id: BlockId,
        patch: BlockPresentationPatch,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        patch.validate()?;
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        apply_block_presentation_patch(&mut current.presentation, patch);
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn patch_inline_range(
        &mut self,
        block_id: BlockId,
        range: TextRange,
        patch: InlineStylePatch,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        patch.validate()?;
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        let Some(content) = current.content.as_mut() else {
            return Err(DocumentEngineError::NotTextBlock(block_id));
        };
        let text_len = content.text.chars().count();
        range.validate(text_len)?;
        patch_rich_text_runs(content, range, patch);
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn convert_to_link(
        &mut self,
        block_id: BlockId,
        url: String,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        validate_link_url(&url)?;
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        if current.content.is_none() {
            return Err(DocumentEngineError::NotTextBlock(block_id));
        }
        current.kind = DocumentBlockKind::Link;
        current.data = BlockData::Link(LinkBlock { url });
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn set_link_target(
        &mut self,
        block_id: BlockId,
        url: String,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        validate_link_url(&url)?;
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        if current.kind != DocumentBlockKind::Link {
            return Err(DocumentEngineError::NotLinkBlock(block_id));
        }
        let BlockData::Link(link) = &mut current.data else {
            return Err(DocumentEngineError::NotLinkBlock(block_id));
        };
        link.url = url;
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn set_code_config(
        &mut self,
        block_id: BlockId,
        config: CodeBlockConfig,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        if current.kind != DocumentBlockKind::Code {
            return Err(DocumentEngineError::NotCodeBlock(block_id));
        }
        let BlockData::Code(current_config) = &mut current.data else {
            return Err(DocumentEngineError::NotCodeBlock(block_id));
        };
        *current_config = config;
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn set_image_config(
        &mut self,
        block_id: BlockId,
        patch: ImageBlockPatch,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        if patch == ImageBlockPatch::default() {
            return Ok(());
        }
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        if current.kind != DocumentBlockKind::Image {
            return Err(DocumentEngineError::NotImageBlock(block_id));
        }
        let BlockData::Image(image) = &mut current.data else {
            return Err(DocumentEngineError::NotImageBlock(block_id));
        };
        apply_image_patch(image, patch, &block_id)?;
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn replace_block_text(
        &mut self,
        block_id: BlockId,
        content: RichText,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        if current.content.is_none() {
            return Err(DocumentEngineError::NotTextBlock(block_id));
        }
        current.content = Some(content);
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn convert_block(
        &mut self,
        block_id: BlockId,
        kind: DocumentBlockKind,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        if current.content.is_none() {
            return Err(DocumentEngineError::NotTextBlock(block_id));
        }
        let data = match &kind {
            DocumentBlockKind::Paragraph
            | DocumentBlockKind::Heading { .. }
            | DocumentBlockKind::Quote
            | DocumentBlockKind::Callout => BlockData::None,
            DocumentBlockKind::Todo => match &current.data {
                BlockData::Todo(todo) => BlockData::Todo(todo.clone()),
                _ => BlockData::Todo(Default::default()),
            },
            DocumentBlockKind::Code => match &current.data {
                BlockData::Code(config) => BlockData::Code(config.clone()),
                _ => BlockData::Code(Default::default()),
            },
            DocumentBlockKind::Link
            | DocumentBlockKind::Image
            | DocumentBlockKind::Table
            | DocumentBlockKind::Divider
            | DocumentBlockKind::Page
            | DocumentBlockKind::Columns
            | DocumentBlockKind::Column
            | DocumentBlockKind::Extension { .. }
            | DocumentBlockKind::Unknown { .. } => {
                return Err(DocumentEngineError::UnsupportedBlockConversion(block_id));
            }
        };
        current.kind = kind;
        current.data = data;
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn replace_table_cell_text(
        &mut self,
        block_id: BlockId,
        row_id: String,
        cell_id: String,
        content: RichText,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        if current.kind != DocumentBlockKind::Table {
            return Err(DocumentEngineError::NotTableBlock(block_id));
        }
        let BlockData::Table(table) = &mut current.data else {
            return Err(DocumentEngineError::NotTableBlock(block_id));
        };
        let row = table
            .rows
            .iter_mut()
            .find(|row| row.id == row_id)
            .ok_or_else(|| {
                DocumentEngineError::InvalidTableSelection(format!("不存在行 {row_id}"))
            })?;
        let cell = row
            .cells
            .iter_mut()
            .find(|cell| cell.id == cell_id)
            .ok_or_else(|| {
                DocumentEngineError::InvalidTableSelection(format!("不存在单元格 {cell_id}"))
            })?;
        cell.content = content;
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn patch_table_cell_inline_range(
        &mut self,
        request: TableCellInlineRangePatch,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        let TableCellInlineRangePatch {
            block_id,
            row_id,
            cell_id,
            range,
            patch,
        } = request;
        patch.validate()?;
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        let BlockData::Table(table) = &mut current.data else {
            return Err(DocumentEngineError::NotTableBlock(block_id));
        };
        let row = table
            .rows
            .iter_mut()
            .find(|row| row.id == row_id)
            .ok_or_else(|| {
                DocumentEngineError::InvalidTableSelection(format!("不存在行 {row_id}"))
            })?;
        let cell = row
            .cells
            .iter_mut()
            .find(|cell| cell.id == cell_id)
            .ok_or_else(|| {
                DocumentEngineError::InvalidTableSelection(format!("不存在单元格 {cell_id}"))
            })?;
        range.validate(cell.content.text.chars().count())?;
        patch_rich_text_runs(&mut cell.content, range, patch);
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn insert_table_row(
        &mut self,
        block_id: BlockId,
        index: usize,
        row: TableRow,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        let BlockData::Table(table) = &mut current.data else {
            return Err(DocumentEngineError::NotTableBlock(block_id));
        };
        if table.rows.len() >= 100
            || row.cells.len() != table.columns.len()
            || index > table.rows.len()
        {
            return Err(DocumentEngineError::InvalidTableSelection(
                "无效的表格行插入".into(),
            ));
        }
        table.rows.insert(index, row);
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn insert_table_column(
        &mut self,
        block_id: BlockId,
        index: usize,
        column: TableColumn,
        cells: Vec<TableCell>,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        let BlockData::Table(table) = &mut current.data else {
            return Err(DocumentEngineError::NotTableBlock(block_id));
        };
        if table.columns.len() >= 20
            || cells.len() != table.rows.len()
            || index > table.columns.len()
        {
            return Err(DocumentEngineError::InvalidTableSelection(
                "无效的表格列插入".into(),
            ));
        }
        table.columns.insert(index, column);
        for (row, cell) in table.rows.iter_mut().zip(cells) {
            row.cells.insert(index, cell);
        }
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn set_table_column_width(
        &mut self,
        block_id: BlockId,
        column_id: String,
        width: f32,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        if !width.is_finite() || !(32.0..=2_000.0).contains(&width) {
            return Err(DocumentEngineError::InvalidTableSelection(
                "表格列宽必须在 32 到 2000 像素之间".into(),
            ));
        }
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        let BlockData::Table(table) = &mut current.data else {
            return Err(DocumentEngineError::NotTableBlock(block_id));
        };
        let column = table
            .columns
            .iter_mut()
            .find(|column| column.id == column_id)
            .ok_or_else(|| {
                DocumentEngineError::InvalidTableSelection(format!("不存在列 {column_id}"))
            })?;
        column.width = Some(width);
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn set_table_row_height(
        &mut self,
        block_id: BlockId,
        row_id: String,
        height: f32,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        if !height.is_finite() || !(34.0..=2_000.0).contains(&height) {
            return Err(DocumentEngineError::InvalidTableSelection(
                "表格行高必须在 34 到 2000 像素之间".into(),
            ));
        }
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        let BlockData::Table(table) = &mut current.data else {
            return Err(DocumentEngineError::NotTableBlock(block_id));
        };
        let row = table
            .rows
            .iter_mut()
            .find(|row| row.id == row_id)
            .ok_or_else(|| {
                DocumentEngineError::InvalidTableSelection(format!("不存在行 {row_id}"))
            })?;
        row.height = Some(height);
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn delete_table_row(
        &mut self,
        block_id: BlockId,
        row_id: String,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        let BlockData::Table(table) = &mut current.data else {
            return Err(DocumentEngineError::NotTableBlock(block_id));
        };
        if table.rows.len() <= 1 {
            return Err(DocumentEngineError::InvalidTableSelection(
                "表格至少保留一行".into(),
            ));
        }
        let index = table
            .rows
            .iter()
            .position(|row| row.id == row_id)
            .ok_or_else(|| {
                DocumentEngineError::InvalidTableSelection(format!("不存在行 {row_id}"))
            })?;
        table.rows.remove(index);
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn delete_table_column(
        &mut self,
        block_id: BlockId,
        column_id: String,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        let BlockData::Table(table) = &mut current.data else {
            return Err(DocumentEngineError::NotTableBlock(block_id));
        };
        if table.columns.len() <= 1 {
            return Err(DocumentEngineError::InvalidTableSelection(
                "表格至少保留一列".into(),
            ));
        }
        let index = table
            .columns
            .iter()
            .position(|column| column.id == column_id)
            .ok_or_else(|| {
                DocumentEngineError::InvalidTableSelection(format!("不存在列 {column_id}"))
            })?;
        table.columns.remove(index);
        for row in &mut table.rows {
            row.cells.remove(index);
        }
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn merge_table_cells(
        &mut self,
        block_id: BlockId,
        range: TableRange,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        let BlockData::Table(table) = &mut current.data else {
            return Err(DocumentEngineError::NotTableBlock(block_id));
        };
        let grid = TableGridProjection::new(table);
        let requested_bounds = grid.merge_bounds(&range)?;
        let overlapping_ranges = table
            .merged_ranges
            .iter()
            .filter_map(|existing| {
                grid.bounds_for_range(existing)
                    .ok()
                    .filter(|bounds| requested_bounds.overlaps(*bounds))
                    .map(|bounds| (existing.clone(), bounds))
            })
            .collect::<Vec<_>>();
        if overlapping_ranges
            .iter()
            .any(|(_, existing_bounds)| !requested_bounds.contains_bounds(*existing_bounds))
        {
            return Err(DocumentEngineError::InvalidTableSelection(
                "合并范围仅覆盖已有合并区域的一部分".into(),
            ));
        }
        drop(grid);
        // A larger rectangular merge may absorb fully enclosed merged cells.
        // This preserves the table's non-overlap invariant without requiring
        // the caller to split then merge in two transactions.
        table.merged_ranges.retain(|existing| {
            !overlapping_ranges
                .iter()
                .any(|(absorbed, _)| absorbed == existing)
        });
        table.merged_ranges.push(range);
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn split_table_cells(
        &mut self,
        block_id: BlockId,
        range: TableRange,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        let BlockData::Table(table) = &mut current.data else {
            return Err(DocumentEngineError::NotTableBlock(block_id));
        };
        table_range_targets(table, &range)?;
        let Some(index) = table
            .merged_ranges
            .iter()
            .position(|existing| existing == &range)
        else {
            return Err(DocumentEngineError::InvalidTableSelection(
                "合并范围不存在".into(),
            ));
        };
        table.merged_ranges.remove(index);
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn format_table_cells(
        &mut self,
        block_id: BlockId,
        selection: TableCellSelection,
        patch: TableCellFormatPatch,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        let BlockData::Table(table) = &mut current.data else {
            return Err(DocumentEngineError::NotTableBlock(block_id));
        };
        let targets = table_cell_targets(table, &selection)?;
        for (row_index, cell_index) in targets {
            let cell = &mut table.rows[row_index].cells[cell_index];
            apply_table_cell_format(cell, &patch);
        }
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn set_table_borders(
        &mut self,
        block_id: BlockId,
        selection: TableCellSelection,
        patch: TableBorderPatch,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        patch.validate()?;
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        let BlockData::Table(table) = &mut current.data else {
            return Err(DocumentEngineError::NotTableBlock(block_id));
        };
        let targets = table_cell_targets(table, &selection)?;
        for (row_index, cell_index) in targets {
            let cell = &mut table.rows[row_index].cells[cell_index];
            apply_table_border_patch(cell, &patch);
        }
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn apply_table_border_preset(
        &mut self,
        block_id: BlockId,
        selection: TableCellSelection,
        preset: TableBorderPreset,
        border: Option<TableBorder>,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        validate_table_border_preset(preset, border.as_ref())?;
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        let BlockData::Table(table) = &mut current.data else {
            return Err(DocumentEngineError::NotTableBlock(block_id));
        };
        let bounds = TableGridProjection::new(table).selection_bounds(&selection)?;
        apply_table_border_preset_to_bounds(table, bounds, preset, border);
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn insert_block(
        &mut self,
        block: DocumentBlock,
        parent_id: Option<BlockId>,
        index: usize,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        let id = block.id.clone();
        if id.trim().is_empty() {
            return Err(DocumentEngineError::EmptyBlockId);
        }
        if self.store.index.positions.contains_key(&id) {
            return Err(DocumentEngineError::DuplicateBlock(id));
        }
        ensure_parent_exists(&self.store, parent_id.as_deref())?;
        insert_child(
            &mut self.store.model,
            &self.store.index,
            parent_id.as_deref(),
            index,
            id.clone(),
        )?;
        let position = self.store.model.blocks.len();
        self.store.model.blocks.push(block.clone());
        self.store
            .insert_position(id.clone(), position, parent_id.clone());
        journal.push(Mutation::Insert {
            block,
            parent_id: parent_id.clone(),
            index,
        });
        changes.changed_blocks.insert(id);
        if let Some(parent_id) = parent_id {
            changes.changed_blocks.insert(parent_id.clone());
            changes.changed_containers.insert(parent_id);
        }
        changes.structure_changed = true;
        Ok(())
    }

    fn delete_block(
        &mut self,
        block_id: &str,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        if self.store.model.root.len() == 1 && self.store.model.root[0] == block_id {
            return Err(DocumentEngineError::CannotDeleteLastRootBlock);
        }
        self.store.position(block_id)?;
        let (parent_id, index) = self.store.container_position(block_id)?;
        let subtree = collect_subtree(&self.store.model, &self.store.index, block_id)?;
        let subtree_set: HashSet<&str> = subtree.iter().map(String::as_str).collect();
        let mut removed: Vec<RemovedBlock> = self
            .store
            .model
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| subtree_set.contains(block.id.as_str()))
            .map(|(position, block)| RemovedBlock {
                position,
                block: block.clone(),
            })
            .collect();
        removed.sort_by_key(|item| item.position);
        remove_from_parent(&mut self.store.model, &self.store.index, block_id)?;
        self.store
            .model
            .blocks
            .retain(|block| !subtree_set.contains(block.id.as_str()));
        let removed_positions: Vec<_> = removed
            .iter()
            .map(|item| (item.position, item.block.id.clone()))
            .collect();
        self.store.remove_positions(&removed_positions);
        journal.push(Mutation::Delete {
            block_id: block_id.to_string(),
            removed,
            parent_id: parent_id.clone(),
            index,
        });
        changes.changed_blocks.extend(subtree);
        if let Some(parent_id) = parent_id {
            changes.changed_blocks.insert(parent_id.clone());
            changes.changed_containers.insert(parent_id);
        }
        changes.structure_changed = true;
        Ok(())
    }

    fn reset_block(
        &mut self,
        block_id: BlockId,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        let before = self.store.block(&block_id)?.clone();
        let current = self.store.block_mut(&block_id)?;
        if current.content.is_none() {
            return Err(DocumentEngineError::NotTextBlock(block_id));
        }
        current.kind = DocumentBlockKind::Paragraph;
        current.presentation = BlockPresentation::default();
        current.content = Some(RichText {
            text: String::new(),
            runs: Vec::new(),
        });
        current.data = BlockData::None;
        let after = current.clone();
        journal.push(Mutation::Update {
            block_id: block_id.clone(),
            before: Box::new(before),
            after: Box::new(after),
        });
        changes.changed_blocks.insert(block_id);
        Ok(())
    }

    fn move_block(
        &mut self,
        block_id: BlockId,
        parent_id: Option<BlockId>,
        index: usize,
        journal: &mut Vec<Mutation>,
        changes: &mut ChangeTracker,
    ) -> Result<(), DocumentEngineError> {
        self.store.position(&block_id)?;
        ensure_parent_exists(&self.store, parent_id.as_deref())?;
        if parent_id.as_deref() == Some(block_id.as_str())
            || contains_descendant(
                &self.store.model,
                &self.store.index,
                &block_id,
                parent_id.as_deref(),
            )
        {
            return Err(DocumentEngineError::CannotMoveIntoDescendant {
                block_id,
                parent_id,
            });
        }
        let (old_parent, old_index) = self.store.container_position(&block_id)?;
        let destination_len =
            container_len(&self.store.model, &self.store.index, parent_id.as_deref())?;
        let max_index = if old_parent == parent_id {
            destination_len.saturating_sub(1)
        } else {
            destination_len
        };
        if index > max_index {
            return Err(DocumentEngineError::InvalidIndex {
                index,
                len: max_index,
            });
        }
        remove_from_parent(&mut self.store.model, &self.store.index, &block_id)?;
        insert_child(
            &mut self.store.model,
            &self.store.index,
            parent_id.as_deref(),
            index,
            block_id.clone(),
        )?;
        self.store
            .index
            .parents
            .insert(block_id.clone(), parent_id.clone());
        journal.push(Mutation::Move {
            block_id: block_id.clone(),
            from_parent_id: old_parent.clone(),
            from_index: old_index,
            to_parent_id: parent_id.clone(),
            to_index: index,
        });
        changes.changed_blocks.insert(block_id);
        if let Some(parent_id) = old_parent {
            changes.changed_blocks.insert(parent_id.clone());
            changes.changed_containers.insert(parent_id);
        }
        if let Some(parent_id) = parent_id {
            changes.changed_blocks.insert(parent_id.clone());
            changes.changed_containers.insert(parent_id);
        }
        changes.structure_changed = true;
        Ok(())
    }

    fn rollback(&mut self, journal: &[Mutation]) {
        for mutation in journal.iter().rev() {
            self.apply_inverse(mutation);
        }
    }

    fn apply_inverse(&mut self, mutation: &Mutation) {
        match mutation {
            Mutation::Insert { block, .. } => {
                let _ = remove_from_parent(&mut self.store.model, &self.store.index, &block.id);
                let position = self.store.index.positions.get(&block.id).copied();
                self.store.model.blocks.retain(|item| item.id != block.id);
                if let Some(position) = position {
                    self.store.remove_positions(&[(position, block.id.clone())]);
                }
            }
            Mutation::Delete {
                removed,
                parent_id,
                index,
                ..
            } => {
                self.restore_removed(removed, parent_id.as_ref(), *index);
            }
            Mutation::RemoveInserted {
                block,
                parent_id,
                index,
            } => {
                insert_child(
                    &mut self.store.model,
                    &self.store.index,
                    parent_id.as_deref(),
                    *index,
                    block.id.clone(),
                )
                .ok();
                self.store.model.blocks.push(block.clone());
                self.reindex_from_model();
            }
            Mutation::Restore {
                removed,
                parent_id,
                index,
                ..
            } => {
                for item in removed {
                    let _ = remove_from_parent(
                        &mut self.store.model,
                        &self.store.index,
                        &item.block.id,
                    );
                    self.store
                        .model
                        .blocks
                        .retain(|block| block.id != item.block.id);
                    self.store.index.positions.remove(&item.block.id);
                    self.store.index.parents.remove(&item.block.id);
                }
                let _ = (parent_id, index);
                self.reindex_positions_only();
            }
            Mutation::Update {
                block_id, before, ..
            } => {
                if let Ok(block) = self.store.block_mut(block_id) {
                    *block = (**before).clone();
                }
            }
            Mutation::Move {
                block_id,
                from_parent_id,
                from_index,
                to_parent_id,
                ..
            } => {
                let _ = remove_from_parent(&mut self.store.model, &self.store.index, block_id);
                let _ = insert_child(
                    &mut self.store.model,
                    &self.store.index,
                    from_parent_id.as_deref(),
                    *from_index,
                    block_id.clone(),
                );
                self.store
                    .index
                    .parents
                    .insert(block_id.clone(), from_parent_id.clone());
                let _ = to_parent_id;
            }
            Mutation::SetPageSetup { before, .. } => {
                self.store.model.page_setup = before.clone();
            }
        }
        self.reindex_positions_only();
    }

    fn apply_forward(&mut self, mutation: &Mutation) {
        if let Mutation::Update {
            block_id, after, ..
        } = mutation
        {
            if let Ok(block) = self.store.block_mut(block_id) {
                *block = (**after).clone();
            }
            self.reindex_positions_only();
            return;
        }
        let Some(command) = mutation_to_command(mutation) else {
            return;
        };
        let mut journal = Vec::new();
        let mut changes = ChangeTracker::default();
        let _ = self.apply_command(command, &mut journal, &mut changes);
    }

    fn restore_removed(
        &mut self,
        removed: &[RemovedBlock],
        parent_id: Option<&BlockId>,
        index: usize,
    ) {
        let mut records = removed.to_vec();
        records.sort_by_key(|item| item.position);
        for item in &records {
            let position = item.position.min(self.store.model.blocks.len());
            for existing_position in self.store.index.positions.values_mut() {
                if *existing_position >= position {
                    *existing_position += 1;
                }
            }
            self.store.model.blocks.insert(position, item.block.clone());
            self.store
                .index
                .positions
                .insert(item.block.id.clone(), position);
            self.store.index.parents.remove(&item.block.id);
            for child in &item.block.children {
                self.store
                    .index
                    .parents
                    .insert(child.clone(), Some(item.block.id.clone()));
            }
        }
        if let Some(root) = records.first() {
            let root_id = root.block.id.clone();
            let _ = insert_child(
                &mut self.store.model,
                &self.store.index,
                parent_id.map(String::as_str),
                index,
                root_id.clone(),
            );
            self.store.index.parents.insert(root_id, parent_id.cloned());
        }
    }

    fn reindex_positions_only(&mut self) {
        for (position, block) in self.store.model.blocks.iter().enumerate() {
            self.store
                .index
                .positions
                .insert(block.id.clone(), position);
        }
    }

    fn reindex_from_model(&mut self) {
        if let Ok(index) = store::DocumentIndex::build(&self.store.model) {
            self.store.index = index;
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DocumentEngineError {
    #[error("command batch 不能为空")]
    EmptyTransaction,
    #[error("revision 冲突：服务端是 {expected}，事务基于 {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("block id 不能为空")]
    EmptyBlockId,
    #[error("block {0} 已存在")]
    DuplicateBlock(BlockId),
    #[error("找不到 block {0}")]
    MissingBlock(BlockId),
    #[error("block {0} 不是表格")]
    NotTableBlock(BlockId),
    #[error("block {0} 不是待办事项")]
    NotTodoBlock(BlockId),
    #[error("block {0} 不是链接")]
    NotLinkBlock(BlockId),
    #[error("block {0} 不是代码块")]
    NotCodeBlock(BlockId),
    #[error("block {0} 不是图片")]
    NotImageBlock(BlockId),
    #[error("图片设置无效：{0}")]
    InvalidImageConfig(String),
    #[error("block {0} 不支持转换为链接")]
    NotTextBlock(BlockId),
    #[error("block {0} 不支持目标类型转换")]
    UnsupportedBlockConversion(BlockId),
    #[error("链接地址无效")]
    InvalidLinkUrl,
    #[error("表格选区无效：{0}")]
    InvalidTableSelection(String),
    #[error("表格边框设置无效：{0}")]
    InvalidTableBorderPatch(String),
    #[error("Block 排版设置无效：{0}")]
    InvalidBlockPresentation(String),
    #[error("行内范围无效：{start}..{end}，文本长度 {text_len}")]
    InvalidInlineRange {
        start: usize,
        end: usize,
        text_len: usize,
    },
    #[error("行内样式设置无效：{0}")]
    InvalidInlinePatch(String),
    #[error("父 block {0} 不存在")]
    MissingParent(BlockId),
    #[error("插入位置 {index} 超出当前容器长度 {len}")]
    InvalidIndex { index: usize, len: usize },
    #[error("不能把 block {block_id} 移到自己的后代 {parent_id:?} 下")]
    CannotMoveIntoDescendant {
        block_id: BlockId,
        parent_id: Option<BlockId>,
    },
    #[error("不能删除文档唯一的根 block")]
    CannotDeleteLastRootBlock,
    #[error("revision 溢出")]
    RevisionOverflow,
    #[error("没有可撤销的事务")]
    NothingToUndo,
    #[error("没有可重做的事务")]
    NothingToRedo,
    #[error("事务历史包含无法重放的 mutation")]
    InvalidHistoryMutation,
    #[error("Block Tree 校验失败：{0}")]
    Schema(#[from] SchemaValidationError),
}

impl From<TableGridQueryError> for DocumentEngineError {
    fn from(error: TableGridQueryError) -> Self {
        Self::InvalidTableSelection(error.to_string())
    }
}

fn ensure_parent_exists(
    store: &BlockStore,
    parent_id: Option<&str>,
) -> Result<(), DocumentEngineError> {
    if let Some(parent_id) = parent_id {
        store.position(parent_id)?;
    }
    Ok(())
}

fn insert_child(
    model: &mut DocumentModel,
    index_map: &store::DocumentIndex,
    parent_id: Option<&str>,
    index: usize,
    child_id: BlockId,
) -> Result<(), DocumentEngineError> {
    let children = match parent_id {
        None => &mut model.root,
        Some(parent_id) => {
            let position = index_map
                .positions
                .get(parent_id)
                .copied()
                .ok_or_else(|| DocumentEngineError::MissingParent(parent_id.to_string()))?;
            &mut model.blocks[position].children
        }
    };
    if index > children.len() {
        return Err(DocumentEngineError::InvalidIndex {
            index,
            len: children.len(),
        });
    }
    children.insert(index, child_id);
    Ok(())
}

fn container_len(
    model: &DocumentModel,
    index_map: &store::DocumentIndex,
    parent_id: Option<&str>,
) -> Result<usize, DocumentEngineError> {
    match parent_id {
        None => Ok(model.root.len()),
        Some(parent_id) => Ok(model
            .blocks
            .get(index_map.position(parent_id)?)
            .ok_or_else(|| DocumentEngineError::MissingParent(parent_id.to_string()))?
            .children
            .len()),
    }
}

fn remove_from_parent(
    model: &mut DocumentModel,
    index_map: &store::DocumentIndex,
    block_id: &str,
) -> Result<(), DocumentEngineError> {
    if let Some(index) = model.root.iter().position(|id| id == block_id) {
        model.root.remove(index);
        return Ok(());
    }
    if let Some(parent_id) = index_map.parent(block_id) {
        let position = index_map.position(&parent_id)?;
        let parent = &mut model.blocks[position];
        if let Some(index) = parent.children.iter().position(|id| id == block_id) {
            parent.children.remove(index);
            return Ok(());
        }
    }
    // A valid model cannot contain an unreachable block, so reaching here means the caller
    // supplied a malformed candidate after an earlier operation. Returning a domain error keeps
    // the transaction atomic and avoids an `unwrap` in this core path.
    Err(DocumentEngineError::MissingBlock(block_id.to_string()))
}

/// 返回 block 的直接父容器。root 没有可供布局缓存失效的实体 id，因此 root 结构变化
/// 由 `structure_changed` 表示；嵌套容器则必须进入 changed_blocks。
fn collect_subtree(
    model: &DocumentModel,
    index_map: &store::DocumentIndex,
    root: &str,
) -> Result<Vec<BlockId>, DocumentEngineError> {
    let mut result = Vec::new();
    let mut stack = vec![root.to_string()];
    while let Some(id) = stack.pop() {
        let position = index_map.position(&id)?;
        let block = model
            .blocks
            .get(position)
            .ok_or_else(|| DocumentEngineError::MissingBlock(id.clone()))?;
        result.push(id);
        stack.extend(block.children.iter().cloned());
    }
    Ok(result)
}

fn contains_descendant(
    model: &DocumentModel,
    index_map: &store::DocumentIndex,
    root: &str,
    candidate: Option<&str>,
) -> bool {
    let Some(candidate) = candidate else {
        return false;
    };
    collect_subtree(model, index_map, root)
        .map(|ids| ids.iter().any(|id| id == candidate))
        .unwrap_or(false)
}

fn validate_link_url(url: &str) -> Result<(), DocumentEngineError> {
    if url.trim().is_empty() || url.chars().count() > 8_192 {
        return Err(DocumentEngineError::InvalidLinkUrl);
    }
    Ok(())
}

fn apply_image_patch(
    image: &mut ImageBlock,
    patch: ImageBlockPatch,
    block_id: &str,
) -> Result<(), DocumentEngineError> {
    if let Some(asset_id) = patch.asset_id {
        if asset_id.trim().is_empty() {
            return Err(DocumentEngineError::InvalidImageConfig(
                "assetId 不能为空".into(),
            ));
        }
        image.asset_id = asset_id;
    }
    if let Some(original_asset_id) = patch.original_asset_id {
        if original_asset_id
            .as_deref()
            .is_some_and(|asset_id| asset_id.trim().is_empty())
        {
            return Err(DocumentEngineError::InvalidImageConfig(
                "originalAssetId 不能为空".into(),
            ));
        }
        image.original_asset_id = original_asset_id;
    }
    if let Some(transform) = patch.transform {
        transform
            .crop
            .validate(block_id)
            .map_err(|error| DocumentEngineError::InvalidImageConfig(error.to_string()))?;
        image.transform = transform;
    }
    if let Some(caption) = patch.caption {
        if caption.chars().count() > 512 {
            return Err(DocumentEngineError::InvalidImageConfig(
                "题注不能超过 512 个字符".into(),
            ));
        }
        image.caption = caption;
    }
    Ok(())
}

fn apply_block_presentation_patch(
    presentation: &mut BlockPresentation,
    patch: BlockPresentationPatch,
) {
    apply_defaulted(&mut presentation.align, patch.align);
    apply_optional(&mut presentation.list, patch.list);
    apply_defaulted(&mut presentation.indent_start, patch.indent_start);
    apply_defaulted(&mut presentation.indent_end, patch.indent_end);
    apply_defaulted(&mut presentation.spacing_before, patch.spacing_before);
    apply_defaulted(&mut presentation.spacing_after, patch.spacing_after);
    apply_defaulted(&mut presentation.line_height, patch.line_height);
    apply_optional(&mut presentation.named_style, patch.named_style);
}

fn patch_rich_text_runs(content: &mut RichText, range: TextRange, patch: InlineStylePatch) {
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
    let mut next_runs: Vec<InlineRun> = Vec::with_capacity(source_runs.len() + 2);
    for run in source_runs {
        if run.end <= range.start || run.start >= range.end {
            push_inline_run(&mut next_runs, run.start, run.end, run.style);
            continue;
        }
        if run.start < range.start {
            push_inline_run(&mut next_runs, run.start, range.start, run.style.clone());
        }
        let overlap_start = run.start.max(range.start);
        let overlap_end = run.end.min(range.end);
        let mut selected_style = run.style.clone();
        apply_inline_style_patch(&mut selected_style, &patch);
        push_inline_run(&mut next_runs, overlap_start, overlap_end, selected_style);
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
    let can_extend = runs
        .last()
        .is_some_and(|previous| previous.end == start && previous.style == style);
    if can_extend {
        if let Some(previous) = runs.last_mut() {
            previous.end = end;
        }
    } else {
        runs.push(InlineRun { start, end, style });
    }
}

fn apply_inline_style_patch(style: &mut InlineStyle, patch: &InlineStylePatch) {
    apply_bool(&mut style.bold, patch.bold);
    apply_bool(&mut style.italic, patch.italic);
    apply_bool(&mut style.underline, patch.underline);
    apply_bool(&mut style.strikethrough, patch.strikethrough);
    apply_optional(&mut style.font_family, patch.font_family.clone());
    apply_optional(&mut style.font_size, patch.font_size);
    apply_optional_color(&mut style.color, patch.color.clone());
    apply_optional_color(&mut style.highlight, patch.highlight.clone());
    apply_optional(&mut style.vertical_align, patch.vertical_align.clone());
}

fn apply_bool(target: &mut bool, patch: Option<Option<bool>>) {
    if let Some(value) = patch {
        *target = value.unwrap_or(false);
    }
}

fn apply_optional<T>(target: &mut Option<T>, patch: Option<Option<T>>) {
    if let Some(value) = patch {
        *target = value;
    }
}

fn apply_defaulted<T: Default>(target: &mut T, patch: Option<Option<T>>) {
    if let Some(value) = patch {
        *target = value.unwrap_or_default();
    }
}

fn apply_optional_color(target: &mut Option<Color>, patch: Option<Option<String>>) {
    if let Some(value) = patch {
        *target = value.map(|color| Color::new(color).expect("validated inline color"));
    }
}

fn table_cell_targets(
    table: &TableBlock,
    selection: &TableCellSelection,
) -> Result<Vec<(usize, usize)>, DocumentEngineError> {
    TableGridProjection::new(table)
        .selection_targets(selection)
        .map(|targets| {
            targets
                .into_iter()
                .map(|target| (target.row, target.column))
                .collect()
        })
        .map_err(Into::into)
}

fn table_range_targets(
    table: &TableBlock,
    range: &TableRange,
) -> Result<(usize, usize, usize, usize), DocumentEngineError> {
    TableGridProjection::new(table)
        .merge_bounds(range)
        .map(|bounds| {
            (
                bounds.start_row,
                bounds.end_row,
                bounds.start_column,
                bounds.end_column,
            )
        })
        .map_err(Into::into)
}

fn apply_table_cell_format(cell: &mut TableCell, patch: &TableCellFormatPatch) {
    if let Some(fill_color) = &patch.fill_color {
        cell.style.fill_color = fill_color.clone();
    }
    if let Some(horizontal_align) = &patch.horizontal_align {
        cell.style.horizontal_align = Some(horizontal_align.clone());
    }
    if let Some(vertical_align) = &patch.vertical_align {
        cell.style.vertical_align = Some(vertical_align.clone());
    }
    if patch.text_attrs.is_empty() || cell.content.text.is_empty() {
        return;
    }
    if cell.content.runs.is_empty() {
        cell.content.runs.push(oo_schema::InlineRun {
            start: 0,
            end: cell.content.text.chars().count(),
            style: InlineStyle::default(),
        });
    }
    for run in &mut cell.content.runs {
        for (key, value) in &patch.text_attrs {
            match key.as_str() {
                "bold" => {
                    if let Some(value) = value.as_bool() {
                        run.style.bold = value;
                    }
                }
                "italic" => {
                    if let Some(value) = value.as_bool() {
                        run.style.italic = value;
                    }
                }
                "underline" => {
                    if let Some(value) = value.as_bool() {
                        run.style.underline = value;
                    }
                }
                "strikethrough" => {
                    if let Some(value) = value.as_bool() {
                        run.style.strikethrough = value;
                    }
                }
                "fontFamily" => {
                    if let Some(value) = value.as_str() {
                        run.style.font_family = Some(value.to_string());
                    }
                }
                "fontSize" => {
                    if let Some(value) = value
                        .as_f64()
                        .filter(|value| value.is_finite() && *value > 0.0 && *value <= 512.0)
                    {
                        run.style.font_size = Some(value as f32);
                    }
                }
                "color" => {
                    if let Some(value) = value.as_str() {
                        if let Ok(value) = Color::new(value) {
                            run.style.color = Some(value);
                        }
                    }
                }
                "highlight" => {
                    if let Some(value) = value.as_str() {
                        if let Ok(value) = Color::new(value) {
                            run.style.highlight = Some(value);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn apply_table_border_patch(cell: &mut TableCell, patch: &TableBorderPatch) {
    if let Some(border) = &patch.top {
        cell.style.borders.top = border.clone();
    }
    if let Some(border) = &patch.right {
        cell.style.borders.right = border.clone();
    }
    if let Some(border) = &patch.bottom {
        cell.style.borders.bottom = border.clone();
    }
    if let Some(border) = &patch.left {
        cell.style.borders.left = border.clone();
    }
    if let Some(border) = &patch.diagonal_down {
        cell.style.borders.diagonal_down = border.clone();
    }
    if let Some(border) = &patch.diagonal_up {
        cell.style.borders.diagonal_up = border.clone();
    }
}

fn validate_table_border_preset(
    preset: TableBorderPreset,
    border: Option<&TableBorder>,
) -> Result<(), DocumentEngineError> {
    if matches!(preset, TableBorderPreset::None) {
        return Ok(());
    }
    let Some(border) = border else {
        return Err(DocumentEngineError::InvalidTableBorderPatch(
            "边框预设需要提供颜色、线型和宽度".into(),
        ));
    };
    TableBorderPatch {
        top: Some(Some(border.clone())),
        ..TableBorderPatch::default()
    }
    .validate()
}

fn set_border_edge(cell: &mut TableCell, edge: BorderEdge, border: Option<TableBorder>) {
    match edge {
        BorderEdge::Top => cell.style.borders.top = border,
        BorderEdge::Right => cell.style.borders.right = border,
        BorderEdge::Bottom => cell.style.borders.bottom = border,
        BorderEdge::Left => cell.style.borders.left = border,
        BorderEdge::DiagonalDown => cell.style.borders.diagonal_down = border,
        BorderEdge::DiagonalUp => cell.style.borders.diagonal_up = border,
    }
}

#[derive(Clone, Copy)]
enum BorderEdge {
    Top,
    Right,
    Bottom,
    Left,
    DiagonalDown,
    DiagonalUp,
}

/// Applies a gallery preset directly to the canonical physical cells inside
/// `bounds`. No DOM geometry or renderer state participates in this logic.
fn apply_table_border_preset_to_bounds(
    table: &mut TableBlock,
    bounds: GridBounds,
    preset: TableBorderPreset,
    border: Option<TableBorder>,
) {
    let set = |cell: &mut TableCell, edge| set_border_edge(cell, edge, border.clone());
    for row in bounds.start_row..=bounds.end_row {
        for column in bounds.start_column..=bounds.end_column {
            let cell = &mut table.rows[row].cells[column];
            match preset {
                TableBorderPreset::None => {
                    for edge in [
                        BorderEdge::Top,
                        BorderEdge::Right,
                        BorderEdge::Bottom,
                        BorderEdge::Left,
                        BorderEdge::DiagonalDown,
                        BorderEdge::DiagonalUp,
                    ] {
                        set_border_edge(cell, edge, None);
                    }
                }
                TableBorderPreset::All => {
                    for edge in [
                        BorderEdge::Top,
                        BorderEdge::Right,
                        BorderEdge::Bottom,
                        BorderEdge::Left,
                    ] {
                        set(cell, edge);
                    }
                }
                TableBorderPreset::Top if row == bounds.start_row => set(cell, BorderEdge::Top),
                TableBorderPreset::Right if column == bounds.end_column => {
                    set(cell, BorderEdge::Right)
                }
                TableBorderPreset::Bottom if row == bounds.end_row => set(cell, BorderEdge::Bottom),
                TableBorderPreset::Left if column == bounds.start_column => {
                    set(cell, BorderEdge::Left)
                }
                TableBorderPreset::Outer => {
                    if row == bounds.start_row {
                        set(cell, BorderEdge::Top);
                    }
                    if row == bounds.end_row {
                        set(cell, BorderEdge::Bottom);
                    }
                    if column == bounds.start_column {
                        set(cell, BorderEdge::Left);
                    }
                    if column == bounds.end_column {
                        set(cell, BorderEdge::Right);
                    }
                }
                TableBorderPreset::Inner => {
                    if row > bounds.start_row {
                        set(cell, BorderEdge::Top);
                    }
                    if row < bounds.end_row {
                        set(cell, BorderEdge::Bottom);
                    }
                    if column > bounds.start_column {
                        set(cell, BorderEdge::Left);
                    }
                    if column < bounds.end_column {
                        set(cell, BorderEdge::Right);
                    }
                }
                TableBorderPreset::InnerHorizontal => {
                    if row > bounds.start_row {
                        set(cell, BorderEdge::Top);
                    }
                    if row < bounds.end_row {
                        set(cell, BorderEdge::Bottom);
                    }
                }
                TableBorderPreset::InnerVertical => {
                    if column > bounds.start_column {
                        set(cell, BorderEdge::Left);
                    }
                    if column < bounds.end_column {
                        set(cell, BorderEdge::Right);
                    }
                }
                TableBorderPreset::DiagonalDown => set(cell, BorderEdge::DiagonalDown),
                TableBorderPreset::DiagonalUp => set(cell, BorderEdge::DiagonalUp),
                _ => {}
            }
        }
    }
}

fn is_hex_color(value: &str) -> bool {
    let bytes = value.as_bytes();
    (bytes.len() == 7 || bytes.len() == 9)
        && bytes[0] == b'#'
        && bytes[1..].iter().all(u8::is_ascii_hexdigit)
}

fn mutation_to_command(mutation: &Mutation) -> Option<DocumentCommand> {
    match mutation {
        Mutation::Insert {
            block,
            parent_id,
            index,
        } => Some(DocumentCommand::InsertBlock {
            block: block.clone(),
            parent_id: parent_id.clone(),
            index: *index,
        }),
        Mutation::Delete { block_id, .. } => Some(DocumentCommand::DeleteBlock {
            block_id: block_id.clone(),
        }),
        Mutation::Move {
            block_id,
            to_parent_id,
            to_index,
            ..
        } => Some(DocumentCommand::MoveBlock {
            block_id: block_id.clone(),
            parent_id: to_parent_id.clone(),
            index: *to_index,
        }),
        Mutation::SetPageSetup { after, .. } => Some(DocumentCommand::SetPageSetup {
            page_setup: after.clone(),
        }),
        Mutation::RemoveInserted { .. } | Mutation::Restore { .. } | Mutation::Update { .. } => {
            None
        }
    }
}

fn summarize_mutations(changes: &mut ChangeTracker, mutations: &[Mutation]) {
    for mutation in mutations {
        match mutation {
            Mutation::Insert {
                block, parent_id, ..
            } => {
                changes.changed_blocks.insert(block.id.clone());
                if let Some(parent_id) = parent_id {
                    changes.changed_blocks.insert(parent_id.clone());
                    changes.changed_containers.insert(parent_id.clone());
                }
                changes.structure_changed = true;
            }
            Mutation::Delete {
                removed, parent_id, ..
            }
            | Mutation::Restore {
                removed, parent_id, ..
            } => {
                changes
                    .changed_blocks
                    .extend(removed.iter().map(|item| item.block.id.clone()));
                if let Some(parent_id) = parent_id {
                    changes.changed_blocks.insert(parent_id.clone());
                    changes.changed_containers.insert(parent_id.clone());
                }
                changes.structure_changed = true;
            }
            Mutation::RemoveInserted {
                block, parent_id, ..
            } => {
                changes.changed_blocks.insert(block.id.clone());
                if let Some(parent_id) = parent_id {
                    changes.changed_blocks.insert(parent_id.clone());
                    changes.changed_containers.insert(parent_id.clone());
                }
                changes.structure_changed = true;
            }
            Mutation::Update { block_id, .. } => {
                changes.changed_blocks.insert(block_id.clone());
            }
            Mutation::Move {
                block_id,
                from_parent_id,
                to_parent_id,
                ..
            } => {
                changes.changed_blocks.insert(block_id.clone());
                for parent_id in [from_parent_id, to_parent_id].into_iter().flatten() {
                    changes.changed_blocks.insert(parent_id.clone());
                    changes.changed_containers.insert(parent_id.clone());
                }
                changes.structure_changed = true;
            }
            Mutation::SetPageSetup { .. } => {
                changes.structure_changed = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oo_schema::{
        BlockAlignment, BlockExtension, ImageCrop, InlineRun, LinkBlock, ListKind,
        ListPresentation, TableBorder, TableBorderStyle, TableCellStyle, TableColumn, TableRange,
        TableRow, TodoBlock,
    };

    fn paragraph(id: &str, text: &str) -> DocumentBlock {
        DocumentBlock {
            id: id.into(),
            kind: DocumentBlockKind::Paragraph,
            presentation: BlockPresentation::default(),
            content: Some(RichText {
                text: text.into(),
                runs: Vec::new(),
            }),
            children: Vec::new(),
            data: BlockData::None,
        }
    }

    fn engine() -> DocumentEngine {
        DocumentEngine::new(DocumentModel::default(), 4).unwrap()
    }

    #[test]
    fn read_blocks_uses_stable_ids_and_preserves_requested_order() {
        let model = DocumentModel {
            root: vec!["first".into(), "second".into()],
            blocks: vec![paragraph("first", "one"), paragraph("second", "two")],
            page_setup: None,
        };
        let engine = DocumentEngine::new(model, 4).unwrap();

        let blocks = engine
            .read_blocks(&["second".into(), "first".into()])
            .unwrap();
        assert_eq!(
            blocks
                .iter()
                .map(|block| block.id.as_str())
                .collect::<Vec<_>>(),
            ["second", "first"]
        );
        assert!(matches!(
            engine.read_blocks(&["missing".into()]),
            Err(DocumentEngineError::MissingBlock(id)) if id == "missing"
        ));
    }

    fn table(id: &str) -> DocumentBlock {
        DocumentBlock {
            id: id.into(),
            kind: DocumentBlockKind::Table,
            presentation: BlockPresentation::default(),
            content: None,
            children: Vec::new(),
            data: BlockData::Table(TableBlock {
                columns: vec![
                    TableColumn {
                        id: "column-a".into(),
                        width: None,
                    },
                    TableColumn {
                        id: "column-b".into(),
                        width: None,
                    },
                ],
                rows: vec![TableRow {
                    id: "row-a".into(),
                    height: None,
                    cells: vec![
                        TableCell {
                            id: "cell-a".into(),
                            content: RichText {
                                text: "first".into(),
                                runs: vec![InlineRun {
                                    start: 0,
                                    end: 5,
                                    style: InlineStyle::default(),
                                }],
                            },
                            style: TableCellStyle::default(),
                        },
                        TableCell {
                            id: "cell-b".into(),
                            content: RichText {
                                text: "second".into(),
                                runs: vec![InlineRun {
                                    start: 0,
                                    end: 6,
                                    style: InlineStyle::default(),
                                }],
                            },
                            style: TableCellStyle::default(),
                        },
                    ],
                }],
                merged_ranges: Vec::new(),
            }),
        }
    }

    fn image(id: &str) -> DocumentBlock {
        DocumentBlock {
            id: id.into(),
            kind: DocumentBlockKind::Image,
            presentation: BlockPresentation::default(),
            content: None,
            children: Vec::new(),
            data: BlockData::Image(ImageBlock {
                asset_id: "asset-original".into(),
                alt: "示例图片".into(),
                original_asset_id: None,
                transform: ImageTransform::default(),
                caption: String::new(),
            }),
        }
    }

    #[test]
    fn image_config_is_typed_persistent_and_undoable() {
        let mut engine = engine();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block: image("image-1"),
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();

        engine
            .execute(DocumentCommandBatch {
                base_revision: 5,
                commands: vec![DocumentCommand::SetImageConfig {
                    block_id: "image-1".into(),
                    patch: ImageBlockPatch {
                        asset_id: Some("asset-compressed".into()),
                        original_asset_id: Some(Some("asset-original".into())),
                        transform: Some(ImageTransform {
                            crop: ImageCrop {
                                top: 0.1,
                                right: 0.2,
                                bottom: 0.0,
                                left: 0.05,
                            },
                            flip_horizontal: true,
                            flip_vertical: false,
                        }),
                        caption: Some("图 1：裁剪后的示例".into()),
                    },
                }],
            })
            .unwrap();

        let BlockData::Image(updated) = &engine.model().blocks[0].data else {
            panic!("image data expected");
        };
        assert_eq!(updated.asset_id, "asset-compressed");
        assert_eq!(updated.original_asset_id.as_deref(), Some("asset-original"));
        assert!(updated.transform.flip_horizontal);
        assert_eq!(updated.caption, "图 1：裁剪后的示例");

        engine.undo().unwrap();
        let BlockData::Image(reverted) = &engine.model().blocks[0].data else {
            panic!("image data expected");
        };
        assert_eq!(reverted.asset_id, "asset-original");
        assert!(reverted.caption.is_empty());
    }

    #[test]
    fn image_config_rejects_invalid_crop_and_non_image_target() {
        let mut engine = engine();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![
                    DocumentCommand::InsertBlock {
                        block: image("image-1"),
                        parent_id: None,
                        index: 0,
                    },
                    DocumentCommand::InsertBlock {
                        block: paragraph("paragraph-1", "text"),
                        parent_id: None,
                        index: 1,
                    },
                ],
            })
            .unwrap();

        let invalid = engine.execute(DocumentCommandBatch {
            base_revision: 5,
            commands: vec![DocumentCommand::SetImageConfig {
                block_id: "image-1".into(),
                patch: ImageBlockPatch {
                    transform: Some(ImageTransform {
                        crop: ImageCrop {
                            left: 0.8,
                            right: 0.2,
                            ..ImageCrop::default()
                        },
                        ..ImageTransform::default()
                    }),
                    ..ImageBlockPatch::default()
                },
            }],
        });
        assert!(matches!(
            invalid,
            Err(DocumentEngineError::InvalidImageConfig(_))
        ));

        let non_image = engine.execute(DocumentCommandBatch {
            base_revision: 5,
            commands: vec![DocumentCommand::SetImageConfig {
                block_id: "paragraph-1".into(),
                patch: ImageBlockPatch {
                    caption: Some("not allowed".into()),
                    ..ImageBlockPatch::default()
                },
            }],
        });
        assert!(
            matches!(non_image, Err(DocumentEngineError::NotImageBlock(id)) if id == "paragraph-1")
        );
    }

    #[test]
    fn set_block_presentation_is_typed_and_atomic() {
        let mut engine = engine();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block: paragraph("p-1", "one"),
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();

        engine
            .execute(DocumentCommandBatch {
                base_revision: 5,
                commands: vec![DocumentCommand::SetBlockPresentation {
                    block_id: "p-1".into(),
                    patch: BlockPresentationPatch {
                        align: Some(Some(BlockAlignment::Center)),
                        list: Some(Some(ListPresentation {
                            kind: ListKind::Ordered,
                            level: 1,
                        })),
                        indent_start: Some(Some(2)),
                        line_height: Some(Some(1.5)),
                        ..Default::default()
                    },
                }],
            })
            .unwrap();
        let block = &engine.model().blocks[0];
        assert_eq!(block.presentation.align, BlockAlignment::Center);
        assert_eq!(
            block.presentation.list,
            Some(ListPresentation {
                kind: ListKind::Ordered,
                level: 1,
            })
        );
        assert_eq!(block.presentation.indent_start, 2);
        assert_eq!(block.presentation.line_height, 1.5);

        let error = engine.execute(DocumentCommandBatch {
            base_revision: 6,
            commands: vec![DocumentCommand::SetBlockPresentation {
                block_id: "p-1".into(),
                patch: BlockPresentationPatch {
                    align: Some(Some(BlockAlignment::Right)),
                    indent_start: Some(Some(21)),
                    ..Default::default()
                },
            }],
        });
        assert!(matches!(
            error,
            Err(DocumentEngineError::InvalidBlockPresentation(_))
        ));
        assert_eq!(engine.revision(), 6);
        assert_eq!(
            engine.model().blocks[0].presentation.align,
            BlockAlignment::Center
        );
    }

    #[test]
    fn patch_inline_range_updates_only_selected_runs_and_is_atomic() {
        let mut engine = engine();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block: paragraph("p-1", "Hello"),
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();

        let wire = serde_json::to_value(DocumentCommand::PatchInlineRange {
            block_id: "p-1".into(),
            range: TextRange { start: 1, end: 4 },
            patch: InlineStylePatch {
                bold: Some(Some(true)),
                font_size: Some(Some(18.0)),
                ..Default::default()
            },
        })
        .unwrap();
        assert_eq!(wire["type"], "patchInlineRange");
        assert_eq!(wire["range"]["start"], 1);
        assert_eq!(wire["patch"]["fontSize"], 18.0);

        engine
            .execute(DocumentCommandBatch {
                base_revision: 5,
                commands: vec![DocumentCommand::PatchInlineRange {
                    block_id: "p-1".into(),
                    range: TextRange { start: 1, end: 4 },
                    patch: InlineStylePatch {
                        bold: Some(Some(true)),
                        font_size: Some(Some(18.0)),
                        color: Some(Some("#3366ff".into())),
                        ..Default::default()
                    },
                }],
            })
            .unwrap();

        let runs = &engine.model().blocks[0].content.as_ref().unwrap().runs;
        let expected_runs = runs.clone();
        assert_eq!(runs.len(), 3);
        assert_eq!((runs[0].start, runs[0].end), (0, 1));
        assert_eq!(runs[0].style, InlineStyle::default());
        assert_eq!((runs[1].start, runs[1].end), (1, 4));
        assert!(runs[1].style.bold);
        assert_eq!(runs[1].style.font_size, Some(18.0));
        assert_eq!(runs[1].style.color.as_deref(), Some("#3366ff"));
        assert_eq!((runs[2].start, runs[2].end), (4, 5));

        let error = engine.execute(DocumentCommandBatch {
            base_revision: 6,
            commands: vec![DocumentCommand::PatchInlineRange {
                block_id: "p-1".into(),
                range: TextRange { start: 4, end: 2 },
                patch: InlineStylePatch {
                    bold: Some(Some(false)),
                    ..Default::default()
                },
            }],
        });
        assert!(matches!(
            error,
            Err(DocumentEngineError::InvalidInlineRange { .. })
        ));
        assert_eq!(engine.revision(), 6);
        assert_eq!(
            engine.model().blocks[0].content.as_ref().unwrap().runs,
            expected_runs
        );
    }

    #[test]
    fn format_table_cells_persists_row_and_cell_presentation_through_the_engine() {
        let mut engine = engine();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block: table("table-1"),
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();

        let mut text_attrs = Map::new();
        text_attrs.insert("bold".into(), Value::Bool(true));
        text_attrs.insert("color".into(), Value::String("#f53f3f".into()));
        let changes = engine
            .execute(DocumentCommandBatch {
                base_revision: 5,
                commands: vec![DocumentCommand::FormatTableCells {
                    block_id: "table-1".into(),
                    selection: TableCellSelection::Row {
                        row_id: "row-a".into(),
                    },
                    patch: TableCellFormatPatch {
                        text_attrs,
                        fill_color: Some(Some("#fff2ac".into())),
                        horizontal_align: Some("center".into()),
                        vertical_align: Some("middle".into()),
                    },
                }],
            })
            .unwrap();

        assert_eq!(changes.changed_blocks, vec!["table-1".to_string()]);
        let BlockData::Table(table) = &engine.model().blocks[0].data else {
            panic!("table data expected");
        };
        for cell in &table.rows[0].cells {
            assert_eq!(cell.style.fill_color.as_deref(), Some("#fff2ac"));
            assert_eq!(cell.style.horizontal_align.as_deref(), Some("center"));
            assert_eq!(cell.style.vertical_align.as_deref(), Some("middle"));
            assert!(cell.content.runs[0].style.bold);
            assert_eq!(cell.content.runs[0].style.color.as_deref(), Some("#f53f3f"));
        }
    }

    #[test]
    fn table_cell_text_replacement_targets_one_stable_cell() {
        let mut engine = engine();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block: table("table-1"),
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();

        let changes = engine
            .execute(DocumentCommandBatch {
                base_revision: 5,
                commands: vec![DocumentCommand::ReplaceTableCellText {
                    block_id: "table-1".into(),
                    row_id: "row-a".into(),
                    cell_id: "cell-b".into(),
                    content: RichText {
                        text: "updated".into(),
                        runs: Vec::new(),
                    },
                }],
            })
            .unwrap();

        assert_eq!(changes.changed_blocks, vec!["table-1".to_string()]);
        let BlockData::Table(table) = &engine.model().blocks[0].data else {
            panic!("table data expected");
        };
        assert_eq!(table.rows[0].cells[0].content.text, "first");
        assert_eq!(table.rows[0].cells[1].content.text, "updated");
    }

    #[test]
    fn table_cell_inline_patch_targets_only_the_selected_unicode_range() {
        let mut engine = engine();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block: table("table-1"),
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();

        engine
            .execute(DocumentCommandBatch {
                base_revision: 5,
                commands: vec![DocumentCommand::PatchTableCellInlineRange {
                    block_id: "table-1".into(),
                    row_id: "row-a".into(),
                    cell_id: "cell-a".into(),
                    range: TextRange { start: 1, end: 4 },
                    patch: InlineStylePatch {
                        bold: Some(Some(true)),
                        color: Some(Some("#165dff".into())),
                        ..Default::default()
                    },
                }],
            })
            .unwrap();

        let BlockData::Table(table) = &engine.model().blocks[0].data else {
            panic!("table data expected");
        };
        let runs = &table.rows[0].cells[0].content.runs;
        assert_eq!(runs.len(), 3);
        assert_eq!((runs[0].start, runs[0].end), (0, 1));
        assert_eq!((runs[1].start, runs[1].end), (1, 4));
        assert!(runs[1].style.bold);
        assert_eq!(runs[1].style.color.as_deref(), Some("#165dff"));
        assert_eq!((runs[2].start, runs[2].end), (4, 5));
        assert_eq!(table.rows[0].cells[1].content.runs.len(), 1);
    }

    #[test]
    fn table_border_command_updates_and_clears_edges_by_stable_selection() {
        let mut engine = engine();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block: table("table-1"),
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();

        engine
            .execute(DocumentCommandBatch {
                base_revision: 5,
                commands: vec![DocumentCommand::SetTableBorders {
                    block_id: "table-1".into(),
                    selection: TableCellSelection::Cell {
                        row_id: "row-a".into(),
                        cell_id: "cell-b".into(),
                    },
                    patch: TableBorderPatch {
                        top: Some(Some(TableBorder {
                            style: TableBorderStyle::Solid,
                            color: "#1677ff".into(),
                            width: 1.0,
                        })),
                        ..TableBorderPatch::default()
                    },
                }],
            })
            .unwrap();
        let BlockData::Table(table) = &engine.model().blocks[0].data else {
            panic!("table data expected");
        };
        assert_eq!(table.rows[0].cells[0].style.borders.top, None);
        assert_eq!(
            table.rows[0].cells[1]
                .style
                .borders
                .top
                .as_ref()
                .map(|border| &border.color),
            Some(&"#1677ff".to_string())
        );

        engine
            .execute(DocumentCommandBatch {
                base_revision: 6,
                commands: vec![DocumentCommand::SetTableBorders {
                    block_id: "table-1".into(),
                    selection: TableCellSelection::Cell {
                        row_id: "row-a".into(),
                        cell_id: "cell-b".into(),
                    },
                    patch: TableBorderPatch {
                        top: Some(None),
                        ..TableBorderPatch::default()
                    },
                }],
            })
            .unwrap();
        let BlockData::Table(table) = &engine.model().blocks[0].data else {
            panic!("table data expected");
        };
        assert!(table.rows[0].cells[1].style.borders.top.is_none());
    }

    #[test]
    fn table_border_command_rejects_invalid_color_and_width() {
        let mut engine = engine();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block: table("table-1"),
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();
        let result = engine.execute(DocumentCommandBatch {
            base_revision: 5,
            commands: vec![DocumentCommand::SetTableBorders {
                block_id: "table-1".into(),
                selection: TableCellSelection::All,
                patch: TableBorderPatch {
                    bottom: Some(Some(TableBorder {
                        style: TableBorderStyle::Dashed,
                        color: "red".into(),
                        width: 64.0,
                    })),
                    ..TableBorderPatch::default()
                },
            }],
        });
        assert!(matches!(
            result,
            Err(DocumentEngineError::InvalidTableBorderPatch(_))
        ));
    }

    #[test]
    fn table_border_presets_resolve_outer_inner_and_diagonal_edges_in_engine() {
        let mut engine = engine();
        let mut block = table("table-1");
        let BlockData::Table(table) = &mut block.data else {
            panic!("table expected")
        };
        table.rows.push(TableRow {
            id: "row-b".into(),
            height: None,
            cells: vec![
                TableCell {
                    id: "cell-c".into(),
                    content: RichText::default(),
                    style: TableCellStyle::default(),
                },
                TableCell {
                    id: "cell-d".into(),
                    content: RichText::default(),
                    style: TableCellStyle::default(),
                },
            ],
        });
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block,
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();
        let border = TableBorder {
            style: TableBorderStyle::Solid,
            color: "#1677ff".into(),
            width: 1.0,
        };
        engine
            .execute(DocumentCommandBatch {
                base_revision: 5,
                commands: vec![DocumentCommand::ApplyTableBorderPreset {
                    block_id: "table-1".into(),
                    selection: TableCellSelection::All,
                    preset: TableBorderPreset::Outer,
                    border: Some(border.clone()),
                }],
            })
            .unwrap();
        let BlockData::Table(table) = &engine.model().blocks[0].data else {
            panic!("table expected")
        };
        assert!(table.rows[0].cells[0].style.borders.top.is_some());
        assert!(table.rows[0].cells[0].style.borders.left.is_some());
        assert!(table.rows[0].cells[0].style.borders.right.is_none());
        assert!(table.rows[1].cells[1].style.borders.bottom.is_some());
        assert!(table.rows[1].cells[1].style.borders.right.is_some());

        engine
            .execute(DocumentCommandBatch {
                base_revision: 6,
                commands: vec![DocumentCommand::ApplyTableBorderPreset {
                    block_id: "table-1".into(),
                    selection: TableCellSelection::All,
                    preset: TableBorderPreset::InnerVertical,
                    border: Some(border.clone()),
                }],
            })
            .unwrap();
        let BlockData::Table(table) = &engine.model().blocks[0].data else {
            panic!("table expected")
        };
        assert!(table.rows[0].cells[0].style.borders.right.is_some());
        assert!(table.rows[0].cells[1].style.borders.left.is_some());

        engine
            .execute(DocumentCommandBatch {
                base_revision: 7,
                commands: vec![DocumentCommand::ApplyTableBorderPreset {
                    block_id: "table-1".into(),
                    selection: TableCellSelection::Cell {
                        row_id: "row-a".into(),
                        cell_id: "cell-a".into(),
                    },
                    preset: TableBorderPreset::DiagonalDown,
                    border: Some(border),
                }],
            })
            .unwrap();
        let BlockData::Table(table) = &engine.model().blocks[0].data else {
            panic!("table expected")
        };
        assert!(table.rows[0].cells[0].style.borders.diagonal_down.is_some());
    }

    #[test]
    fn table_structure_commands_target_stable_row_and_column_ids() {
        let mut engine = engine();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block: table("table-1"),
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();

        engine
            .execute(DocumentCommandBatch {
                base_revision: 5,
                commands: vec![
                    DocumentCommand::InsertTableRow {
                        block_id: "table-1".into(),
                        index: 1,
                        row: TableRow {
                            id: "row-b".into(),
                            height: None,
                            cells: vec![
                                TableCell {
                                    id: "cell-c".into(),
                                    content: RichText::default(),
                                    style: TableCellStyle::default(),
                                },
                                TableCell {
                                    id: "cell-d".into(),
                                    content: RichText::default(),
                                    style: TableCellStyle::default(),
                                },
                            ],
                        },
                    },
                    DocumentCommand::InsertTableColumn {
                        block_id: "table-1".into(),
                        index: 1,
                        column: TableColumn {
                            id: "column-c".into(),
                            width: None,
                        },
                        cells: vec![
                            TableCell {
                                id: "cell-e".into(),
                                content: RichText::default(),
                                style: TableCellStyle::default(),
                            },
                            TableCell {
                                id: "cell-f".into(),
                                content: RichText::default(),
                                style: TableCellStyle::default(),
                            },
                        ],
                    },
                    DocumentCommand::DeleteTableRow {
                        block_id: "table-1".into(),
                        row_id: "row-b".into(),
                    },
                    DocumentCommand::DeleteTableColumn {
                        block_id: "table-1".into(),
                        column_id: "column-c".into(),
                    },
                ],
            })
            .unwrap();

        let BlockData::Table(table) = &engine.model().blocks[0].data else {
            panic!("table data expected");
        };
        assert_eq!(table.rows.len(), 1);
        assert_eq!(table.columns.len(), 2);
        assert_eq!(table.rows[0].cells.len(), 2);
        assert_eq!(table.rows[0].cells[0].content.text, "first");
        assert_eq!(table.rows[0].cells[1].content.text, "second");
    }

    #[test]
    fn table_column_width_command_updates_one_stable_column_atomically() {
        let mut engine = engine();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block: table("table-1"),
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();

        engine
            .execute(DocumentCommandBatch {
                base_revision: 5,
                commands: vec![DocumentCommand::SetTableColumnWidth {
                    block_id: "table-1".into(),
                    column_id: "column-b".into(),
                    width: 248.5,
                }],
            })
            .unwrap();

        let BlockData::Table(table) = &engine.model().blocks[0].data else {
            panic!("table data expected");
        };
        assert_eq!(table.columns[0].width, None);
        assert_eq!(table.columns[1].width, Some(248.5));

        let invalid = engine.execute(DocumentCommandBatch {
            base_revision: 6,
            commands: vec![DocumentCommand::SetTableColumnWidth {
                block_id: "table-1".into(),
                column_id: "column-b".into(),
                width: 31.0,
            }],
        });
        assert!(matches!(
            invalid,
            Err(DocumentEngineError::InvalidTableSelection(_))
        ));
        let BlockData::Table(table) = &engine.model().blocks[0].data else {
            panic!("table data expected");
        };
        assert_eq!(table.columns[1].width, Some(248.5));
    }

    #[test]
    fn table_row_height_command_updates_one_stable_row_atomically() {
        let mut engine = engine();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block: table("table-1"),
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();

        engine
            .execute(DocumentCommandBatch {
                base_revision: 5,
                commands: vec![DocumentCommand::SetTableRowHeight {
                    block_id: "table-1".into(),
                    row_id: "row-a".into(),
                    height: 72.0,
                }],
            })
            .unwrap();

        let BlockData::Table(table) = &engine.model().blocks[0].data else {
            panic!("table data expected");
        };
        assert_eq!(table.rows[0].height, Some(72.0));

        let invalid = engine.execute(DocumentCommandBatch {
            base_revision: 6,
            commands: vec![DocumentCommand::SetTableRowHeight {
                block_id: "table-1".into(),
                row_id: "row-a".into(),
                height: 23.0,
            }],
        });
        assert!(matches!(
            invalid,
            Err(DocumentEngineError::InvalidTableSelection(_))
        ));
        let BlockData::Table(table) = &engine.model().blocks[0].data else {
            panic!("table data expected");
        };
        assert_eq!(table.rows[0].height, Some(72.0));
    }

    #[test]
    fn table_range_formatting_uses_stable_row_and_column_ids() {
        let mut engine = engine();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block: table("table-1"),
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 5,
                commands: vec![DocumentCommand::FormatTableCells {
                    block_id: "table-1".into(),
                    selection: TableCellSelection::Range {
                        start_row_id: "row-a".into(),
                        end_row_id: "row-a".into(),
                        start_column_id: "column-a".into(),
                        end_column_id: "column-b".into(),
                    },
                    patch: TableCellFormatPatch {
                        text_attrs: Map::new(),
                        fill_color: Some(Some("#d9e7ff".into())),
                        horizontal_align: None,
                        vertical_align: None,
                    },
                }],
            })
            .unwrap();

        let BlockData::Table(table) = &engine.model().blocks[0].data else {
            panic!("table data expected");
        };
        assert!(table.rows[0]
            .cells
            .iter()
            .all(|cell| cell.style.fill_color.as_deref() == Some("#d9e7ff")));
    }

    #[test]
    fn table_merge_and_split_are_atomic_range_commands() {
        let mut engine = engine();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block: table("table-1"),
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();
        let range = TableRange {
            start_row_id: "row-a".into(),
            end_row_id: "row-a".into(),
            start_column_id: "column-a".into(),
            end_column_id: "column-b".into(),
        };
        engine
            .execute(DocumentCommandBatch {
                base_revision: 5,
                commands: vec![DocumentCommand::MergeTableCells {
                    block_id: "table-1".into(),
                    range: range.clone(),
                }],
            })
            .unwrap();
        let BlockData::Table(table) = &engine.model().blocks[0].data else {
            panic!("table data expected")
        };
        assert_eq!(table.merged_ranges, vec![range.clone()]);
        engine
            .execute(DocumentCommandBatch {
                base_revision: 6,
                commands: vec![DocumentCommand::SplitTableCells {
                    block_id: "table-1".into(),
                    range,
                }],
            })
            .unwrap();
        let BlockData::Table(table) = &engine.model().blocks[0].data else {
            panic!("table data expected")
        };
        assert!(table.merged_ranges.is_empty());
    }

    #[test]
    fn table_merge_absorbs_fully_enclosed_merged_ranges_but_rejects_partial_overlap() {
        let mut engine = engine();
        let mut table_block = table("table-1");
        let BlockData::Table(table) = &mut table_block.data else {
            panic!("table data expected")
        };
        table.columns.push(TableColumn {
            id: "column-c".into(),
            width: None,
        });
        table.rows[0].cells.push(TableCell {
            id: "cell-c".into(),
            content: RichText::default(),
            style: TableCellStyle::default(),
        });
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block: table_block,
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();

        let first_merge = TableRange {
            start_row_id: "row-a".into(),
            end_row_id: "row-a".into(),
            start_column_id: "column-a".into(),
            end_column_id: "column-b".into(),
        };
        engine
            .execute(DocumentCommandBatch {
                base_revision: 5,
                commands: vec![DocumentCommand::MergeTableCells {
                    block_id: "table-1".into(),
                    range: first_merge.clone(),
                }],
            })
            .unwrap();

        let partial_overlap = TableRange {
            start_row_id: "row-a".into(),
            end_row_id: "row-a".into(),
            start_column_id: "column-b".into(),
            end_column_id: "column-c".into(),
        };
        assert!(matches!(
            engine.execute(DocumentCommandBatch {
                base_revision: 6,
                commands: vec![DocumentCommand::MergeTableCells {
                    block_id: "table-1".into(),
                    range: partial_overlap,
                }],
            }),
            Err(DocumentEngineError::InvalidTableSelection(_))
        ));

        let expanded_merge = TableRange {
            start_row_id: "row-a".into(),
            end_row_id: "row-a".into(),
            start_column_id: "column-a".into(),
            end_column_id: "column-c".into(),
        };
        engine
            .execute(DocumentCommandBatch {
                base_revision: 6,
                commands: vec![DocumentCommand::MergeTableCells {
                    block_id: "table-1".into(),
                    range: expanded_merge.clone(),
                }],
            })
            .unwrap();
        let BlockData::Table(table) = &engine.model().blocks[0].data else {
            panic!("table data expected")
        };
        assert_eq!(table.merged_ranges, vec![expanded_merge]);
    }

    #[test]
    fn todo_checked_state_is_a_semantic_transaction() {
        let mut engine = engine();
        let mut todo = paragraph("todo-1", "ship release");
        todo.kind = DocumentBlockKind::Todo;
        todo.data = BlockData::Todo(TodoBlock::default());
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block: todo,
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();

        engine
            .execute(DocumentCommandBatch {
                base_revision: 5,
                commands: vec![DocumentCommand::SetTodoChecked {
                    block_id: "todo-1".into(),
                    checked: true,
                }],
            })
            .unwrap();

        assert_eq!(
            engine.model().blocks[0].data,
            BlockData::Todo(TodoBlock { checked: true })
        );
        assert!(engine
            .execute(DocumentCommandBatch {
                base_revision: 6,
                commands: vec![DocumentCommand::SetTodoChecked {
                    block_id: "todo-1".into(),
                    checked: false,
                }],
            })
            .is_ok());
        assert_eq!(
            engine.model().blocks[0].data,
            BlockData::Todo(TodoBlock { checked: false })
        );
    }

    #[test]
    fn word_insert_commands_create_typed_blocks_and_support_undo_redo() {
        let mut engine = engine();
        let text = |value: &str| RichText {
            text: value.into(),
            runs: Vec::new(),
        };
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![
                    DocumentCommand::InsertQuote {
                        block_id: "quote-1".into(),
                        content: text("quoted"),
                        parent_id: None,
                        index: 0,
                    },
                    DocumentCommand::InsertTodo {
                        block_id: "todo-1".into(),
                        content: text("ship"),
                        checked: true,
                        parent_id: None,
                        index: 1,
                    },
                    DocumentCommand::InsertLink {
                        block_id: "link-1".into(),
                        content: text("docs"),
                        url: "https://openoffice.example/docs".into(),
                        parent_id: None,
                        index: 2,
                    },
                    DocumentCommand::InsertDivider {
                        block_id: "divider-1".into(),
                        parent_id: None,
                        index: 3,
                    },
                ],
            })
            .unwrap();
        assert!(matches!(
            engine.read_block("quote-1").unwrap().kind,
            DocumentBlockKind::Quote
        ));
        assert_eq!(
            engine.read_block("todo-1").unwrap().data,
            BlockData::Todo(TodoBlock { checked: true })
        );
        assert_eq!(
            engine.read_block("link-1").unwrap().data,
            BlockData::Link(LinkBlock {
                url: "https://openoffice.example/docs".into()
            })
        );
        assert!(matches!(
            engine.read_block("divider-1").unwrap().kind,
            DocumentBlockKind::Divider
        ));
        engine.undo().unwrap();
        assert!(engine.read_block("quote-1").is_err());
        engine.redo().unwrap();
        assert_eq!(engine.model().root.len(), 4);
    }

    #[test]
    fn paragraph_and_inline_presentation_patches_cover_typed_style_fields() {
        let mut engine = engine();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block: paragraph("style-1", "styled"),
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 5,
                commands: vec![DocumentCommand::SetBlockPresentation {
                    block_id: "style-1".into(),
                    patch: BlockPresentationPatch {
                        named_style: Some(Some(ParagraphStyleRef {
                            name: "正文".into(),
                        })),
                        ..Default::default()
                    },
                }],
            })
            .unwrap();
        assert_eq!(
            engine
                .read_block("style-1")
                .unwrap()
                .presentation
                .named_style,
            Some(ParagraphStyleRef {
                name: "正文".into()
            })
        );
        engine
            .execute(DocumentCommandBatch {
                base_revision: 6,
                commands: vec![DocumentCommand::PatchInlineRange {
                    block_id: "style-1".into(),
                    range: TextRange { start: 0, end: 6 },
                    patch: InlineStylePatch {
                        vertical_align: Some(Some(VerticalAlign::Superscript)),
                        ..Default::default()
                    },
                }],
            })
            .unwrap();
        assert_eq!(
            engine
                .read_block("style-1")
                .unwrap()
                .content
                .as_ref()
                .unwrap()
                .runs[0]
                .style
                .vertical_align,
            Some(VerticalAlign::Superscript)
        );
    }

    #[test]
    fn link_target_is_owned_by_typed_semantic_commands() {
        let mut engine = engine();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block: paragraph("link-1", "Open Office"),
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();

        engine
            .execute(DocumentCommandBatch {
                base_revision: 5,
                commands: vec![DocumentCommand::ConvertToLink {
                    block_id: "link-1".into(),
                    url: "https://openoffice.example".into(),
                }],
            })
            .unwrap();
        assert_eq!(engine.model().blocks[0].kind, DocumentBlockKind::Link);
        assert_eq!(
            engine.model().blocks[0].data,
            BlockData::Link(LinkBlock {
                url: "https://openoffice.example".into(),
            })
        );

        engine
            .execute(DocumentCommandBatch {
                base_revision: 6,
                commands: vec![DocumentCommand::SetLinkTarget {
                    block_id: "link-1".into(),
                    url: "https://docs.openoffice.example".into(),
                }],
            })
            .unwrap();
        assert_eq!(
            engine.model().blocks[0].data,
            BlockData::Link(LinkBlock {
                url: "https://docs.openoffice.example".into(),
            })
        );
    }

    #[test]
    fn code_config_is_owned_by_a_typed_semantic_command() {
        let mut engine = engine();
        let mut code = paragraph("code-1", "const answer = 42;");
        code.kind = DocumentBlockKind::Code;
        code.data = BlockData::Code(CodeBlockConfig::default());
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block: code,
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();

        let config = CodeBlockConfig {
            language: "typescript".into(),
            theme: "oneDarkPro".into(),
            height: 320,
            ..CodeBlockConfig::default()
        };
        engine
            .execute(DocumentCommandBatch {
                base_revision: 5,
                commands: vec![DocumentCommand::SetCodeConfig {
                    block_id: "code-1".into(),
                    config: config.clone(),
                }],
            })
            .unwrap();
        assert_eq!(engine.model().blocks[0].data, BlockData::Code(config));
    }

    #[test]
    fn block_text_replacement_is_a_semantic_transaction() {
        let mut engine = engine();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block: paragraph("text-1", "before"),
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 5,
                commands: vec![DocumentCommand::ReplaceBlockText {
                    block_id: "text-1".into(),
                    content: RichText {
                        text: "after".into(),
                        runs: vec![InlineRun {
                            start: 0,
                            end: 5,
                            style: InlineStyle {
                                bold: true,
                                ..Default::default()
                            },
                        }],
                    },
                }],
            })
            .unwrap();
        assert_eq!(
            engine.model().blocks[0].content.as_ref().unwrap().text,
            "after"
        );
        assert_eq!(
            engine.model().blocks[0].content.as_ref().unwrap().runs[0].style,
            InlineStyle {
                bold: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn reset_block_is_the_only_document_clear_command_and_retired_update_is_rejected() {
        let retired = serde_json::from_value::<DocumentCommand>(serde_json::json!({
            "type": "updateBlock",
            "blockId": "p-1",
            "patch": { "content": { "text": "legacy", "runs": [] } }
        }));
        assert!(
            retired.is_err(),
            "generic update must not remain a wire command"
        );

        let mut engine = DocumentEngine::new(DocumentModel::empty(), 4).unwrap();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::ResetBlock {
                    block_id: "block-1".into(),
                }],
            })
            .unwrap();
        let root = &engine.model().blocks[0];
        assert_eq!(root.kind, DocumentBlockKind::Paragraph);
        assert_eq!(root.presentation, BlockPresentation::default());
        assert_eq!(root.content.as_ref().unwrap().text, "");
        assert_eq!(root.data, BlockData::None);
    }

    #[test]
    fn block_conversion_creates_required_typed_data_in_one_transaction() {
        let mut engine = engine();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block: paragraph("convert-1", "draft"),
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 5,
                commands: vec![DocumentCommand::ConvertBlock {
                    block_id: "convert-1".into(),
                    kind: DocumentBlockKind::Todo,
                }],
            })
            .unwrap();
        assert_eq!(engine.model().blocks[0].kind, DocumentBlockKind::Todo);
        assert_eq!(
            engine.model().blocks[0].data,
            BlockData::Todo(TodoBlock::default())
        );

        engine
            .execute(DocumentCommandBatch {
                base_revision: 6,
                commands: vec![DocumentCommand::SetTodoChecked {
                    block_id: "convert-1".into(),
                    checked: true,
                }],
            })
            .unwrap();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 7,
                commands: vec![DocumentCommand::ConvertBlock {
                    block_id: "convert-1".into(),
                    kind: DocumentBlockKind::Todo,
                }],
            })
            .unwrap();
        assert_eq!(
            engine.model().blocks[0].data,
            BlockData::Todo(TodoBlock { checked: true })
        );
    }

    #[test]
    fn transaction_inserts_updates_moves_and_deletes_blocks() {
        let mut engine = engine();
        let inserted = engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block: paragraph("p-1", "hello"),
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();
        assert_eq!(inserted.revision, 5);
        assert_eq!(engine.model().root, vec!["p-1"]);

        let updated = engine
            .execute(DocumentCommandBatch {
                base_revision: 5,
                commands: vec![DocumentCommand::ReplaceBlockText {
                    block_id: "p-1".into(),
                    content: RichText {
                        text: "updated".into(),
                        runs: Vec::new(),
                    },
                }],
            })
            .unwrap();
        assert!(!updated.structure_changed);
        assert_eq!(
            engine.model().blocks[0].content.as_ref().unwrap().text,
            "updated"
        );

        engine
            .execute(DocumentCommandBatch {
                base_revision: 6,
                commands: vec![DocumentCommand::InsertBlock {
                    block: paragraph("p-2", "tail"),
                    parent_id: None,
                    index: 1,
                }],
            })
            .unwrap();

        let deleted = engine
            .execute(DocumentCommandBatch {
                base_revision: 7,
                commands: vec![DocumentCommand::DeleteBlock {
                    block_id: "p-1".into(),
                }],
            })
            .unwrap();
        assert_eq!(deleted.changed_blocks, vec!["p-1"]);
        assert_eq!(engine.model().root, vec!["p-2"]);
    }

    #[test]
    fn deleting_the_last_root_block_is_rejected_atomically() {
        let mut engine = DocumentEngine::new(
            DocumentModel {
                root: vec!["p-1".into()],
                blocks: vec![paragraph("p-1", "only")],
                page_setup: None,
            },
            9,
        )
        .unwrap();
        let error = engine
            .execute(DocumentCommandBatch {
                base_revision: 9,
                commands: vec![DocumentCommand::DeleteBlock {
                    block_id: "p-1".into(),
                }],
            })
            .unwrap_err();
        assert!(matches!(
            error,
            DocumentEngineError::CannotDeleteLastRootBlock
        ));
        assert_eq!(engine.revision(), 9);
        assert_eq!(engine.model().root, vec!["p-1"]);
    }

    #[test]
    fn structural_changes_invalidate_the_affected_containers() {
        let mut engine = DocumentEngine::new(
            DocumentModel {
                root: vec!["page".into(), "tail".into()],
                blocks: vec![
                    DocumentBlock {
                        id: "page".into(),
                        kind: DocumentBlockKind::Page,
                        presentation: BlockPresentation::default(),
                        content: None,
                        children: vec![],
                        data: BlockData::None,
                    },
                    paragraph("tail", "tail"),
                ],
                page_setup: None,
            },
            0,
        )
        .unwrap();
        let inserted = engine
            .execute(DocumentCommandBatch {
                base_revision: 0,
                commands: vec![DocumentCommand::InsertBlock {
                    block: paragraph("p-1", "inside"),
                    parent_id: Some("page".into()),
                    index: 0,
                }],
            })
            .unwrap();
        assert_eq!(inserted.changed_blocks, vec!["p-1", "page"]);

        let moved = engine
            .execute(DocumentCommandBatch {
                base_revision: 1,
                commands: vec![DocumentCommand::MoveBlock {
                    block_id: "p-1".into(),
                    parent_id: None,
                    index: 1,
                }],
            })
            .unwrap();
        assert_eq!(moved.changed_blocks, vec!["p-1", "page"]);
    }

    #[test]
    fn nested_move_and_delete_preserve_tree_invariants() {
        let mut engine = DocumentEngine::new(
            DocumentModel {
                root: vec!["page".into(), "tail".into()],
                blocks: vec![
                    DocumentBlock {
                        id: "page".into(),
                        kind: DocumentBlockKind::Page,
                        presentation: BlockPresentation::default(),
                        content: None,
                        children: vec!["p-1".into()],
                        data: BlockData::None,
                    },
                    paragraph("p-1", "inside"),
                    paragraph("tail", "tail"),
                ],
                page_setup: None,
            },
            0,
        )
        .unwrap();

        engine
            .execute(DocumentCommandBatch {
                base_revision: 0,
                commands: vec![DocumentCommand::MoveBlock {
                    block_id: "p-1".into(),
                    parent_id: None,
                    index: 1,
                }],
            })
            .unwrap();
        assert_eq!(engine.model().root, vec!["page", "p-1", "tail"]);

        engine
            .execute(DocumentCommandBatch {
                base_revision: 1,
                commands: vec![DocumentCommand::MoveBlock {
                    block_id: "p-1".into(),
                    parent_id: Some("page".into()),
                    index: 0,
                }],
            })
            .unwrap();

        let error = engine
            .execute(DocumentCommandBatch {
                base_revision: 2,
                commands: vec![DocumentCommand::MoveBlock {
                    block_id: "page".into(),
                    parent_id: Some("p-1".into()),
                    index: 0,
                }],
            })
            .unwrap_err();
        assert!(matches!(
            error,
            DocumentEngineError::CannotMoveIntoDescendant { .. }
        ));
        assert!(engine.model().validate().is_ok());
    }

    #[test]
    fn failed_batch_is_atomic_and_revision_is_checked() {
        let mut engine = engine();
        let error = engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![
                    DocumentCommand::InsertBlock {
                        block: paragraph("p-1", "kept?"),
                        parent_id: None,
                        index: 0,
                    },
                    DocumentCommand::MoveBlock {
                        block_id: "missing".into(),
                        parent_id: None,
                        index: 0,
                    },
                ],
            })
            .unwrap_err();
        assert!(matches!(error, DocumentEngineError::MissingBlock(_)));
        assert!(engine.model().blocks.is_empty());
        assert_eq!(engine.revision(), 4);

        let error = engine
            .execute(DocumentCommandBatch {
                base_revision: 3,
                commands: vec![DocumentCommand::SetPageSetup { page_setup: None }],
            })
            .unwrap_err();
        assert!(matches!(
            error,
            DocumentEngineError::RevisionConflict { .. }
        ));
    }

    #[test]
    fn failed_transaction_does_not_detach_the_copy_on_write_snapshot() {
        let mut engine = engine();
        let before = engine.model().clone();
        let error = engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::MoveBlock {
                    block_id: "missing".into(),
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap_err();
        assert!(matches!(error, DocumentEngineError::MissingBlock(_)));
        assert_eq!(before, *engine.model());
    }

    #[test]
    fn extension_blocks_are_preserved_as_first_class_data() {
        let extension = DocumentBlock {
            id: "ext-1".into(),
            kind: DocumentBlockKind::Extension {
                type_id: "diagram".into(),
            },
            presentation: BlockPresentation::default(),
            content: None,
            children: Vec::new(),
            data: BlockData::Extension(BlockExtension {
                type_id: "diagram".into(),
                raw: serde_json::json!({"provider": "x"}),
            }),
        };
        let mut engine = engine();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 4,
                commands: vec![DocumentCommand::InsertBlock {
                    block: extension.clone(),
                    parent_id: None,
                    index: 0,
                }],
            })
            .unwrap();
        assert_eq!(engine.model().blocks[0], extension);
    }

    #[test]
    fn nested_delete_failure_rolls_back_model_and_indexes_without_snapshot_clone() {
        let model = DocumentModel {
            root: vec!["page".into(), "tail".into()],
            blocks: vec![
                DocumentBlock {
                    id: "page".into(),
                    kind: DocumentBlockKind::Page,
                    presentation: BlockPresentation::default(),
                    content: None,
                    children: vec!["group".into()],
                    data: BlockData::None,
                },
                DocumentBlock {
                    id: "group".into(),
                    kind: DocumentBlockKind::Callout,
                    presentation: BlockPresentation::default(),
                    content: None,
                    children: vec!["leaf".into()],
                    data: BlockData::None,
                },
                paragraph("leaf", "nested"),
                paragraph("tail", "tail"),
            ],
            page_setup: None,
        };
        let mut engine = DocumentEngine::new(model.clone(), 10).unwrap();
        let error = engine
            .execute(DocumentCommandBatch {
                base_revision: 10,
                commands: vec![
                    DocumentCommand::DeleteBlock {
                        block_id: "page".into(),
                    },
                    DocumentCommand::MoveBlock {
                        block_id: "missing".into(),
                        parent_id: None,
                        index: 0,
                    },
                ],
            })
            .unwrap_err();
        assert!(matches!(error, DocumentEngineError::MissingBlock(_)));
        assert_eq!(*engine.model(), model);
        assert_eq!(engine.revision(), 10);
        assert!(engine.model().validate().is_ok());
        assert!(engine.journal().is_empty());
    }

    #[test]
    fn journal_exposes_container_invalidation_and_nested_inverse() {
        let mut engine = DocumentEngine::new(
            DocumentModel {
                root: vec!["page".into(), "tail".into()],
                blocks: vec![
                    DocumentBlock {
                        id: "page".into(),
                        kind: DocumentBlockKind::Page,
                        presentation: BlockPresentation::default(),
                        content: None,
                        children: Vec::new(),
                        data: BlockData::None,
                    },
                    paragraph("tail", "tail"),
                ],
                page_setup: None,
            },
            0,
        )
        .unwrap();
        let result = engine.execute(DocumentCommandBatch {
            base_revision: 0,
            commands: vec![DocumentCommand::InsertBlock {
                block: DocumentBlock {
                    id: "child".into(),
                    kind: DocumentBlockKind::Callout,
                    presentation: BlockPresentation::default(),
                    content: None,
                    children: vec!["tail".into()],
                    data: BlockData::None,
                },
                parent_id: Some("page".into()),
                index: 0,
            }],
        });
        // Existing root `tail` cannot become a child of the inserted block, so this deliberately
        // invalid transaction must not leave an insertion behind.
        assert!(result.is_err());
        assert_eq!(engine.model().root, vec!["page", "tail"]);
        assert!(engine.journal().is_empty());

        let result = engine
            .execute(DocumentCommandBatch {
                base_revision: 0,
                commands: vec![DocumentCommand::InsertBlock {
                    block: DocumentBlock {
                        id: "child".into(),
                        kind: DocumentBlockKind::Callout,
                        presentation: BlockPresentation::default(),
                        content: None,
                        children: Vec::new(),
                        data: BlockData::None,
                    },
                    parent_id: Some("page".into()),
                    index: 0,
                }],
            })
            .unwrap();
        assert_eq!(result.changed_containers, vec!["page"]);
        assert_eq!(engine.journal().len(), 1);
        assert!(matches!(
            engine.journal().last_inverse().unwrap().as_slice(),
            [Mutation::RemoveInserted { .. }]
        ));
    }

    #[test]
    fn undo_redo_replays_content_data_and_nested_structure() {
        let mut engine = DocumentEngine::new(
            DocumentModel {
                root: vec!["page".into(), "tail".into()],
                blocks: vec![
                    DocumentBlock {
                        id: "page".into(),
                        kind: DocumentBlockKind::Page,
                        presentation: BlockPresentation::default(),
                        content: None,
                        children: Vec::new(),
                        data: BlockData::None,
                    },
                    paragraph("tail", "tail"),
                ],
                page_setup: None,
            },
            0,
        )
        .unwrap();
        let mut code = paragraph("code", "one");
        code.kind = DocumentBlockKind::Code;
        code.data = oo_schema::BlockData::Code(oo_schema::CodeBlockConfig::default());
        engine
            .execute(DocumentCommandBatch {
                base_revision: 0,
                commands: vec![DocumentCommand::InsertBlock {
                    block: code,
                    parent_id: Some("page".into()),
                    index: 0,
                }],
            })
            .unwrap();

        let config = oo_schema::CodeBlockConfig {
            title: "Example".into(),
            ..oo_schema::CodeBlockConfig::default()
        };
        engine
            .execute(DocumentCommandBatch {
                base_revision: 1,
                commands: vec![
                    DocumentCommand::ReplaceBlockText {
                        block_id: "code".into(),
                        content: RichText {
                            text: "two".into(),
                            runs: Vec::new(),
                        },
                    },
                    DocumentCommand::SetCodeConfig {
                        block_id: "code".into(),
                        config,
                    },
                ],
            })
            .unwrap();
        assert_eq!(
            engine
                .store
                .block("code")
                .unwrap()
                .content
                .as_ref()
                .unwrap()
                .text,
            "two"
        );

        let undone = engine.undo().unwrap();
        assert_eq!(
            engine
                .store
                .block("code")
                .unwrap()
                .content
                .as_ref()
                .unwrap()
                .text,
            "one"
        );
        assert!(matches!(
            undone.mutations.as_slice(),
            [Mutation::Update { .. }, Mutation::Update { .. }]
        ));
        engine.redo().unwrap();
        assert_eq!(
            engine
                .store
                .block("code")
                .unwrap()
                .content
                .as_ref()
                .unwrap()
                .text,
            "two"
        );
        assert!(matches!(
            engine.store.block("code").unwrap().data,
            oo_schema::BlockData::Code(_)
        ));

        engine.undo().unwrap();
        engine.undo().unwrap();
        assert_eq!(engine.model().root, vec!["page", "tail"]);
        assert!(!engine.journal().can_undo());
        assert!(engine.journal().can_redo());
        engine.redo().unwrap();
        engine.redo().unwrap();
        assert_eq!(engine.model().root, vec!["page", "tail"]);
        assert_eq!(engine.store.block("page").unwrap().children, vec!["code"]);
        assert_eq!(
            engine
                .store
                .block("code")
                .unwrap()
                .content
                .as_ref()
                .unwrap()
                .text,
            "two"
        );
        assert!(engine.model().validate().is_ok());
    }

    #[test]
    fn undo_redo_restores_and_removes_nested_subtrees() {
        let mut engine = DocumentEngine::new(
            DocumentModel {
                root: vec!["page".into(), "tail".into()],
                blocks: vec![
                    DocumentBlock {
                        id: "page".into(),
                        kind: DocumentBlockKind::Page,
                        presentation: BlockPresentation::default(),
                        content: None,
                        children: vec!["group".into()],
                        data: BlockData::None,
                    },
                    DocumentBlock {
                        id: "group".into(),
                        kind: DocumentBlockKind::Callout,
                        presentation: BlockPresentation::default(),
                        content: None,
                        children: vec!["leaf".into()],
                        data: BlockData::None,
                    },
                    paragraph("leaf", "nested"),
                    paragraph("tail", "tail"),
                ],
                page_setup: None,
            },
            0,
        )
        .unwrap();
        engine
            .execute(DocumentCommandBatch {
                base_revision: 0,
                commands: vec![DocumentCommand::DeleteBlock {
                    block_id: "group".into(),
                }],
            })
            .unwrap();
        assert_eq!(
            engine.store.block("page").unwrap().children,
            Vec::<String>::new()
        );
        assert!(engine.store.block("group").is_err());

        engine.undo().unwrap();
        assert_eq!(engine.store.block("page").unwrap().children, vec!["group"]);
        assert_eq!(engine.store.block("group").unwrap().children, vec!["leaf"]);
        assert_eq!(
            engine
                .store
                .block("leaf")
                .unwrap()
                .content
                .as_ref()
                .unwrap()
                .text,
            "nested"
        );
        assert!(engine.model().validate().is_ok());

        engine.redo().unwrap();
        assert_eq!(
            engine.store.block("page").unwrap().children,
            Vec::<String>::new()
        );
        assert!(engine.store.block("group").is_err());
        assert!(engine.model().validate().is_ok());
    }
}
