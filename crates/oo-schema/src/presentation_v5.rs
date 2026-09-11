//! Strict v5 Presentation target schema.
//!
//! This module contains no legacy conversion and is deliberately not wired into
//! [`crate::ArtifactPayload`] until every v4 Presentation consumer is switched in one cutover.
//! Keeping the target model isolated makes its invariants testable without introducing an online
//! dual-read path.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{LegacyPresentationModel, SchemaValidationError};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Deck {
    pub page_spec: SlidePageSpec,
    #[serde(default)]
    pub slides: Vec<Slide>,
    #[serde(default)]
    pub masters: Vec<SlideMaster>,
    #[serde(default)]
    pub layouts: Vec<SlideLayout>,
    pub theme: DeckTheme,
    #[serde(default)]
    pub assets: Vec<AssetRef>,
}

/// Empty decks are valid artifacts: a renderer can show its canonical first-slide affordance
/// without inventing a second, client-owned model. New-slide creation is a semantic command.
impl Default for Deck {
    fn default() -> Self {
        Self {
            page_spec: SlidePageSpec::default(),
            slides: Vec::new(),
            masters: Vec::new(),
            layouts: Vec::new(),
            theme: DeckTheme {
                id: "default".into(),
                ..DeckTheme::default()
            },
            assets: Vec::new(),
        }
    }
}

impl Deck {
    pub fn validate(&self) -> Result<(), SchemaValidationError> {
        self.page_spec.validate()?;
        self.theme.validate()?;
        unique(
            self.slides.iter().map(|value| value.id.as_str()),
            "presentation slide",
        )?;
        unique(
            self.masters.iter().map(|value| value.id.as_str()),
            "presentation master",
        )?;
        unique(
            self.layouts.iter().map(|value| value.id.as_str()),
            "presentation layout",
        )?;
        unique(
            self.assets.iter().map(|value| value.asset_id.as_str()),
            "presentation asset",
        )?;
        for asset in &self.assets {
            asset.validate()?;
        }
        let asset_ids: HashSet<&str> = self
            .assets
            .iter()
            .map(|item| item.asset_id.as_str())
            .collect();
        let master_ids: HashSet<&str> = self.masters.iter().map(|item| item.id.as_str()).collect();
        let masters_by_id: HashMap<&str, &SlideMaster> = self
            .masters
            .iter()
            .map(|master| (master.id.as_str(), master))
            .collect();
        let layouts_by_id: HashMap<&str, &SlideLayout> = self
            .layouts
            .iter()
            .map(|layout| (layout.id.as_str(), layout))
            .collect();
        for master in &self.masters {
            master.validate()?;
        }
        for layout in &self.layouts {
            if !master_ids.contains(layout.master_id.as_str()) {
                return missing(&layout.id, &layout.master_id);
            }
            layout.validate(
                masters_by_id
                    .get(layout.master_id.as_str())
                    .expect("validated master"),
            )?;
        }
        for slide in &self.slides {
            slide.validate(&layouts_by_id, &asset_ids)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlidePageSpec {
    pub width: f64,
    pub height: f64,
    pub unit: PageUnit,
    #[serde(default)]
    pub safe_area: Option<Insets>,
}

impl Default for SlidePageSpec {
    fn default() -> Self {
        Self {
            width: 12_192_000.0,
            height: 6_858_000.0,
            unit: PageUnit::Emu,
            safe_area: None,
        }
    }
}

impl SlidePageSpec {
    fn validate(&self) -> Result<(), SchemaValidationError> {
        finite_positive(self.width, "presentation page width")?;
        finite_positive(self.height, "presentation page height")?;
        if let Some(insets) = &self.safe_area {
            insets.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageUnit {
    Emu,
    Point,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Insets {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}
impl Insets {
    fn validate(&self) -> Result<(), SchemaValidationError> {
        for (name, value) in [
            ("top", self.top),
            ("right", self.right),
            ("bottom", self.bottom),
            ("left", self.left),
        ] {
            if !value.is_finite() || value < 0.0 {
                return invalid(format!("presentation safeArea.{name} 必须是非负有限数"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Slide {
    pub id: String,
    pub order_key: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub layout_id: Option<String>,
    #[serde(default)]
    pub background: SlideBackground,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub transition: Option<SlideTransition>,
    #[serde(default)]
    pub nodes: Vec<SceneNode>,
    #[serde(default)]
    pub timeline: Timeline,
}

impl Slide {
    fn validate(
        &self,
        layouts: &HashMap<&str, &SlideLayout>,
        assets: &HashSet<&str>,
    ) -> Result<(), SchemaValidationError> {
        non_empty(&self.id, "presentation slide")?;
        non_empty(&self.order_key, "presentation slide orderKey")?;
        let layout = match &self.layout_id {
            Some(layout_id) => Some(layouts.get(layout_id.as_str()).copied().ok_or_else(|| {
                SchemaValidationError::MissingReference {
                    owner: self.id.clone(),
                    target: layout_id.clone(),
                }
            })?),
            None => None,
        };
        self.background.validate()?;
        if let Some(transition) = &self.transition {
            transition.validate()?;
        }
        unique(
            self.nodes.iter().map(|node| node.id.as_str()),
            "presentation node",
        )?;
        let by_id: HashMap<&str, &SceneNode> = self
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        for node in &self.nodes {
            node.validate(&by_id, assets)?;
            if let Some(placeholder_id) = &node.layout_placeholder_id {
                let Some(layout) = layout else {
                    return invalid(format!(
                        "node {} 引用了 layout placeholder，但 slide {} 未指定 layout",
                        node.id, self.id
                    ));
                };
                if !layout
                    .placeholders
                    .iter()
                    .any(|placeholder| placeholder.id == *placeholder_id)
                {
                    return missing(&node.id, placeholder_id);
                }
            }
        }
        validate_node_tree(&by_id)?;
        validate_sibling_order_keys(&self.nodes)?;
        self.timeline.validate(&by_id)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum SlideBackground {
    #[default]
    None,
    Solid(ColorRef),
}
impl SlideBackground {
    fn validate(&self) -> Result<(), SchemaValidationError> {
        if let Self::Solid(color) = self {
            color.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlideTransition {
    pub kind: TransitionKind,
    #[serde(default)]
    pub duration_ms: u32,
}
impl SlideTransition {
    fn validate(&self) -> Result<(), SchemaValidationError> {
        // Keep the persisted timing bounded.  Zero remains valid for an explicit
        // instant/disabled transition and for imported Office defaults.
        if self.duration_ms > 600_000 {
            return invalid("presentation transition durationMs 不能超过 600000");
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransitionKind {
    None,
    Fade,
    Push,
    Wipe,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlideMaster {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub background: SlideBackground,
    #[serde(default)]
    pub placeholders: Vec<MasterPlaceholder>,
}

impl SlideMaster {
    fn validate(&self) -> Result<(), SchemaValidationError> {
        non_empty(&self.id, "presentation master")?;
        self.background.validate()?;
        unique(
            self.placeholders
                .iter()
                .map(|placeholder| placeholder.id.as_str()),
            "presentation master placeholder",
        )?;
        for placeholder in &self.placeholders {
            placeholder.validate(&self.id)?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlideLayout {
    pub id: String,
    pub master_id: String,
    pub name: String,
    #[serde(default)]
    pub placeholders: Vec<LayoutPlaceholder>,
}

impl SlideLayout {
    fn validate(&self, master: &SlideMaster) -> Result<(), SchemaValidationError> {
        non_empty(&self.id, "presentation layout")?;
        non_empty(&self.master_id, "presentation layout master")?;
        unique(
            self.placeholders
                .iter()
                .map(|placeholder| placeholder.id.as_str()),
            "presentation layout placeholder",
        )?;
        for placeholder in &self.placeholders {
            placeholder.validate(self, master)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MasterPlaceholder {
    pub id: String,
    pub kind: PlaceholderKind,
    pub transform: NodeTransform,
    #[serde(default)]
    pub default_text: Option<PresentationRichText>,
}

impl MasterPlaceholder {
    fn validate(&self, master_id: &str) -> Result<(), SchemaValidationError> {
        non_empty(&self.id, "presentation master placeholder")?;
        self.transform.validate()?;
        if let Some(text) = &self.default_text {
            text.validate(&format!("master {master_id} placeholder {}", self.id))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayoutPlaceholder {
    pub id: String,
    pub kind: PlaceholderKind,
    #[serde(default)]
    pub master_placeholder_id: Option<String>,
    pub transform: NodeTransform,
    #[serde(default)]
    pub default_text: Option<PresentationRichText>,
}

impl LayoutPlaceholder {
    fn validate(
        &self,
        layout: &SlideLayout,
        master: &SlideMaster,
    ) -> Result<(), SchemaValidationError> {
        non_empty(&self.id, "presentation layout placeholder")?;
        self.transform.validate()?;
        if let Some(master_placeholder_id) = &self.master_placeholder_id {
            if !master
                .placeholders
                .iter()
                .any(|placeholder| placeholder.id == *master_placeholder_id)
            {
                return missing(&self.id, master_placeholder_id);
            }
        }
        if let Some(text) = &self.default_text {
            text.validate(&format!("layout {} placeholder {}", layout.id, self.id))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlaceholderKind {
    Title,
    CenteredTitle,
    Subtitle,
    Body,
    Picture,
    Table,
    Chart,
    Object,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeckTheme {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub colors: BTreeMap<ThemeColorToken, Rgba>,
    #[serde(default)]
    pub fonts: BTreeMap<ThemeFontToken, String>,
}
impl DeckTheme {
    fn validate(&self) -> Result<(), SchemaValidationError> {
        non_empty(&self.id, "presentation theme")?;
        if self.fonts.values().any(|font| font.trim().is_empty()) {
            return invalid("presentation theme 字体不能为空");
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemeColorToken {
    Background,
    Text,
    Accent1,
    Accent2,
    Accent3,
    Accent4,
    Accent5,
    Accent6,
    Hyperlink,
    FollowedHyperlink,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemeFontToken {
    Heading,
    Body,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    #[serde(default = "opaque")]
    pub a: u8,
}
fn opaque() -> u8 {
    u8::MAX
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum ColorRef {
    Theme(ThemeColorToken),
    Rgba(Rgba),
}
impl ColorRef {
    fn validate(&self) -> Result<(), SchemaValidationError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetRef {
    pub asset_id: String,
    pub digest: String,
    pub mime_type: String,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub original_asset_id: Option<String>,
}
impl AssetRef {
    fn validate(&self) -> Result<(), SchemaValidationError> {
        non_empty(&self.asset_id, "presentation asset")?;
        non_empty(&self.digest, "presentation asset digest")?;
        non_empty(&self.mime_type, "presentation asset mimeType")?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneNode {
    pub id: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub order_key: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub alt_text: Option<String>,
    /// Links an instance node to a typed placeholder declared by the slide layout.
    /// The slide validates this separately because only it knows the active layout.
    #[serde(default)]
    pub layout_placeholder_id: Option<String>,
    pub transform: NodeTransform,
    #[serde(default = "visible_default")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default = "opacity_default")]
    pub opacity: f32,
    pub kind: SceneNodeKind,
}
fn visible_default() -> bool {
    true
}
fn opacity_default() -> f32 {
    1.0
}
impl SceneNode {
    fn validate(
        &self,
        nodes: &HashMap<&str, &SceneNode>,
        assets: &HashSet<&str>,
    ) -> Result<(), SchemaValidationError> {
        non_empty(&self.id, "presentation node")?;
        non_empty(&self.order_key, "presentation node orderKey")?;
        self.transform.validate()?;
        if !self.opacity.is_finite() || !(0.0..=1.0).contains(&self.opacity) {
            return invalid(format!("node {} opacity 无效", self.id));
        }
        if let Some(parent) = &self.parent_id {
            if parent == &self.id {
                return invalid(format!("node {} 不能成为自身父节点", self.id));
            }
            if !nodes.contains_key(parent.as_str()) {
                return missing(&self.id, parent);
            }
        }
        self.kind.validate(&self.id, nodes, assets)
    }
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NodeTransform {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub rotation: f64,
}
impl NodeTransform {
    fn validate(&self) -> Result<(), SchemaValidationError> {
        for value in [self.x, self.y, self.rotation] {
            if !value.is_finite() {
                return invalid("presentation node transform 必须是有限数");
            }
        }
        finite_positive(self.width, "presentation node width")?;
        finite_positive(self.height, "presentation node height")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum SceneNodeKind {
    Shape(ShapeNode),
    Text(TextNode),
    Image(ImageNode),
    Video(MediaNode),
    Audio(MediaNode),
    Table(TableNode),
    Chart(ChartNode),
    Connector(ConnectorNode),
    Group(GroupNode),
    Embed(EmbedNode),
    Extension(ExtensionNode),
}
impl SceneNodeKind {
    fn validate(
        &self,
        node_id: &str,
        nodes: &HashMap<&str, &SceneNode>,
        assets: &HashSet<&str>,
    ) -> Result<(), SchemaValidationError> {
        match self {
            Self::Text(text) => text.frame.validate(node_id),
            Self::Image(image) => {
                validate_asset_ref(node_id, &image.asset_id, assets)?;
                if let Some(original) = &image.original_asset_id {
                    validate_asset_ref(node_id, original, assets)?;
                }
                image.crop.validate()?;
                Ok(())
            }
            Self::Video(media) | Self::Audio(media) => {
                validate_asset_ref(node_id, &media.asset_id, assets)?;
                if let Some(poster) = &media.poster_asset_id {
                    validate_asset_ref(node_id, poster, assets)?;
                }
                Ok(())
            }
            Self::Connector(connector) => connector.validate(node_id, nodes),
            Self::Embed(embed) => {
                non_empty(&embed.source, "presentation embed source")?;
                if let Some(poster) = &embed.poster_asset_id {
                    validate_asset_ref(node_id, poster, assets)?;
                }
                Ok(())
            }
            Self::Extension(extension) => {
                non_empty(&extension.namespace, "presentation extension namespace")?;
                non_empty(&extension.version, "presentation extension version")?;
                non_empty(&extension.type_id, "presentation extension typeId")?;
                Ok(())
            }
            Self::Shape(shape) => {
                shape.style.validate()?;
                Ok(())
            }
            Self::Table(table) => table.validate(node_id),
            Self::Chart(chart) => chart.spec.validate(node_id),
            Self::Group(_) => Ok(()),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShapeNode {
    pub geometry: ShapeGeometry,
    pub style: ShapeStyle,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShapeStyle {
    #[serde(default)]
    pub fill: Paint,
    #[serde(default)]
    pub stroke: Option<Stroke>,
}
impl Default for ShapeStyle {
    fn default() -> Self {
        Self {
            fill: Paint::None,
            stroke: None,
        }
    }
}
impl ShapeStyle {
    fn validate(&self) -> Result<(), SchemaValidationError> {
        self.fill.validate()?;
        if let Some(stroke) = &self.stroke {
            stroke.validate()?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum Paint {
    #[default]
    None,
    Solid(ColorRef),
}
impl Paint {
    fn validate(&self) -> Result<(), SchemaValidationError> {
        match self {
            Self::None => Ok(()),
            Self::Solid(color) => color.validate(),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShapeGeometry {
    Rectangle,
    Ellipse,
    Line,
    Arrow,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Stroke {
    pub color: ColorRef,
    pub width: f64,
}
impl Stroke {
    fn validate(&self) -> Result<(), SchemaValidationError> {
        self.color.validate()?;
        finite_positive(self.width, "presentation stroke width")
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextNode {
    pub frame: TextFrame,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextFrame {
    pub body: PresentationRichText,
    #[serde(default)]
    pub vertical_align: TextVerticalAlign,
    pub padding: Insets,
    #[serde(default)]
    pub auto_fit: TextAutoFit,
}
impl TextFrame {
    fn validate(&self, owner: &str) -> Result<(), SchemaValidationError> {
        self.body.validate(owner)?;
        self.padding.validate()
    }
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresentationRichText {
    pub text: String,
    #[serde(default)]
    pub runs: Vec<PresentationTextRun>,
    pub paragraphs: Vec<PresentationParagraph>,
}
impl PresentationRichText {
    pub fn plain(text: impl Into<String>) -> Self {
        let text = text.into();
        let paragraphs = paragraph_ranges(&text)
            .into_iter()
            .map(|(start, end)| PresentationParagraph {
                start,
                end,
                alignment: TextHorizontalAlign::Left,
                list: None,
                indent_level: 0,
            })
            .collect();
        Self {
            text,
            runs: Vec::new(),
            paragraphs,
        }
    }

    fn validate(&self, owner: &str) -> Result<(), SchemaValidationError> {
        let text_len = self.text.chars().count();
        if self.runs.is_empty() {
            self.validate_paragraphs(owner, text_len)?;
            return Ok(());
        }
        let mut expected_start = 0;
        for (index, run) in self.runs.iter().enumerate() {
            run.style.validate(owner)?;
            if run.start != expected_start || run.start >= run.end || run.end > text_len {
                return invalid(format!(
                    "presentation text {owner} run[{index}] 区间无效：{}..{}，文本长度 {text_len}",
                    run.start, run.end
                ));
            }
            expected_start = run.end;
        }
        if expected_start != text_len {
            return invalid(format!("presentation text {owner} runs 未覆盖全文"));
        }
        self.validate_paragraphs(owner, text_len)?;
        Ok(())
    }

    fn validate_paragraphs(
        &self,
        owner: &str,
        text_len: usize,
    ) -> Result<(), SchemaValidationError> {
        if text_len == 0 {
            return if self.paragraphs.is_empty() {
                Ok(())
            } else {
                invalid(format!(
                    "presentation text {owner} 空文本不能包含 paragraph"
                ))
            };
        }
        let mut expected_start = 0;
        for (index, paragraph) in self.paragraphs.iter().enumerate() {
            if paragraph.start != expected_start
                || paragraph.start >= paragraph.end
                || paragraph.end > text_len
            {
                return invalid(format!("presentation text {owner} paragraph[{index}] 区间无效：{}..{}，文本长度 {text_len}", paragraph.start, paragraph.end));
            }
            if paragraph.indent_level > 8 {
                return invalid(format!(
                    "presentation text {owner} paragraph[{index}] indentLevel 必须在 0 到 8 之间"
                ));
            }
            if matches!(
                paragraph.list,
                Some(PresentationListStyle::Ordered { start_at: 0 })
            ) {
                return invalid(format!(
                    "presentation text {owner} paragraph[{index}] ordered startAt 必须大于 0"
                ));
            }
            expected_start = paragraph.end;
        }
        if expected_start != text_len {
            return invalid(format!("presentation text {owner} paragraphs 未覆盖全文"));
        }
        Ok(())
    }
}

fn paragraph_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for (index, character) in text.chars().enumerate() {
        if character == '\n' {
            ranges.push((start, index + 1));
            start = index + 1;
        }
    }
    let len = text.chars().count();
    if start < len {
        ranges.push((start, len));
    }
    ranges
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresentationParagraph {
    pub start: usize,
    pub end: usize,
    pub alignment: TextHorizontalAlign,
    pub list: Option<PresentationListStyle>,
    pub indent_level: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextHorizontalAlign {
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PresentationListStyle {
    Bullet,
    Ordered { start_at: u32 },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresentationTextRun {
    pub start: usize,
    pub end: usize,
    pub style: PresentationTextStyle,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresentationTextStyle {
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
    pub color: Option<ColorRef>,
}
impl PresentationTextStyle {
    fn validate(&self, owner: &str) -> Result<(), SchemaValidationError> {
        if self
            .font_family
            .as_ref()
            .is_some_and(|font| font.trim().is_empty())
        {
            return invalid(format!("presentation text {owner} fontFamily 不能为空"));
        }
        if self
            .font_size
            .is_some_and(|size| !size.is_finite() || size <= 0.0 || size > 512.0)
        {
            return invalid(format!(
                "presentation text {owner} fontSize 必须在 0 到 512 之间"
            ));
        }
        if let Some(color) = &self.color {
            color.validate()?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextVerticalAlign {
    Top,
    #[default]
    Middle,
    Bottom,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextAutoFit {
    #[default]
    None,
    ShrinkText,
    ResizeShape,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageNode {
    pub asset_id: String,
    #[serde(default)]
    pub original_asset_id: Option<String>,
    #[serde(default)]
    pub crop: ImageCrop,
    #[serde(default)]
    pub flip_h: bool,
    #[serde(default)]
    pub flip_v: bool,
    #[serde(default)]
    pub caption: Option<String>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageCrop {
    #[serde(default)]
    pub top: f64,
    #[serde(default)]
    pub right: f64,
    #[serde(default)]
    pub bottom: f64,
    #[serde(default)]
    pub left: f64,
}
impl ImageCrop {
    fn validate(&self) -> Result<(), SchemaValidationError> {
        for (name, value) in [
            ("top", self.top),
            ("right", self.right),
            ("bottom", self.bottom),
            ("left", self.left),
        ] {
            if !value.is_finite() || !(0.0..1.0).contains(&value) {
                return invalid(format!("presentation image crop.{name} 必须在 0 到 1 之间"));
            }
        }
        if self.left + self.right >= 1.0 || self.top + self.bottom >= 1.0 {
            return invalid("presentation image crop 不能裁掉整张图片");
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaNode {
    pub asset_id: String,
    #[serde(default)]
    pub poster_asset_id: Option<String>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableNode {
    pub rows: u32,
    pub columns: u32,
    #[serde(default)]
    pub cells: Vec<TableCell>,
}
impl TableNode {
    fn validate(&self, node_id: &str) -> Result<(), SchemaValidationError> {
        if self.rows == 0 || self.columns == 0 {
            return invalid("presentation table 行列数必须大于 0");
        }
        let mut occupied = HashSet::new();
        for cell in &self.cells {
            cell.validate(node_id, self.rows, self.columns, &mut occupied)?;
        }
        if occupied.len() != (self.rows as usize) * (self.columns as usize) {
            return invalid(format!("presentation table {node_id} cells 未覆盖完整网格"));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableCell {
    pub row: u32,
    pub column: u32,
    #[serde(default = "one")]
    pub row_span: u32,
    #[serde(default = "one")]
    pub column_span: u32,
    pub content: PresentationRichText,
    pub style: TableCellStyle,
}
fn one() -> u32 {
    1
}
impl TableCell {
    fn validate(
        &self,
        node_id: &str,
        rows: u32,
        columns: u32,
        occupied: &mut HashSet<(u32, u32)>,
    ) -> Result<(), SchemaValidationError> {
        if self.row_span == 0
            || self.column_span == 0
            || self.row >= rows
            || self.column >= columns
            || self.row.saturating_add(self.row_span) > rows
            || self.column.saturating_add(self.column_span) > columns
        {
            return invalid(format!("presentation table {node_id} cell 范围无效"));
        }
        self.content.validate(node_id)?;
        self.style.validate()?;
        for row in self.row..self.row + self.row_span {
            for column in self.column..self.column + self.column_span {
                if !occupied.insert((row, column)) {
                    return invalid(format!("presentation table {node_id} cell 范围重叠"));
                }
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableCellStyle {
    #[serde(default)]
    pub fill: Paint,
    #[serde(default)]
    pub horizontal_align: HorizontalAlign,
    #[serde(default)]
    pub vertical_align: TextVerticalAlign,
}
impl TableCellStyle {
    fn validate(&self) -> Result<(), SchemaValidationError> {
        self.fill.validate()
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HorizontalAlign {
    #[default]
    Left,
    Center,
    Right,
}
/// The deliberately small, editable chart subset.  These are semantic chart
/// families rather than names copied from an OOXML part, so a renderer cannot
/// accidentally persist a vendor-specific chart type string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChartType {
    Column,
    Bar,
    Line,
    Pie,
}

/// One named data series.  `values` is aligned with `ChartSpec.categories` by
/// index; no renderer-side sparse-data reconciliation is permitted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChartSeries {
    pub name: String,
    pub values: Vec<f64>,
    #[serde(default)]
    pub color: Option<ColorRef>,
}

/// Canonical editable chart data.  This is intentionally not an arbitrary
/// chart XML or JSON payload: unsupported chart families and per-point
/// formatting stay out of the online model until they have typed semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChartSpec {
    pub chart_type: ChartType,
    #[serde(default)]
    pub title: Option<String>,
    pub categories: Vec<String>,
    pub series: Vec<ChartSeries>,
}
impl ChartSpec {
    fn validate(&self, node_id: &str) -> Result<(), SchemaValidationError> {
        if let Some(title) = &self.title {
            non_empty(title, "presentation chart title")?;
        }
        if self.categories.is_empty() {
            return invalid(format!("presentation chart {node_id} categories 不能为空"));
        }
        if self
            .categories
            .iter()
            .any(|category| category.trim().is_empty())
        {
            return invalid(format!("presentation chart {node_id} category 不能为空"));
        }
        if self.series.is_empty() {
            return invalid(format!("presentation chart {node_id} series 不能为空"));
        }
        if self.chart_type == ChartType::Pie && self.series.len() != 1 {
            return invalid(format!(
                "presentation chart {node_id} pie 只支持一个 data series"
            ));
        }
        let mut names = BTreeSet::new();
        for series in &self.series {
            non_empty(&series.name, "presentation chart series name")?;
            if !names.insert(&series.name) {
                return invalid(format!("presentation chart {node_id} series 名称重复"));
            }
            if series.values.len() != self.categories.len() {
                return invalid(format!(
                    "presentation chart {node_id} series {} values 数量必须与 categories 一致",
                    series.name
                ));
            }
            if series.values.iter().any(|value| !value.is_finite()) {
                return invalid(format!(
                    "presentation chart {node_id} series {} values 必须是有限数",
                    series.name
                ));
            }
            if self.chart_type == ChartType::Pie && series.values.iter().any(|value| *value < 0.0) {
                return invalid(format!(
                    "presentation chart {node_id} pie series {} values 不能为负数",
                    series.name
                ));
            }
            if let Some(color) = &series.color {
                color.validate()?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChartNode {
    pub spec: ChartSpec,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectorNode {
    pub start: ConnectorEndpoint,
    pub end: ConnectorEndpoint,
}
impl ConnectorNode {
    fn validate(
        &self,
        node_id: &str,
        nodes: &HashMap<&str, &SceneNode>,
    ) -> Result<(), SchemaValidationError> {
        self.start.validate(node_id, nodes)?;
        self.end.validate(node_id, nodes)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum ConnectorEndpoint {
    Free(Point),
    Node { node_id: String, anchor: Anchor },
}
impl ConnectorEndpoint {
    fn validate(
        &self,
        node_id: &str,
        nodes: &HashMap<&str, &SceneNode>,
    ) -> Result<(), SchemaValidationError> {
        match self {
            Self::Free(point) => point.validate(),
            Self::Node {
                node_id: target, ..
            } => {
                non_empty(target, "presentation connector target")?;
                if target == node_id {
                    return invalid(format!("connector {node_id} 不能连接自身"));
                }
                if !nodes.contains_key(target.as_str()) {
                    return missing(node_id, target);
                }
                Ok(())
            }
        }
    }
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
impl Point {
    fn validate(&self) -> Result<(), SchemaValidationError> {
        if self.x.is_finite() && self.y.is_finite() {
            Ok(())
        } else {
            invalid("presentation point 必须是有限数")
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Anchor {
    Top,
    Right,
    Bottom,
    Left,
    Center,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupNode {}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmbedNode {
    pub source: String,
    #[serde(default)]
    pub poster_asset_id: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionNode {
    pub namespace: String,
    pub version: String,
    /// Stable extension-local type. The host never interprets this value, but it
    /// gives a registered renderer a concrete contract instead of an untyped
    /// `raw` escape hatch.
    pub type_id: String,
    /// Opaque extension data. It is structurally constrained to an object so
    /// a plugin payload is always safe to preserve, inspect and version.
    #[serde(default)]
    pub data: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Timeline {
    #[serde(default)]
    pub entries: Vec<AnimationEntry>,
}
impl Timeline {
    fn validate(&self, nodes: &HashMap<&str, &SceneNode>) -> Result<(), SchemaValidationError> {
        unique(
            self.entries.iter().map(|entry| entry.id.as_str()),
            "presentation animation",
        )?;
        unique(
            self.entries.iter().map(|entry| entry.order_key.as_str()),
            "presentation animation orderKey",
        )?;
        for entry in &self.entries {
            non_empty(&entry.id, "presentation animation")?;
            non_empty(&entry.order_key, "presentation animation orderKey")?;
            non_empty(&entry.target_node_id, "presentation animation target")?;
            if entry.duration_ms > 600_000 || entry.delay_ms > 600_000 {
                return invalid(format!(
                    "presentation animation {} 的 durationMs/delayMs 不能超过 600000",
                    entry.id
                ));
            }
            if !nodes.contains_key(entry.target_node_id.as_str()) {
                return missing(&entry.id, &entry.target_node_id);
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnimationEntry {
    pub id: String,
    pub target_node_id: String,
    pub trigger: AnimationTrigger,
    pub preset: AnimationPreset,
    #[serde(default)]
    pub duration_ms: u32,
    #[serde(default)]
    pub delay_ms: u32,
    pub order_key: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnimationTrigger {
    OnClick,
    WithPrevious,
    AfterPrevious,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnimationPreset {
    Appear,
    Fade,
    FlyIn,
    Wipe,
}

fn validate_node_tree(nodes: &HashMap<&str, &SceneNode>) -> Result<(), SchemaValidationError> {
    for node in nodes.values() {
        let mut seen = HashSet::new();
        let mut cursor = node.parent_id.as_deref();
        while let Some(parent) = cursor {
            if !seen.insert(parent) {
                return invalid(format!("presentation node hierarchy 存在环：{}", node.id));
            }
            cursor = nodes
                .get(parent)
                .and_then(|value| value.parent_id.as_deref());
        }
    }
    Ok(())
}
fn validate_sibling_order_keys(nodes: &[SceneNode]) -> Result<(), SchemaValidationError> {
    let mut keys = HashSet::new();
    for node in nodes {
        let key = (
            node.parent_id.as_deref().unwrap_or("<root>"),
            node.order_key.as_str(),
        );
        if !keys.insert(key) {
            return invalid(format!(
                "presentation sibling orderKey 重复：{}",
                node.order_key
            ));
        }
    }
    Ok(())
}
fn validate_asset_ref(
    owner: &str,
    asset_id: &str,
    assets: &HashSet<&str>,
) -> Result<(), SchemaValidationError> {
    non_empty(asset_id, "presentation node asset")?;
    if assets.contains(asset_id) {
        Ok(())
    } else {
        missing(owner, asset_id)
    }
}
fn unique<'a>(
    ids: impl Iterator<Item = &'a str>,
    kind: &'static str,
) -> Result<(), SchemaValidationError> {
    let mut seen = HashSet::new();
    for id in ids {
        non_empty(id, kind)?;
        if !seen.insert(id) {
            return Err(SchemaValidationError::DuplicateId(kind));
        }
    }
    Ok(())
}
fn non_empty(value: &str, kind: &'static str) -> Result<(), SchemaValidationError> {
    if value.trim().is_empty() {
        Err(SchemaValidationError::EmptyId(kind))
    } else {
        Ok(())
    }
}
fn finite_positive(value: f64, field: &str) -> Result<(), SchemaValidationError> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        invalid(format!("{field} 必须是正有限数"))
    }
}
fn missing(owner: &str, target: &str) -> Result<(), SchemaValidationError> {
    Err(SchemaValidationError::MissingReference {
        owner: owner.into(),
        target: target.into(),
    })
}
fn invalid<T>(message: impl Into<String>) -> Result<T, SchemaValidationError> {
    Err(SchemaValidationError::InvalidValue(message.into()))
}

/// Offline-only conversion result for a v4 Presentation scene graph. The artifact-envelope
/// migrator will consume this after the v5 payload cutover; it is intentionally not reachable
/// from HTTP, WASM or browser parsing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LegacyMigrationReport {
    #[serde(default)]
    pub preserved_extensions: Vec<String>,
    #[serde(default)]
    pub losses: Vec<String>,
}

/// Converts a validated v4 scene graph to the typed v5 Deck target.
///
/// The compiler is intentionally conservative: unsupported node types and unsupported shape
/// geometry retain their entire legacy payload in an `Extension` node. A field is only discarded
/// when it has no v5 representation, and each such discard is added to `losses`.
pub(crate) fn migrate_legacy_v4(
    legacy: &LegacyPresentationModel,
) -> Result<(Deck, LegacyMigrationReport), SchemaValidationError> {
    let mut report = LegacyMigrationReport::default();
    report_loss(
        &mut report,
        "deck pageSpec defaulted to 12192000x6858000 emu because v4 did not persist page geometry",
    );
    let mut slides = Vec::with_capacity(legacy.slides.len());
    for (slide_index, legacy_slide) in legacy.slides.iter().enumerate() {
        let mut parents = HashMap::<&str, &str>::new();
        for element in &legacy_slide.elements {
            for child in &element.children {
                if let Some(previous) = parents.insert(child, element.id.as_str()) {
                    if previous != element.id {
                        return invalid(format!("legacy scene child {child} 存在多个父节点"));
                    }
                }
            }
        }
        let mut nodes = Vec::with_capacity(legacy_slide.elements.len());
        for (node_index, legacy_node) in legacy_slide.elements.iter().enumerate() {
            let kind = migrate_legacy_node_kind(legacy_node, &mut report);
            let name = legacy_node
                .attrs
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string);
            if legacy_node.attrs.contains_key("name") && name.is_none() {
                report_loss(
                    &mut report,
                    format!("node {} dropped non-string attrs.name", legacy_node.id),
                );
            }
            nodes.push(SceneNode {
                id: legacy_node.id.clone(),
                parent_id: parents
                    .get(legacy_node.id.as_str())
                    .map(|value| (*value).into()),
                order_key: format!("{:08}", node_index),
                name,
                alt_text: None,
                layout_placeholder_id: None,
                transform: migrate_legacy_transform(legacy_node, &mut report),
                visible: true,
                locked: false,
                opacity: 1.0,
                kind,
            });
        }
        slides.push(Slide {
            id: legacy_slide.id.clone(),
            order_key: format!("{:08}", slide_index),
            name: legacy_slide.name.clone(),
            layout_id: None,
            background: SlideBackground::None,
            notes: legacy_slide.notes.clone(),
            transition: None,
            nodes,
            timeline: migrate_legacy_timeline(legacy_slide, &mut report),
        });
    }
    let deck = Deck {
        page_spec: SlidePageSpec::default(),
        slides,
        masters: Vec::new(),
        layouts: Vec::new(),
        theme: migrate_legacy_theme(legacy.theme.as_ref(), &mut report),
        assets: Vec::new(),
    };
    deck.validate()?;
    Ok((deck, report))
}

fn migrate_legacy_node_kind(
    legacy_node: &crate::SceneElement,
    report: &mut LegacyMigrationReport,
) -> SceneNodeKind {
    match legacy_node.type_id.as_str() {
        "shape" => match legacy_shape_geometry(legacy_node.attrs.get("shapeKind")) {
            Some(geometry) => {
                report_unmapped_attrs(legacy_node, &["name", "shapeKind"], report);
                SceneNodeKind::Shape(ShapeNode {
                    geometry,
                    style: ShapeStyle::default(),
                })
            }
            None => legacy_extension(legacy_node, report),
        },
        "text" => match legacy_node.attrs.get("text") {
            None | Some(Value::String(_)) => {
                report_unmapped_attrs(legacy_node, &["name", "text"], report);
                SceneNodeKind::Text(TextNode {
                    frame: TextFrame {
                        body: PresentationRichText::plain(
                            legacy_node
                                .attrs
                                .get("text")
                                .and_then(Value::as_str)
                                .unwrap_or_default(),
                        ),
                        vertical_align: TextVerticalAlign::Middle,
                        padding: Insets::default(),
                        auto_fit: TextAutoFit::None,
                    },
                })
            }
            Some(_) => legacy_extension(legacy_node, report),
        },
        "group" => {
            report_unmapped_attrs(legacy_node, &["name"], report);
            SceneNodeKind::Group(GroupNode {})
        }
        _ => legacy_extension(legacy_node, report),
    }
}

fn legacy_shape_geometry(value: Option<&Value>) -> Option<ShapeGeometry> {
    let kind = value.and_then(Value::as_str).unwrap_or("rectangle");
    match kind.to_ascii_lowercase().as_str() {
        "rect" | "rectangle" => Some(ShapeGeometry::Rectangle),
        "ellipse" | "oval" => Some(ShapeGeometry::Ellipse),
        "line" => Some(ShapeGeometry::Line),
        "arrow" | "rightarrow" | "right-arrow" => Some(ShapeGeometry::Arrow),
        _ => None,
    }
}

fn legacy_extension(
    legacy_node: &crate::SceneElement,
    report: &mut LegacyMigrationReport,
) -> SceneNodeKind {
    report.preserved_extensions.push(legacy_node.id.clone());
    SceneNodeKind::Extension(ExtensionNode {
        namespace: "legacy.presentation".into(),
        version: "4".into(),
        type_id: legacy_node.type_id.clone(),
        data: BTreeMap::from([
            (
                "attrs".into(),
                serde_json::to_value(&legacy_node.attrs).expect("legacy attrs are serializable"),
            ),
            (
                "children".into(),
                serde_json::to_value(&legacy_node.children)
                    .expect("legacy children are serializable"),
            ),
        ]),
    })
}

fn report_unmapped_attrs(
    legacy_node: &crate::SceneElement,
    accepted: &[&str],
    report: &mut LegacyMigrationReport,
) {
    for key in legacy_node.attrs.keys() {
        if !accepted.contains(&key.as_str()) {
            report_loss(
                report,
                format!(
                    "node {} dropped unsupported legacy attr {key}",
                    legacy_node.id
                ),
            );
        }
    }
}

fn migrate_legacy_transform(
    legacy_node: &crate::SceneElement,
    report: &mut LegacyMigrationReport,
) -> NodeTransform {
    let transform = &legacy_node.transform;
    NodeTransform {
        x: f64::from(transform.x),
        y: f64::from(transform.y),
        width: normalize_legacy_dimension(legacy_node, "width", transform.width, report),
        height: normalize_legacy_dimension(legacy_node, "height", transform.height, report),
        rotation: f64::from(transform.rotation),
    }
}

fn normalize_legacy_dimension(
    legacy_node: &crate::SceneElement,
    field: &str,
    value: f32,
    report: &mut LegacyMigrationReport,
) -> f64 {
    if value > 0.0 {
        f64::from(value)
    } else {
        report_loss(
            report,
            format!(
                "node {} normalized non-positive transform.{field} {value} to 1",
                legacy_node.id
            ),
        );
        1.0
    }
}

fn migrate_legacy_timeline(
    legacy_slide: &crate::LegacySlideModel,
    report: &mut LegacyMigrationReport,
) -> Timeline {
    let mut entries = legacy_slide
        .animations
        .iter()
        .enumerate()
        .filter_map(|(index, animation)| {
            let preset = match animation.effect.as_str() {
                "appear" => AnimationPreset::Appear,
                "fade" => AnimationPreset::Fade,
                "flyIn" | "fly-in" | "flyin" => AnimationPreset::FlyIn,
                "wipe" => AnimationPreset::Wipe,
                _ => {
                    report_loss(
                        report,
                        format!(
                            "animation {} on slide {} dropped unsupported effect {}",
                            animation.id, legacy_slide.id, animation.effect
                        ),
                    );
                    return None;
                }
            };
            report_loss(
                report,
                format!(
                    "animation {} on slide {} defaulted trigger to onClick because v4 had no trigger",
                    animation.id, legacy_slide.id
                ),
            );
            Some((animation.order, index, animation, preset))
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|(order, index, _, _)| (*order, *index));
    Timeline {
        entries: entries
            .into_iter()
            .enumerate()
            .map(|(index, (_, _, animation, preset))| AnimationEntry {
                id: animation.id.clone(),
                target_node_id: animation.target_element_id.clone(),
                trigger: AnimationTrigger::OnClick,
                preset,
                duration_ms: animation.duration_ms,
                delay_ms: 0,
                order_key: format!("{:08}", index),
            })
            .collect(),
    }
}

fn migrate_legacy_theme(
    legacy: Option<&crate::LegacyPresentationTheme>,
    report: &mut LegacyMigrationReport,
) -> DeckTheme {
    let Some(legacy) = legacy else {
        report_loss(
            report,
            "deck defaulted theme because v4 had no theme".to_string(),
        );
        return DeckTheme {
            id: "migrated-default-theme".into(),
            ..DeckTheme::default()
        };
    };
    let mut theme = DeckTheme {
        id: legacy.id.clone(),
        name: legacy.name.clone(),
        ..DeckTheme::default()
    };
    for (name, value) in &legacy.colors {
        let Some(token) = legacy_theme_color_token(name) else {
            report_loss(
                report,
                format!("theme dropped unsupported color token {name}"),
            );
            continue;
        };
        let Some(color) = parse_legacy_hex_color(value) else {
            report_loss(
                report,
                format!("theme dropped non-RGBA color {name}={value}"),
            );
            continue;
        };
        theme.colors.insert(token, color);
    }
    for (name, value) in &legacy.fonts {
        let token = match name.as_str() {
            "heading" => ThemeFontToken::Heading,
            "body" => ThemeFontToken::Body,
            _ => {
                report_loss(
                    report,
                    format!("theme dropped unsupported font token {name}"),
                );
                continue;
            }
        };
        theme.fonts.insert(token, value.clone());
    }
    theme
}

fn legacy_theme_color_token(name: &str) -> Option<ThemeColorToken> {
    match name {
        "background" => Some(ThemeColorToken::Background),
        "text" => Some(ThemeColorToken::Text),
        "accent1" => Some(ThemeColorToken::Accent1),
        "accent2" => Some(ThemeColorToken::Accent2),
        "accent3" => Some(ThemeColorToken::Accent3),
        "accent4" => Some(ThemeColorToken::Accent4),
        "accent5" => Some(ThemeColorToken::Accent5),
        "accent6" => Some(ThemeColorToken::Accent6),
        "hyperlink" => Some(ThemeColorToken::Hyperlink),
        "followedHyperlink" => Some(ThemeColorToken::FollowedHyperlink),
        _ => None,
    }
}

fn parse_legacy_hex_color(value: &str) -> Option<Rgba> {
    let value = value.strip_prefix('#')?;
    let expand = |nibble: u8| nibble.saturating_mul(17);
    match value.len() {
        3 => Some(Rgba {
            r: expand(u8::from_str_radix(&value[0..1], 16).ok()?),
            g: expand(u8::from_str_radix(&value[1..2], 16).ok()?),
            b: expand(u8::from_str_radix(&value[2..3], 16).ok()?),
            a: u8::MAX,
        }),
        6 | 8 => Some(Rgba {
            r: u8::from_str_radix(&value[0..2], 16).ok()?,
            g: u8::from_str_radix(&value[2..4], 16).ok()?,
            b: u8::from_str_radix(&value[4..6], 16).ok()?,
            a: if value.len() == 8 {
                u8::from_str_radix(&value[6..8], 16).ok()?
            } else {
                u8::MAX
            },
        }),
        _ => None,
    }
}

fn report_loss(report: &mut LegacyMigrationReport, detail: impl Into<String>) {
    report.losses.push(detail.into());
}

#[cfg(test)]
mod tests {
    use super::*;
    fn deck() -> Deck {
        Deck {
            page_spec: SlidePageSpec::default(),
            theme: DeckTheme {
                id: "theme-1".into(),
                ..DeckTheme::default()
            },
            assets: vec![AssetRef {
                asset_id: "asset-1".into(),
                digest: "sha256:abc".into(),
                mime_type: "image/png".into(),
                width: None,
                height: None,
                original_asset_id: None,
            }],
            masters: vec![],
            layouts: vec![],
            slides: vec![Slide {
                id: "slide-1".into(),
                order_key: "a".into(),
                name: "Slide 1".into(),
                layout_id: None,
                background: SlideBackground::None,
                notes: None,
                transition: None,
                timeline: Timeline::default(),
                nodes: vec![SceneNode {
                    id: "text-1".into(),
                    parent_id: None,
                    order_key: "a".into(),
                    name: None,
                    alt_text: None,
                    layout_placeholder_id: None,
                    transform: NodeTransform {
                        x: 0.0,
                        y: 0.0,
                        width: 10.0,
                        height: 10.0,
                        rotation: 0.0,
                    },
                    visible: true,
                    locked: false,
                    opacity: 1.0,
                    kind: SceneNodeKind::Text(TextNode {
                        frame: TextFrame {
                            body: PresentationRichText::default(),
                            vertical_align: TextVerticalAlign::Middle,
                            padding: Insets::default(),
                            auto_fit: TextAutoFit::None,
                        },
                    }),
                }],
            }],
        }
    }
    #[test]
    fn validates_strict_typed_deck() {
        deck().validate().unwrap();
    }
    #[test]
    fn rejects_node_cycles_and_dangling_assets() {
        let mut value = deck();
        value.slides[0].nodes[0].parent_id = Some("text-1".into());
        assert!(value.validate().is_err());
        let mut value = deck();
        value.slides[0].nodes[0].kind = SceneNodeKind::Image(ImageNode {
            asset_id: "lost".into(),
            original_asset_id: None,
            crop: ImageCrop::default(),
            flip_h: false,
            flip_v: false,
            caption: None,
        });
        assert!(value.validate().is_err());
    }

    #[test]
    fn rejects_invalid_typed_node_payloads_and_references() {
        let mut value = deck();
        value.slides[0].nodes[0].kind = SceneNodeKind::Text(TextNode {
            frame: TextFrame {
                body: PresentationRichText {
                    text: "Hello".into(),
                    runs: vec![PresentationTextRun {
                        start: 1,
                        end: 5,
                        style: PresentationTextStyle::default(),
                    }],
                    paragraphs: vec![PresentationParagraph {
                        start: 0,
                        end: 5,
                        alignment: TextHorizontalAlign::Left,
                        list: None,
                        indent_level: 0,
                    }],
                },
                vertical_align: TextVerticalAlign::Middle,
                padding: Insets::default(),
                auto_fit: TextAutoFit::None,
            },
        });
        assert!(value.validate().is_err());

        let mut value = deck();
        value.slides[0].nodes[0].kind = SceneNodeKind::Image(ImageNode {
            asset_id: "asset-1".into(),
            original_asset_id: None,
            crop: ImageCrop {
                left: 0.5,
                right: 0.5,
                ..ImageCrop::default()
            },
            flip_h: false,
            flip_v: false,
            caption: None,
        });
        assert!(value.validate().is_err());

        let mut value = deck();
        value.slides[0].nodes[0].kind = SceneNodeKind::Table(TableNode {
            rows: 1,
            columns: 2,
            cells: vec![TableCell {
                row: 0,
                column: 0,
                row_span: 1,
                column_span: 1,
                content: PresentationRichText::default(),
                style: TableCellStyle::default(),
            }],
        });
        assert!(value.validate().is_err());

        let mut value = deck();
        value.slides[0].timeline.entries.push(AnimationEntry {
            id: "animation-1".into(),
            target_node_id: "lost".into(),
            trigger: AnimationTrigger::OnClick,
            preset: AnimationPreset::Appear,
            duration_ms: 0,
            delay_ms: 0,
            order_key: "a".into(),
        });
        assert!(value.validate().is_err());
    }

    #[test]
    fn chart_specs_are_typed_and_validate_data_alignment() {
        let mut value = deck();
        value.slides[0].nodes[0].kind = SceneNodeKind::Chart(ChartNode {
            spec: ChartSpec {
                chart_type: ChartType::Column,
                title: Some("季度营收".into()),
                categories: vec!["Q1".into(), "Q2".into()],
                series: vec![ChartSeries {
                    name: "营收".into(),
                    values: vec![12.0, 18.0],
                    color: Some(ColorRef::Theme(ThemeColorToken::Accent1)),
                }],
            },
        });
        value.validate().expect("valid ChartSpec");

        {
            let SceneNodeKind::Chart(chart) = &mut value.slides[0].nodes[0].kind else {
                panic!("expected chart");
            };
            chart.spec.series[0].values.pop();
        }
        assert!(value.validate().is_err());

        {
            let SceneNodeKind::Chart(chart) = &mut value.slides[0].nodes[0].kind else {
                panic!("expected chart");
            };
            chart.spec.series[0].values.push(18.0);
            chart.spec.chart_type = ChartType::Pie;
            chart.spec.series.push(ChartSeries {
                name: "成本".into(),
                values: vec![5.0, 8.0],
                color: None,
            });
        }
        assert!(value.validate().is_err());
    }

    #[test]
    fn rejects_generic_node_fields_during_deserialization() {
        let mut fixture: Value = serde_json::from_str(include_str!(
            "../../../fixtures/presentation/v5/minimal-deck.json"
        ))
        .expect("shared fixture is JSON");
        fixture["slides"][0]["nodes"][0]["attrs"] = serde_json::json!({});
        assert!(serde_json::from_value::<Deck>(fixture).is_err());
    }

    #[test]
    fn extension_nodes_require_a_versioned_object_payload() {
        let mut value = deck();
        value.slides[0].nodes[0].kind = SceneNodeKind::Extension(ExtensionNode {
            namespace: "com.example.widget".into(),
            version: "1".into(),
            type_id: "widget".into(),
            data: BTreeMap::from([("answer".into(), serde_json::json!(42))]),
        });
        value.validate().expect("strict extension payload is valid");

        let serialized = serde_json::to_value(&value).expect("deck serializes");
        assert_eq!(
            serialized["slides"][0]["nodes"][0]["kind"]["data"]["typeId"],
            "widget"
        );

        let mut invalid = value;
        let SceneNodeKind::Extension(extension) = &mut invalid.slides[0].nodes[0].kind else {
            panic!("expected extension");
        };
        extension.type_id.clear();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn legacy_migration_preserves_unknown_nodes_as_extensions() {
        let legacy = LegacyPresentationModel {
            slides: vec![crate::LegacySlideModel {
                id: "slide".into(),
                name: "legacy".into(),
                elements: vec![crate::SceneElement {
                    id: "legacy-1".into(),
                    type_id: "custom.widget".into(),
                    transform: crate::Transform {
                        width: 10.0,
                        height: 10.0,
                        ..crate::Transform::default()
                    },
                    attrs: Default::default(),
                    children: vec![],
                }],
                notes: None,
                animations: vec![],
            }],
            theme: None,
        };
        let (deck, report) = migrate_legacy_v4(&legacy).unwrap();
        assert!(matches!(
            deck.slides[0].nodes[0].kind,
            SceneNodeKind::Extension(_)
        ));
        assert_eq!(report.preserved_extensions, ["legacy-1"]);
    }

    #[test]
    fn shared_minimal_fixture_is_valid() {
        let deck: Deck = serde_json::from_str(include_str!(
            "../../../fixtures/presentation/v5/minimal-deck.json"
        ))
        .expect("shared v5 fixture must deserialize");
        deck.validate().expect("shared v5 fixture must validate");
    }
}
