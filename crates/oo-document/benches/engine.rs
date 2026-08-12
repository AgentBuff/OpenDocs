//! Small dependency-free benchmarks for the document engine.
//!
//! This intentionally uses the standard library instead of a benchmarking framework so it can
//! run in an offline checkout.  The output is JSON and is suitable for checking into a local
//! performance report without making the engine depend on a test-only crate.

use std::hint::black_box;
use std::time::{Duration, Instant};

use oo_document::{DocumentCommand, DocumentCommandBatch, DocumentEngine};
use oo_schema::{
    BlockData, BlockPresentation, DocumentBlock, DocumentBlockKind, DocumentModel, RichText,
};

const DEFAULT_BLOCKS: usize = 1_000;
const DEFAULT_ITERATIONS: usize = 100;

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value: &usize| *value > 0)
        .unwrap_or(default)
}

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

fn model(blocks: usize) -> DocumentModel {
    let mut document = DocumentModel::default();
    for index in 0..blocks {
        let id = format!("block-{index}");
        document.root.push(id.clone());
        document.blocks.push(paragraph(
            &id,
            "A small editable paragraph used by the benchmark.",
        ));
    }
    document
}

fn timed<F>(iterations: usize, mut operation: F) -> Duration
where
    F: FnMut(usize),
{
    // A short warm-up removes one-time allocator and branch-prediction noise from the report.
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
    let blocks = env_usize("OO_BENCH_BLOCKS", DEFAULT_BLOCKS);
    let iterations = env_usize("OO_BENCH_ITERATIONS", DEFAULT_ITERATIONS);
    assert!(
        blocks >= 2,
        "OO_BENCH_BLOCKS must be at least 2 for delete/undo"
    );
    let baseline = model(blocks);

    let mut update_engine = DocumentEngine::new(baseline.clone(), 0).expect("valid baseline");
    let update_duration = timed(iterations, |index| {
        let block_id = format!("block-{}", index % blocks);
        let revision = update_engine.revision();
        let result = update_engine.execute(DocumentCommandBatch {
            base_revision: revision,
            commands: vec![DocumentCommand::ReplaceBlockText {
                block_id,
                content: RichText {
                    text: format!("updated paragraph {index}"),
                    runs: Vec::new(),
                },
            }],
        });
        black_box(result.expect("single block update"));
    });

    let mut move_engine = DocumentEngine::new(baseline.clone(), 0).expect("valid baseline");
    let move_duration = timed(iterations, |index| {
        let block_id = format!("block-{}", index % blocks);
        let revision = move_engine.revision();
        let result = move_engine.execute(DocumentCommandBatch {
            base_revision: revision,
            commands: vec![DocumentCommand::MoveBlock {
                block_id,
                parent_id: None,
                index: (index + 1) % blocks,
            }],
        });
        black_box(result.expect("single block move"));
    });

    // Keep one spare root so delete/restore is legal and exercise the subtree journal path.
    let mut delete_engine = DocumentEngine::new(baseline.clone(), 0).expect("valid baseline");
    let delete_iterations = iterations.min(blocks.saturating_sub(1)).max(1);
    let delete_duration = timed(delete_iterations, |index| {
        let block_id = format!("block-{}", index % (blocks.saturating_sub(1).max(1)));
        let revision = delete_engine.revision();
        let deleted = delete_engine.execute(DocumentCommandBatch {
            base_revision: revision,
            commands: vec![DocumentCommand::DeleteBlock {
                block_id: block_id.clone(),
            }],
        });
        black_box(deleted.expect("delete block"));
        let revision = delete_engine.revision();
        let restored = delete_engine.undo();
        black_box(restored.expect("restore deleted block"));
        assert_eq!(delete_engine.revision(), revision + 1);
    });

    let snapshot_engine = DocumentEngine::new(baseline, 0).expect("valid baseline");
    let snapshot_duration = timed(iterations, |_| {
        let bytes = serde_json::to_vec(black_box(snapshot_engine.model())).expect("serialize");
        black_box(bytes);
    });

    let mut rollback_engine = DocumentEngine::new(model(blocks), 0).expect("valid baseline");
    let rollback_duration = timed(iterations, |index| {
        let block_id = format!("block-{}", index % blocks);
        let revision = rollback_engine.revision();
        let result = rollback_engine.execute(DocumentCommandBatch {
            base_revision: revision,
            commands: vec![
                DocumentCommand::ReplaceBlockText {
                    block_id: block_id.clone(),
                    content: RichText {
                        text: "this update must roll back".into(),
                        runs: Vec::new(),
                    },
                },
                DocumentCommand::MoveBlock {
                    block_id: block_id.clone(),
                    parent_id: Some(block_id),
                    index: 0,
                },
            ],
        });
        assert!(result.is_err());
        assert_eq!(rollback_engine.revision(), revision);
    });

    println!(
        "{{\n  \"blocks\": {blocks},\n  \"iterations\": {iterations},\n  \"singleBlockUpdateMs\": {:.4},\n  \"singleBlockMoveMs\": {:.4},\n  \"deleteUndoMs\": {:.4},\n  \"snapshotSerializeMs\": {:.4},\n  \"failedTransactionRollbackMs\": {:.4}\n}}",
        per_operation(update_duration, iterations),
        per_operation(move_duration, iterations),
        per_operation(delete_duration, delete_iterations),
        per_operation(snapshot_duration, iterations),
        per_operation(rollback_duration, iterations),
    );
}
