//! Dependency-free benchmark for sparse Grid writes and local rollback.
//!
//! Run with `cargo bench -p oo-spreadsheet --bench engine`.  The benchmark intentionally uses
//! only the standard library so the canonical engine has no test-framework dependency.

use std::hint::black_box;
use std::time::{Duration, Instant};

use oo_schema::{SheetModel, SpreadsheetModel};
use oo_spreadsheet::{SpreadsheetCommand, SpreadsheetCommandBatch, SpreadsheetEngine};
use serde_json::{Map, Value};

const DEFAULT_CELLS: usize = 10_000;
const DEFAULT_ITERATIONS: usize = 100;

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value: &usize| *value > 0)
        .unwrap_or(default)
}

fn baseline(cells: usize) -> SpreadsheetModel {
    let mut sheet = SheetModel {
        id: "sheet-1".into(),
        name: "Sheet 1".into(),
        cells: Vec::with_capacity(cells),
        ..SheetModel::default()
    };
    for index in 0..cells {
        sheet.cells.push(oo_schema::CellModel {
            row: index as u32,
            column: 0,
            value: Some(Value::Number((index as u64).into())),
            formula: None,
            attrs: Map::new(),
            style: None,
        });
    }
    SpreadsheetModel {
        sheets: vec![sheet],
        ..SpreadsheetModel::default()
    }
}

fn timed<F>(iterations: usize, mut operation: F) -> Duration
where
    F: FnMut(usize),
{
    for index in 0..iterations.min(10) {
        operation(index);
    }
    let started = Instant::now();
    for index in 0..iterations {
        operation(index);
    }
    started.elapsed()
}

fn per_operation(duration: Duration, iterations: usize) -> f64 {
    duration.as_secs_f64() * 1_000.0 / iterations as f64
}

fn main() {
    let cells = env_usize("OO_BENCH_CELLS", DEFAULT_CELLS);
    let iterations = env_usize("OO_BENCH_ITERATIONS", DEFAULT_ITERATIONS);
    let baseline = baseline(cells);

    let mut update_engine = SpreadsheetEngine::new(baseline.clone(), 0).expect("valid baseline");
    let update_duration = timed(iterations, |index| {
        let revision = update_engine.revision();
        let result = update_engine.execute(SpreadsheetCommandBatch {
            base_revision: revision,
            commands: vec![SpreadsheetCommand::SetCell {
                sheet_id: "sheet-1".into(),
                row: (index % cells) as u32,
                column: 0,
                // Deliberately differ from the baseline value so every sample is a real commit.
                value: Some(Value::Number((revision + cells as u64).into())),
                formula: None,
                attrs: Map::new(),
            }],
        });
        black_box(result.expect("single cell update"));
    });

    let mut rollback_engine = SpreadsheetEngine::new(baseline, 0).expect("valid baseline");
    let rollback_duration = timed(iterations, |index| {
        let revision = rollback_engine.revision();
        let result = rollback_engine.execute(SpreadsheetCommandBatch {
            base_revision: revision,
            commands: vec![
                SpreadsheetCommand::SetCell {
                    sheet_id: "sheet-1".into(),
                    row: (index % cells) as u32,
                    column: 0,
                    value: Some(Value::String("transient".into())),
                    formula: None,
                    attrs: Map::new(),
                },
                SpreadsheetCommand::RenameSheet {
                    sheet_id: "missing".into(),
                    name: "must rollback".into(),
                },
            ],
        });
        assert!(result.is_err());
        assert_eq!(rollback_engine.revision(), revision);
    });

    println!(
        "{{\n  \"cells\": {cells},\n  \"iterations\": {iterations},\n  \"singleCellUpdateMs\": {:.4},\n  \"failedTransactionRollbackMs\": {:.4}\n}}",
        per_operation(update_duration, iterations),
        per_operation(rollback_duration, iterations),
    );
}
