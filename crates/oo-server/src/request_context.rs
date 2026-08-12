//! Shared HTTP write-boundary parsing.
//!
//! The body carries a typed transaction envelope while HTTP carries the
//! concurrency and retry contract. Keeping the two checks here prevents a
//! route from accidentally accepting a stale or non-idempotent write.

use axum::http::{header, HeaderMap};
use oo_protocol::ArtifactCommandEnvelope;

use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionContext {
    pub expected_revision: u64,
    pub transaction_id: String,
    pub request_id: Option<String>,
}

/// Parse a quoted (or unquoted) HTTP revision value.
pub fn if_match(headers: &HeaderMap) -> Result<u64, AppError> {
    let value = headers
        .get(header::IF_MATCH)
        .ok_or_else(|| AppError::BadRequest("写入请求缺少 If-Match 版本号".into()))?
        .to_str()
        .map_err(|_| AppError::BadRequest("If-Match 版本号格式无效".into()))?
        .trim();
    let value = value.strip_prefix("W/").unwrap_or(value).trim_matches('"');
    value
        .parse()
        .map_err(|_| AppError::BadRequest("If-Match 版本号格式无效".into()))
}

/// Resolve the canonical transaction id header. `Idempotency-Key` is accepted
/// for generic clients, but when both headers are present they must agree.
pub fn transaction_id(headers: &HeaderMap) -> Result<String, AppError> {
    let x_transaction = header_value(headers, "x-transaction-id")?;
    let idempotency = header_value(headers, "idempotency-key")?;
    match (x_transaction, idempotency) {
        (Some(left), Some(right)) if left != right => Err(AppError::BadRequest(
            "x-transaction-id 与 Idempotency-Key 必须一致".into(),
        )),
        (Some(value), _) | (_, Some(value)) => Ok(value),
        (None, None) => Err(AppError::BadRequest("写入请求缺少幂等键".into())),
    }
}

/// Validate that the HTTP write headers describe the same transaction as the
/// typed body. This is intentionally strict: silently preferring one value
/// makes retries impossible to reason about for SDKs and agents.
pub fn transaction(
    headers: &HeaderMap,
    envelope: &ArtifactCommandEnvelope,
) -> Result<TransactionContext, AppError> {
    let expected_revision = if_match(headers)?;
    if expected_revision != envelope.base_revision {
        return Err(AppError::BadRequest(
            "If-Match 必须与事务 baseRevision 一致".into(),
        ));
    }
    let transaction_id = transaction_id(headers)?;
    if transaction_id != envelope.transaction_id {
        return Err(AppError::BadRequest(
            "幂等键必须与事务 transactionId 一致".into(),
        ));
    }
    Ok(TransactionContext {
        expected_revision,
        transaction_id,
        request_id: request_id(headers)?,
    })
}

fn request_id(headers: &HeaderMap) -> Result<Option<String>, AppError> {
    let Some(value) = header_value(headers, "x-request-id")? else {
        return Ok(None);
    };
    if value.len() > 128 {
        return Err(AppError::BadRequest("x-request-id 长度超出限制".into()));
    }
    Ok(Some(value))
}

fn header_value(headers: &HeaderMap, name: &'static str) -> Result<Option<String>, AppError> {
    let Some(value) = headers.get(name) else {
        return Ok(None);
    };
    let value = value
        .to_str()
        .map(str::trim)
        .map_err(|_| AppError::BadRequest(format!("{name} 格式无效")))?;
    if value.is_empty() {
        return Err(AppError::BadRequest(format!("{name} 格式无效")));
    }
    Ok(Some(value.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use oo_protocol::{
        ArtifactCommandEnvelope, CommandRecord, TransactionOrigin, CURRENT_PROTOCOL_VERSION,
    };
    use serde_json::json;

    fn envelope() -> ArtifactCommandEnvelope {
        ArtifactCommandEnvelope {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            transaction_id: "tx-1".into(),
            intent_id: "intent-1".into(),
            artifact_id: "artifact-1".into(),
            actor_id: "actor-1".into(),
            base_revision: 7,
            origin: TransactionOrigin::Local,
            commands: vec![CommandRecord {
                command_id: "command-1".into(),
                type_id: "document.insertBlock".into(),
                payload: json!({"type":"insertBlock"}),
            }],
        }
    }

    fn headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_MATCH, HeaderValue::from_static("\"7\""));
        headers.insert("x-transaction-id", HeaderValue::from_static("tx-1"));
        headers.insert("x-request-id", HeaderValue::from_static("req-1"));
        headers
    }

    #[test]
    fn transaction_headers_match_body() {
        assert_eq!(
            transaction(&headers(), &envelope()).unwrap(),
            TransactionContext {
                expected_revision: 7,
                transaction_id: "tx-1".into(),
                request_id: Some("req-1".into()),
            }
        );
    }

    #[test]
    fn conflicting_header_values_are_rejected() {
        let mut headers = headers();
        headers.insert("idempotency-key", HeaderValue::from_static("tx-2"));
        assert!(transaction(&headers, &envelope()).is_err());
        headers.insert("idempotency-key", HeaderValue::from_static("tx-1"));
        headers.insert(header::IF_MATCH, HeaderValue::from_static("\"6\""));
        assert!(transaction(&headers, &envelope()).is_err());
    }
}
