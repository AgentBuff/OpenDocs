//! 统一的错误类型与 HTTP 响应映射。

use crate::db::ArtifactKind;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("未认证")]
    Unauthorized,

    #[error("没有权限访问该文档")]
    Forbidden,

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    BadRequest(String),

    #[error("文档版本已变化，请刷新后重试")]
    VersionConflict,

    #[error("文档版本已变化，请刷新后重试")]
    VersionConflictDetails(ConflictDetails),

    /// The canonical Artifact route is typed, but an engine must be wired for
    /// a kind before creation can be accepted.  Returning 501 makes this
    /// boundary explicit instead of silently persisting a spreadsheet/PPT as
    /// a Document.
    #[error("Artifact 类型 {0:?} 尚未接入独立 engine")]
    UnsupportedArtifact(ArtifactKind),

    #[error("Artifact 能力暂不支持：{0}")]
    UnsupportedCapability(String),

    #[error("文档解析失败：{0}")]
    Docx(#[from] oo_docx::DocxError),

    #[error("存储错误：{0}")]
    Store(#[from] crate::store::StoreError),

    #[error("数据库错误：{0}")]
    Database(#[from] sqlx::Error),

    #[error("内部错误：{0}")]
    Internal(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictDetails {
    pub artifact_id: String,
    pub requested_revision: u64,
    pub current_revision: u64,
    pub changed_entities: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorBody {
    error: String,
    /// 稳定的机器可读错误码，前端据此决定提示文案。
    code: &'static str,
    /// 可供日志、SDK 和 agent 关联一次失败请求的稳定标识。
    request_id: String,
    /// Stable retry guidance for SDKs. A client may retry only after applying
    /// the code-specific precondition (for example refreshing on 409).
    retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<ConflictDetails>,
}

impl AppError {
    fn parts(&self) -> (StatusCode, &'static str) {
        match self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            AppError::VersionConflict | AppError::VersionConflictDetails(_) => {
                (StatusCode::CONFLICT, "version_conflict")
            }
            AppError::UnsupportedArtifact(_) => {
                (StatusCode::NOT_IMPLEMENTED, "unsupported_artifact")
            }
            AppError::UnsupportedCapability(_) => {
                (StatusCode::NOT_IMPLEMENTED, "unsupported_capability")
            }
            // 上传了坏文件是客户端的问题，不是服务端故障。
            AppError::Docx(_) => (StatusCode::UNPROCESSABLE_ENTITY, "invalid_document"),
            AppError::Store(crate::store::StoreError::NotFound(_)) => {
                (StatusCode::NOT_FOUND, "not_found")
            }
            AppError::Store(_) | AppError::Database(_) | AppError::Internal(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal")
            }
        }
    }

    fn public_message(&self) -> String {
        match self {
            Self::Unauthorized => "未认证".into(),
            Self::Forbidden => "没有权限访问该文档".into(),
            Self::NotFound(message) | Self::BadRequest(message) => message.clone(),
            Self::VersionConflict | Self::VersionConflictDetails(_) => {
                "文档版本已变化，请刷新后重试".into()
            }
            Self::UnsupportedArtifact(kind) => {
                format!("Artifact 类型 {kind:?} 尚未接入独立 engine")
            }
            Self::UnsupportedCapability(message) => message.clone(),
            Self::Docx(_) => "文档解析失败，请检查文件内容后重试".into(),
            Self::Store(crate::store::StoreError::NotFound(_)) => "资源不存在".into(),
            Self::Store(_) => "存储服务暂不可用".into(),
            Self::Database(_) | Self::Internal(_) => "服务暂时不可用，请稍后重试".into(),
        }
    }

    fn retryable(&self) -> bool {
        matches!(
            self,
            Self::VersionConflict
                | Self::VersionConflictDetails(_)
                | Self::Store(_)
                | Self::Database(_)
                | Self::Internal(_)
        )
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = self.parts();
        if status.is_server_error() {
            tracing::error!(error = %self, "请求处理失败");
        }
        let details = match &self {
            AppError::VersionConflictDetails(details) => Some(details.clone()),
            _ => None,
        };
        let request_id = format!("err-{}", uuid::Uuid::new_v4());
        let retryable = self.retryable();
        (
            status,
            [(
                axum::http::header::HeaderName::from_static("x-request-id"),
                axum::http::HeaderValue::from_str(&request_id)
                    .expect("UUID request id is a valid header value"),
            )],
            Json(ErrorBody {
                error: self.public_message(),
                code,
                request_id,
                retryable,
                details,
            }),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_errors_do_not_map_to_500() {
        assert_eq!(
            AppError::NotFound("x".into()).parts().0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            AppError::BadRequest("x".into()).parts().0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(AppError::Unauthorized.parts().0, StatusCode::UNAUTHORIZED);
        assert!(!AppError::BadRequest("x".into()).retryable());
        assert!(AppError::VersionConflict.retryable());
    }

    #[test]
    fn broken_upload_is_reported_as_unprocessable() {
        let err = AppError::Docx(oo_docx::DocxError::MissingPart("word/document.xml"));
        assert_eq!(err.parts().0, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn missing_blob_is_a_404_not_a_500() {
        let err = AppError::Store(crate::store::StoreError::NotFound("k".into()));
        assert_eq!(err.parts().0, StatusCode::NOT_FOUND);
    }

    #[test]
    fn server_errors_do_not_expose_internal_details() {
        assert_eq!(
            AppError::Internal("sqlite password=secret".into()).public_message(),
            "服务暂时不可用，请稍后重试"
        );
        assert_eq!(
            AppError::Database(sqlx::Error::Protocol("driver detail".into())).public_message(),
            "服务暂时不可用，请稍后重试"
        );
    }

    #[test]
    fn conflict_details_are_machine_readable() {
        let details = ConflictDetails {
            artifact_id: "artifact-1".into(),
            requested_revision: 3,
            current_revision: 4,
            changed_entities: vec!["block:p-1".into()],
        };
        let response = AppError::VersionConflictDetails(details).into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert!(response.headers().get("x-request-id").is_some());
    }
}
