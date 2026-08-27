//! open-office 的 API 服务。
//!
//! 当前提供统一 Artifact 的导入、列表、snapshot/revision、事务与导出边界。
//! 各 Artifact engine 通过同一资源层接入，尚未实现的命令能力会返回稳定的能力错误。

pub mod artifact_routes;
pub mod auth;
pub mod db;
#[path = "routes.rs"]
pub mod document_support;
pub mod error;
pub mod events;
pub mod mindmap_support;
pub mod presence;
pub mod presentation_migration;
pub mod presentation_support;
pub mod projection;
pub mod request_context;
pub mod spreadsheet_support;
pub mod store;
pub mod whiteboard_support;

use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::http::{header, HeaderName, HeaderValue, Method};
use axum::routing::{get, post, put};
use axum::Router;
use sqlx::SqlitePool;
use tower_http::cors::CorsLayer;

use crate::store::BlobStore;
use oo_protocol::CAPABILITIES_PATH;

/// 处理器共享的应用状态。
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub store: Arc<dyn BlobStore>,
    /// 本地 BlobStore 与 SQLite 之间没有分布式事务。单机阶段先串行化可冲突的写入，
    /// 让「检查版本 → 写内容 → 推进版本」成为一个一致的临界区。
    pub write_lock: Arc<tokio::sync::Mutex<()>>,
    /// Ephemeral collaboration state. Never write this into SQLite/blob storage.
    pub presence: Arc<tokio::sync::Mutex<presence::PresenceStore>>,
}

const MAX_UPLOAD_BODY_BYTES: usize = 32 * 1024 * 1024;
const MAX_JSON_BODY_BYTES: usize = 8 * 1024 * 1024;

/// 组装路由。
///
/// 独立成函数是为了让集成测试能直接拿到 `Router` 用 `tower::ServiceExt::oneshot`
/// 发请求，不必真的监听端口。
pub fn build_router(state: AppState) -> Router {
    // 前端开发服务器固定使用 5174。不要用 Any：它会把本地 API 暴露给任意网页，
    // 也会让后续接入真实认证时很难发现跨源边界已经失守。
    let cors = CorsLayer::new()
        .allow_origin([
            HeaderValue::from_static("http://127.0.0.1:5174"),
            HeaderValue::from_static("http://localhost:5174"),
        ])
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::PUT,
            Method::DELETE,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::IF_MATCH,
            header::IF_NONE_MATCH,
            HeaderName::from_static("x-transaction-id"),
            HeaderName::from_static("idempotency-key"),
            HeaderName::from_static("x-request-id"),
        ])
        // Revision-safe projection polling is useful to browser SDKs and
        // agent adapters alike.  The body still carries `revision`, while
        // these headers enable standard conditional GET without inventing a
        // second transport protocol.
        .expose_headers([header::ETAG, HeaderName::from_static("x-artifact-revision")]);

    Router::new()
        .route("/api/health", get(document_support::health))
        .route(CAPABILITIES_PATH, get(artifact_routes::capabilities))
        .route("/api/artifacts/{id}/events", get(events::list))
        .route("/api/artifacts/{id}/presence", get(presence::list))
        .route(
            "/api/artifacts/{id}/presence/{session_id}",
            axum::routing::put(presence::put),
        )
        .route("/api/artifacts/{id}/outline", get(projection::outline))
        .route("/api/artifacts/{id}/blocks", get(projection::blocks))
        .route(
            "/api/artifacts/{id}/presentation/outline",
            get(projection::presentation_outline),
        )
        .route(
            "/api/artifacts/{id}/presentation/slides/{slide_id}",
            get(projection::presentation_slide),
        )
        .route(
            "/api/artifacts/{id}/presentation/slides/{slide_id}/nodes/{node_id}",
            get(projection::presentation_node),
        )
        .route(
            "/api/artifacts/{id}/projection/{kind}",
            get(projection::artifact_projection),
        )
        .route(
            "/api/artifacts/{id}/blocks/{block_id}",
            get(projection::block),
        )
        // Canonical Artifact resource. There is intentionally no `/api/docs`
        // alias: clients choose the artifact resource and the persisted kind
        // selects the engine boundary.
        .route(
            artifact_routes::ARTIFACTS_PATH,
            get(artifact_routes::list).post(artifact_routes::create),
        )
        .route(
            artifact_routes::ARTIFACT_IMPORT_PATH,
            post(artifact_routes::import).layer(DefaultBodyLimit::max(MAX_UPLOAD_BODY_BYTES)),
        )
        .route(
            artifact_routes::ARTIFACT_ASSETS_PATH,
            get(artifact_routes::list_assets)
                .post(artifact_routes::upload_asset)
                .layer(DefaultBodyLimit::max(MAX_UPLOAD_BODY_BYTES)),
        )
        .route(
            artifact_routes::ARTIFACT_COLLABORATORS_PATH,
            get(artifact_routes::list_collaborators),
        )
        .route(
            artifact_routes::ARTIFACT_COLLABORATOR_PATH,
            put(artifact_routes::upsert_collaborator).delete(artifact_routes::delete_collaborator),
        )
        .route(
            artifact_routes::ARTIFACT_ASSET_PATH,
            get(artifact_routes::get_asset).delete(artifact_routes::delete_asset),
        )
        .route(
            artifact_routes::ARTIFACT_META_PATH,
            get(artifact_routes::meta)
                .patch(artifact_routes::patch)
                .delete(artifact_routes::delete),
        )
        .route(
            artifact_routes::ARTIFACT_SNAPSHOT_PATH,
            get(artifact_routes::snapshot)
                .put(artifact_routes::put_snapshot)
                .layer(DefaultBodyLimit::max(MAX_JSON_BODY_BYTES)),
        )
        .route(
            artifact_routes::ARTIFACT_REVISIONS_PATH,
            get(artifact_routes::revisions),
        )
        .route(
            artifact_routes::ARTIFACT_REVISION_PATH,
            get(artifact_routes::revision),
        )
        .route(
            artifact_routes::ARTIFACT_REVISION_RESTORE_PATH,
            post(artifact_routes::restore_revision),
        )
        .route(
            artifact_routes::ARTIFACT_HISTORY_PATH,
            get(artifact_routes::history),
        )
        .route(
            artifact_routes::ARTIFACT_TRANSACTIONS_PATH,
            post(artifact_routes::transactions).layer(DefaultBodyLimit::max(MAX_JSON_BODY_BYTES)),
        )
        .route(
            artifact_routes::ARTIFACT_SOURCE_PATH,
            get(artifact_routes::source),
        )
        .route(
            artifact_routes::ARTIFACT_EXPORT_PATH,
            get(artifact_routes::export),
        )
        .layer(cors)
        .with_state(state)
}
