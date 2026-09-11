//! Contract tests for the v3 Grid merge model.
//!
//! These tests intentionally exercise the public schema crate from outside its implementation
//! module. They make sure callers cannot persist a merge range that the DOM projection cannot
//! represent (missing endpoints, reversed ranges, singleton ranges, or overlapping spans).

use oo_schema::{
    BlockData, BlockPresentation, DocumentBlock, DocumentBlockKind, DocumentModel, RichText,
    TableBlock, TableCell, TableColumn, TableRange, TableRow,
};

fn table_document(merged_ranges: Vec<TableRange>) -> DocumentModel {
    let columns = vec![
        TableColumn {
            id: "c-1".into(),
            width: None,
        },
        TableColumn {
            id: "c-2".into(),
            width: None,
        },
        TableColumn {
            id: "c-3".into(),
            width: None,
        },
    ];
    let rows = (1..=3)
        .map(|row| TableRow {
            id: format!("r-{row}"),
            height: None,
            cells: (1..=3)
                .map(|column| TableCell {
                    id: format!("cell-{row}-{column}"),
                    content: RichText {
                        text: format!("{row},{column}"),
                        runs: Vec::new(),
                    },
                    style: Default::default(),
                })
                .collect(),
        })
        .collect();
    let table = DocumentBlock {
        id: "table-1".into(),
        kind: DocumentBlockKind::Table,
        presentation: BlockPresentation::default(),
        content: None,
        children: Vec::new(),
        data: BlockData::Table(TableBlock {
            columns,
            rows,
            merged_ranges,
        }),
    };
    DocumentModel {
        root: vec![table.id.clone()],
        blocks: vec![table],
        page_setup: None,
        page_semantics: Default::default(),
    }
}

fn range(start_row: &str, end_row: &str, start_column: &str, end_column: &str) -> TableRange {
    TableRange {
        start_row_id: start_row.into(),
        end_row_id: end_row.into(),
        start_column_id: start_column.into(),
        end_column_id: end_column.into(),
    }
}

#[test]
fn valid_v3_merge_range_round_trips_with_stable_ids() {
    let document = table_document(vec![range("r-1", "r-2", "c-1", "c-3")]);
    document.validate().expect("valid merge range");

    let encoded = serde_json::to_string(&document).expect("serialize document");
    let decoded: DocumentModel = serde_json::from_str(&encoded).expect("deserialize document");
    assert_eq!(decoded, document);
}

#[test]
fn invalid_v3_merge_ranges_are_rejected_at_schema_boundary() {
    let invalid_ranges = [
        range("missing", "r-2", "c-1", "c-2"),
        range("r-2", "r-1", "c-1", "c-2"),
        range("r-1", "r-1", "c-1", "c-1"),
        range("r-1", "r-2", "missing", "c-2"),
    ];
    for invalid in invalid_ranges {
        assert!(
            table_document(vec![invalid]).validate().is_err(),
            "schema accepted invalid merge range"
        );
    }

    let overlap = table_document(vec![
        range("r-1", "r-2", "c-1", "c-2"),
        range("r-2", "r-3", "c-2", "c-3"),
    ]);
    assert!(
        overlap.validate().is_err(),
        "schema accepted overlapping spans"
    );
}
