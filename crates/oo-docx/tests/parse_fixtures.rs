//! 针对真实 .docx 包的 canonical Block Tree 集成测试。

use oo_schema::{
    DocumentBlock, DocumentBlockKind, DocumentModel, DocumentPageNumbering, PageNumberFormat,
};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;

fn fixture(name: &str) -> Vec<u8> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/");
    std::fs::read(format!("{path}{name}"))
        .unwrap_or_else(|e| panic!("读取 fixture {name} 失败：{e}（先跑 make_fixtures.py）"))
}

fn parse(name: &str) -> DocumentModel {
    oo_docx::parse_docx(&fixture(name), "test-doc").expect("解析失败")
}

fn roots(doc: &DocumentModel) -> Vec<&DocumentBlock> {
    doc.root
        .iter()
        .map(|id| doc.blocks.iter().find(|block| &block.id == id).unwrap())
        .collect()
}

fn text(block: &DocumentBlock) -> &str {
    block
        .content
        .as_ref()
        .map(|content| content.text.as_str())
        .unwrap_or("")
}

#[test]
fn font_stack_exports_as_a_real_office_family() {
    let mut original = parse("sample.docx");
    for block in &mut original.blocks {
        if let Some(content) = &mut block.content {
            for run in &mut content.runs {
                run.style.font_family = Some("\"Noto Serif SC\", serif".into());
            }
        }
    }
    let bytes = oo_docx::write_docx(&original).unwrap();
    let imported = oo_docx::parse_docx(&bytes, "font-roundtrip").unwrap();
    let families: Vec<_> = imported
        .blocks
        .iter()
        .filter_map(|block| block.content.as_ref())
        .flat_map(|content| &content.runs)
        .filter_map(|run| run.style.font_family.as_deref())
        .collect();
    assert!(!families.is_empty());
    assert!(families.iter().all(|family| *family == "Noto Serif SC"));
}

fn image_docx_fixture() -> (Vec<u8>, Vec<u8>) {
    let image = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let document = br#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/word" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <w:body><w:p><w:r><w:t>before</w:t></w:r><w:r><w:drawing><wp:inline><a:graphic><a:graphicData><a:blip r:embed="rId5"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p></w:body>
</w:document>"#;
    let relationships = br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#;
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in [
        ("word/document.xml", document.as_slice()),
        ("word/_rels/document.xml.rels", relationships.as_slice()),
        ("word/media/image1.png", image.as_slice()),
    ] {
        archive
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        archive.write_all(content).unwrap();
    }
    (archive.finish().unwrap().into_inner(), image)
}

#[test]
fn import_reports_unmapped_header_and_note_parts() {
    let document = br#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>body</w:t></w:r></w:p></w:body></w:document>"#;
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in [
        ("word/document.xml", document.as_slice()),
        ("word/header1.xml", b"<w:hdr/>".as_slice()),
        ("word/footnotes.xml", b"<w:footnotes/>".as_slice()),
        ("word/endnotes.xml", b"<w:endnotes/>".as_slice()),
    ] {
        archive
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        archive.write_all(content).unwrap();
    }
    let bytes = archive.finish().unwrap().into_inner();
    let imported = oo_docx::parse_docx_with_assets(&bytes, "loss-report").unwrap();
    let capabilities = imported
        .loss_report
        .unsupported
        .iter()
        .map(|loss| loss.capability)
        .collect::<Vec<_>>();
    assert_eq!(capabilities, ["headerFooter", "footnotes", "endnotes"]);
}

#[test]
fn minimal_document_yields_one_paragraph() {
    let doc = parse("minimal.docx");
    assert_eq!(doc.root.len(), 1);
    assert_eq!(text(roots(&doc)[0]), "Hello, World!");
    doc.validate().unwrap();
}

#[test]
fn sample_document_has_a_valid_block_tree() {
    let doc = parse("sample.docx");
    doc.validate().expect("canonical DocumentModel 应通过校验");
    assert_eq!(doc.root.len(), 7);
    assert_eq!(doc.blocks.len(), 7);
    assert!(matches!(doc.blocks[0].kind, DocumentBlockKind::Paragraph));
    assert_eq!(text(&doc.blocks[1]), "普通文本，加粗，斜体，红色下划线。");
    assert_eq!(
        doc.blocks[0].presentation.align,
        oo_schema::BlockAlignment::Center
    );
    let wire = serde_json::to_value(&doc.blocks[0]).unwrap();
    assert!(wire.get("attrs").is_none());
    assert!(wire.get("payload").is_none());
    assert_eq!(wire["data"]["type"], "none");
}

#[test]
fn run_style_carries_direct_formatting() {
    let doc = parse("sample.docx");
    let block = roots(&doc)[1];
    let content = block.content.as_ref().unwrap();
    let bold_at = content.text.chars().position(|c| c == '加').unwrap();
    let bold = content
        .runs
        .iter()
        .find(|run| run.start <= bold_at && bold_at < run.end)
        .unwrap();
    assert!(bold.style.bold);
    let italic_at = content.text.chars().position(|c| c == '斜').unwrap();
    let italic = content
        .runs
        .iter()
        .find(|run| run.start <= italic_at && italic_at < run.end)
        .unwrap();
    assert!(italic.style.italic);
    let colored_at = content.text.chars().position(|c| c == '红').unwrap();
    let colored = content
        .runs
        .iter()
        .find(|run| run.start <= colored_at && colored_at < run.end)
        .unwrap();
    assert!(colored.style.underline);
    assert_eq!(colored.style.color.as_deref(), Some("#C00000"));
}

#[test]
fn named_and_direct_paragraph_presentation_is_resolved() {
    let doc = parse("sample.docx");
    let heading = &roots(&doc)[0].presentation;
    assert_eq!(heading.align, oo_schema::BlockAlignment::Center);
    let heading_run = roots(&doc)[0].content.as_ref().unwrap().runs[0]
        .style
        .clone();
    assert!(heading_run.bold);
    assert_eq!(heading_run.font_size, Some(16.0));
    assert_eq!(heading_run.color.as_deref(), Some("#2F5496"));

    let right = &roots(&doc)[2].presentation;
    assert_eq!(right.align, oo_schema::BlockAlignment::Right);
    assert_eq!(
        roots(&doc)[2].content.as_ref().unwrap().runs[0]
            .style
            .font_size,
        Some(14.0)
    );
}

#[test]
fn paragraph_spacing_and_indent_are_in_typed_presentation() {
    let doc = parse("sample.docx");
    let style = &roots(&doc)[3].presentation;
    assert_eq!(style.indent_start, 0);
    assert_eq!(style.indent_end, 0.0);
    assert_eq!(style.spacing_after, 8.0);
    assert!((style.line_height - 1.15).abs() < 1e-6);
}

#[test]
fn soft_break_stays_inside_rich_text() {
    let doc = parse("sample.docx");
    assert_eq!(
        text(roots(&doc)[6]),
        "末段：空段落之后应当仍然正确排版。\n软换行之后的内容。"
    );
}

#[test]
fn section_properties_define_page_setup() {
    let doc = parse("sample.docx");
    let page = doc.page_setup.clone().unwrap();
    assert!((page.width - 595.3).abs() < 0.1);
    assert!((page.height - 841.9).abs() < 0.1);
    assert_eq!(page.margin_left, 72.0);
    assert_eq!(page.margin_top, 72.0);
    assert_eq!(doc.page_semantics.sections.len(), 1);
    assert_eq!(
        doc.page_semantics.sections[0].page_setup.as_ref(),
        Some(&page)
    );
}

#[test]
fn page_numbering_round_trips_through_section_properties() {
    let mut doc = parse("sample.docx");
    let section = doc.page_semantics.sections.first_mut().expect("section");
    section.page_numbering = Some(DocumentPageNumbering {
        start_at: 3,
        format: PageNumberFormat::LowerRoman,
    });
    let bytes = oo_docx::write_docx(&doc).unwrap();
    let roundtrip = oo_docx::parse_docx(&bytes, "page-numbering").unwrap();
    let numbering = roundtrip.page_semantics.sections[0]
        .page_numbering
        .as_ref()
        .expect("page numbering");
    assert_eq!(numbering.start_at, 3);
    assert_eq!(numbering.format, PageNumberFormat::LowerRoman);
}

#[test]
fn run_intervals_are_contiguous_and_ordered() {
    for block in roots(&parse("sample.docx")) {
        let Some(content) = &block.content else {
            continue;
        };
        let mut cursor = 0usize;
        for run in &content.runs {
            assert!(run.start >= cursor);
            assert!(run.end > run.start);
            cursor = run.end;
        }
        assert!(cursor <= content.text.chars().count());
    }
}

#[test]
fn non_docx_input_is_rejected_without_panicking() {
    let err = oo_docx::parse_docx(b"this is not a zip archive", "x").unwrap_err();
    assert!(matches!(err, oo_docx::DocxError::Zip(_)), "实际错误：{err}");
}

#[test]
fn docx_media_relationships_become_stable_image_assets() {
    let (bytes, image) = image_docx_fixture();
    let imported = oo_docx::parse_docx_with_assets(&bytes, "image-doc").unwrap();
    imported.document.validate().unwrap();
    assert_eq!(imported.assets.len(), 1);
    assert_eq!(imported.assets[0].bytes, image);
    assert_eq!(imported.assets[0].content_type, "image/png");
    assert_eq!(imported.assets[0].file_name, "image1.png");
    let expected = format!("docx-{}", hex::encode(Sha256::digest(&image)));
    assert_eq!(imported.assets[0].asset_id, expected);
    let image_block = imported
        .document
        .blocks
        .iter()
        .find(|block| matches!(block.kind, DocumentBlockKind::Image))
        .unwrap();
    assert!(matches!(
        &image_block.data,
        oo_schema::BlockData::Image(image) if image.asset_id == expected
    ));
}

#[test]
fn exported_docx_roundtrips_image_asset_relationships() {
    let (bytes, _) = image_docx_fixture();
    let imported = oo_docx::parse_docx_with_assets(&bytes, "image-doc").unwrap();
    let exported = oo_docx::write_docx_with_assets(&imported.document, &imported.assets).unwrap();
    let roundtrip = oo_docx::parse_docx_with_assets(&exported, "roundtrip-image").unwrap();
    assert_eq!(roundtrip.assets.len(), 1);
    assert_eq!(roundtrip.assets[0].bytes, imported.assets[0].bytes);
    assert!(roundtrip
        .document
        .blocks
        .iter()
        .any(|block| matches!(block.data, oo_schema::BlockData::Image(_))));
}

#[test]
fn exported_docx_reports_missing_or_unsupported_image_assets() {
    let (bytes, _) = image_docx_fixture();
    let imported = oo_docx::parse_docx_with_assets(&bytes, "image-doc").unwrap();
    let missing = oo_docx::write_docx_with_assets(&imported.document, &[]).unwrap_err();
    assert!(matches!(missing, oo_docx::DocxError::MissingAsset(_)));

    let mut unsupported = imported.assets[0].clone();
    unsupported.content_type = "video/mp4".into();
    let error = oo_docx::write_docx_with_assets(&imported.document, &[unsupported]).unwrap_err();
    assert!(matches!(error, oo_docx::DocxError::UnsupportedMedia(_)));
}

#[test]
fn exported_document_roundtrips_text_and_formatting() {
    let original = parse("sample.docx");
    let bytes = oo_docx::write_docx(&original).expect("导出失败");
    let roundtrip = oo_docx::parse_docx(&bytes, "roundtrip").expect("导出的 docx 无法重新导入");
    roundtrip
        .validate()
        .expect("导出后的模型应通过 schema 校验");
    assert_eq!(roundtrip.plain_text(), original.plain_text());
    let original_run = roots(&original)[1]
        .content
        .as_ref()
        .unwrap()
        .runs
        .iter()
        .find(|run| run.style.bold)
        .unwrap();
    let roundtrip_run = roots(&roundtrip)[1]
        .content
        .as_ref()
        .unwrap()
        .runs
        .iter()
        .find(|run| run.style.bold)
        .unwrap();
    assert_eq!(original_run.start, roundtrip_run.start);
    assert_eq!(original_run.end, roundtrip_run.end);
    assert_eq!(
        roots(&original)[3].presentation.line_height,
        roots(&roundtrip)[3].presentation.line_height
    );
}

#[test]
fn zip_without_document_part_is_reported() {
    let mut buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        zip.start_file::<_, ()>("hello.txt", Default::default())
            .unwrap();
        std::io::Write::write_all(&mut zip, b"hi").unwrap();
        zip.finish().unwrap();
    }
    let err = oo_docx::parse_docx(&buf, "x").unwrap_err();
    assert!(
        matches!(err, oo_docx::DocxError::MissingPart(_)),
        "实际错误：{err}"
    );
}

#[test]
fn export_reports_semantic_losses_instead_of_dropping_them_silently() {
    let mut document = parse("minimal.docx");
    let text = |text: &str| {
        Some(oo_schema::RichText {
            text: text.to_string(),
            runs: Vec::new(),
        })
    };
    let make_block = |id: &str,
                      kind: &oo_schema::DocumentBlockKind,
                      content: &str|
     -> oo_schema::DocumentBlock {
        let kind = kind.clone();
        oo_schema::DocumentBlock {
            id: id.to_string(),
            kind: kind.clone(),
            presentation: Default::default(),
            content: text(content),
            children: Vec::new(),
            data: match kind {
                oo_schema::DocumentBlockKind::Todo => {
                    oo_schema::BlockData::Todo(oo_schema::TodoBlock { checked: true })
                }
                oo_schema::DocumentBlockKind::Link => {
                    oo_schema::BlockData::Link(oo_schema::LinkBlock {
                        url: "https://example.com".into(),
                    })
                }
                _ => oo_schema::BlockData::None,
            },
        }
    };
    let kinds: Vec<(&str, oo_schema::DocumentBlockKind, &str)> = vec![
        ("todo-1", oo_schema::DocumentBlockKind::Todo, "买牛奶"),
        ("link-1", oo_schema::DocumentBlockKind::Link, "示例链接"),
        ("callout-1", oo_schema::DocumentBlockKind::Callout, "注意"),
        ("page-1", oo_schema::DocumentBlockKind::Page, ""),
        ("cols-1", oo_schema::DocumentBlockKind::Columns, ""),
    ];
    for (id, kind, content) in &kinds {
        document.blocks.push(make_block(id, kind, content));
        document.root.push(id.to_string());
    }
    document.validate().expect("测试模型应通过 schema 校验");

    let exported = oo_docx::write_docx_with_report(&document, &[]).expect("导出失败");
    assert!(!exported.loss_report.is_empty());
    assert_eq!(
        exported.loss_report.header_summary().as_deref(),
        Some("todoState:1,linkTarget:1,calloutStyle:1,structuralContainer:2")
    );
    // Text content survives every approximation.
    let roundtrip = oo_docx::parse_docx(&exported.bytes, "loss-roundtrip").unwrap();
    assert!(roundtrip.plain_text().contains("买牛奶"));
    assert!(roundtrip.plain_text().contains("示例链接"));

    // Tables keep the hard-error discipline and never enter the report.
    let mut with_table = parse("minimal.docx");
    with_table
        .blocks
        .push(make_block("tbl", &oo_schema::DocumentBlockKind::Table, ""));
    with_table.root.push("tbl".into());
    // A table block requires table data; reuse the parser's own model instead of
    // hand-building it: flip an existing paragraph's data would fail validate(),
    // so assert via the dedicated error path using a synthetic table payload.
    with_table.blocks.last_mut().unwrap().data = {
        use oo_schema::TableRange;
        use oo_schema::{TableBlock, TableCell, TableColumn, TableRow};
        oo_schema::BlockData::Table(TableBlock {
            columns: vec![TableColumn {
                id: "c1".into(),
                width: None,
            }],
            rows: vec![TableRow {
                id: "r1".into(),
                height: None,
                cells: vec![TableCell {
                    id: "cell-1".into(),
                    content: text("cell").unwrap(),
                    style: Default::default(),
                }],
            }],
            merged_ranges: Vec::<TableRange>::new(),
        })
    };
    let error = oo_docx::write_docx_with_assets(&with_table, &[]).unwrap_err();
    assert!(matches!(
        error,
        oo_docx::DocxError::UnsupportedBlock("table")
    ));
}
