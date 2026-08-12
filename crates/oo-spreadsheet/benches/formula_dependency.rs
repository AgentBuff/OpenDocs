//! Dependency-free benchmark for formula parsing and reverse-dependent closure.

use std::hint::black_box;
use std::time::{Duration, Instant};

use oo_schema::{CellModel, SheetModel, SpreadsheetModel};
use oo_spreadsheet::{CellAddress, FormulaDependencyIndex};

const DEFAULT_FORMULAS: usize = 5_000;
const DEFAULT_ITERATIONS: usize = 20;

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value: &usize| *value > 0)
        .unwrap_or(default)
}

fn model(formulas: usize) -> SpreadsheetModel {
    let mut cells = Vec::with_capacity(formulas);
    cells.push(CellModel {
        row: 0,
        column: 0,
        value: Some(0.into()),
        ..CellModel::default()
    });
    for row in 1..formulas {
        cells.push(CellModel {
            row: row as u32,
            column: 0,
            formula: Some(format!("=A{}", row)),
            ..CellModel::default()
        });
    }
    SpreadsheetModel {
        sheets: vec![SheetModel {
            id: "sheet-1".into(),
            name: "Sheet 1".into(),
            cells,
            ..SheetModel::default()
        }],
        ..SpreadsheetModel::default()
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
    let formulas = env_usize("OO_BENCH_FORMULAS", DEFAULT_FORMULAS).max(2);
    let iterations = env_usize("OO_BENCH_ITERATIONS", DEFAULT_ITERATIONS);
    let snapshot = model(formulas);

    let build_duration = timed(iterations, || {
        black_box(FormulaDependencyIndex::from_model(&snapshot).expect("valid formula index"));
    });
    let index = FormulaDependencyIndex::from_model(&snapshot).expect("valid formula index");
    let source = CellAddress {
        sheet_id: "sheet-1".into(),
        row: 0,
        column: 0,
    };
    let closure_duration = timed(iterations, || {
        black_box(index.affected_topology(std::slice::from_ref(&source)));
    });

    println!(
        "{{\n  \"formulaCells\": {formulas},\n  \"iterations\": {iterations},\n  \"indexBuildMs\": {:.4},\n  \"affectedTopologyMs\": {:.4}\n}}",
        per_operation(build_duration, iterations),
        per_operation(closure_duration, iterations),
    );
}
