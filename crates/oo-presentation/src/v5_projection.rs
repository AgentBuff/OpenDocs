//! Borrowed v5 Deck projection/index.
//!
//! This is the common read boundary for a later transaction engine, renderer and agent SDK.
//! It indexes immutable canonical data and does not retain selection, pointer or Canvas state.

use std::collections::{BTreeSet, HashMap};

use oo_schema::presentation_v5::{ConnectorEndpoint, Deck, SceneNode, SceneNodeKind, Slide};
use oo_schema::SchemaValidationError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeLocation {
    pub slide_index: usize,
    pub node_index: usize,
}

/// A stable node identity is scoped to a slide. The v5 schema intentionally permits two
/// independent slides to use the same node ID, so a renderer or Agent must never use a bare
/// node ID as an addressing key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeRef {
    pub slide_id: String,
    pub node_id: String,
}

impl NodeRef {
    pub fn new(slide_id: impl Into<String>, node_id: impl Into<String>) -> Self {
        Self {
            slide_id: slide_id.into(),
            node_id: node_id.into(),
        }
    }
}

/// The role an asset plays in a node. It deliberately describes canonical references only;
/// decoded bitmap, video frame and font caches remain renderer-local state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetUseRole {
    Image,
    ImageOriginal,
    Video,
    Audio,
    MediaPoster,
    EmbedPoster,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetUse {
    pub location: NodeLocation,
    pub role: AssetUseRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimelineLocation {
    pub slide_index: usize,
    pub entry_index: usize,
}

/// Immutable input from a future semantic command ChangeSet. This is intentionally not a
/// command API: projection invalidation must not become a second write path for the Deck.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionChange {
    DeckThemeChanged,
    MasterChanged { master_id: String },
    LayoutChanged { layout_id: String },
    AssetChanged { asset_id: String },
    SlideChanged { slide_id: String },
    SlideRemoved { slide_id: String },
    NodeChanged { node: NodeRef },
    NodeRemoved { node: NodeRef },
    TimelineChanged { slide_id: String },
}

/// Renderer-neutral invalidation plan derived from canonical references. A canvas, DOM overlay
/// or thumbnail service consumes this plan, but cannot write it back into the immutable Deck.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeckInvalidation {
    pub dirty_slides: BTreeSet<String>,
    pub dirty_nodes: BTreeSet<NodeRef>,
    pub removed_nodes: BTreeSet<NodeRef>,
    pub dirty_thumbnails: BTreeSet<String>,
    pub dirty_timeline_slides: BTreeSet<String>,
    pub dirty_layout_slides: BTreeSet<String>,
    pub theme_changed: bool,
}

impl DeckInvalidation {
    pub fn is_empty(&self) -> bool {
        self.dirty_slides.is_empty()
            && self.dirty_nodes.is_empty()
            && self.removed_nodes.is_empty()
            && self.dirty_thumbnails.is_empty()
            && self.dirty_timeline_slides.is_empty()
            && self.dirty_layout_slides.is_empty()
            && !self.theme_changed
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DeckProjectionError {
    #[error("v5 deck 无效：{0}")]
    Invalid(#[from] SchemaValidationError),
    #[error("不存在的 slide：{0}")]
    MissingSlide(String),
    #[error("不存在的 node：{0}")]
    MissingNode(String),
    #[error("node ID {0} 在多个 slide 中出现；请使用 slide-scoped 查询")]
    AmbiguousNode(String),
}

/// O(slides + nodes) construction; every subsequent ID lookup is O(1). References are borrowed
/// from the Deck, so a projection cannot accidentally become a writable clone of it.
pub struct DeckProjection<'a> {
    deck: &'a Deck,
    slides: HashMap<&'a str, usize>,
    nodes: HashMap<(&'a str, &'a str), NodeLocation>,
    nodes_by_id: HashMap<&'a str, Vec<NodeLocation>>,
    children: HashMap<(&'a str, Option<&'a str>), Vec<usize>>,
    asset_uses: HashMap<&'a str, Vec<AssetUse>>,
    connector_targets: HashMap<NodeRef, Vec<NodeLocation>>,
    timeline_targets: HashMap<NodeRef, Vec<TimelineLocation>>,
    layout_slides: HashMap<&'a str, Vec<usize>>,
    master_layouts: HashMap<&'a str, Vec<usize>>,
}

impl<'a> DeckProjection<'a> {
    pub fn new(deck: &'a Deck) -> Result<Self, DeckProjectionError> {
        deck.validate()?;
        let mut slides = HashMap::with_capacity(deck.slides.len());
        let mut nodes = HashMap::new();
        let mut nodes_by_id: HashMap<&str, Vec<NodeLocation>> = HashMap::new();
        let mut children = HashMap::new();
        let mut asset_uses = HashMap::new();
        let mut connector_targets = HashMap::new();
        let mut timeline_targets = HashMap::new();
        let mut layout_slides = HashMap::new();
        let mut master_layouts = HashMap::new();
        for (layout_index, layout) in deck.layouts.iter().enumerate() {
            master_layouts
                .entry(layout.master_id.as_str())
                .or_insert_with(Vec::new)
                .push(layout_index);
        }
        for (slide_index, slide) in deck.slides.iter().enumerate() {
            slides.insert(slide.id.as_str(), slide_index);
            if let Some(layout_id) = slide.layout_id.as_deref() {
                layout_slides
                    .entry(layout_id)
                    .or_insert_with(Vec::new)
                    .push(slide_index);
            }
            for (node_index, node) in slide.nodes.iter().enumerate() {
                let location = NodeLocation {
                    slide_index,
                    node_index,
                };
                nodes.insert((slide.id.as_str(), node.id.as_str()), location);
                nodes_by_id
                    .entry(node.id.as_str())
                    .or_default()
                    .push(location);
                children
                    .entry((slide.id.as_str(), node.parent_id.as_deref()))
                    .or_insert_with(Vec::new)
                    .push(node_index);
                index_node_references(
                    slide,
                    node,
                    location,
                    &mut asset_uses,
                    &mut connector_targets,
                );
            }
            for (entry_index, entry) in slide.timeline.entries.iter().enumerate() {
                timeline_targets
                    .entry(NodeRef::new(&slide.id, &entry.target_node_id))
                    .or_insert_with(Vec::new)
                    .push(TimelineLocation {
                        slide_index,
                        entry_index,
                    });
            }
        }
        for ((slide_id, _), positions) in &mut children {
            positions.sort_unstable_by(|left, right| {
                deck.slides[*slides.get(slide_id).expect("indexed slide")].nodes[*left]
                    .order_key
                    .cmp(
                        &deck.slides[*slides.get(slide_id).expect("indexed slide")].nodes[*right]
                            .order_key,
                    )
            });
        }
        Ok(Self {
            deck,
            slides,
            nodes,
            nodes_by_id,
            children,
            asset_uses,
            connector_targets,
            timeline_targets,
            layout_slides,
            master_layouts,
        })
    }

    pub fn slide(&self, id: &str) -> Result<&'a Slide, DeckProjectionError> {
        self.slides
            .get(id)
            .map(|index| &self.deck.slides[*index])
            .ok_or_else(|| DeckProjectionError::MissingSlide(id.into()))
    }

    pub fn node(&self, id: &str) -> Result<&'a SceneNode, DeckProjectionError> {
        let locations = self
            .nodes_by_id
            .get(id)
            .ok_or_else(|| DeckProjectionError::MissingNode(id.into()))?;
        if locations.len() != 1 {
            return Err(DeckProjectionError::AmbiguousNode(id.into()));
        }
        Ok(self.node_at(locations[0]))
    }

    pub fn node_location(&self, id: &str) -> Result<NodeLocation, DeckProjectionError> {
        let locations = self
            .nodes_by_id
            .get(id)
            .ok_or_else(|| DeckProjectionError::MissingNode(id.into()))?;
        if locations.len() != 1 {
            return Err(DeckProjectionError::AmbiguousNode(id.into()));
        }
        Ok(locations[0])
    }

    /// Stable renderer/Agent lookup. Node identity is always `(slideId, nodeId)`.
    pub fn node_in_slide(
        &self,
        slide_id: &str,
        node_id: &str,
    ) -> Result<&'a SceneNode, DeckProjectionError> {
        self.node_location_in_slide(slide_id, node_id)
            .map(|location| self.node_at(location))
    }

    pub fn node_location_in_slide(
        &self,
        slide_id: &str,
        node_id: &str,
    ) -> Result<NodeLocation, DeckProjectionError> {
        self.nodes
            .get(&(slide_id, node_id))
            .copied()
            .ok_or_else(|| DeckProjectionError::MissingNode(format!("{slide_id}/{node_id}")))
    }

    /// Child order is derived from `parentId + orderKey`; no persisted children list is read.
    pub fn children(
        &self,
        slide_id: &str,
        parent_id: Option<&str>,
    ) -> Result<Vec<&'a SceneNode>, DeckProjectionError> {
        let slide = self.slide(slide_id)?;
        Ok(self
            .children
            .get(&(slide_id, parent_id))
            .into_iter()
            .flatten()
            .map(|index| &slide.nodes[*index])
            .collect())
    }

    pub fn root_nodes(&self, slide_id: &str) -> Result<Vec<&'a SceneNode>, DeckProjectionError> {
        self.children(slide_id, None)
    }

    pub fn asset_uses(&self, asset_id: &str) -> &[AssetUse] {
        self.asset_uses
            .get(asset_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn connectors_targeting(&self, node: &NodeRef) -> &[NodeLocation] {
        self.connector_targets
            .get(node)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn timeline_entries_targeting(&self, node: &NodeRef) -> &[TimelineLocation] {
        self.timeline_targets
            .get(node)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn slides_using_layout(&self, layout_id: &str) -> Vec<&'a Slide> {
        self.layout_slides
            .get(layout_id)
            .into_iter()
            .flatten()
            .map(|index| &self.deck.slides[*index])
            .collect()
    }

    pub fn slides_using_master(&self, master_id: &str) -> Vec<&'a Slide> {
        self.master_layouts
            .get(master_id)
            .into_iter()
            .flatten()
            .flat_map(|layout_index| self.slides_using_layout(&self.deck.layouts[*layout_index].id))
            .collect()
    }

    /// Expands a semantic ChangeSet into deterministic, renderer-neutral invalidation. It is
    /// safe to call against the pre-transaction projection for removals, because reverse
    /// references describe the canonical state immediately before the change.
    pub fn invalidate(&self, changes: &[ProjectionChange]) -> DeckInvalidation {
        let mut result = DeckInvalidation::default();
        for change in changes {
            match change {
                ProjectionChange::DeckThemeChanged => {
                    result.theme_changed = true;
                    for slide in &self.deck.slides {
                        mark_slide(&mut result, &slide.id);
                    }
                }
                ProjectionChange::MasterChanged { master_id } => {
                    for slide in self.slides_using_master(master_id) {
                        result.dirty_layout_slides.insert(slide.id.clone());
                        mark_slide(&mut result, &slide.id);
                    }
                }
                ProjectionChange::LayoutChanged { layout_id } => {
                    for slide in self.slides_using_layout(layout_id) {
                        result.dirty_layout_slides.insert(slide.id.clone());
                        mark_slide(&mut result, &slide.id);
                    }
                }
                ProjectionChange::AssetChanged { asset_id } => {
                    for usage in self.asset_uses(asset_id) {
                        self.mark_location_dirty(&mut result, usage.location);
                    }
                }
                ProjectionChange::SlideChanged { slide_id } => mark_slide(&mut result, slide_id),
                ProjectionChange::SlideRemoved { slide_id } => {
                    result.dirty_slides.insert(slide_id.clone());
                    result.dirty_thumbnails.insert(slide_id.clone());
                }
                ProjectionChange::NodeChanged { node } => self.mark_node_dirty(&mut result, node),
                ProjectionChange::NodeRemoved { node } => {
                    result.removed_nodes.insert(node.clone());
                    self.mark_node_dirty(&mut result, node);
                }
                ProjectionChange::TimelineChanged { slide_id } => {
                    result.dirty_timeline_slides.insert(slide_id.clone());
                    mark_slide(&mut result, slide_id);
                }
            }
        }
        result
    }

    fn node_at(&self, location: NodeLocation) -> &'a SceneNode {
        &self.deck.slides[location.slide_index].nodes[location.node_index]
    }

    fn mark_location_dirty(&self, result: &mut DeckInvalidation, location: NodeLocation) {
        let slide = &self.deck.slides[location.slide_index];
        let node = &slide.nodes[location.node_index];
        self.mark_node_dirty(result, &NodeRef::new(&slide.id, &node.id));
    }

    fn mark_node_dirty(&self, result: &mut DeckInvalidation, node: &NodeRef) {
        result.dirty_nodes.insert(node.clone());
        mark_slide(result, &node.slide_id);
        for connector in self.connectors_targeting(node) {
            let connector_slide = &self.deck.slides[connector.slide_index];
            result.dirty_nodes.insert(NodeRef::new(
                &connector_slide.id,
                &connector_slide.nodes[connector.node_index].id,
            ));
            mark_slide(result, &connector_slide.id);
        }
        for entry in self.timeline_entries_targeting(node) {
            let slide = &self.deck.slides[entry.slide_index];
            result.dirty_timeline_slides.insert(slide.id.clone());
            mark_slide(result, &slide.id);
        }
    }
}

fn mark_slide(result: &mut DeckInvalidation, slide_id: &str) {
    result.dirty_slides.insert(slide_id.into());
    result.dirty_thumbnails.insert(slide_id.into());
}

fn index_node_references<'a>(
    slide: &'a Slide,
    node: &'a SceneNode,
    location: NodeLocation,
    asset_uses: &mut HashMap<&'a str, Vec<AssetUse>>,
    connector_targets: &mut HashMap<NodeRef, Vec<NodeLocation>>,
) {
    let mut asset = |asset_id: &'a str, role| {
        asset_uses
            .entry(asset_id)
            .or_default()
            .push(AssetUse { location, role });
    };
    match &node.kind {
        SceneNodeKind::Image(image) => {
            asset(&image.asset_id, AssetUseRole::Image);
            if let Some(original) = image.original_asset_id.as_deref() {
                asset(original, AssetUseRole::ImageOriginal);
            }
        }
        SceneNodeKind::Video(media) => {
            asset(&media.asset_id, AssetUseRole::Video);
            if let Some(poster) = media.poster_asset_id.as_deref() {
                asset(poster, AssetUseRole::MediaPoster);
            }
        }
        SceneNodeKind::Audio(media) => {
            asset(&media.asset_id, AssetUseRole::Audio);
            if let Some(poster) = media.poster_asset_id.as_deref() {
                asset(poster, AssetUseRole::MediaPoster);
            }
        }
        SceneNodeKind::Embed(embed) => {
            if let Some(poster) = embed.poster_asset_id.as_deref() {
                asset(poster, AssetUseRole::EmbedPoster);
            }
        }
        SceneNodeKind::Connector(connector) => {
            for endpoint in [&connector.start, &connector.end] {
                if let ConnectorEndpoint::Node { node_id, .. } = endpoint {
                    connector_targets
                        .entry(NodeRef::new(&slide.id, node_id))
                        .or_default()
                        .push(location);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oo_schema::presentation_v5::{
        Anchor, AnimationEntry, AnimationPreset, AnimationTrigger, AssetRef, ConnectorEndpoint,
        ConnectorNode, Deck, DeckTheme, ImageNode, NodeTransform, SceneNode, SceneNodeKind,
        ShapeGeometry, ShapeNode, ShapeStyle, Slide, SlideBackground, SlidePageSpec, Timeline,
    };

    fn deck() -> Deck {
        Deck {
            page_spec: SlidePageSpec::default(),
            theme: DeckTheme {
                id: "theme".into(),
                ..DeckTheme::default()
            },
            masters: vec![],
            layouts: vec![],
            assets: vec![],
            slides: vec![Slide {
                id: "slide".into(),
                order_key: "a".into(),
                name: String::new(),
                layout_id: None,
                background: SlideBackground::None,
                notes: None,
                transition: None,
                timeline: Timeline::default(),
                nodes: vec![
                    node("parent", None, "b"),
                    node("child", Some("parent"), "a"),
                    node("root", None, "a"),
                ],
            }],
        }
    }
    fn node(id: &str, parent_id: Option<&str>, order_key: &str) -> SceneNode {
        SceneNode {
            id: id.into(),
            parent_id: parent_id.map(str::to_owned),
            order_key: order_key.into(),
            name: None,
            alt_text: None,
            layout_placeholder_id: None,
            transform: NodeTransform {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
                rotation: 0.0,
            },
            visible: true,
            locked: false,
            opacity: 1.0,
            kind: SceneNodeKind::Shape(ShapeNode {
                geometry: ShapeGeometry::Rectangle,
                style: ShapeStyle::default(),
            }),
        }
    }

    fn dependency_deck() -> Deck {
        let mut model = deck();
        model.assets.push(AssetRef {
            asset_id: "image-asset".into(),
            digest: "sha256:image".into(),
            mime_type: "image/png".into(),
            width: Some(32),
            height: Some(32),
            original_asset_id: None,
        });
        let slide = &mut model.slides[0];
        slide.nodes.push(SceneNode {
            id: "image".into(),
            parent_id: None,
            order_key: "c".into(),
            name: None,
            alt_text: None,
            layout_placeholder_id: None,
            transform: NodeTransform {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
                rotation: 0.0,
            },
            visible: true,
            locked: false,
            opacity: 1.0,
            kind: SceneNodeKind::Image(ImageNode {
                asset_id: "image-asset".into(),
                original_asset_id: None,
                crop: Default::default(),
                flip_h: false,
                flip_v: false,
                caption: None,
            }),
        });
        slide.nodes.push(SceneNode {
            id: "connector".into(),
            parent_id: None,
            order_key: "d".into(),
            name: None,
            alt_text: None,
            layout_placeholder_id: None,
            transform: NodeTransform {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
                rotation: 0.0,
            },
            visible: true,
            locked: false,
            opacity: 1.0,
            kind: SceneNodeKind::Connector(ConnectorNode {
                start: ConnectorEndpoint::Node {
                    node_id: "root".into(),
                    anchor: Anchor::Right,
                },
                end: ConnectorEndpoint::Node {
                    node_id: "parent".into(),
                    anchor: Anchor::Left,
                },
            }),
        });
        slide.timeline.entries.push(AnimationEntry {
            id: "entry".into(),
            target_node_id: "root".into(),
            trigger: AnimationTrigger::OnClick,
            preset: AnimationPreset::Appear,
            duration_ms: 100,
            delay_ms: 0,
            order_key: "a".into(),
        });
        model
    }
    #[test]
    fn indexes_nodes_without_cloning_and_derives_child_order() {
        let model = deck();
        let projection = DeckProjection::new(&model).unwrap();
        assert_eq!(projection.node_location("child").unwrap().node_index, 1);
        assert_eq!(
            projection
                .root_nodes("slide")
                .unwrap()
                .into_iter()
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>(),
            vec!["root", "parent"]
        );
        assert_eq!(
            projection.children("slide", Some("parent")).unwrap()[0].id,
            "child"
        );
    }

    #[test]
    fn resolves_asset_connector_and_timeline_dependents_without_renderer_state() {
        let model = dependency_deck();
        let projection = DeckProjection::new(&model).unwrap();
        let root = NodeRef::new("slide", "root");

        assert_eq!(projection.asset_uses("image-asset").len(), 1);
        assert_eq!(
            projection.asset_uses("image-asset")[0].role,
            AssetUseRole::Image
        );
        assert_eq!(projection.connectors_targeting(&root).len(), 1);
        assert_eq!(projection.timeline_entries_targeting(&root).len(), 1);

        let invalidation = projection.invalidate(&[
            ProjectionChange::NodeChanged { node: root.clone() },
            ProjectionChange::AssetChanged {
                asset_id: "image-asset".into(),
            },
        ]);
        assert!(invalidation.dirty_slides.contains("slide"));
        assert!(invalidation.dirty_thumbnails.contains("slide"));
        assert!(invalidation.dirty_timeline_slides.contains("slide"));
        assert!(invalidation.dirty_nodes.contains(&root));
        assert!(invalidation
            .dirty_nodes
            .contains(&NodeRef::new("slide", "connector")));
        assert!(invalidation
            .dirty_nodes
            .contains(&NodeRef::new("slide", "image")));
    }

    #[test]
    fn requires_slide_scoped_lookup_when_node_ids_repeat_across_slides() {
        let mut model = deck();
        model.slides.push(Slide {
            id: "slide-2".into(),
            order_key: "b".into(),
            name: String::new(),
            layout_id: None,
            background: SlideBackground::None,
            notes: None,
            transition: None,
            timeline: Timeline::default(),
            nodes: vec![node("root", None, "a")],
        });
        let projection = DeckProjection::new(&model).unwrap();

        assert!(matches!(
            projection.node("root"),
            Err(DeckProjectionError::AmbiguousNode(_))
        ));
        assert_eq!(
            projection.node_in_slide("slide-2", "root").unwrap().id,
            "root"
        );
    }

    #[test]
    fn builds_a_2k_node_read_index_with_stable_lookup_contract() {
        let mut model = deck();
        model.slides.clear();
        for slide_index in 0..20 {
            let id = format!("slide-{slide_index}");
            let nodes = (0..100)
                .map(|node_index| {
                    node(
                        &format!("node-{slide_index}-{node_index}"),
                        None,
                        &format!("{node_index:04}"),
                    )
                })
                .collect();
            model.slides.push(Slide {
                id: id.clone(),
                order_key: format!("{slide_index:04}"),
                name: String::new(),
                layout_id: None,
                background: SlideBackground::None,
                notes: None,
                transition: None,
                timeline: Timeline::default(),
                nodes,
            });
        }
        let projection = DeckProjection::new(&model).unwrap();
        assert_eq!(projection.root_nodes("slide-19").unwrap().len(), 100);
        assert_eq!(
            projection
                .node_location_in_slide("slide-10", "node-10-50")
                .unwrap(),
            NodeLocation {
                slide_index: 10,
                node_index: 50,
            }
        );
    }
}
