//! Golden 快照：协议 schema 的机器可读形状必须保持稳定。
//!
//! 这些 schema 由 Rust 类型派生（`generate_contract_schemas`，ADR-0010），
//! 是 OpenAPI components 与 SDK 类型生成的单源。任何 wire 兼容性破坏都会
//! 改变本快照并使测试失败；有意演进协议时，运行
//! `UPDATE_CONTRACT_SNAPSHOT=1 cargo test -p oo-protocol`
//! 刷新快照并把 diff 作为 PR 的评审焦点。

use std::fs;
use std::path::PathBuf;

const SNAPSHOT_RELATIVE: &str = "tests/snapshots/contract_schemas.json";

#[test]
fn contract_schema_snapshot_is_stable() {
    let schemas = oo_protocol::generate_contract_schemas();
    let actual = serde_json::to_string_pretty(&serde_json::Value::Object(schemas.clone()))
        .expect("schema map must serialize");

    let snapshot_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SNAPSHOT_RELATIVE);
    if std::env::var_os("UPDATE_CONTRACT_SNAPSHOT").is_some() {
        if let Some(parent) = snapshot_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&snapshot_path, format!("{actual}\n")).unwrap();
    }

    let expected = fs::read_to_string(&snapshot_path).unwrap_or_else(|_| {
        panic!(
            "缺少快照文件 {}；请先以 UPDATE_CONTRACT_SNAPSHOT=1 生成",
            SNAPSHOT_RELATIVE
        )
    });
    assert_eq!(
        actual.trim(),
        expected.trim(),
        "协议 schema 形状发生变化。若为有意的破坏式演进（需要版本迁移），\
         以 UPDATE_CONTRACT_SNAPSHOT=1 刷新快照并在 PR 中评审 diff。"
    );

    // The generator must cover exactly the advertised surface — no phantom
    // entries, no forgotten types.
    let mut names: Vec<_> = schemas.keys().cloned().collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "ArtifactCapability",
            "ArtifactCapabilityStatus",
            "ArtifactCommandCapability",
            "ArtifactCommandEnvelope",
            "ArtifactFeatureCapabilities",
            "ArtifactTransportCapability",
            "CapabilityCatalog",
            "CommandRecord",
            "CommitResult",
            "DomainEventRecord",
            "EntityRef",
            "Invalidation",
            "MutationRecord",
            "OperationRecord",
            "TransactionOrigin",
        ]
    );
}
