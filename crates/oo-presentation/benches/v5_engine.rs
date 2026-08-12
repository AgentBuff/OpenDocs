//! Mutation-journal smoke benchmark for the v5 Presentation engine.
//!
//! Run with `cargo bench -p oo-presentation --bench v5_engine`. It exercises a single transform
//! on a 2k-node deck and immediately undoes it, so a regression to whole-deck snapshot history is
//! visible in the measured hot path.

use std::hint::black_box;
use std::time::{Duration, Instant};

use oo_presentation::{PresentationCommand, PresentationCommandBatch, PresentationEngine};
use oo_schema::presentation_v5::{
    Deck, DeckTheme, NodeTransform, SceneNode, SceneNodeKind, ShapeGeometry, ShapeNode, ShapeStyle,
    Slide, SlideBackground, SlidePageSpec, Timeline,
};

const NODES: usize = 2_000;
const SLIDES: usize = 20;
const ITERATIONS: usize = 100;

fn node(slide_index: usize, node_index: usize) -> SceneNode {
    SceneNode {
        id: format!("node-{slide_index}-{node_index}"),
        parent_id: None,
        order_key: format!("{node_index:06}"),
        name: None,
        alt_text: None,
        layout_placeholder_id: None,
        transform: NodeTransform {
            x: node_index as f64,
            y: 0.0,
            width: 100.0,
            height: 100.0,
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

fn deck() -> Deck {
    let nodes_per_slide = NODES / SLIDES;
    Deck {
        page_spec: SlidePageSpec::default(),
        slides: (0..SLIDES)
            .map(|slide_index| Slide {
                id: format!("slide-{slide_index}"),
                order_key: format!("{slide_index:06}"),
                name: String::new(),
                layout_id: None,
                background: SlideBackground::None,
                notes: None,
                transition: None,
                nodes: (0..nodes_per_slide)
                    .map(|node_index| node(slide_index, node_index))
                    .collect(),
                timeline: Timeline::default(),
            })
            .collect(),
        masters: vec![],
        layouts: vec![],
        theme: DeckTheme {
            id: "theme".into(),
            ..DeckTheme::default()
        },
        assets: vec![],
    }
}

fn per_operation(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0 / ITERATIONS as f64
}

fn main() {
    let mut engine = PresentationEngine::new(deck(), 0).expect("valid benchmark deck");
    let started = Instant::now();
    for iteration in 0..ITERATIONS {
        let changes = engine
            .execute(PresentationCommandBatch {
                base_revision: engine.revision(),
                commands: vec![PresentationCommand::SetNodeTransform {
                    slide_id: "slide-0".into(),
                    node_id: "node-0-0".into(),
                    transform: NodeTransform {
                        x: iteration as f64,
                        y: 0.0,
                        width: 100.0,
                        height: 100.0,
                        rotation: 0.0,
                    },
                }],
            })
            .expect("transform command");
        black_box(changes);
        black_box(engine.undo(engine.revision()).expect("undo command"));
    }
    println!(
        "{{\n  \"nodes\": {NODES},\n  \"slides\": {SLIDES},\n  \"iterations\": {ITERATIONS},\n  \"transformPlusUndoMs\": {:.4}\n}}",
        per_operation(started.elapsed()),
    );
}
