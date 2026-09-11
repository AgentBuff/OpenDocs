use std::collections::HashMap;

use oo_schema::{
    BlockData, DocumentBlock, DocumentBlockKind, DocumentModel, InlineRun, InlineStyle, RichText,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSearchOptions {
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub whole_word: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DocumentTextTarget {
    Block {
        block_id: String,
    },
    TableCell {
        block_id: String,
        row_id: String,
        cell_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSearchMatch {
    pub target: DocumentTextTarget,
    /// Half-open Unicode scalar offsets in the target RichText.
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentTocItem {
    pub block_id: String,
    pub level: u8,
    pub text: String,
}

pub fn find_text(
    document: &DocumentModel,
    query: &str,
    options: DocumentSearchOptions,
) -> Vec<DocumentSearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }
    let by_id = document
        .blocks
        .iter()
        .map(|block| (block.id.as_str(), block))
        .collect::<HashMap<_, _>>();
    let mut matches = Vec::new();
    for root in &document.root {
        visit_text(root, &by_id, query, options, &mut matches);
    }
    matches
}

pub fn table_of_contents(document: &DocumentModel) -> Vec<DocumentTocItem> {
    let by_id = document
        .blocks
        .iter()
        .map(|block| (block.id.as_str(), block))
        .collect::<HashMap<_, _>>();
    let mut items = Vec::new();
    fn visit(id: &str, by_id: &HashMap<&str, &DocumentBlock>, items: &mut Vec<DocumentTocItem>) {
        let Some(block) = by_id.get(id) else { return };
        if let (DocumentBlockKind::Heading { level }, Some(content)) = (&block.kind, &block.content)
        {
            items.push(DocumentTocItem {
                block_id: block.id.clone(),
                level: *level,
                text: content.text.clone(),
            });
        }
        for child in &block.children {
            visit(child, by_id, items);
        }
    }
    for root in &document.root {
        visit(root, &by_id, &mut items);
    }
    items
}

fn visit_text(
    id: &str,
    by_id: &HashMap<&str, &DocumentBlock>,
    query: &str,
    options: DocumentSearchOptions,
    matches: &mut Vec<DocumentSearchMatch>,
) {
    let Some(block) = by_id.get(id) else { return };
    if let Some(content) = &block.content {
        append_matches(
            content,
            query,
            options,
            DocumentTextTarget::Block {
                block_id: block.id.clone(),
            },
            matches,
        );
    }
    if let BlockData::Table(table) = &block.data {
        for row in &table.rows {
            for cell in &row.cells {
                append_matches(
                    &cell.content,
                    query,
                    options,
                    DocumentTextTarget::TableCell {
                        block_id: block.id.clone(),
                        row_id: row.id.clone(),
                        cell_id: cell.id.clone(),
                    },
                    matches,
                );
            }
        }
    }
    for child in &block.children {
        visit_text(child, by_id, query, options, matches);
    }
}

fn append_matches(
    content: &RichText,
    query: &str,
    options: DocumentSearchOptions,
    target: DocumentTextTarget,
    output: &mut Vec<DocumentSearchMatch>,
) {
    let source = content.text.chars().collect::<Vec<_>>();
    let (haystack, source_map) = folded_chars(&content.text, options.case_sensitive);
    let (needle, _) = folded_chars(query, options.case_sensitive);
    if needle.is_empty() || needle.len() > haystack.len() {
        return;
    }
    let mut last_source_end = 0;
    for start in 0..=haystack.len() - needle.len() {
        if haystack[start..start + needle.len()] != needle {
            continue;
        }
        let source_start = source_map[start];
        let source_end = source_map[start + needle.len() - 1] + 1;
        // Search navigation and replace-all share this projection. Returning
        // overlapping matches (for example both positions in `aaa` for `aa`)
        // would make replacement order ambiguous, so matches are always the
        // conventional left-most, non-overlapping sequence per target.
        if source_start < last_source_end {
            continue;
        }
        if options.whole_word
            && (!word_boundary(source.get(source_start.wrapping_sub(1)))
                || !word_boundary(source.get(source_end)))
        {
            continue;
        }
        if output.last().is_some_and(|previous| {
            previous.target == target
                && previous.start == source_start
                && previous.end == source_end
        }) {
            continue;
        }
        output.push(DocumentSearchMatch {
            target: target.clone(),
            start: source_start,
            end: source_end,
        });
        last_source_end = source_end;
    }
}

/// Replaces a non-overlapping set of scalar ranges while retaining the style
/// of all untouched characters. Inserted text inherits the first replaced
/// character's style (or the preceding/default style for a zero-width range).
pub fn replace_rich_text_matches(
    content: &RichText,
    matches: &[DocumentSearchMatch],
    replacement: &str,
) -> RichText {
    if matches.is_empty() {
        return content.clone();
    }
    let source_chars = content.text.chars().collect::<Vec<_>>();
    let source_styles = styles_by_character(content, source_chars.len());
    let replacement_chars = replacement.chars().collect::<Vec<_>>();
    let mut output_chars = Vec::new();
    let mut output_styles = Vec::new();
    let mut cursor = 0;
    for matched in matches {
        debug_assert!(matched.start >= cursor && matched.end <= source_chars.len());
        output_chars.extend_from_slice(&source_chars[cursor..matched.start]);
        output_styles.extend_from_slice(&source_styles[cursor..matched.start]);
        let replacement_style = source_styles
            .get(matched.start)
            .or_else(|| {
                matched
                    .start
                    .checked_sub(1)
                    .and_then(|index| source_styles.get(index))
            })
            .cloned()
            .unwrap_or_default();
        output_chars.extend(replacement_chars.iter().copied());
        output_styles.extend(std::iter::repeat_n(
            replacement_style,
            replacement_chars.len(),
        ));
        cursor = matched.end;
    }
    output_chars.extend_from_slice(&source_chars[cursor..]);
    output_styles.extend_from_slice(&source_styles[cursor..]);
    RichText {
        text: output_chars.into_iter().collect(),
        runs: compact_styles(output_styles),
    }
}

fn styles_by_character(content: &RichText, text_len: usize) -> Vec<InlineStyle> {
    let mut styles = vec![InlineStyle::default(); text_len];
    for run in &content.runs {
        for style in styles
            .iter_mut()
            .take(run.end.min(text_len))
            .skip(run.start.min(text_len))
        {
            *style = run.style.clone();
        }
    }
    styles
}

fn compact_styles(styles: Vec<InlineStyle>) -> Vec<InlineRun> {
    let mut runs: Vec<InlineRun> = Vec::new();
    for (index, style) in styles.into_iter().enumerate() {
        if let Some(previous) = runs.last_mut().filter(|previous| previous.style == style) {
            previous.end = index + 1;
        } else {
            runs.push(InlineRun {
                start: index,
                end: index + 1,
                style,
            });
        }
    }
    runs
}

fn folded_chars(value: &str, case_sensitive: bool) -> (Vec<char>, Vec<usize>) {
    let mut folded = Vec::new();
    let mut source_map = Vec::new();
    for (index, character) in value.chars().enumerate() {
        if case_sensitive {
            folded.push(character);
            source_map.push(index);
        } else {
            for lowered in character.to_lowercase() {
                folded.push(lowered);
                source_map.push(index);
            }
        }
    }
    (folded, source_map)
}

fn word_boundary(character: Option<&char>) -> bool {
    character.is_none_or(|character| !character.is_alphanumeric() && *character != '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use oo_schema::{
        BlockPresentation, InlineRun, InlineStyle, TableBlock, TableCell, TableCellStyle,
        TableColumn, TableRow,
    };

    #[test]
    fn finds_cjk_emoji_across_runs_blocks_and_table_cells_in_tree_order() {
        let rich = |text: &str| RichText {
            text: text.into(),
            runs: if text.is_empty() {
                Vec::new()
            } else {
                vec![InlineRun {
                    start: 0,
                    end: text.chars().count(),
                    style: InlineStyle::default(),
                }]
            },
        };
        let document = DocumentModel {
            root: vec!["heading".into(), "paragraph".into(), "table".into()],
            blocks: vec![
                DocumentBlock {
                    id: "heading".into(),
                    kind: DocumentBlockKind::Heading { level: 2 },
                    presentation: BlockPresentation::default(),
                    content: Some(rich("路线图 😀")),
                    children: Vec::new(),
                    data: BlockData::None,
                },
                DocumentBlock {
                    id: "paragraph".into(),
                    kind: DocumentBlockKind::Paragraph,
                    presentation: BlockPresentation::default(),
                    content: Some(RichText {
                        text: "路线图和路线图".into(),
                        runs: vec![
                            InlineRun {
                                start: 0,
                                end: 2,
                                style: InlineStyle::default(),
                            },
                            InlineRun {
                                start: 2,
                                end: 7,
                                style: InlineStyle {
                                    bold: true,
                                    ..InlineStyle::default()
                                },
                            },
                        ],
                    }),
                    children: Vec::new(),
                    data: BlockData::None,
                },
                DocumentBlock {
                    id: "table".into(),
                    kind: DocumentBlockKind::Table,
                    presentation: BlockPresentation::default(),
                    content: None,
                    children: Vec::new(),
                    data: BlockData::Table(TableBlock {
                        columns: vec![TableColumn {
                            id: "c1".into(),
                            width: None,
                        }],
                        rows: vec![TableRow {
                            id: "r1".into(),
                            height: None,
                            cells: vec![TableCell {
                                id: "cell1".into(),
                                content: rich("路线图"),
                                style: TableCellStyle::default(),
                            }],
                        }],
                        merged_ranges: Vec::new(),
                    }),
                },
            ],
            page_setup: None,
            page_semantics: Default::default(),
        };
        let matches = find_text(&document, "路线图", DocumentSearchOptions::default());
        assert_eq!(matches.len(), 4);
        assert_eq!(matches[1].start, 0);
        assert_eq!(matches[2].start, 4);
        assert!(matches!(
            matches[3].target,
            DocumentTextTarget::TableCell { .. }
        ));
        assert_eq!(table_of_contents(&document)[0].text, "路线图 😀");
        assert_eq!(
            find_text(&document, "😀", DocumentSearchOptions::default())[0].start,
            4
        );
    }

    #[test]
    fn supports_case_and_whole_word_without_byte_offsets() {
        let document = DocumentModel {
            root: vec!["p".into()],
            blocks: vec![DocumentBlock {
                id: "p".into(),
                kind: DocumentBlockKind::Paragraph,
                presentation: BlockPresentation::default(),
                content: Some(RichText {
                    text: "Alpha alphabet ALPHA".into(),
                    runs: Vec::new(),
                }),
                children: Vec::new(),
                data: BlockData::None,
            }],
            page_setup: None,
            page_semantics: Default::default(),
        };
        assert_eq!(
            find_text(&document, "alpha", DocumentSearchOptions::default()).len(),
            3
        );
        assert_eq!(
            find_text(
                &document,
                "alpha",
                DocumentSearchOptions {
                    whole_word: true,
                    ..Default::default()
                }
            )
            .len(),
            2
        );
        assert_eq!(
            find_text(
                &document,
                "alpha",
                DocumentSearchOptions {
                    case_sensitive: true,
                    whole_word: false
                }
            )
            .len(),
            1
        );
    }

    #[test]
    fn replacement_preserves_unmatched_styles_and_inherits_match_style() {
        let bold = InlineStyle {
            bold: true,
            ..InlineStyle::default()
        };
        let content = RichText {
            text: "甲路线图乙路线图".into(),
            runs: vec![
                InlineRun {
                    start: 0,
                    end: 4,
                    style: bold.clone(),
                },
                InlineRun {
                    start: 4,
                    end: 8,
                    style: InlineStyle::default(),
                },
            ],
        };
        let matches = find_text(
            &DocumentModel {
                root: vec!["p".into()],
                blocks: vec![DocumentBlock {
                    id: "p".into(),
                    kind: DocumentBlockKind::Paragraph,
                    presentation: BlockPresentation::default(),
                    content: Some(content.clone()),
                    children: Vec::new(),
                    data: BlockData::None,
                }],
                page_setup: None,
                page_semantics: Default::default(),
            },
            "路线图",
            DocumentSearchOptions::default(),
        );
        let replaced = replace_rich_text_matches(&content, &matches, "🗺️");
        assert_eq!(replaced.text, "甲🗺️乙🗺️");
        assert_eq!(replaced.runs[0].style, bold);
        assert_eq!(replaced.runs[0].end, 3);
        assert_eq!(replaced.runs[1].start, 3);
    }

    #[test]
    fn search_results_do_not_overlap() {
        let document = DocumentModel {
            root: vec!["p".into()],
            blocks: vec![DocumentBlock {
                id: "p".into(),
                kind: DocumentBlockKind::Paragraph,
                presentation: BlockPresentation::default(),
                content: Some(RichText {
                    text: "aaa".into(),
                    runs: Vec::new(),
                }),
                children: Vec::new(),
                data: BlockData::None,
            }],
            page_setup: None,
            page_semantics: Default::default(),
        };
        assert_eq!(
            find_text(&document, "aa", DocumentSearchOptions::default()).len(),
            1
        );
    }
}
