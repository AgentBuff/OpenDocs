//! Derived pagination/print input for Document renderers.
//!
//! The projection resolves section inheritance and logical root-block ranges. It deliberately
//! contains no page coordinates: physical pagination belongs to a renderer and is never persisted.

use oo_schema::{
    BlockId, DocumentHeaderFooter, DocumentModel, DocumentNote, DocumentPageNumbering, PageSetup,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentPrintProjection {
    pub revision: u64,
    pub sections: Vec<DocumentPrintSection>,
    pub footnotes: Vec<DocumentNote>,
    pub endnotes: Vec<DocumentNote>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentPrintSection {
    /// `None` identifies the derived default section used by documents without explicit sections.
    pub section_id: Option<String>,
    pub root_block_ids: Vec<BlockId>,
    pub page_setup: Option<PageSetup>,
    pub header: Option<DocumentHeaderFooter>,
    pub footer: Option<DocumentHeaderFooter>,
    pub page_numbering: Option<DocumentPageNumbering>,
}

pub fn document_print_projection(model: &DocumentModel, revision: u64) -> DocumentPrintProjection {
    let sections = if model.page_semantics.sections.is_empty() {
        vec![DocumentPrintSection {
            section_id: None,
            root_block_ids: model.root.clone(),
            page_setup: model.page_setup.clone(),
            header: None,
            footer: None,
            page_numbering: None,
        }]
    } else {
        model
            .page_semantics
            .sections
            .iter()
            .enumerate()
            .map(|(index, section)| {
                let start = model
                    .root
                    .iter()
                    .position(|block_id| block_id == &section.start_block_id)
                    .expect("validated section start exists in root");
                let end = model
                    .page_semantics
                    .sections
                    .get(index + 1)
                    .and_then(|next| {
                        model
                            .root
                            .iter()
                            .position(|block_id| block_id == &next.start_block_id)
                    })
                    .unwrap_or(model.root.len());
                DocumentPrintSection {
                    section_id: Some(section.id.clone()),
                    root_block_ids: model.root[start..end].to_vec(),
                    page_setup: section
                        .page_setup
                        .clone()
                        .or_else(|| model.page_setup.clone()),
                    header: section.header.clone(),
                    footer: section.footer.clone(),
                    page_numbering: section.page_numbering.clone(),
                }
            })
            .collect()
    };

    DocumentPrintProjection {
        revision,
        sections,
        footnotes: model.page_semantics.footnotes.clone(),
        endnotes: model.page_semantics.endnotes.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oo_schema::{
        BlockData, BlockPresentation, DocumentBlock, DocumentBlockKind, DocumentSection,
        HeaderFooterContent, HeaderFooterSegment, RichText,
    };

    #[test]
    fn resolves_logical_section_ranges_and_inherited_page_setup() {
        let mut model = DocumentModel::empty();
        model.root = vec!["a".into(), "b".into(), "c".into()];
        model.blocks = model
            .root
            .iter()
            .map(|id| DocumentBlock {
                id: id.clone(),
                kind: DocumentBlockKind::Paragraph,
                presentation: BlockPresentation::default(),
                content: Some(RichText::default()),
                children: Vec::new(),
                data: BlockData::None,
            })
            .collect();
        model.page_setup = Some(PageSetup {
            width: 612.0,
            height: 792.0,
            margin_top: 72.0,
            margin_right: 72.0,
            margin_bottom: 72.0,
            margin_left: 72.0,
        });
        model.page_semantics.sections = vec![
            DocumentSection {
                id: "s1".into(),
                start_block_id: "a".into(),
                page_setup: None,
                header: Some(DocumentHeaderFooter {
                    default: HeaderFooterContent {
                        segments: vec![HeaderFooterSegment::PageNumber],
                    },
                    ..Default::default()
                }),
                footer: None,
                page_numbering: None,
            },
            DocumentSection {
                id: "s2".into(),
                start_block_id: "c".into(),
                page_setup: None,
                header: None,
                footer: None,
                page_numbering: None,
            },
        ];
        model.validate().unwrap();

        let projection = document_print_projection(&model, 7);
        assert_eq!(projection.revision, 7);
        assert_eq!(projection.sections[0].root_block_ids, ["a", "b"]);
        assert_eq!(projection.sections[1].root_block_ids, ["c"]);
        assert_eq!(projection.sections[1].page_setup, model.page_setup);
    }
}
