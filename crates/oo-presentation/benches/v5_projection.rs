//! Dependency-free smoke benchmark for the v5 Deck read projection.
//!
//! Run with `cargo bench -p oo-presentation --bench v5_projection`. The default is a 2k-node
//! deck, matching the P2 baseline. This is evidence collection rather than a CI timing gate:
//! build machines differ, while the structural regression tests enforce the same input shape.

use std::hint::black_box;
use std::time::{Duration, Instant};

use oo_presentation::v5_projection::{DeckProjection, NodeRef, ProjectionChange};
use oo_schema::presentation_v5::{
    Deck, DeckTheme, NodeTransform, SceneNode, SceneNodeKind, ShapeGeometry, ShapeNode, ShapeStyle,
    Slide, SlideBackground, SlidePageSpec, Timeline,
};

const DEFAULT_NODES: usize = 2_000;
const DEFAULT_SLIDES: usize = 20;
const DEFAULT_ITERATIONS: usize = 20;

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value: &usize| *value > 0)
        .unwrap_or(default)
}

fn node(slide_index: usize, node_index: usize) -> SceneNode {
    SceneNode {
        id: format!("node-{slide_index}-{node_index}"),
        parent_id: None,
        order_key: format!("{node_index:06}"),
        name: None,
        alt_text: None,
        layout_placeholder_id: None,
        transform: NodeTransform {
            x: node_index as f64 * 10.0,
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

fn deck(node_count: usize, requested_slides: usize) -> Deck {
    let slide_count = requested_slides.min(node_count).max(1);
    let base = node_count / slide_count;
    let extra = node_count % slide_count;
    let slides = (0..slide_count)
        .map(|slide_index| {
            let nodes_in_slide = base + usize::from(slide_index < extra);
            Slide {
                id: format!("slide-{slide_index}"),
                order_key: format!("{slide_index:06}"),
                name: String::new(),
                layout_id: None,
                background: SlideBackground::None,
                notes: None,
                transition: None,
                timeline: Timeline::default(),
                nodes: (0..nodes_in_slide)
                    .map(|node_index| node(slide_index, node_index))
                    .collect(),
            }
        })
        .collect();
    Deck {
        page_spec: SlidePageSpec::default(),
        slides,
        masters: vec![],
        layouts: vec![],
        theme: DeckTheme {
            id: "theme".into(),
            ..DeckTheme::default()
        },
        assets: vec![],
    }
}

fn timed<F>(iterations: usize, mut operation: F) -> Duration
where
    F: FnMut(),
{
    for _ in 0..iterations.min(2) {
        operation();
    }
    let started = Instant::now();
    for _ in 0..iterations {
        operation();
    }
    started.elapsed()
}

fn per_operation(duration: Duration, iterations: usize) -> f64 {
    duration.as_secs_f64() * 1_000.0 / iterations as f64
}

fn main() {
    let nodes = env_usize("OO_BENCH_PRESENTATION_NODES", DEFAULT_NODES);
    let slides = env_usize("OO_BENCH_PRESENTATION_SLIDES", DEFAULT_SLIDES);
    let iterations = env_usize("OO_BENCH_ITERATIONS", DEFAULT_ITERATIONS);
    let snapshot = deck(nodes, slides);
    let build_duration = timed(iterations, || {
        black_box(DeckProjection::new(&snapshot).expect("valid benchmark deck"));
    });
    let projection = DeckProjection::new(&snapshot).expect("valid benchmark deck");
    let lookup_duration = timed(iterations, || {
        black_box(
            projection
                .node_location_in_slide("slide-0", "node-0-0")
                .expect("indexed node"),
        );
    });
    let invalidation_duration = timed(iterations, || {
        black_box(projection.invalidate(&[ProjectionChange::NodeChanged {
            node: NodeRef::new("slide-0", "node-0-0"),
        }]));
    });

    println!(
        "{{\n  \"nodes\": {nodes},\n  \"slides\": {slides},\n  \"iterations\": {iterations},\n  \"projectionBuildMs\": {:.4},\n  \"stableIdLookupMs\": {:.6},\n  \"singleNodeInvalidationMs\": {:.6}\n}}",
        per_operation(build_duration, iterations),
        per_operation(lookup_duration, iterations),
        per_operation(invalidation_duration, iterations),
    );
}
