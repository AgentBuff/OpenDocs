//! open-office 的版本化 Artifact schema。
//!
//! 这里故意不依赖 Canvas、DOM、WASM 或具体编辑器。它只描述可持久化、可同步、可迁移
//! 的数据结构。Document 使用 Block Tree；Spreadsheet、Presentation、Mindmap、Whiteboard
//! 使用各自的模型，避免把所有产品错误地抽象成 Paragraph 或 Block。

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

mod migration;
/// Offline v4 → v5 Presentation compiler. It is an import/migration boundary only; online
/// snapshots accept the v5 Deck and reject retired v4 presentation payloads.
pub mod presentation_migration;
/// The canonical v5 Presentation schema. It is the only Presentation payload accepted by an
/// online Artifact envelope; legacy v4 scene graph data is parsed exclusively by the offline
/// migration compiler above.
pub mod presentation_v5;

pub use migration::{
    migrate_artifact_v1_to_v3, migrate_artifact_v2_to_v3, migrate_artifact_v3_to_v4,
    migrate_artifact_v4_to_v5, ArtifactMigrationError,
};

pub type ArtifactId = String;
pub type BlockId = String;
pub type ElementId = String;
pub type AssetId = String;

/// 持久化 schema 版本，与服务端 revision、协同 clock 完全分离。
pub const CURRENT_SCHEMA_VERSION: u16 = 5;

/// 所有可编辑内容共享的外层信封。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactEnvelope {
    pub format: String,
    pub schema_version: u16,
    pub artifact_id: ArtifactId,
    pub revision: u64,
    pub kind: ArtifactKind,
    pub payload: ArtifactPayload,
}

impl ArtifactEnvelope {
    pub fn new(artifact_id: impl Into<ArtifactId>, payload: ArtifactPayload) -> Self {
        let kind = payload.kind();
        Self {
            format: "open-office-artifact".into(),
            schema_version: CURRENT_SCHEMA_VERSION,
            artifact_id: artifact_id.into(),
            revision: 0,
            kind,
            payload,
        }
    }

    pub fn validate(&self) -> Result<(), SchemaValidationError> {
        if self.format != "open-office-artifact" {
            return Err(SchemaValidationError::UnsupportedFormat(
                self.format.clone(),
            ));
        }
        if self.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(SchemaValidationError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }
        if self.artifact_id.trim().is_empty() {
            return Err(SchemaValidationError::EmptyId("artifactId"));
        }
        if self.kind != self.payload.kind() {
            return Err(SchemaValidationError::KindMismatch {
                envelope: self.kind,
                payload: self.payload.kind(),
            });
        }
        self.payload.validate()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArtifactKind {
    Document,
    Spreadsheet,
    Presentation,
    Mindmap,
    Whiteboard,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "camelCase")]
pub enum ArtifactPayload {
    Document(DocumentModel),
    Spreadsheet(SpreadsheetModel),
    Presentation(presentation_v5::Deck),
    Mindmap(MindmapModel),
    Whiteboard(WhiteboardModel),
}

impl ArtifactPayload {
    pub fn kind(&self) -> ArtifactKind {
        match self {
            Self::Document(_) => ArtifactKind::Document,
            Self::Spreadsheet(_) => ArtifactKind::Spreadsheet,
            Self::Presentation(_) => ArtifactKind::Presentation,
            Self::Mindmap(_) => ArtifactKind::Mindmap,
            Self::Whiteboard(_) => ArtifactKind::Whiteboard,
        }
    }

    fn validate(&self) -> Result<(), SchemaValidationError> {
        match self {
            Self::Document(document) => document.validate(),
            Self::Spreadsheet(sheet) => sheet.validate(),
            Self::Presentation(deck) => deck.validate(),
            Self::Mindmap(map) => map.validate(),
            Self::Whiteboard(board) => board.validate(),
        }
    }
}

/// 文档模型：根节点和 block 记录分开保存，children 是唯一的父子顺序真相。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentModel {
    #[serde(default)]
    pub root: Vec<BlockId>,
    #[serde(default)]
    pub blocks: Vec<DocumentBlock>,
    #[serde(default)]
    pub page_setup: Option<PageSetup>,
}

impl DocumentModel {
    /// 返回文档中可见文本的线性摘要。
    ///
    /// 这只是元数据（例如未命名文档的标题）需要的查询，不参与编辑和排版；真正的
    /// 内容仍然保存在 block tree 中。只遍历 root，避免把同一 block 因为未来扩展
    /// 容器而重复计入摘要。
    pub fn plain_text(&self) -> String {
        let by_id: HashMap<&str, &DocumentBlock> = self
            .blocks
            .iter()
            .map(|block| (block.id.as_str(), block))
            .collect();
        let mut lines = Vec::new();
        for id in &self.root {
            collect_text(id, &by_id, &mut lines);
        }
        lines.join("\n")
    }

    /// 创建一份可立即编辑的空白文档。
    pub fn empty() -> Self {
        let block = DocumentBlock {
            id: "block-1".into(),
            kind: DocumentBlockKind::Paragraph,
            presentation: BlockPresentation::default(),
            content: Some(RichText::default()),
            children: Vec::new(),
            data: BlockData::None,
        };
        Self {
            root: vec![block.id.clone()],
            blocks: vec![block],
            page_setup: None,
        }
    }

    pub fn validate(&self) -> Result<(), SchemaValidationError> {
        let mut ids = HashSet::new();
        let by_id: HashMap<&str, &DocumentBlock> = self
            .blocks
            .iter()
            .map(|block| (block.id.as_str(), block))
            .collect();

        if by_id.len() != self.blocks.len() {
            return Err(SchemaValidationError::DuplicateId("block"));
        }
        for id in &self.root {
            if id.trim().is_empty() {
                return Err(SchemaValidationError::EmptyId("block"));
            }
            if !ids.insert(id) {
                return Err(SchemaValidationError::DuplicateChild(id.clone()));
            }
            if !by_id.contains_key(id.as_str()) {
                return Err(SchemaValidationError::MissingReference {
                    owner: "root".into(),
                    target: id.clone(),
                });
            }
        }

        for block in &self.blocks {
            if block.id.trim().is_empty() {
                return Err(SchemaValidationError::EmptyId("block"));
            }
            block.kind.validate(&block.id)?;
            block.presentation.validate(&block.id)?;
            if let Some(content) = &block.content {
                content.validate(&block.id)?;
            }
            validate_block_data(block)?;
            let mut children = HashSet::new();
            for child in &block.children {
                if child.trim().is_empty() {
                    return Err(SchemaValidationError::EmptyId("block"));
                }
                if !children.insert(child) {
                    return Err(SchemaValidationError::DuplicateChild(child.clone()));
                }
                if !by_id.contains_key(child.as_str()) {
                    return Err(SchemaValidationError::MissingReference {
                        owner: block.id.clone(),
                        target: child.clone(),
                    });
                }
            }
        }

        // Block Tree 不能有环。父节点只通过 children 表达，避免 parent_id/children 双写漂移。
        let mut visiting = HashSet::new();
        let mut visited = HashSet::new();
        let mut owners = HashMap::new();
        for id in &self.root {
            visit_block(
                id,
                "<root>",
                &by_id,
                &mut visiting,
                &mut visited,
                &mut owners,
            )?;
        }
        if let Some(block) = self
            .blocks
            .iter()
            .find(|block| !visited.contains(block.id.as_str()))
        {
            return Err(SchemaValidationError::UnreachableBlock(block.id.clone()));
        }
        if let Some(page_setup) = &self.page_setup {
            page_setup.validate()?;
        }
        Ok(())
    }
}

fn collect_text(id: &str, by_id: &HashMap<&str, &DocumentBlock>, lines: &mut Vec<String>) {
    let Some(block) = by_id.get(id) else { return };
    if let Some(content) = &block.content {
        lines.push(content.text.clone());
    }
    if let BlockData::Table(table) = &block.data {
        for row in &table.rows {
            lines.push(
                row.cells
                    .iter()
                    .map(|cell| cell.content.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\t"),
            );
        }
    }
    for child in &block.children {
        collect_text(child, by_id, lines);
    }
}

fn validate_block_data(block: &DocumentBlock) -> Result<(), SchemaValidationError> {
    match (&block.kind, &block.data) {
        (DocumentBlockKind::Image, BlockData::Image(image)) => {
            if image.asset_id.trim().is_empty() {
                return Err(SchemaValidationError::EmptyId("image assetId"));
            }
            if image
                .original_asset_id
                .as_deref()
                .is_some_and(|asset_id| asset_id.trim().is_empty())
            {
                return Err(SchemaValidationError::EmptyId("image originalAssetId"));
            }
            if image.caption.chars().count() > 512 {
                return Err(SchemaValidationError::InvalidValue(format!(
                    "image block {} 的题注不能超过 512 个字符",
                    block.id
                )));
            }
            image.transform.crop.validate(&block.id)?;
        }
        (DocumentBlockKind::Table, BlockData::Table(table)) => {
            validate_table_payload(table, &block.id)?;
        }
        (DocumentBlockKind::Code, BlockData::Code(code)) => {
            code.validate(&block.id)?;
        }
        (DocumentBlockKind::Todo, BlockData::Todo(_)) => {}
        (DocumentBlockKind::Link, BlockData::Link(link)) => {
            link.validate(&block.id)?;
        }
        (DocumentBlockKind::Image, _) => {
            return Err(SchemaValidationError::InvalidValue(format!(
                "image block {} 必须包含 image data",
                block.id
            )));
        }
        (DocumentBlockKind::Table, _) => {
            return Err(SchemaValidationError::InvalidValue(format!(
                "table block {} 必须包含 table data",
                block.id
            )));
        }
        (DocumentBlockKind::Code, BlockData::None) => {
            return Err(SchemaValidationError::InvalidValue(format!(
                "code block {} 必须包含 code data",
                block.id
            )));
        }
        (DocumentBlockKind::Code, _) => {
            return Err(SchemaValidationError::InvalidValue(format!(
                "code block {} 的 data.type 必须是 code",
                block.id
            )));
        }
        (DocumentBlockKind::Todo, _) => {
            return Err(SchemaValidationError::InvalidValue(format!(
                "todo block {} 必须包含 todo data",
                block.id
            )));
        }
        (DocumentBlockKind::Link, _) => {
            return Err(SchemaValidationError::InvalidValue(format!(
                "link block {} 必须包含 link data",
                block.id
            )));
        }
        (DocumentBlockKind::Extension { type_id }, BlockData::Extension(extension))
            if type_id == &extension.type_id =>
        {
            validate_extension(extension, &block.id)?;
        }
        (DocumentBlockKind::Unknown { type_id, .. }, BlockData::Extension(extension))
            if type_id == &extension.type_id =>
        {
            validate_extension(extension, &block.id)?;
        }
        (DocumentBlockKind::Extension { .. } | DocumentBlockKind::Unknown { .. }, _) => {
            return Err(SchemaValidationError::InvalidValue(format!(
                "扩展 block {} 必须携带匹配的 extension data",
                block.id
            )));
        }
        (_, BlockData::None) => {}
        (_, BlockData::Extension(_)) => {
            return Err(SchemaValidationError::InvalidValue(format!(
                "文本 block {} 不允许携带 extension data",
                block.id
            )));
        }
        (_, _) => {
            return Err(SchemaValidationError::InvalidValue(format!(
                "block {} 的 data 与 kind 不匹配",
                block.id
            )));
        }
    }
    Ok(())
}

fn validate_extension(
    extension: &BlockExtension,
    block_id: &str,
) -> Result<(), SchemaValidationError> {
    if extension.type_id.trim().is_empty() || extension.type_id.chars().count() > 128 {
        return Err(SchemaValidationError::InvalidValue(format!(
            "block {block_id} 的 extension.typeId 必须是 1 到 128 个字符"
        )));
    }
    Ok(())
}

fn validate_table_payload(table: &TableBlock, block_id: &str) -> Result<(), SchemaValidationError> {
    if table.columns.is_empty() {
        return Err(SchemaValidationError::InvalidValue(format!(
            "table block {block_id} 至少需要一列"
        )));
    }
    let column_ids = table.columns.iter().map(|column| column.id.as_str());
    unique_ids(column_ids, "table column")?;
    for column in &table.columns {
        if let Some(width) = column.width {
            if !width.is_finite() || width <= 0.0 {
                return Err(SchemaValidationError::InvalidValue(format!(
                    "table block {block_id} 的列 {} 宽度必须是正数",
                    column.id
                )));
            }
        }
    }
    unique_ids(table.rows.iter().map(|row| row.id.as_str()), "table row")?;
    for row in &table.rows {
        if let Some(height) = row.height {
            if !height.is_finite() || height <= 0.0 {
                return Err(SchemaValidationError::InvalidValue(format!(
                    "table block {block_id} 的行 {} 高度必须是正数",
                    row.id
                )));
            }
        }
        if row.cells.len() != table.columns.len() {
            return Err(SchemaValidationError::InvalidValue(format!(
                "table block {block_id} 的行 {} 单元格数量与列数不一致",
                row.id
            )));
        }
        unique_ids(row.cells.iter().map(|cell| cell.id.as_str()), "table cell")?;
        for cell in &row.cells {
            cell.content.validate(block_id)?;
            cell.style.borders.validate(block_id, &cell.id)?;
        }
    }
    for (index, range) in table.merged_ranges.iter().enumerate() {
        let start_row = table
            .rows
            .iter()
            .position(|row| row.id == range.start_row_id)
            .ok_or_else(|| {
                SchemaValidationError::InvalidValue(format!(
                    "table block {block_id} 合并范围起始行不存在"
                ))
            })?;
        let end_row = table
            .rows
            .iter()
            .position(|row| row.id == range.end_row_id)
            .ok_or_else(|| {
                SchemaValidationError::InvalidValue(format!(
                    "table block {block_id} 合并范围结束行不存在"
                ))
            })?;
        let start_column = table
            .columns
            .iter()
            .position(|column| column.id == range.start_column_id)
            .ok_or_else(|| {
                SchemaValidationError::InvalidValue(format!(
                    "table block {block_id} 合并范围起始列不存在"
                ))
            })?;
        let end_column = table
            .columns
            .iter()
            .position(|column| column.id == range.end_column_id)
            .ok_or_else(|| {
                SchemaValidationError::InvalidValue(format!(
                    "table block {block_id} 合并范围结束列不存在"
                ))
            })?;
        if start_row > end_row
            || start_column > end_column
            || (start_row == end_row && start_column == end_column)
        {
            return Err(SchemaValidationError::InvalidValue(format!(
                "table block {block_id} 合并范围无效"
            )));
        }
        for previous in &table.merged_ranges[..index] {
            let previous_start_row = table
                .rows
                .iter()
                .position(|row| row.id == previous.start_row_id)
                .unwrap();
            let previous_end_row = table
                .rows
                .iter()
                .position(|row| row.id == previous.end_row_id)
                .unwrap();
            let previous_start_column = table
                .columns
                .iter()
                .position(|column| column.id == previous.start_column_id)
                .unwrap();
            let previous_end_column = table
                .columns
                .iter()
                .position(|column| column.id == previous.end_column_id)
                .unwrap();
            if start_row <= previous_end_row
                && previous_start_row <= end_row
                && start_column <= previous_end_column
                && previous_start_column <= end_column
            {
                return Err(SchemaValidationError::InvalidValue(format!(
                    "table block {block_id} 合并范围重叠"
                )));
            }
        }
    }
    Ok(())
}

fn visit_block(
    id: &str,
    owner: &str,
    by_id: &HashMap<&str, &DocumentBlock>,
    visiting: &mut HashSet<String>,
    visited: &mut HashSet<String>,
    owners: &mut HashMap<String, String>,
) -> Result<(), SchemaValidationError> {
    if visiting.contains(id) {
        return Err(SchemaValidationError::BlockCycle(id.to_string()));
    }
    if let Some(previous_owner) = owners.insert(id.to_string(), owner.to_string()) {
        if previous_owner != owner {
            return Err(SchemaValidationError::MultipleParents {
                child: id.to_string(),
                first: previous_owner,
                second: owner.to_string(),
            });
        }
        if visited.contains(id) {
            return Ok(());
        }
    }
    visiting.insert(id.to_string());
    let block = by_id
        .get(id)
        .ok_or_else(|| SchemaValidationError::MissingReference {
            owner: "block".into(),
            target: id.to_string(),
        })?;
    for child in &block.children {
        visit_block(child, id, by_id, visiting, visited, owners)?;
    }
    visiting.remove(id);
    visited.insert(id.to_string());
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentBlock {
    pub id: BlockId,
    pub kind: DocumentBlockKind,
    #[serde(default)]
    pub presentation: BlockPresentation,
    #[serde(default)]
    pub content: Option<RichText>,
    #[serde(default)]
    pub children: Vec<BlockId>,
    /// 结构化 block 的领域 data；文本 block 使用 `none`。
    #[serde(default)]
    pub data: BlockData,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum BlockData {
    #[default]
    None,
    Image(ImageBlock),
    Table(TableBlock),
    Code(CodeBlockConfig),
    Todo(TodoBlock),
    Link(LinkBlock),
    Extension(BlockExtension),
}

/// Opaque, namespaced data for a block type unknown to this schema version.
///
/// Unknown data is intentionally kept behind a typed envelope rather than a free-form field on
/// every block. A client which does not understand the extension can still round-trip its raw
/// payload without applying renderer semantics to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlockExtension {
    pub type_id: String,
    pub raw: Value,
}

/// Paragraph-level presentation. This is the only place where paragraph formatting is persisted;
/// content, renderer state and extension data must not be put in a generic map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlockPresentation {
    #[serde(default)]
    pub align: BlockAlignment,
    #[serde(default)]
    pub list: Option<ListPresentation>,
    #[serde(default)]
    pub indent_start: u8,
    #[serde(default)]
    pub indent_end: f32,
    #[serde(default)]
    pub spacing_before: f32,
    #[serde(default)]
    pub spacing_after: f32,
    #[serde(default = "default_line_height")]
    pub line_height: f32,
    #[serde(default)]
    pub named_style: Option<ParagraphStyleRef>,
}

impl Default for BlockPresentation {
    fn default() -> Self {
        Self {
            align: BlockAlignment::default(),
            list: None,
            indent_start: 0,
            indent_end: 0.0,
            spacing_before: 0.0,
            spacing_after: 0.0,
            line_height: default_line_height(),
            named_style: None,
        }
    }
}

impl BlockPresentation {
    pub(crate) fn validate(&self, block_id: &str) -> Result<(), SchemaValidationError> {
        if let Some(list) = &self.list {
            list.validate(block_id)?;
        }
        if self.indent_start > 20 {
            return Err(SchemaValidationError::InvalidValue(format!(
                "block {block_id} 的 indentStart 必须是 0 到 20 的整数"
            )));
        }
        for (field, value) in [
            ("indentEnd", self.indent_end),
            ("spacingBefore", self.spacing_before),
            ("spacingAfter", self.spacing_after),
        ] {
            if !value.is_finite() || !(0.0..=1_000.0).contains(&value) {
                return Err(SchemaValidationError::InvalidValue(format!(
                    "block {block_id} 的 {field} 必须在 0 到 1000 之间"
                )));
            }
        }
        if !self.line_height.is_finite() || !(0.1..=10.0).contains(&self.line_height) {
            return Err(SchemaValidationError::InvalidValue(format!(
                "block {block_id} 的 lineHeight 必须在 0.1 到 10 之间"
            )));
        }
        if let Some(named_style) = &self.named_style {
            named_style.validate(block_id)?;
        }
        Ok(())
    }
}

fn default_line_height() -> f32 {
    1.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlockAlignment {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListPresentation {
    pub kind: ListKind,
    #[serde(default)]
    pub level: u8,
}

impl ListPresentation {
    fn validate(&self, block_id: &str) -> Result<(), SchemaValidationError> {
        if self.level > 20 {
            return Err(SchemaValidationError::InvalidValue(format!(
                "block {block_id} 的 list.level 必须是 0 到 20 的整数"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ListKind {
    Bullet,
    Ordered,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ParagraphStyleRef {
    pub name: String,
}

impl ParagraphStyleRef {
    fn validate(&self, block_id: &str) -> Result<(), SchemaValidationError> {
        if self.name.trim().is_empty() || self.name.chars().count() > 256 {
            return Err(SchemaValidationError::InvalidValue(format!(
                "block {block_id} 的 namedStyle.name 必须是 1 到 256 个字符"
            )));
        }
        Ok(())
    }
}

/// Todo 的完成状态是文档领域数据，不属于 renderer 或自由 attrs。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoBlock {
    #[serde(default)]
    pub checked: bool,
}

/// Link 的目标地址是 block 领域数据；内容仍由 RichText 承载，避免 URL 混入自由 attrs。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkBlock {
    pub url: String,
}

impl LinkBlock {
    fn validate(&self, block_id: &str) -> Result<(), SchemaValidationError> {
        if self.url.trim().is_empty() {
            return Err(SchemaValidationError::InvalidValue(format!(
                "link block {block_id} 必须包含非空 url"
            )));
        }
        if self.url.chars().count() > 8_192 {
            return Err(SchemaValidationError::InvalidValue(format!(
                "link block {block_id} 的 url 不能超过 8192 个字符"
            )));
        }
        Ok(())
    }
}

/// Code block 的可持久化配置。
///
/// 源码仍然保存在 `DocumentBlock.content` 中，配置只描述代码块的展示和编辑偏好。
/// 这样高亮 token、滚动位置等渲染态不会污染 Artifact snapshot，也不会形成第二份
/// 源码真相。height 只保存用户调整后的编辑 viewport，高度以外的代码通过 renderer 滚动。
/// 每个 code block 都必须显式携带此配置，默认值由创建命令写入模型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeBlockConfig {
    #[serde(default)]
    pub title: String,
    #[serde(default = "default_code_language")]
    pub language: String,
    #[serde(default = "default_code_theme")]
    pub theme: String,
    #[serde(default = "default_code_height")]
    pub height: u16,
    #[serde(default = "default_code_show_line_numbers")]
    pub show_line_numbers: bool,
    #[serde(default)]
    pub wrap: bool,
    #[serde(default)]
    pub indent_mode: CodeIndentMode,
    #[serde(default = "default_code_indent_width")]
    pub indent_width: u8,
    #[serde(default = "default_code_font_size")]
    pub font_size: u8,
}

impl Default for CodeBlockConfig {
    fn default() -> Self {
        Self {
            title: String::new(),
            language: default_code_language(),
            theme: default_code_theme(),
            height: default_code_height(),
            show_line_numbers: default_code_show_line_numbers(),
            wrap: false,
            indent_mode: CodeIndentMode::default(),
            indent_width: default_code_indent_width(),
            font_size: default_code_font_size(),
        }
    }
}

impl CodeBlockConfig {
    fn validate(&self, block_id: &str) -> Result<(), SchemaValidationError> {
        validate_code_identifier(&self.language, "language", block_id)?;
        validate_code_identifier(&self.theme, "theme", block_id)?;
        if !(160..=640).contains(&self.height) {
            return Err(SchemaValidationError::InvalidValue(format!(
                "code block {block_id} 的 height 必须在 160 到 640 之间"
            )));
        }
        if self.title.chars().count() > 256 {
            return Err(SchemaValidationError::InvalidValue(format!(
                "code block {block_id} 的 title 不能超过 256 个字符"
            )));
        }
        if !matches!(self.indent_width, 2 | 4 | 8) {
            return Err(SchemaValidationError::InvalidValue(format!(
                "code block {block_id} 的 indentWidth 必须是 2、4 或 8"
            )));
        }
        if !(8..=32).contains(&self.font_size) {
            return Err(SchemaValidationError::InvalidValue(format!(
                "code block {block_id} 的 fontSize 必须在 8 到 32 之间"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CodeIndentMode {
    #[default]
    Spaces,
    Tabs,
}

fn default_code_language() -> String {
    "plainText".into()
}

fn default_code_theme() -> String {
    "light".into()
}

const fn default_code_show_line_numbers() -> bool {
    true
}

const fn default_code_height() -> u16 {
    200
}

const fn default_code_indent_width() -> u8 {
    2
}

const fn default_code_font_size() -> u8 {
    14
}

fn validate_code_identifier(
    value: &str,
    field: &str,
    block_id: &str,
) -> Result<(), SchemaValidationError> {
    if value.trim().is_empty()
        || value.chars().count() > 64
        || value.chars().any(char::is_whitespace)
    {
        return Err(SchemaValidationError::InvalidValue(format!(
            "code block {block_id} 的 {field} 必须是 1 到 64 个不含空白的字符"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageBlock {
    pub asset_id: AssetId,
    #[serde(default)]
    pub alt: String,
    /// The original uploaded asset is retained when a browser-side compression
    /// creates a derivative. This makes "restore original" deterministic and
    /// avoids keeping renderer-only history state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_asset_id: Option<AssetId>,
    #[serde(default)]
    pub transform: ImageTransform,
    #[serde(default)]
    pub caption: String,
}

/// Persisted, non-destructive image display state. Percentages use the
/// original bitmap's coordinate space, so every renderer can reproduce the
/// same crop without owning a second image model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageTransform {
    #[serde(default)]
    pub crop: ImageCrop,
    #[serde(default)]
    pub flip_horizontal: bool,
    #[serde(default)]
    pub flip_vertical: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageCrop {
    #[serde(default)]
    pub top: f32,
    #[serde(default)]
    pub right: f32,
    #[serde(default)]
    pub bottom: f32,
    #[serde(default)]
    pub left: f32,
}

impl ImageCrop {
    pub fn validate(&self, block_id: &str) -> Result<(), SchemaValidationError> {
        for (edge, value) in [
            ("top", self.top),
            ("right", self.right),
            ("bottom", self.bottom),
            ("left", self.left),
        ] {
            if !value.is_finite() || !(0.0..1.0).contains(&value) {
                return Err(SchemaValidationError::InvalidValue(format!(
                    "image block {block_id} 的 crop.{edge} 必须在 0 到 1 之间"
                )));
            }
        }
        if self.left + self.right >= 0.95 || self.top + self.bottom >= 0.95 {
            return Err(SchemaValidationError::InvalidValue(format!(
                "image block {block_id} 的裁剪区域不能为空"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableBlock {
    pub columns: Vec<TableColumn>,
    pub rows: Vec<TableRow>,
    pub merged_ranges: Vec<TableRange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableRange {
    pub start_row_id: String,
    pub end_row_id: String,
    pub start_column_id: String,
    pub end_column_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableColumn {
    pub id: String,
    #[serde(default)]
    pub width: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableRow {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    pub cells: Vec<TableCell>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableCell {
    pub id: String,
    pub content: RichText,
    #[serde(default)]
    pub style: TableCellStyle,
}

/// Persisted cell-level presentation. Inline text formatting remains in
/// `RichText.runs`; this object owns only the cell surface and alignment.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableCellStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horizontal_align: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical_align: Option<String>,
    #[serde(default, skip_serializing_if = "TableBorderEdges::is_empty")]
    pub borders: TableBorderEdges,
}

/// Per-cell border edges. Keeping edges on the cell style makes the grid
/// renderer deterministic while a range command can still update many cells
/// atomically. `None` means the edge is not painted.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableBorderEdges {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<TableBorder>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<TableBorder>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<TableBorder>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<TableBorder>,
    /// Cell-local diagonal from top-left to bottom-right. It is intentionally
    /// distinct from grid edges because it never crosses into a neighbour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagonal_down: Option<TableBorder>,
    /// Cell-local diagonal from bottom-left to top-right.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagonal_up: Option<TableBorder>,
}

impl TableBorderEdges {
    pub fn is_empty(&self) -> bool {
        self.top.is_none()
            && self.right.is_none()
            && self.bottom.is_none()
            && self.left.is_none()
            && self.diagonal_down.is_none()
            && self.diagonal_up.is_none()
    }

    fn validate(&self, block_id: &str, cell_id: &str) -> Result<(), SchemaValidationError> {
        for (edge, border) in [
            ("top", &self.top),
            ("right", &self.right),
            ("bottom", &self.bottom),
            ("left", &self.left),
            ("diagonalDown", &self.diagonal_down),
            ("diagonalUp", &self.diagonal_up),
        ] {
            if let Some(border) = border {
                border.validate(block_id, cell_id, edge)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TableBorderStyle {
    Solid,
    Dashed,
    Dotted,
    Double,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableBorder {
    pub style: TableBorderStyle,
    pub color: String,
    pub width: f32,
}

impl TableBorder {
    fn validate(
        &self,
        block_id: &str,
        cell_id: &str,
        edge: &str,
    ) -> Result<(), SchemaValidationError> {
        if !is_hex_color(&self.color) {
            return Err(SchemaValidationError::InvalidValue(format!(
                "table block {block_id} 单元格 {cell_id} 的 {edge} 边框颜色必须是 #RRGGBB 或 #RRGGBBAA"
            )));
        }
        if !self.width.is_finite() || self.width <= 0.0 || self.width > 32.0 {
            return Err(SchemaValidationError::InvalidValue(format!(
                "table block {block_id} 单元格 {cell_id} 的 {edge} 边框宽度必须大于 0 且不超过 32"
            )));
        }
        Ok(())
    }
}

fn is_hex_color(value: &str) -> bool {
    let bytes = value.as_bytes();
    (bytes.len() == 7 || bytes.len() == 9)
        && bytes[0] == b'#'
        && bytes[1..].iter().all(u8::is_ascii_hexdigit)
}

#[derive(Debug, Clone, PartialEq)]
pub enum DocumentBlockKind {
    Paragraph,
    Heading { level: u8 },
    Quote,
    Code,
    Image,
    Table,
    Callout,
    Todo,
    Divider,
    Page,
    Columns,
    Column,
    Link,
    Extension { type_id: String },
    Unknown { type_id: String, raw: Value },
}

/// Known block kinds use the compact tagged representation. Unknown kinds retain the complete
/// original object so a client that does not understand a future block can still round-trip it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum KnownDocumentBlockKind {
    Paragraph,
    Heading { level: u8 },
    Quote,
    Code,
    Image,
    Table,
    Callout,
    Todo,
    Divider,
    Page,
    Columns,
    Column,
    Link,
    Extension { type_id: String },
}

impl Serialize for DocumentBlockKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Unknown { type_id, raw } => {
                let mut object = match raw {
                    Value::Object(object) => object.clone(),
                    _ => Map::new(),
                };
                object.insert("type".into(), Value::String(type_id.clone()));
                if !matches!(raw, Value::Object(_)) {
                    object.insert("raw".into(), raw.clone());
                }
                Value::Object(object).serialize(serializer)
            }
            known => {
                let known =
                    KnownDocumentBlockKind::try_from(known).map_err(serde::ser::Error::custom)?;
                known.serialize(serializer)
            }
        }
    }
}

impl<'de> Deserialize<'de> for DocumentBlockKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let object = value
            .as_object()
            .ok_or_else(|| de::Error::custom("block kind 必须是对象"))?;
        let type_id = object
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| de::Error::custom("block kind.type 必须是字符串"))?;
        if type_id == "unknown" {
            let original_type = object
                .get("typeId")
                .and_then(Value::as_str)
                .ok_or_else(|| de::Error::custom("unknown block 缺少 typeId"))?;
            let raw = object.get("raw").cloned().unwrap_or_else(|| value.clone());
            return Ok(Self::Unknown {
                type_id: original_type.to_string(),
                raw,
            });
        }
        match type_id {
            "paragraph" | "heading" | "quote" | "code" | "image" | "table" | "callout" | "todo"
            | "divider" | "page" | "columns" | "column" | "link" | "extension" => {
                let known: KnownDocumentBlockKind =
                    serde_json::from_value(value).map_err(de::Error::custom)?;
                Ok(known.into())
            }
            _ => Ok(Self::Unknown {
                type_id: type_id.to_string(),
                raw: value,
            }),
        }
    }
}

impl TryFrom<&DocumentBlockKind> for KnownDocumentBlockKind {
    type Error = &'static str;

    fn try_from(value: &DocumentBlockKind) -> Result<Self, Self::Error> {
        Ok(match value {
            DocumentBlockKind::Paragraph => Self::Paragraph,
            DocumentBlockKind::Heading { level } => Self::Heading { level: *level },
            DocumentBlockKind::Quote => Self::Quote,
            DocumentBlockKind::Code => Self::Code,
            DocumentBlockKind::Image => Self::Image,
            DocumentBlockKind::Table => Self::Table,
            DocumentBlockKind::Callout => Self::Callout,
            DocumentBlockKind::Todo => Self::Todo,
            DocumentBlockKind::Divider => Self::Divider,
            DocumentBlockKind::Page => Self::Page,
            DocumentBlockKind::Columns => Self::Columns,
            DocumentBlockKind::Column => Self::Column,
            DocumentBlockKind::Link => Self::Link,
            DocumentBlockKind::Extension { type_id } => Self::Extension {
                type_id: type_id.clone(),
            },
            DocumentBlockKind::Unknown { .. } => return Err("unknown block kind"),
        })
    }
}

impl From<KnownDocumentBlockKind> for DocumentBlockKind {
    fn from(value: KnownDocumentBlockKind) -> Self {
        match value {
            KnownDocumentBlockKind::Paragraph => Self::Paragraph,
            KnownDocumentBlockKind::Heading { level } => Self::Heading { level },
            KnownDocumentBlockKind::Quote => Self::Quote,
            KnownDocumentBlockKind::Code => Self::Code,
            KnownDocumentBlockKind::Image => Self::Image,
            KnownDocumentBlockKind::Table => Self::Table,
            KnownDocumentBlockKind::Callout => Self::Callout,
            KnownDocumentBlockKind::Todo => Self::Todo,
            KnownDocumentBlockKind::Divider => Self::Divider,
            KnownDocumentBlockKind::Page => Self::Page,
            KnownDocumentBlockKind::Columns => Self::Columns,
            KnownDocumentBlockKind::Column => Self::Column,
            KnownDocumentBlockKind::Link => Self::Link,
            KnownDocumentBlockKind::Extension { type_id } => Self::Extension { type_id },
        }
    }
}

impl DocumentBlockKind {
    fn validate(&self, block_id: &str) -> Result<(), SchemaValidationError> {
        match self {
            Self::Heading { level } if !(1..=6).contains(level) => {
                Err(SchemaValidationError::InvalidValue(format!(
                    "block {block_id} 的 heading level 必须在 1 到 6 之间"
                )))
            }
            Self::Extension { type_id } | Self::Unknown { type_id, .. }
                if type_id.trim().is_empty() =>
            {
                Err(SchemaValidationError::EmptyId("block type"))
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RichText {
    pub text: String,
    #[serde(default)]
    pub runs: Vec<InlineRun>,
}

impl RichText {
    fn validate(&self, block_id: &str) -> Result<(), SchemaValidationError> {
        let text_len = self.text.chars().count();
        // runs 为空表示整段使用默认样式，合法且是最小模型；一旦存在 runs，则必须
        // 连续覆盖整个文本，避免出现不可解释的区间空洞。
        if self.runs.is_empty() {
            return Ok(());
        }

        let mut expected_start = 0;
        for (index, run) in self.runs.iter().enumerate() {
            run.style.validate(block_id)?;
            if run.start != expected_start || run.start >= run.end || run.end > text_len {
                return Err(SchemaValidationError::InvalidValue(format!(
                    "block {block_id} 的 RichText run[{index}] 区间无效：{}..{}，文本长度 {text_len}",
                    run.start, run.end
                )));
            }
            expected_start = run.end;
        }
        if expected_start != text_len {
            return Err(SchemaValidationError::InvalidValue(format!(
                "block {block_id} 的 RichText runs 未覆盖全文"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InlineRun {
    /// 下标是 Unicode scalar value 计数；浏览器输入层负责与 UTF-16 selection 做转换。
    pub start: usize,
    pub end: usize,
    pub style: InlineStyle,
}

/// A strict, renderer-independent inline presentation value.
///
/// The wire format deliberately has no free-form `attrs` map.  Future values must use an
/// explicitly namespaced extension envelope rather than silently becoming part of the core
/// document style contract.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InlineStyle {
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
    #[serde(default)]
    pub strikethrough: bool,
    #[serde(default)]
    pub font_family: Option<String>,
    #[serde(default)]
    pub font_size: Option<f32>,
    #[serde(default)]
    pub color: Option<Color>,
    #[serde(default)]
    pub highlight: Option<Color>,
    #[serde(default)]
    pub vertical_align: Option<VerticalAlign>,
}

impl InlineStyle {
    fn validate(&self, block_id: &str) -> Result<(), SchemaValidationError> {
        if self
            .font_family
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(SchemaValidationError::InvalidValue(format!(
                "block {block_id} 的 fontFamily 必须是非空字符串"
            )));
        }
        if self
            .font_size
            .is_some_and(|value| !value.is_finite() || value <= 0.0 || value > 512.0)
        {
            return Err(SchemaValidationError::InvalidValue(format!(
                "block {block_id} 的 fontSize 必须在 0 到 512 之间"
            )));
        }
        if self
            .color
            .as_ref()
            .is_some_and(|value| !is_valid_color(value.as_str()))
        {
            return Err(SchemaValidationError::InvalidValue(format!(
                "block {block_id} 的 color 必须是合法的 hex/rgb 颜色"
            )));
        }
        if self
            .highlight
            .as_ref()
            .is_some_and(|value| !is_valid_color(value.as_str()))
        {
            return Err(SchemaValidationError::InvalidValue(format!(
                "block {block_id} 的 highlight 必须是合法的 hex/rgb 颜色"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VerticalAlign {
    Baseline,
    Superscript,
    Subscript,
}

/// CSS hex/rgb token carried by the persisted schema. Validation happens at the schema boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Color(String);

impl Color {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if is_valid_color(&value) {
            Ok(Self(value))
        } else {
            Err(format!("非法颜色 token: {value}"))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for Color {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

pub(crate) fn is_valid_color(value: &str) -> bool {
    let value = value.trim();
    let hex = value.strip_prefix('#').unwrap_or("");
    if matches!(hex.len(), 3 | 4 | 6 | 8)
        && hex.chars().all(|character| character.is_ascii_hexdigit())
    {
        return true;
    }
    let lower = value.to_ascii_lowercase();
    let Some((name, body)) = lower.split_once('(') else {
        return false;
    };
    if !matches!(name, "rgb" | "rgba") || !body.ends_with(')') {
        return false;
    }
    let parts: Vec<_> = body[..body.len() - 1].split(',').map(str::trim).collect();
    if (name == "rgb" && parts.len() != 3) || (name == "rgba" && parts.len() != 4) {
        return false;
    }
    if parts[..3].iter().any(|part| part.parse::<u8>().is_err()) {
        return false;
    }
    name == "rgb"
        || parts[3]
            .parse::<f32>()
            .is_ok_and(|alpha| alpha.is_finite() && (0.0..=1.0).contains(&alpha))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageSetup {
    pub width: f32,
    pub height: f32,
    pub margin_top: f32,
    pub margin_right: f32,
    pub margin_bottom: f32,
    pub margin_left: f32,
}

impl PageSetup {
    fn validate(&self) -> Result<(), SchemaValidationError> {
        let values = [
            ("width", self.width),
            ("height", self.height),
            ("marginTop", self.margin_top),
            ("marginRight", self.margin_right),
            ("marginBottom", self.margin_bottom),
            ("marginLeft", self.margin_left),
        ];
        if values.iter().any(|(_, value)| !value.is_finite()) {
            return Err(SchemaValidationError::InvalidValue(
                "pageSetup 只能包含有限数字".into(),
            ));
        }
        if self.width <= 0.0 || self.height <= 0.0 {
            return Err(SchemaValidationError::InvalidValue(
                "pageSetup 宽高必须大于 0".into(),
            ));
        }
        if [
            self.margin_top,
            self.margin_right,
            self.margin_bottom,
            self.margin_left,
        ]
        .iter()
        .any(|margin| *margin < 0.0)
        {
            return Err(SchemaValidationError::InvalidValue(
                "pageSetup 页边距不能为负数".into(),
            ));
        }
        if self.margin_left + self.margin_right >= self.width
            || self.margin_top + self.margin_bottom >= self.height
        {
            return Err(SchemaValidationError::InvalidValue(
                "pageSetup 页边距必须小于页面尺寸".into(),
            ));
        }
        Ok(())
    }
}

/// 表格使用稀疏 cell 模型，不复用文档 Block。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpreadsheetModel {
    /// Workbook-level metadata that is part of the Spreadsheet artifact, never renderer state.
    #[serde(default)]
    pub metadata: SpreadsheetMetadata,
    #[serde(default)]
    pub sheets: Vec<SheetModel>,
}

impl SpreadsheetModel {
    fn validate(&self) -> Result<(), SchemaValidationError> {
        unique_ids(self.sheets.iter().map(|sheet| sheet.id.as_str()), "sheet")?;
        if let Some(active_sheet_id) = &self.metadata.active_sheet_id {
            if !self.sheets.iter().any(|sheet| &sheet.id == active_sheet_id) {
                return Err(SchemaValidationError::InvalidValue(format!(
                    "activeSheetId {} 不存在",
                    active_sheet_id
                )));
            }
        }
        for sheet in &self.sheets {
            if sheet.name.trim().is_empty() {
                return Err(SchemaValidationError::InvalidValue(format!(
                    "sheet {} 名称不能为空",
                    sheet.id
                )));
            }
            let mut cells = HashSet::new();
            for cell in &sheet.cells {
                if !cells.insert((cell.row, cell.column)) {
                    return Err(SchemaValidationError::InvalidValue(format!(
                        "sheet {} 的 cell ({}, {}) 重复",
                        sheet.id, cell.row, cell.column
                    )));
                }
            }
            sheet.metadata.validate(&sheet.id)?;
        }
        Ok(())
    }
}

/// Spreadsheet 的 workbook 级设置。它们是持久化语义，不包含 viewport 的像素位置。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpreadsheetMetadata {
    #[serde(default)]
    pub active_sheet_id: Option<String>,
    #[serde(default)]
    pub calculation_mode: CalculationMode,
    #[serde(default)]
    pub date_system: DateSystem,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CalculationMode {
    #[default]
    Automatic,
    Manual,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DateSystem {
    #[default]
    Excel1900,
    Excel1904,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetModel {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub cells: Vec<CellModel>,
    #[serde(default)]
    pub metadata: SheetMetadata,
}

/// Worksheet metadata and data-management rules. Rules are typed so adapters can report loss
/// explicitly instead of putting opaque renderer flags into `CellModel.attrs`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetMetadata {
    #[serde(default)]
    pub visibility: SheetVisibility,
    #[serde(default)]
    pub row_count: Option<u32>,
    #[serde(default)]
    pub column_count: Option<u32>,
    #[serde(default)]
    pub freeze: FreezePane,
    #[serde(default)]
    pub auto_filter: Option<FilterSpec>,
    #[serde(default)]
    pub sort: Option<SortSpec>,
    #[serde(default)]
    pub conditional_formats: Vec<ConditionalFormatRule>,
    #[serde(default)]
    pub data_validations: Vec<DataValidationRule>,
    #[serde(default)]
    pub merged_ranges: Vec<GridRange>,
    #[serde(default)]
    pub media: Vec<SheetMedia>,
}

impl SheetMetadata {
    fn validate(&self, sheet_id: &str) -> Result<(), SchemaValidationError> {
        if self.freeze.rows > self.row_count.unwrap_or(u32::MAX)
            || self.freeze.columns > self.column_count.unwrap_or(u32::MAX)
        {
            return Err(SchemaValidationError::InvalidValue(format!(
                "sheet {} freeze pane 超出 worksheet 边界",
                sheet_id
            )));
        }
        for range in &self.merged_ranges {
            range.validate(sheet_id)?;
        }
        for rule in &self.conditional_formats {
            rule.range.validate(sheet_id)?;
        }
        for rule in &self.data_validations {
            rule.range.validate(sheet_id)?;
        }
        if let Some(filter) = &self.auto_filter {
            filter.range.validate(sheet_id)?;
            for column in &filter.columns {
                if !filter.range.contains_column(column.column) {
                    return Err(SchemaValidationError::InvalidValue(format!(
                        "sheet {} filter column {} 超出范围",
                        sheet_id, column.column
                    )));
                }
            }
        }
        if let Some(sort) = &self.sort {
            sort.range.validate(sheet_id)?;
            for key in &sort.keys {
                if !sort.range.contains_column(key.column) {
                    return Err(SchemaValidationError::InvalidValue(format!(
                        "sheet {} sort column {} 超出范围",
                        sheet_id, key.column
                    )));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SheetVisibility {
    #[default]
    Visible,
    Hidden,
    VeryHidden,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FreezePane {
    pub rows: u32,
    pub columns: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GridRange {
    pub start_row: u32,
    pub start_column: u32,
    pub end_row: u32,
    pub end_column: u32,
}

impl GridRange {
    fn validate(&self, sheet_id: &str) -> Result<(), SchemaValidationError> {
        if self.start_row > self.end_row || self.start_column > self.end_column {
            return Err(SchemaValidationError::InvalidValue(format!(
                "sheet {} GridRange 起点不得超过终点",
                sheet_id
            )));
        }
        Ok(())
    }

    fn contains_column(&self, column: u32) -> bool {
        self.start_column <= column && column <= self.end_column
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterSpec {
    pub range: GridRange,
    #[serde(default)]
    pub columns: Vec<FilterColumn>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterColumn {
    pub column: u32,
    pub predicate: FilterPredicate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum FilterPredicate {
    Values(Vec<Value>),
    Contains(String),
    Equals(Value),
    GreaterThan(f64),
    LessThan(f64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SortSpec {
    pub range: GridRange,
    #[serde(default)]
    pub keys: Vec<SortKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SortKey {
    pub column: u32,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConditionalFormatRule {
    pub id: String,
    pub range: GridRange,
    pub predicate: ConditionalPredicate,
    #[serde(default)]
    pub style: CellStyle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum ConditionalPredicate {
    CellIs {
        operator: ComparisonOperator,
        value: Value,
    },
    Formula(String),
    ColorScale {
        min: String,
        max: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComparisonOperator {
    Equal,
    NotEqual,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataValidationRule {
    pub id: String,
    pub range: GridRange,
    pub kind: DataValidationKind,
    #[serde(default)]
    pub allow_blank: bool,
    #[serde(default)]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum DataValidationKind {
    List(Vec<String>),
    WholeNumber { min: i64, max: i64 },
    Decimal { min: f64, max: f64 },
    Date { min_serial: f64, max_serial: f64 },
    CustomFormula(String),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellStyle {
    #[serde(default)]
    pub number_format: Option<String>,
    #[serde(default)]
    pub font: Option<FontStyle>,
    #[serde(default)]
    pub fill: Option<FillStyle>,
    #[serde(default)]
    pub alignment: Option<AlignmentStyle>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontStyle {
    #[serde(default)]
    pub family: Option<String>,
    #[serde(default)]
    pub size: Option<f32>,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillStyle {
    #[serde(default)]
    pub foreground: Option<String>,
    #[serde(default)]
    pub background: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlignmentStyle {
    #[serde(default)]
    pub horizontal: Option<String>,
    #[serde(default)]
    pub vertical: Option<String>,
    #[serde(default)]
    pub wrap: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetMedia {
    pub id: String,
    pub relationship: String,
    pub content_type: String,
    pub target: String,
    pub anchor: GridRange,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellModel {
    pub row: u32,
    pub column: u32,
    #[serde(default)]
    pub value: Option<Value>,
    #[serde(default)]
    pub formula: Option<String>,
    #[serde(default)]
    pub attrs: Map<String, Value>,
    /// Typed cell style. `attrs` remains only for adapter-owned extensions and is never used for
    /// core formatting, so spreadsheet consumers can validate styles without guessing keys.
    #[serde(default)]
    pub style: Option<CellStyle>,
}

/// 幻灯片使用 scene graph；白板复用 scene element，但拥有自己的 camera。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyPresentationModel {
    #[serde(default)]
    pub slides: Vec<LegacySlideModel>,
    /// Theme tokens are presentation-scoped rather than renderer state.  Keeping them on the
    /// artifact lets PPTX import/export preserve a stable theme identity without leaking theme
    /// details into Document or the canvas renderer.
    #[serde(default)]
    pub theme: Option<LegacyPresentationTheme>,
}

impl LegacyPresentationModel {
    /// Validates the retired v4 Presentation scene graph for the offline v4 → v5 compiler.
    ///
    /// This remains crate-private: no HTTP, WASM or browser path can call it to accept a legacy
    /// payload after the destructive v5 cutover.
    pub(crate) fn validate(&self) -> Result<(), SchemaValidationError> {
        unique_ids(self.slides.iter().map(|slide| slide.id.as_str()), "slide")?;
        for slide in &self.slides {
            validate_scene_elements(&slide.elements, &format!("slide {}", slide.id))?;
            validate_slide_metadata(slide, &self.theme)?;
        }
        if let Some(theme) = &self.theme {
            theme.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacySlideModel {
    pub id: String,
    /// Human-readable slide title from the source package. It is metadata, not renderer state.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub elements: Vec<SceneElement>,
    /// Speaker notes remain part of the slide artifact and are not rendered as scene elements.
    #[serde(default)]
    pub notes: Option<String>,
    /// Playback order is explicit and stable; renderers may ignore it when in edit mode.
    #[serde(default)]
    pub animations: Vec<LegacyPresentationAnimation>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyPresentationTheme {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub colors: BTreeMap<String, String>,
    #[serde(default)]
    pub fonts: BTreeMap<String, String>,
}

impl LegacyPresentationTheme {
    fn validate(&self) -> Result<(), SchemaValidationError> {
        if self.id.trim().is_empty() {
            return Err(SchemaValidationError::EmptyId("presentation theme"));
        }
        for (name, color) in &self.colors {
            if name.trim().is_empty() || !is_css_color(color) {
                return Err(SchemaValidationError::InvalidValue(format!(
                    "presentation theme color {name} 无效"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyPresentationAnimation {
    pub id: String,
    pub target_element_id: String,
    pub effect: String,
    pub order: u32,
    #[serde(default)]
    pub duration_ms: u32,
}

impl Default for LegacyPresentationAnimation {
    fn default() -> Self {
        Self {
            id: String::new(),
            target_element_id: String::new(),
            effect: "appear".into(),
            order: 0,
            duration_ms: 0,
        }
    }
}

fn validate_slide_metadata(
    slide: &LegacySlideModel,
    theme: &Option<LegacyPresentationTheme>,
) -> Result<(), SchemaValidationError> {
    let mut animation_ids = HashSet::new();
    let element_ids: HashSet<&str> = slide
        .elements
        .iter()
        .map(|element| element.id.as_str())
        .collect();
    for animation in &slide.animations {
        if animation.id.trim().is_empty() {
            return Err(SchemaValidationError::EmptyId("presentation animation"));
        }
        if !animation_ids.insert(animation.id.as_str()) {
            return Err(SchemaValidationError::DuplicateId("presentation animation"));
        }
        if !element_ids.contains(animation.target_element_id.as_str()) {
            return Err(SchemaValidationError::MissingReference {
                owner: animation.id.clone(),
                target: animation.target_element_id.clone(),
            });
        }
        if animation.effect.trim().is_empty() {
            return Err(SchemaValidationError::InvalidValue(format!(
                "animation {} effect 不能为空",
                animation.id
            )));
        }
    }
    if theme.is_some() && slide.name.trim().is_empty() {
        // Empty names are valid for legacy slides; this branch intentionally does not reject them.
    }
    Ok(())
}

fn is_css_color(value: &str) -> bool {
    let value = value.trim();
    value.starts_with('#') && (value.len() == 4 || value.len() == 7 || value.len() == 9)
        || value.starts_with("rgb(")
        || value.starts_with("rgba(")
        || value.starts_with("var(")
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MindmapModel {
    pub root: Option<String>,
    #[serde(default)]
    pub nodes: Vec<MindmapNode>,
    /// Optional cross-links between nodes. Parent/child hierarchy remains the
    /// canonical tree; edges are explicit graph semantics and are validated as
    /// references, never inferred by a renderer.
    #[serde(default)]
    pub edges: Vec<MindmapEdge>,
}

impl MindmapModel {
    fn validate(&self) -> Result<(), SchemaValidationError> {
        unique_ids(
            self.nodes.iter().map(|node| node.id.as_str()),
            "mindmap node",
        )?;
        unique_ids(
            self.edges.iter().map(|edge| edge.id.as_str()),
            "mindmap edge",
        )?;
        let by_id: HashMap<&str, &MindmapNode> = self
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        if self.nodes.is_empty() {
            if self.root.is_some() {
                return Err(SchemaValidationError::InvalidValue(
                    "空 mindmap 不能设置 root".into(),
                ));
            }
            if !self.edges.is_empty() {
                return Err(SchemaValidationError::InvalidValue(
                    "空 mindmap 不能设置 edge".into(),
                ));
            }
            return Ok(());
        }
        let root = self.root.as_deref().ok_or_else(|| {
            SchemaValidationError::InvalidValue("非空 mindmap 必须设置 root".into())
        })?;
        let root_node = by_id
            .get(root)
            .ok_or_else(|| SchemaValidationError::MissingReference {
                owner: "mindmap root".into(),
                target: root.to_string(),
            })?;
        if root_node.parent_id.is_some() {
            return Err(SchemaValidationError::InvalidValue(
                "mindmap root 不能有 parentId".into(),
            ));
        }
        for node in &self.nodes {
            if let Some(parent_id) = &node.parent_id {
                if parent_id == &node.id {
                    return Err(SchemaValidationError::BlockCycle(node.id.clone()));
                }
                if !by_id.contains_key(parent_id.as_str()) {
                    return Err(SchemaValidationError::MissingReference {
                        owner: node.id.clone(),
                        target: parent_id.clone(),
                    });
                }
            } else if node.id != root {
                return Err(SchemaValidationError::InvalidValue(format!(
                    "mindmap 节点 {} 缺少 parentId",
                    node.id
                )));
            }
            if let Some(content) = &node.content {
                content.validate(&node.id)?;
            }
        }
        let node_ids: HashSet<&str> = by_id.keys().copied().collect();
        for edge in &self.edges {
            if edge.source_id == edge.target_id {
                return Err(SchemaValidationError::InvalidValue(format!(
                    "mindmap edge {} 不能连接自身",
                    edge.id
                )));
            }
            if !node_ids.contains(edge.source_id.as_str()) {
                return Err(SchemaValidationError::MissingReference {
                    owner: edge.id.clone(),
                    target: edge.source_id.clone(),
                });
            }
            if !node_ids.contains(edge.target_id.as_str()) {
                return Err(SchemaValidationError::MissingReference {
                    owner: edge.id.clone(),
                    target: edge.target_id.clone(),
                });
            }
        }
        let mut visiting = HashSet::new();
        let mut visited = HashSet::new();
        for node in &self.nodes {
            visit_mindmap(node.id.as_str(), &by_id, &mut visiting, &mut visited)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MindmapNode {
    pub id: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub content: Option<RichText>,
    #[serde(default)]
    pub attrs: Map<String, Value>,
    /// Whether descendants are hidden in the layout projection. This is graph
    /// presentation state, but persisted with the node so collaborators see the
    /// same collapsed tree deterministically.
    #[serde(default)]
    pub collapsed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MindmapEdge {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    #[serde(default)]
    pub attrs: Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhiteboardModel {
    #[serde(default)]
    pub elements: Vec<SceneElement>,
    #[serde(default)]
    pub camera: Camera,
}

impl WhiteboardModel {
    /// Validate a whiteboard snapshot without taking ownership or cloning scene elements. The
    /// runtime engines use this borrowed boundary after live mutations and before committing a
    /// revision.
    pub fn validate(&self) -> Result<(), SchemaValidationError> {
        validate_scene_elements(&self.elements, "whiteboard")?;
        if !self.camera.x.is_finite()
            || !self.camera.y.is_finite()
            || !self.camera.scale.is_finite()
            || self.camera.scale <= 0.0
        {
            return Err(SchemaValidationError::InvalidValue(
                "whiteboard camera 必须是有限数字且 scale 大于 0".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneElement {
    pub id: ElementId,
    pub type_id: String,
    #[serde(default)]
    pub transform: Transform,
    #[serde(default)]
    pub attrs: Map<String, Value>,
    #[serde(default)]
    pub children: Vec<ElementId>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transform {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub rotation: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Camera {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            scale: 1.0,
        }
    }
}

fn unique_ids<'a>(
    ids: impl Iterator<Item = &'a str>,
    kind: &'static str,
) -> Result<(), SchemaValidationError> {
    let mut seen = HashSet::new();
    for id in ids {
        if id.trim().is_empty() {
            return Err(SchemaValidationError::EmptyId(kind));
        }
        if !seen.insert(id) {
            return Err(SchemaValidationError::DuplicateId(kind));
        }
    }
    Ok(())
}

fn validate_scene_elements(
    elements: &[SceneElement],
    owner: &str,
) -> Result<(), SchemaValidationError> {
    unique_ids(
        elements.iter().map(|element| element.id.as_str()),
        "scene element",
    )?;
    let by_id: HashMap<&str, &SceneElement> = elements
        .iter()
        .map(|element| (element.id.as_str(), element))
        .collect();
    for element in elements {
        if element.type_id.trim().is_empty() {
            return Err(SchemaValidationError::EmptyId("scene element type"));
        }
        let transform = &element.transform;
        if [
            transform.x,
            transform.y,
            transform.width,
            transform.height,
            transform.rotation,
        ]
        .iter()
        .any(|value| !value.is_finite())
            || transform.width < 0.0
            || transform.height < 0.0
        {
            return Err(SchemaValidationError::InvalidValue(format!(
                "{owner} element {} 的 transform 无效",
                element.id
            )));
        }
        let mut children = HashSet::new();
        for child in &element.children {
            if !children.insert(child) {
                return Err(SchemaValidationError::DuplicateChild(child.clone()));
            }
            if !by_id.contains_key(child.as_str()) {
                return Err(SchemaValidationError::MissingReference {
                    owner: element.id.clone(),
                    target: child.clone(),
                });
            }
        }
    }
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    let mut owners = HashMap::new();
    let referenced: HashSet<&str> = elements
        .iter()
        .flat_map(|element| element.children.iter().map(String::as_str))
        .collect();
    for element in elements {
        if referenced.contains(element.id.as_str()) {
            continue;
        }
        visit_scene_element(
            element.id.as_str(),
            "<root>",
            &by_id,
            &mut visiting,
            &mut visited,
            &mut owners,
        )?;
    }
    if let Some(unvisited) = elements
        .iter()
        .find(|element| !visited.contains(&element.id))
    {
        visit_scene_element(
            unvisited.id.as_str(),
            "<root>",
            &by_id,
            &mut visiting,
            &mut visited,
            &mut owners,
        )?;
    }
    Ok(())
}

fn visit_scene_element(
    id: &str,
    owner: &str,
    by_id: &HashMap<&str, &SceneElement>,
    visiting: &mut HashSet<String>,
    visited: &mut HashSet<String>,
    owners: &mut HashMap<String, String>,
) -> Result<(), SchemaValidationError> {
    if visiting.contains(id) {
        return Err(SchemaValidationError::BlockCycle(id.to_string()));
    }
    if let Some(previous_owner) = owners.insert(id.to_string(), owner.to_string()) {
        if previous_owner != owner {
            return Err(SchemaValidationError::MultipleParents {
                child: id.to_string(),
                first: previous_owner,
                second: owner.to_string(),
            });
        }
        if visited.contains(id) {
            return Ok(());
        }
    }
    if visited.contains(id) {
        return Ok(());
    }
    visiting.insert(id.to_string());
    let element = by_id
        .get(id)
        .ok_or_else(|| SchemaValidationError::MissingReference {
            owner: "scene element".into(),
            target: id.to_string(),
        })?;
    for child in &element.children {
        visit_scene_element(child, id, by_id, visiting, visited, owners)?;
    }
    visiting.remove(id);
    visited.insert(id.to_string());
    Ok(())
}

fn visit_mindmap(
    id: &str,
    by_id: &HashMap<&str, &MindmapNode>,
    visiting: &mut HashSet<String>,
    visited: &mut HashSet<String>,
) -> Result<(), SchemaValidationError> {
    if visiting.contains(id) {
        return Err(SchemaValidationError::BlockCycle(id.to_string()));
    }
    if visited.contains(id) {
        return Ok(());
    }
    visiting.insert(id.to_string());
    if let Some(parent_id) = &by_id
        .get(id)
        .ok_or_else(|| SchemaValidationError::MissingReference {
            owner: "mindmap".into(),
            target: id.to_string(),
        })?
        .parent_id
    {
        visit_mindmap(parent_id, by_id, visiting, visited)?;
    }
    visiting.remove(id);
    visited.insert(id.to_string());
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SchemaValidationError {
    #[error("不支持的格式：{0}")]
    UnsupportedFormat(String),
    #[error("不支持的 schema 版本：{0}")]
    UnsupportedSchemaVersion(u16),
    #[error("{0} 不能为空")]
    EmptyId(&'static str),
    #[error("{0} id 重复")]
    DuplicateId(&'static str),
    #[error("子节点 {0} 重复")]
    DuplicateChild(String),
    #[error("节点 {child} 被多个父节点引用：{first} 和 {second}")]
    MultipleParents {
        child: String,
        first: String,
        second: String,
    },
    #[error("{owner} 引用了不存在的节点 {target}")]
    MissingReference { owner: String, target: String },
    #[error("Block Tree 存在环：{0}")]
    BlockCycle(String),
    #[error("节点 {0} 不在 root 可达树中")]
    UnreachableBlock(String),
    #[error("Artifact kind 不匹配：信封是 {envelope:?}，payload 是 {payload:?}")]
    KindMismatch {
        envelope: ArtifactKind,
        payload: ArtifactKind,
    },
    #[error("schema 值无效：{0}")]
    InvalidValue(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(id: &str, kind: DocumentBlockKind, children: Vec<&str>) -> DocumentBlock {
        DocumentBlock {
            id: id.into(),
            kind,
            presentation: BlockPresentation::default(),
            content: None,
            children: children.into_iter().map(str::to_string).collect(),
            data: BlockData::None,
        }
    }

    #[test]
    fn document_tree_round_trips_and_validates() {
        let document = DocumentModel {
            root: vec!["p-1".into()],
            blocks: vec![
                block("p-1", DocumentBlockKind::Columns, vec!["col-1", "col-2"]),
                block("col-1", DocumentBlockKind::Column, vec!["text-1"]),
                block("col-2", DocumentBlockKind::Column, vec![]),
                block("text-1", DocumentBlockKind::Paragraph, vec![]),
            ],
            page_setup: None,
        };
        document.validate().unwrap();
        let json = serde_json::to_string(&document).unwrap();
        let back: DocumentModel = serde_json::from_str(&json).unwrap();
        assert_eq!(document, back);
    }

    #[test]
    fn document_rejects_cycles() {
        let document = DocumentModel {
            root: vec!["a".into()],
            blocks: vec![
                block("a", DocumentBlockKind::Page, vec!["b"]),
                block("b", DocumentBlockKind::Page, vec!["a"]),
            ],
            page_setup: None,
        };
        assert!(matches!(
            document.validate(),
            Err(SchemaValidationError::BlockCycle(_))
        ));
    }

    #[test]
    fn document_rejects_multiple_parents_and_orphans() {
        let multiple_parent = DocumentModel {
            root: vec!["a".into(), "b".into()],
            blocks: vec![
                block("a", DocumentBlockKind::Page, vec!["child"]),
                block("b", DocumentBlockKind::Page, vec!["child"]),
                block("child", DocumentBlockKind::Paragraph, vec![]),
            ],
            page_setup: None,
        };
        assert!(matches!(
            multiple_parent.validate(),
            Err(SchemaValidationError::MultipleParents { .. })
        ));

        let orphan = DocumentModel {
            root: vec!["a".into()],
            blocks: vec![
                block("a", DocumentBlockKind::Page, vec![]),
                block("orphan", DocumentBlockKind::Paragraph, vec![]),
            ],
            page_setup: None,
        };
        assert!(matches!(
            orphan.validate(),
            Err(SchemaValidationError::UnreachableBlock(_))
        ));
    }

    #[test]
    fn envelope_separates_kind_schema_and_revision() {
        let envelope =
            ArtifactEnvelope::new("doc-1", ArtifactPayload::Document(DocumentModel::default()));
        assert_eq!(envelope.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(envelope.revision, 0);
        assert_eq!(envelope.kind, ArtifactKind::Document);
        envelope.validate().unwrap();
    }

    #[test]
    fn unknown_block_kind_round_trips_without_losing_the_original_object() {
        let raw = serde_json::json!({
            "type": "future.databaseView",
            "provider": "example",
            "config": {"columns": ["name", "status"]}
        });
        let kind: DocumentBlockKind = serde_json::from_value(raw.clone()).unwrap();
        assert!(matches!(kind, DocumentBlockKind::Unknown { .. }));
        assert_eq!(serde_json::to_value(kind).unwrap(), raw);
    }

    #[test]
    fn document_rejects_invalid_rich_text_ranges_and_page_setup() {
        let invalid_runs = DocumentModel {
            root: vec!["p".into()],
            blocks: vec![DocumentBlock {
                id: "p".into(),
                kind: DocumentBlockKind::Paragraph,
                presentation: BlockPresentation::default(),
                content: Some(RichText {
                    text: "你好".into(),
                    runs: vec![InlineRun {
                        start: 1,
                        end: 3,
                        style: InlineStyle::default(),
                    }],
                }),
                children: Vec::new(),
                data: BlockData::None,
            }],
            page_setup: None,
        };
        assert!(matches!(
            invalid_runs.validate(),
            Err(SchemaValidationError::InvalidValue(_))
        ));

        let invalid_page = DocumentModel {
            page_setup: Some(PageSetup {
                width: 100.0,
                height: 100.0,
                margin_top: 100.0,
                margin_right: 60.0,
                margin_bottom: 0.0,
                margin_left: 0.0,
            }),
            ..DocumentModel::default()
        };
        assert!(matches!(
            invalid_page.validate(),
            Err(SchemaValidationError::InvalidValue(_))
        ));
    }

    #[test]
    fn document_rejects_invalid_word_format_attributes() {
        let mut attrs = Map::new();
        attrs.insert("align".into(), Value::String("diagonal".into()));
        let invalid_block = DocumentModel {
            root: vec!["p".into()],
            blocks: vec![DocumentBlock {
                id: "p".into(),
                kind: DocumentBlockKind::Paragraph,
                presentation: BlockPresentation {
                    align: serde_json::from_value(
                        attrs
                            .remove("align")
                            .unwrap_or(Value::String("left".into())),
                    )
                    .unwrap_or_default(),
                    ..BlockPresentation::default()
                },
                content: Some(RichText {
                    text: "text".into(),
                    runs: vec![InlineRun {
                        start: 0,
                        end: 4,
                        style: InlineStyle {
                            font_size: Some(1024.0),
                            ..Default::default()
                        },
                    }],
                }),
                children: Vec::new(),
                data: BlockData::None,
            }],
            page_setup: None,
        };
        assert!(matches!(
            invalid_block.validate(),
            Err(SchemaValidationError::InvalidValue(_))
        ));

        let invalid_link = DocumentModel {
            root: vec!["link".into()],
            blocks: vec![DocumentBlock {
                id: "link".into(),
                kind: DocumentBlockKind::Link,
                presentation: BlockPresentation::default(),
                content: Some(RichText {
                    text: "Open".into(),
                    runs: Vec::new(),
                }),
                children: Vec::new(),
                data: BlockData::None,
            }],
            page_setup: None,
        };
        assert!(matches!(
            invalid_link.validate(),
            Err(SchemaValidationError::InvalidValue(_))
        ));
    }

    #[test]
    fn inline_style_is_strict_and_uses_highlight_not_background_alias() {
        let legacy = serde_json::json!({ "start": 0, "end": 1, "attrs": { "bold": true } });
        assert!(serde_json::from_value::<InlineRun>(legacy).is_err());

        let invalid_color = DocumentModel {
            root: vec!["p".into()],
            blocks: vec![DocumentBlock {
                id: "p".into(),
                kind: DocumentBlockKind::Paragraph,
                presentation: BlockPresentation::default(),
                content: Some(RichText {
                    text: "x".into(),
                    runs: vec![InlineRun {
                        start: 0,
                        end: 1,
                        style: InlineStyle {
                            highlight: Some(Color("not-a-color".into())),
                            ..Default::default()
                        },
                    }],
                }),
                children: Vec::new(),
                data: BlockData::None,
            }],
            page_setup: None,
        };
        assert!(matches!(
            invalid_color.validate(),
            Err(SchemaValidationError::InvalidValue(message)) if message.contains("highlight")
        ));
    }

    #[test]
    fn structured_table_payload_round_trips_and_validates() {
        let mut table = DocumentBlock {
            id: "table-1".into(),
            kind: DocumentBlockKind::Table,
            presentation: BlockPresentation::default(),
            content: None,
            children: Vec::new(),
            data: BlockData::Table(TableBlock {
                columns: vec![
                    TableColumn {
                        id: "c-1".into(),
                        width: None,
                    },
                    TableColumn {
                        id: "c-2".into(),
                        width: Some(120.0),
                    },
                ],
                rows: vec![TableRow {
                    id: "r-1".into(),
                    height: None,
                    cells: vec![
                        TableCell {
                            id: "cell-1".into(),
                            content: RichText {
                                text: "A".into(),
                                runs: Vec::new(),
                            },
                            style: TableCellStyle::default(),
                        },
                        TableCell {
                            id: "cell-2".into(),
                            content: RichText {
                                text: "B".into(),
                                runs: Vec::new(),
                            },
                            style: TableCellStyle::default(),
                        },
                    ],
                }],
                merged_ranges: Vec::new(),
            }),
        };
        if let BlockData::Table(table_payload) = &mut table.data {
            table_payload.rows[0].cells[0].style.borders.top = Some(TableBorder {
                style: TableBorderStyle::Solid,
                color: "#1677ff".into(),
                width: 1.0,
            });
        }
        let document = DocumentModel {
            root: vec![table.id.clone()],
            blocks: vec![table],
            page_setup: None,
        };
        document.validate().unwrap();
        assert_eq!(document.plain_text(), "A\tB");
        let json = serde_json::to_string(&document).unwrap();
        let parsed: DocumentModel = serde_json::from_str(&json).unwrap();
        assert_eq!(document, parsed);

        let mut invalid = document.clone();
        if let BlockData::Table(table_payload) = &mut invalid.blocks[0].data {
            table_payload.rows[0].cells[0]
                .style
                .borders
                .top
                .as_mut()
                .unwrap()
                .color = "red".into();
        }
        assert!(matches!(
            invalid.validate(),
            Err(SchemaValidationError::InvalidValue(_))
        ));
    }

    #[test]
    fn structured_payload_rejects_kind_mismatch_and_bad_dimensions() {
        let mut table = DocumentBlock {
            id: "table-1".into(),
            kind: DocumentBlockKind::Table,
            presentation: BlockPresentation::default(),
            content: None,
            children: Vec::new(),
            data: BlockData::Table(TableBlock {
                columns: vec![TableColumn {
                    id: "c-1".into(),
                    width: None,
                }],
                rows: vec![TableRow {
                    id: "r-1".into(),
                    height: None,
                    cells: Vec::new(),
                }],
                merged_ranges: Vec::new(),
            }),
        };
        let document = DocumentModel {
            root: vec![table.id.clone()],
            blocks: vec![table.clone()],
            page_setup: None,
        };
        assert!(matches!(
            document.validate(),
            Err(SchemaValidationError::InvalidValue(_))
        ));
        table.kind = DocumentBlockKind::Paragraph;
        let mismatch = DocumentModel {
            root: vec![table.id.clone()],
            blocks: vec![table],
            page_setup: None,
        };
        assert!(matches!(
            mismatch.validate(),
            Err(SchemaValidationError::InvalidValue(_))
        ));
    }

    #[test]
    fn code_payload_round_trips_and_uses_explicit_defaults() {
        let code = DocumentBlock {
            id: "code-1".into(),
            kind: DocumentBlockKind::Code,
            presentation: BlockPresentation::default(),
            content: Some(RichText {
                text: "fn main() {}".into(),
                runs: Vec::new(),
            }),
            children: Vec::new(),
            data: BlockData::Code(CodeBlockConfig {
                title: "Example".into(),
                language: "rust".into(),
                theme: "dark".into(),
                height: 200,
                show_line_numbers: true,
                wrap: false,
                indent_mode: CodeIndentMode::Spaces,
                indent_width: 4,
                font_size: 16,
            }),
        };
        let document = DocumentModel {
            root: vec![code.id.clone()],
            blocks: vec![code],
            page_setup: None,
        };
        document.validate().unwrap();
        let json = serde_json::to_value(&document).unwrap();
        assert_eq!(json["blocks"][0]["data"]["type"], "code");
        assert_eq!(json["blocks"][0]["data"]["data"]["indentMode"], "spaces");
        let parsed: DocumentModel = serde_json::from_value(json).unwrap();
        assert_eq!(document, parsed);

        let defaults: CodeBlockConfig = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(defaults, CodeBlockConfig::default());
        assert_eq!(defaults.language, "plainText");
        assert_eq!(defaults.theme, "light");
        assert_eq!(defaults.height, 200);
        assert!(defaults.show_line_numbers);
        assert_eq!(defaults.indent_width, 2);
        assert_eq!(defaults.font_size, 14);
    }

    #[test]
    fn code_without_payload_is_rejected_and_wrong_payload_is_rejected() {
        let code = DocumentBlock {
            id: "code-missing-config".into(),
            kind: DocumentBlockKind::Code,
            presentation: BlockPresentation::default(),
            content: Some(RichText::default()),
            children: Vec::new(),
            data: BlockData::None,
        };
        let document = DocumentModel {
            root: vec![code.id.clone()],
            blocks: vec![code.clone()],
            page_setup: None,
        };
        assert!(matches!(
            document.validate(),
            Err(SchemaValidationError::InvalidValue(message)) if message.contains("必须包含 code")
        ));

        let mut wrong = code;
        wrong.data = BlockData::Image(ImageBlock {
            asset_id: "asset-1".into(),
            alt: String::new(),
            original_asset_id: None,
            transform: Default::default(),
            caption: String::new(),
        });
        let document = DocumentModel {
            root: vec![wrong.id.clone()],
            blocks: vec![wrong],
            page_setup: None,
        };
        assert!(matches!(
            document.validate(),
            Err(SchemaValidationError::InvalidValue(message)) if message.contains("data.type")
        ));
    }

    #[test]
    fn code_payload_rejects_invalid_settings() {
        let config = CodeBlockConfig {
            language: "java script".into(),
            ..CodeBlockConfig::default()
        };
        let invalid_language = DocumentBlock {
            id: "code-language".into(),
            kind: DocumentBlockKind::Code,
            presentation: BlockPresentation::default(),
            content: None,
            children: Vec::new(),
            data: BlockData::Code(config),
        };
        let document = DocumentModel {
            root: vec![invalid_language.id.clone()],
            blocks: vec![invalid_language],
            page_setup: None,
        };
        assert!(matches!(
            document.validate(),
            Err(SchemaValidationError::InvalidValue(message)) if message.contains("language")
        ));

        let config = CodeBlockConfig {
            indent_width: 3,
            ..CodeBlockConfig::default()
        };
        let invalid_indent = DocumentBlock {
            id: "code-indent".into(),
            kind: DocumentBlockKind::Code,
            presentation: BlockPresentation::default(),
            content: None,
            children: Vec::new(),
            data: BlockData::Code(config),
        };
        let document = DocumentModel {
            root: vec![invalid_indent.id.clone()],
            blocks: vec![invalid_indent],
            page_setup: None,
        };
        assert!(matches!(
            document.validate(),
            Err(SchemaValidationError::InvalidValue(message)) if message.contains("indentWidth")
        ));
    }

    #[test]
    fn spreadsheet_rejects_duplicate_cells() {
        let spreadsheet = SpreadsheetModel {
            metadata: SpreadsheetMetadata::default(),
            sheets: vec![SheetModel {
                id: "sheet-1".into(),
                name: "Sheet 1".into(),
                cells: vec![
                    CellModel {
                        row: 1,
                        column: 1,
                        ..CellModel::default()
                    },
                    CellModel {
                        row: 1,
                        column: 1,
                        ..CellModel::default()
                    },
                ],
                metadata: SheetMetadata::default(),
            }],
        };
        let envelope =
            ArtifactEnvelope::new("sheet-artifact", ArtifactPayload::Spreadsheet(spreadsheet));
        assert!(matches!(
            envelope.validate(),
            Err(SchemaValidationError::InvalidValue(_))
        ));
    }

    #[test]
    fn mindmap_and_scene_graph_reject_invalid_references() {
        let mindmap = MindmapModel {
            root: Some("root".into()),
            nodes: vec![
                MindmapNode {
                    id: "root".into(),
                    ..MindmapNode::default()
                },
                MindmapNode {
                    id: "a".into(),
                    parent_id: Some("b".into()),
                    ..MindmapNode::default()
                },
                MindmapNode {
                    id: "b".into(),
                    parent_id: Some("a".into()),
                    ..MindmapNode::default()
                },
            ],
            edges: vec![],
        };
        let mindmap = ArtifactEnvelope::new("mindmap-artifact", ArtifactPayload::Mindmap(mindmap));
        assert!(matches!(
            mindmap.validate(),
            Err(SchemaValidationError::BlockCycle(_))
        ));

        let whiteboard = ArtifactEnvelope::new(
            "whiteboard-artifact",
            ArtifactPayload::Whiteboard(WhiteboardModel {
                elements: vec![SceneElement {
                    id: "shape-1".into(),
                    type_id: "rect".into(),
                    children: vec!["missing".into()],
                    ..SceneElement::default()
                }],
                camera: Camera {
                    x: 0.0,
                    y: 0.0,
                    scale: 1.0,
                },
            }),
        );
        assert!(matches!(
            whiteboard.validate(),
            Err(SchemaValidationError::MissingReference { .. })
        ));
    }

    #[test]
    fn online_presentation_rejects_retired_generic_scene_graph() {
        let legacy = serde_json::json!({
            "format": "open-office-artifact",
            "schemaVersion": CURRENT_SCHEMA_VERSION,
            "artifactId": "presentation-artifact",
            "revision": 0,
            "kind": "presentation",
            "payload": { "kind": "presentation", "data": {
                "slides": [{ "id": "slide-1", "elements": [{
                    "id": "shape-1", "typeId": "shape", "attrs": {},
                    "transform": { "x": 0, "y": 0, "width": 1, "height": 1 }, "children": []
                }], "theme": null }]
            }}
        });
        assert!(serde_json::from_value::<ArtifactEnvelope>(legacy).is_err());
    }
}
