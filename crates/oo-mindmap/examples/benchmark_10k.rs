//! Reproducible release harness for P2D-04.
//!
//! Run with `cargo run --release -p oo-mindmap --example benchmark_10k`.
//! Wall-clock samples are evidence, not CI correctness thresholds; unit tests
//! own deterministic node/route/recomputation-count assertions.

use std::process::Command;
use std::time::Instant;

use oo_mindmap::{layout, route_edges, MindmapLayoutOptions, MindmapProjection, MindmapTheme};
use oo_schema::{
    MindmapBoundary, MindmapEdge, MindmapFormula, MindmapFormulaDisplay, MindmapImage,
    MindmapModel, MindmapNode, MindmapSummary, RichText,
};
use serde::Serialize;

const NODE_COUNT: usize = 10_000;
const SAMPLES: usize = 21;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Stats {
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    dataset: &'static str,
    nodes: usize,
    routes: usize,
    model_json_bytes: usize,
    projection_json_bytes: usize,
    process_rss_kib: Option<u64>,
    layout: Stats,
    route: Stats,
    projection: Stats,
}

fn main() {
    let reports = ["wide", "deep", "mixed", "explicitEdges", "imagesAdvanced"]
        .map(|kind| benchmark(kind, fixture(kind)));
    println!("{}", serde_json::to_string_pretty(&reports).unwrap());
}

fn benchmark(dataset: &'static str, model: MindmapModel) -> Report {
    let options = MindmapLayoutOptions::default();
    let baseline = MindmapProjection::build(&model, options, MindmapTheme::Light).unwrap();
    let layout_stats = samples(|| {
        std::hint::black_box(layout(std::hint::black_box(&model), options).unwrap());
    });
    let fixed_layout = layout(&model, options).unwrap();
    let route_stats = samples(|| {
        std::hint::black_box(route_edges(std::hint::black_box(&model), &fixed_layout).unwrap());
    });
    let projection_stats = samples(|| {
        std::hint::black_box(
            MindmapProjection::build(std::hint::black_box(&model), options, MindmapTheme::Light)
                .unwrap(),
        );
    });
    Report {
        dataset,
        nodes: baseline.layout.nodes.len(),
        routes: baseline.edges.routes.len(),
        model_json_bytes: serde_json::to_vec(&model).unwrap().len(),
        projection_json_bytes: serde_json::to_vec(&baseline).unwrap().len(),
        process_rss_kib: process_rss_kib(),
        layout: layout_stats,
        route: route_stats,
        projection: projection_stats,
    }
}

fn samples(mut operation: impl FnMut()) -> Stats {
    let mut values = (0..SAMPLES)
        .map(|_| {
            let started = Instant::now();
            operation();
            started.elapsed().as_secs_f64() * 1_000.0
        })
        .collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    Stats {
        p50_ms: percentile(&values, 0.50),
        p95_ms: percentile(&values, 0.95),
        p99_ms: percentile(&values, 0.99),
    }
}

fn percentile(values: &[f64], percentile: f64) -> f64 {
    values[((values.len() - 1) as f64 * percentile).ceil() as usize]
}

fn fixture(kind: &str) -> MindmapModel {
    let mut nodes = Vec::with_capacity(NODE_COUNT);
    for index in 0..NODE_COUNT {
        let parent = match (kind, index) {
            (_, 0) => None,
            ("wide", _) | ("explicitEdges", _) | ("imagesAdvanced", _) => Some(0),
            ("deep", _) => Some(index - 1 - ((index - 1) % 250 == 249) as usize * 249),
            ("mixed", _) if index < 100 => Some(0),
            ("mixed", _) => Some(1 + (index - 100) % 99),
            _ => Some(0),
        };
        let mut node = MindmapNode {
            id: format!("node-{index}"),
            parent_id: parent.map(|value| format!("node-{value}")),
            content: Some(text(&format!("Topic {index}"))),
            ..Default::default()
        };
        if kind == "imagesAdvanced" && index % 100 == 0 {
            node.supplement.image = Some(MindmapImage {
                asset_id: format!("asset-{index}"),
                alt: format!("Image {index}"),
                width: Some(96.0),
                height: Some(64.0),
            });
        }
        nodes.push(node);
    }
    let mut model = MindmapModel {
        root: Some("node-0".into()),
        nodes,
        ..Default::default()
    };
    if kind == "explicitEdges" {
        model.edges = (1..NODE_COUNT)
            .step_by(2)
            .map(|index| MindmapEdge {
                id: format!("edge-{index}"),
                source_id: format!("node-{index}"),
                target_id: format!("node-{}", (index + 101) % NODE_COUNT),
                ..Default::default()
            })
            .collect();
    }
    if kind == "imagesAdvanced" {
        model.summaries.push(MindmapSummary {
            id: "summary-all".into(),
            start_node_id: "node-1".into(),
            end_node_id: format!("node-{}", NODE_COUNT - 1),
            content: text("All topics"),
        });
        model.boundaries.push(MindmapBoundary {
            id: "boundary".into(),
            root_node_id: "node-1".into(),
            label: Some(text("Scope")),
        });
        model.formulas.push(MindmapFormula {
            id: "formula".into(),
            node_id: format!("node-{}", NODE_COUNT - 1),
            source: "x^2+y^2".into(),
            display: MindmapFormulaDisplay::Block,
        });
    }
    model
}

fn text(value: &str) -> RichText {
    RichText {
        text: value.into(),
        runs: Vec::new(),
    }
}

fn process_rss_kib() -> Option<u64> {
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    String::from_utf8(output.stdout).ok()?.trim().parse().ok()
}
