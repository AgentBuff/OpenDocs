//! Offline-only Artifact schema migration.
//!
//! Runtime parsers accept only the current schema version. This module is intentionally a
//! one-way converter used by maintenance tooling before a deployment; it is never called from
//! HTTP handlers, the editor, or WASM.

use serde_json::{Map, Value};

use crate::{ArtifactEnvelope, ArtifactKind, BlockPresentation, CURRENT_SCHEMA_VERSION};

#[derive(Debug, thiserror::Error)]
pub enum ArtifactMigrationError {
    #[error("迁移输入必须是 Artifact object")]
    InvalidEnvelope,
    #[error("仅支持从 schema v1/v2/v3/v4 迁移，实际为 {0}")]
    UnsupportedSourceVersion(u64),
    #[error("schema v4 的 Presentation 必须使用专用的 v4→v5 staging migration")]
    PresentationRequiresDedicatedMigration,
    #[error("v1 Link block {0} 缺少非空 attrs.url")]
    MissingLinkTarget(String),
    #[error("v1 Todo block {0} 的 attrs.checked 必须是 boolean")]
    InvalidTodoChecked(String),
    #[error("block {0} 的 attrs/payload 与 v4 schema 不兼容")]
    InvalidBlockShape(String),
    #[error("block {0} 包含无法迁移的 attrs 字段：{1}")]
    UnsupportedBlockAttribute(String, String),
    #[error("block {0} 的旧段落格式无效：{1}")]
    InvalidBlockPresentation(String, String),
    #[error("迁移后的 Artifact 无效：{0}")]
    InvalidTarget(#[from] crate::SchemaValidationError),
    #[error("迁移后的 Artifact 无法反序列化：{0}")]
    Deserialize(#[from] serde_json::Error),
}

/// Convert a raw v1 Artifact JSON object into a validated current envelope.
///
/// V3 includes the typed Todo/Link payloads introduced by the first cutover and the explicit
/// `mergedRanges` field for table payloads. The function deliberately performs the complete
/// offline cutover in one pass; there is no v1/v2 runtime representation to expose.
pub fn migrate_artifact_v1_to_v3(
    mut raw: Value,
) -> Result<ArtifactEnvelope, ArtifactMigrationError> {
    let envelope = raw
        .as_object_mut()
        .ok_or(ArtifactMigrationError::InvalidEnvelope)?;
    let version = envelope
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .ok_or(ArtifactMigrationError::InvalidEnvelope)?;
    if version != 1 {
        return Err(ArtifactMigrationError::UnsupportedSourceVersion(version));
    }

    if let Some(document) = document_data_mut(envelope) {
        let blocks = document
            .get_mut("blocks")
            .and_then(Value::as_array_mut)
            .ok_or(ArtifactMigrationError::InvalidEnvelope)?;
        for block in blocks {
            migrate_document_block(block)?;
        }
    }

    envelope.insert(
        "schemaVersion".into(),
        Value::Number(serde_json::Number::from(3)),
    );
    migrate_artifact_v3_to_v4(raw)
}

/// Add the v3 table merge-range field to a validated v2 snapshot and finish the current cutover.
pub fn migrate_artifact_v2_to_v3(
    mut raw: Value,
) -> Result<ArtifactEnvelope, ArtifactMigrationError> {
    let envelope = raw
        .as_object_mut()
        .ok_or(ArtifactMigrationError::InvalidEnvelope)?;
    let version = envelope
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .ok_or(ArtifactMigrationError::InvalidEnvelope)?;
    if version != 2 {
        return Err(ArtifactMigrationError::UnsupportedSourceVersion(version));
    }
    if let Some(document) = document_data_mut(envelope) {
        let blocks = document
            .get_mut("blocks")
            .and_then(Value::as_array_mut)
            .ok_or(ArtifactMigrationError::InvalidEnvelope)?;
        for block in blocks {
            migrate_inline_styles(block)?;
            let Some(block_object) = block.as_object_mut() else {
                return Err(ArtifactMigrationError::InvalidEnvelope);
            };
            let is_table = block_object
                .get("kind")
                .and_then(Value::as_object)
                .and_then(|kind| kind.get("type"))
                .and_then(Value::as_str)
                == Some("table");
            if is_table {
                if let Some(data) = block_object
                    .get_mut("payload")
                    .and_then(Value::as_object_mut)
                    .and_then(|payload| payload.get_mut("data"))
                    .and_then(Value::as_object_mut)
                {
                    data.entry("mergedRanges")
                        .or_insert_with(|| Value::Array(Vec::new()));
                } else {
                    return Err(ArtifactMigrationError::InvalidEnvelope);
                }
            }
        }
    }
    envelope.insert(
        "schemaVersion".into(),
        Value::Number(serde_json::Number::from(3)),
    );
    migrate_artifact_v3_to_v4(raw)
}

/// Convert a validated v3 snapshot into the strict Document block shape and current envelope.
///
/// This is an offline-only operation. It removes the open-ended `attrs` and `payload` fields,
/// maps paragraph values into `presentation`, and writes the typed `data` envelope. The online
/// parser never accepts the old fields and therefore cannot accidentally perform a dual read.
pub fn migrate_artifact_v3_to_v4(
    mut raw: Value,
) -> Result<ArtifactEnvelope, ArtifactMigrationError> {
    let envelope = raw
        .as_object_mut()
        .ok_or(ArtifactMigrationError::InvalidEnvelope)?;
    let version = envelope
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .ok_or(ArtifactMigrationError::InvalidEnvelope)?;
    if version != 3 {
        return Err(ArtifactMigrationError::UnsupportedSourceVersion(version));
    }
    if let Some(document) = document_data_mut(envelope) {
        let blocks = document
            .get_mut("blocks")
            .and_then(Value::as_array_mut)
            .ok_or(ArtifactMigrationError::InvalidEnvelope)?;
        for block in blocks {
            migrate_v3_block_to_v4(block)?;
        }
    }
    envelope.insert(
        "schemaVersion".into(),
        Value::Number(serde_json::Number::from(CURRENT_SCHEMA_VERSION)),
    );
    let artifact: ArtifactEnvelope = serde_json::from_value(raw)?;
    artifact.validate()?;
    Ok(artifact)
}

/// Promote an already strict v4 Document envelope to the current v5 Artifact
/// envelope. v5 changes Presentation only; Document's typed block shape is
/// unchanged. Keeping this as an explicit offline step prevents the online
/// parser from accepting two schema versions.
pub fn migrate_artifact_v4_to_v5(raw: Value) -> Result<ArtifactEnvelope, ArtifactMigrationError> {
    let mut envelope = raw
        .as_object()
        .ok_or(ArtifactMigrationError::InvalidEnvelope)?
        .clone();
    let version = envelope
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .ok_or(ArtifactMigrationError::InvalidEnvelope)?;
    if version != 4 {
        return Err(ArtifactMigrationError::UnsupportedSourceVersion(version));
    }
    let kind = envelope
        .get("kind")
        .and_then(Value::as_str)
        .ok_or(ArtifactMigrationError::InvalidEnvelope)?;
    if kind != "document" {
        return Err(ArtifactMigrationError::PresentationRequiresDedicatedMigration);
    }
    envelope.insert(
        "schemaVersion".into(),
        Value::Number(serde_json::Number::from(CURRENT_SCHEMA_VERSION)),
    );
    let artifact: ArtifactEnvelope = serde_json::from_value(Value::Object(envelope))?;
    if artifact.kind != ArtifactKind::Document {
        return Err(ArtifactMigrationError::InvalidEnvelope);
    }
    artifact.validate()?;
    Ok(artifact)
}

fn migrate_v3_block_to_v4(value: &mut Value) -> Result<(), ArtifactMigrationError> {
    let block = value
        .as_object_mut()
        .ok_or(ArtifactMigrationError::InvalidEnvelope)?;
    let block_id = block
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("<unknown>")
        .to_owned();
    if block.contains_key("presentation") || block.contains_key("data") {
        return Err(ArtifactMigrationError::InvalidBlockShape(block_id));
    }

    let kind = block
        .get("kind")
        .and_then(Value::as_object)
        .and_then(|kind| kind.get("type"))
        .and_then(Value::as_str)
        .ok_or(ArtifactMigrationError::InvalidEnvelope)?
        .to_owned();
    let attrs = match block.remove("attrs") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(attrs)) => attrs,
        Some(_) => return Err(ArtifactMigrationError::InvalidBlockShape(block_id)),
    };
    let presentation = migrate_block_presentation(&kind, &block_id, &attrs)?;
    let data = migrate_block_data(&kind, &block_id, block.remove("payload"))?;
    block.insert("presentation".into(), presentation);
    block.insert("data".into(), data);
    Ok(())
}

fn migrate_block_presentation(
    kind: &str,
    block_id: &str,
    attrs: &Map<String, Value>,
) -> Result<Value, ArtifactMigrationError> {
    let mut presentation = serde_json::Map::new();
    presentation.insert("align".into(), Value::String("left".into()));
    presentation.insert("list".into(), Value::Null);
    presentation.insert("indentStart".into(), Value::Number(0.into()));
    presentation.insert("indentEnd".into(), Value::from(0.0));
    presentation.insert("spacingBefore".into(), Value::from(0.0));
    presentation.insert("spacingAfter".into(), Value::from(0.0));
    presentation.insert("lineHeight".into(), Value::from(1.0));

    let style = attrs.get("paragraphStyle").and_then(Value::as_object);
    let known = [
        "align",
        "listType",
        "listLevel",
        "indentLevel",
        "indentRight",
        "spacingBefore",
        "spacingAfter",
        "lineHeight",
        "paragraphStyle",
        "namedStyle",
    ];
    for key in attrs.keys() {
        if !(known.contains(&key.as_str())
            || (kind == "todo" && key == "checked")
            || (kind == "link" && key == "url"))
        {
            return Err(ArtifactMigrationError::UnsupportedBlockAttribute(
                block_id.to_owned(),
                key.clone(),
            ));
        }
    }

    let align = attrs.get("align").and_then(Value::as_str).or_else(|| {
        style
            .and_then(|style| style.get("alignment"))
            .and_then(Value::as_str)
    });
    if let Some(align) = align {
        if !matches!(align, "left" | "center" | "right" | "justify") {
            return Err(ArtifactMigrationError::InvalidBlockPresentation(
                block_id.to_owned(),
                "align 必须是 left、center、right 或 justify".into(),
            ));
        }
        presentation.insert("align".into(), Value::String(align.into()));
    }

    let list_type = attrs.get("listType").and_then(Value::as_str).or_else(|| {
        style
            .and_then(|style| style.get("list"))
            .and_then(Value::as_object)
            .and_then(|list| list.get("kind"))
            .and_then(Value::as_str)
    });
    let list_level = attrs.get("listLevel").and_then(Value::as_u64).or_else(|| {
        style
            .and_then(|style| style.get("list"))
            .and_then(Value::as_object)
            .and_then(|list| list.get("level"))
            .and_then(Value::as_u64)
    });
    if let Some(list_type) = list_type {
        if !matches!(list_type, "bullet" | "ordered") {
            return Err(ArtifactMigrationError::InvalidBlockPresentation(
                block_id.to_owned(),
                "listType 必须是 bullet 或 ordered".into(),
            ));
        }
        let level = list_level.unwrap_or(0);
        if level > 20 {
            return Err(ArtifactMigrationError::InvalidBlockPresentation(
                block_id.to_owned(),
                "listLevel 必须是 0 到 20 的整数".into(),
            ));
        }
        presentation.insert(
            "list".into(),
            serde_json::json!({ "kind": list_type, "level": level }),
        );
    } else if list_level.is_some() {
        return Err(ArtifactMigrationError::InvalidBlockPresentation(
            block_id.to_owned(),
            "listLevel 不能脱离 listType".into(),
        ));
    }

    let indent_start = attrs
        .get("indentLevel")
        .and_then(Value::as_u64)
        .or_else(|| {
            style
                .and_then(|style| style.get("indentLeft"))
                .and_then(Value::as_f64)
                .map(|value| value.round() as u64)
        })
        .unwrap_or(0);
    if indent_start > 20 {
        return Err(ArtifactMigrationError::InvalidBlockPresentation(
            block_id.to_owned(),
            "indentLevel 必须是 0 到 20 的整数".into(),
        ));
    }
    presentation.insert("indentStart".into(), Value::Number(indent_start.into()));

    set_non_negative_number(
        &mut presentation,
        "indentEnd",
        attrs
            .get("indentRight")
            .or_else(|| style.and_then(|style| style.get("indentRight"))),
        block_id,
    )?;
    set_non_negative_number(
        &mut presentation,
        "spacingBefore",
        attrs
            .get("spacingBefore")
            .or_else(|| style.and_then(|style| style.get("spaceBefore"))),
        block_id,
    )?;
    set_non_negative_number(
        &mut presentation,
        "spacingAfter",
        attrs
            .get("spacingAfter")
            .or_else(|| style.and_then(|style| style.get("spaceAfter"))),
        block_id,
    )?;
    set_line_height(
        &mut presentation,
        attrs
            .get("lineHeight")
            .or_else(|| style.and_then(|style| style.get("lineHeight"))),
        block_id,
    )?;

    let named_style = attrs
        .get("namedStyle")
        .or_else(|| style.and_then(|style| style.get("name")))
        .and_then(|value| {
            value
                .as_str()
                .map(|name| serde_json::json!({ "name": name }))
        });
    if let Some(named_style) = named_style {
        presentation.insert("namedStyle".into(), named_style);
    }
    // Materialize the type once here so migration errors identify the block before the whole
    // artifact is deserialized, while leaving the final validation to ArtifactEnvelope.
    serde_json::from_value::<BlockPresentation>(Value::Object(presentation.clone())).map_err(
        |error| {
            ArtifactMigrationError::InvalidBlockPresentation(block_id.to_owned(), error.to_string())
        },
    )?;
    Ok(Value::Object(presentation))
}

fn set_non_negative_number(
    presentation: &mut Map<String, Value>,
    field: &str,
    value: Option<&Value>,
    block_id: &str,
) -> Result<(), ArtifactMigrationError> {
    let Some(value) = value else { return Ok(()) };
    let value = value.as_f64().ok_or_else(|| {
        ArtifactMigrationError::InvalidBlockPresentation(
            block_id.to_owned(),
            format!("{field} 必须是数字"),
        )
    })?;
    if !value.is_finite() || !(0.0..=1_000.0).contains(&value) {
        return Err(ArtifactMigrationError::InvalidBlockPresentation(
            block_id.to_owned(),
            format!("{field} 必须在 0 到 1000 之间"),
        ));
    }
    presentation.insert(field.into(), Value::from(value));
    Ok(())
}

fn set_line_height(
    presentation: &mut Map<String, Value>,
    value: Option<&Value>,
    block_id: &str,
) -> Result<(), ArtifactMigrationError> {
    let Some(value) = value else { return Ok(()) };
    let value = value.as_f64().ok_or_else(|| {
        ArtifactMigrationError::InvalidBlockPresentation(
            block_id.to_owned(),
            "lineHeight 必须是数字".into(),
        )
    })?;
    if !value.is_finite() || !(0.1..=10.0).contains(&value) {
        return Err(ArtifactMigrationError::InvalidBlockPresentation(
            block_id.to_owned(),
            "lineHeight 必须在 0.1 到 10 之间".into(),
        ));
    }
    presentation.insert("lineHeight".into(), Value::from(value));
    Ok(())
}

fn migrate_block_data(
    kind: &str,
    block_id: &str,
    payload: Option<Value>,
) -> Result<Value, ArtifactMigrationError> {
    let Some(payload) = payload.filter(|value| !value.is_null()) else {
        if matches!(kind, "todo" | "link" | "image" | "table" | "code") {
            return Err(ArtifactMigrationError::InvalidBlockShape(
                block_id.to_owned(),
            ));
        }
        return Ok(serde_json::json!({ "type": "none" }));
    };
    let payload = payload
        .as_object()
        .ok_or_else(|| ArtifactMigrationError::InvalidBlockShape(block_id.to_owned()))?;
    let type_id = payload
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| ArtifactMigrationError::InvalidBlockShape(block_id.to_owned()))?;
    let data = payload.get("data").cloned().unwrap_or(Value::Null);
    if matches!(type_id, "image" | "table" | "code" | "todo" | "link") {
        return Ok(Value::Object(Map::from_iter([
            ("type".into(), Value::String(type_id.into())),
            ("data".into(), data),
        ])));
    }
    if matches!(kind, "extension")
        || !matches!(
            kind,
            "paragraph"
                | "heading"
                | "quote"
                | "callout"
                | "divider"
                | "page"
                | "columns"
                | "column"
        )
    {
        return Ok(
            serde_json::json!({ "type": "extension", "data": { "typeId": kind, "raw": payload } }),
        );
    }
    Err(ArtifactMigrationError::InvalidBlockShape(
        block_id.to_owned(),
    ))
}

fn document_data_mut(envelope: &mut Map<String, Value>) -> Option<&mut Map<String, Value>> {
    let payload = envelope.get_mut("payload")?.as_object_mut()?;
    if payload.get("kind")?.as_str()? != "document" {
        return None;
    }
    payload.get_mut("data")?.as_object_mut()
}

/// Convert the retired inline `attrs` object into the strict v3 `style` object. This is an
/// offline-only step; the online parser never reads the old field or keeps a compatibility alias.
fn migrate_inline_styles(block: &mut Value) -> Result<(), ArtifactMigrationError> {
    let block_object = block
        .as_object_mut()
        .ok_or(ArtifactMigrationError::InvalidEnvelope)?;
    if let Some(content) = block_object.get_mut("content") {
        migrate_rich_text(content)?;
    }
    if let Some(rows) = block_object
        .get_mut("payload")
        .and_then(Value::as_object_mut)
        .and_then(|payload| payload.get_mut("data"))
        .and_then(Value::as_object_mut)
        .and_then(|data| data.get_mut("rows"))
        .and_then(Value::as_array_mut)
    {
        for row in rows {
            if let Some(cells) = row.get_mut("cells").and_then(Value::as_array_mut) {
                for cell in cells {
                    if let Some(content) = cell.get_mut("content") {
                        migrate_rich_text(content)?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn migrate_rich_text(value: &mut Value) -> Result<(), ArtifactMigrationError> {
    let Some(runs) = value
        .as_object_mut()
        .and_then(|content| content.get_mut("runs"))
        .and_then(Value::as_array_mut)
    else {
        return Ok(());
    };
    for run in runs {
        let run_object = run
            .as_object_mut()
            .ok_or(ArtifactMigrationError::InvalidEnvelope)?;
        let Some(attrs) = run_object.remove("attrs") else {
            continue;
        };
        if run_object.contains_key("style") {
            return Err(ArtifactMigrationError::InvalidEnvelope);
        }
        let attrs = attrs
            .as_object()
            .ok_or(ArtifactMigrationError::InvalidEnvelope)?;
        let mut style = Map::new();
        for (key, value) in attrs {
            let canonical = if key == "backgroundColor" {
                "highlight"
            } else {
                key.as_str()
            };
            if !matches!(
                canonical,
                "bold"
                    | "italic"
                    | "underline"
                    | "strikethrough"
                    | "fontFamily"
                    | "fontSize"
                    | "color"
                    | "highlight"
                    | "verticalAlign"
            ) {
                return Err(ArtifactMigrationError::InvalidEnvelope);
            }
            style.insert(canonical.to_string(), value.clone());
        }
        run_object.insert("style".into(), Value::Object(style));
    }
    Ok(())
}

fn migrate_document_block(value: &mut Value) -> Result<(), ArtifactMigrationError> {
    migrate_inline_styles(value)?;
    let block = value
        .as_object_mut()
        .ok_or(ArtifactMigrationError::InvalidEnvelope)?;
    let block_id = block
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("<unknown>")
        .to_string();
    let kind = block
        .get("kind")
        .and_then(Value::as_object)
        .and_then(|kind| kind.get("type"))
        .and_then(Value::as_str)
        .ok_or(ArtifactMigrationError::InvalidEnvelope)?;
    match kind {
        "table" => {
            if let Some(data) = block
                .get_mut("payload")
                .and_then(Value::as_object_mut)
                .and_then(|payload| payload.get_mut("data"))
                .and_then(Value::as_object_mut)
            {
                data.entry("mergedRanges")
                    .or_insert_with(|| Value::Array(Vec::new()));
            }
        }
        "todo" => {
            if block.get("payload").is_none() || block.get("payload").is_some_and(Value::is_null) {
                let checked = match block
                    .get_mut("attrs")
                    .and_then(Value::as_object_mut)
                    .and_then(|attrs| attrs.remove("checked"))
                {
                    None => false,
                    Some(Value::Bool(checked)) => checked,
                    Some(_) => return Err(ArtifactMigrationError::InvalidTodoChecked(block_id)),
                };
                block.insert(
                    "payload".into(),
                    serde_json::json!({ "type": "todo", "data": { "checked": checked } }),
                );
            }
        }
        "code" => {
            // Early v1 code blocks did not persist a configuration payload at all.  This is
            // an offline-only normalization: materialize the canonical defaults before the
            // strict v4 converter runs.  The online v4 schema remains strict and still rejects
            // a code block whose data envelope is missing.
            if block.get("payload").is_none() || block.get("payload").is_some_and(Value::is_null) {
                let config = serde_json::to_value(crate::CodeBlockConfig::default())?;
                block.insert(
                    "payload".into(),
                    serde_json::json!({ "type": "code", "data": config }),
                );
            }
        }
        "link"
            if block.get("payload").is_none()
                || block.get("payload").is_some_and(Value::is_null) =>
        {
            let url = block
                .get_mut("attrs")
                .and_then(Value::as_object_mut)
                .and_then(|attrs| attrs.remove("url"))
                .and_then(|url| url.as_str().map(str::to_owned))
                .filter(|url| !url.trim().is_empty())
                .ok_or(ArtifactMigrationError::MissingLinkTarget(block_id))?;
            block.insert(
                "payload".into(),
                serde_json::json!({ "type": "link", "data": { "url": url } }),
            );
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_todo_and_link_data_without_runtime_fallback() {
        let raw = serde_json::json!({
            "format": "open-office-artifact",
            "schemaVersion": 1,
            "artifactId": "doc-1",
            "revision": 4,
            "kind": "document",
            "payload": {
                "kind": "document",
                "data": {
                    "root": ["todo", "link"],
                    "blocks": [
                        { "id": "todo", "kind": { "type": "todo" }, "attrs": { "checked": true }, "content": { "text": "ship", "runs": [] }, "children": [] },
                        { "id": "link", "kind": { "type": "link" }, "attrs": { "url": "https://openoffice.example" }, "content": { "text": "Open Office", "runs": [] }, "children": [] }
                    ],
                    "pageSetup": null
                }
            }
        });

        let migrated = migrate_artifact_v1_to_v3(raw).unwrap();
        assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
        let crate::ArtifactPayload::Document(document) = migrated.payload else {
            panic!("expected document");
        };
        let wire = serde_json::to_value(&document).unwrap();
        assert!(wire["blocks"][0].get("attrs").is_none());
        assert!(wire["blocks"][0].get("payload").is_none());
        assert!(matches!(document.blocks[0].data, crate::BlockData::Todo(_)));
        assert!(matches!(document.blocks[1].data, crate::BlockData::Link(_)));
    }

    #[test]
    fn migrates_inline_attrs_to_strict_style_once() {
        let raw = serde_json::json!({
            "format": "open-office-artifact",
            "schemaVersion": 1,
            "artifactId": "doc-inline",
            "revision": 0,
            "kind": "document",
            "payload": { "kind": "document", "data": {
                "root": ["p"],
                "blocks": [{ "id": "p", "kind": { "type": "paragraph" }, "attrs": {},
                    "content": { "text": "hi", "runs": [{ "start": 0, "end": 2,
                        "attrs": { "bold": true, "backgroundColor": "#fff2ac" } }] },
                    "children": [] }], "pageSetup": null
            }}
        });
        let migrated = migrate_artifact_v1_to_v3(raw).unwrap();
        let crate::ArtifactPayload::Document(document) = migrated.payload else {
            panic!("expected document")
        };
        let run = document.blocks[0].content.as_ref().unwrap().runs[0].clone();
        assert!(run.style.bold);
        assert_eq!(run.style.highlight.as_deref(), Some("#fff2ac"));
        let wire = serde_json::to_value(run).unwrap();
        assert!(wire.get("attrs").is_none());
        assert_eq!(wire["style"]["highlight"], "#fff2ac");
    }

    #[test]
    fn materializes_defaults_for_legacy_code_without_payload() {
        let raw = serde_json::json!({
            "format": "open-office-artifact",
            "schemaVersion": 1,
            "artifactId": "doc-code-v1",
            "revision": 1,
            "kind": "document",
            "payload": { "kind": "document", "data": {
                "root": ["code"],
                "blocks": [{
                    "id": "code",
                    "kind": { "type": "code" },
                    "attrs": {},
                    "content": { "text": "fn main() {}", "runs": [] },
                    "children": [],
                    "payload": null
                }],
                "pageSetup": null
            }}
        });

        let migrated = migrate_artifact_v1_to_v3(raw).unwrap();
        let crate::ArtifactPayload::Document(document) = migrated.payload else {
            panic!("expected document")
        };
        let crate::BlockData::Code(config) = &document.blocks[0].data else {
            panic!("expected code data")
        };
        assert_eq!(config, &crate::CodeBlockConfig::default());
    }

    #[test]
    fn migrates_v2_table_merge_ranges_without_runtime_fallback() {
        let raw = serde_json::json!({
            "format": "open-office-artifact",
            "schemaVersion": 2,
            "artifactId": "doc-1",
            "revision": 4,
            "kind": "document",
            "payload": { "kind": "document", "data": { "root": ["table"], "blocks": [{
                "id": "table", "kind": { "type": "table" }, "attrs": {}, "content": null, "children": [],
                "payload": { "type": "table", "data": { "columns": [{ "id": "c-1", "width": null }, { "id": "c-2", "width": null }], "rows": [{ "id": "r-1", "cells": [{ "id": "a", "content": { "text": "A", "runs": [] } }, { "id": "b", "content": { "text": "B", "runs": [] } }]}] } }
            }], "pageSetup": null } }
        });
        let migrated = migrate_artifact_v2_to_v3(raw).unwrap();
        let crate::ArtifactPayload::Document(document) = migrated.payload else {
            panic!("expected document")
        };
        let crate::BlockData::Table(table) = &document.blocks[0].data else {
            panic!("expected table")
        };
        assert!(table.merged_ranges.is_empty());
    }

    #[test]
    fn migrates_v3_block_attrs_and_payload_to_strict_v4_shape() {
        let raw = serde_json::json!({
            "format": "open-office-artifact",
            "schemaVersion": 3,
            "artifactId": "doc-v3",
            "revision": 7,
            "kind": "document",
            "payload": { "kind": "document", "data": {
                "root": ["p", "future"],
                "blocks": [
                    { "id": "p", "kind": { "type": "paragraph" },
                      "attrs": { "align": "center", "listType": "ordered", "listLevel": 2, "lineHeight": 1.5 },
                      "content": { "text": "hello", "runs": [] }, "children": [] },
                    { "id": "future", "kind": { "type": "future.widget", "provider": "demo" },
                      "attrs": {}, "content": null, "children": [], "payload": { "type": "future.widget", "data": { "answer": 42 } } }
                ], "pageSetup": null
            }}
        });
        let migrated = migrate_artifact_v3_to_v4(raw).unwrap();
        assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
        let crate::ArtifactPayload::Document(document) = migrated.payload else {
            panic!("expected document")
        };
        assert_eq!(
            document.blocks[0].presentation.align,
            crate::BlockAlignment::Center
        );
        assert_eq!(
            document.blocks[0].presentation.list.as_ref().unwrap().level,
            2
        );
        assert_eq!(document.blocks[0].presentation.line_height, 1.5);
        let crate::BlockData::Extension(extension) = &document.blocks[1].data else {
            panic!("expected extension")
        };
        assert_eq!(extension.type_id, "future.widget");
        assert_eq!(extension.raw["data"]["answer"], 42);
        let wire = serde_json::to_value(&document).unwrap();
        assert!(wire["blocks"][0].get("attrs").is_none());
        assert!(wire["blocks"][0].get("payload").is_none());
        assert!(wire["blocks"][0].get("presentation").is_some());
        assert!(wire["blocks"][0].get("data").is_some());
        let round_trip: crate::DocumentModel = serde_json::from_value(wire).unwrap();
        assert_eq!(round_trip, document);
    }

    #[test]
    fn rejects_ambiguous_v3_shape_and_unknown_attrs_without_guessing() {
        let raw = serde_json::json!({
            "format": "open-office-artifact", "schemaVersion": 3,
            "artifactId": "doc-v3", "revision": 0, "kind": "document",
            "payload": { "kind": "document", "data": {
                "root": ["p"], "blocks": [{
                    "id": "p", "kind": { "type": "paragraph" }, "attrs": { "madeUp": true },
                    "presentation": { "align": "left" }, "content": null, "children": [], "payload": null
                }], "pageSetup": null
            }}
        });
        assert!(matches!(
            migrate_artifact_v3_to_v4(raw),
            Err(ArtifactMigrationError::InvalidBlockShape(_))
        ));

        let raw = serde_json::json!({
            "format": "open-office-artifact", "schemaVersion": 3,
            "artifactId": "doc-v3", "revision": 0, "kind": "document",
            "payload": { "kind": "document", "data": {
                "root": ["p"], "blocks": [{
                    "id": "p", "kind": { "type": "paragraph" }, "attrs": { "madeUp": true },
                    "content": null, "children": [], "payload": null
                }], "pageSetup": null
            }}
        });
        assert!(matches!(
            migrate_artifact_v3_to_v4(raw),
            Err(ArtifactMigrationError::UnsupportedBlockAttribute(_, _))
        ));
    }

    #[test]
    fn promotes_a_strict_v4_document_without_online_dual_read() {
        let current = crate::ArtifactEnvelope::new(
            "doc-v4",
            crate::ArtifactPayload::Document(crate::DocumentModel::default()),
        );
        let mut raw = serde_json::to_value(current).unwrap();
        raw["schemaVersion"] = serde_json::json!(4);
        let migrated = migrate_artifact_v4_to_v5(raw).unwrap();
        assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(migrated.kind, ArtifactKind::Document);

        let presentation = serde_json::json!({
            "format": "open-office-artifact", "schemaVersion": 4,
            "artifactId": "deck-v4", "revision": 1, "kind": "presentation",
            "payload": { "kind": "presentation", "data": {} }
        });
        assert!(matches!(
            migrate_artifact_v4_to_v5(presentation),
            Err(ArtifactMigrationError::PresentationRequiresDedicatedMigration)
        ));
    }
}
