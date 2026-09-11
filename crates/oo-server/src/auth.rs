//! 认证。
//!
//! 当前只有一个固定的开发用户：把精力先放在模型与事务边界上。但**所有取用户身份
//! 的代码路径都已经走这里**，接入真正的 JWT 校验时只需要改 [`authenticate`] 一处，
//! 处理器不用动。
//!
//! ## `X-OO-User` 是显式开关，不是默认行为
//!
//! 真实认证接入之前，协作权限、presence 与审计需要能演练不同 Principal，为此保留了
//! `X-OO-User: <id>` 头。但它**默认不生效**：
//!
//! - 设置 `OO_TRUST_USER_HEADER=1`：头被信任，可指定任意 Principal，仅限本地开发；
//! - 未设置（默认）：头**不被信任**，带了就返回 400。
//!
//! 之所以是拒绝而不是忽略：静默降级成 `dev-user` 会让 ACL 演练得出错误结论——
//! 请求方以为自己是 `editor-1`，实际却以 `dev-user` 提交，失败原因极难定位。
//! 服务启动时若信任该头会打一条 warn，提醒这不是生产配置。

use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;

use crate::error::AppError;
use crate::AppState;

/// 请求的调用者。
#[derive(Debug, Clone, PartialEq)]
pub struct CurrentUser {
    pub id: String,
    pub display_name: String,
}

/// 开发环境固定用户。
pub const DEV_USER_ID: &str = "dev-user";

pub fn dev_user() -> CurrentUser {
    CurrentUser {
        id: DEV_USER_ID.to_string(),
        display_name: "开发用户".to_string(),
    }
}

/// 显式信任 `X-OO-User` 请求头的开关。
pub const TRUST_USER_HEADER_ENV: &str = "OO_TRUST_USER_HEADER";

/// 从环境变量读取开关。只有 `1` / `true` 视为开启，其余（含未设置）一律关闭。
pub fn trust_user_header_from_env() -> bool {
    matches!(
        std::env::var(TRUST_USER_HEADER_ENV).as_deref(),
        Ok("1") | Ok("true")
    )
}

/// 从请求中解析出调用者。
///
/// 未来这里要做的是：读取 `Authorization: Bearer <jwt>`，验签、校验过期时间，
/// 再把 claims 里的用户 id 取出来。目前先返回开发用户，但保留了返回
/// [`AppError::Unauthorized`] 的能力，调用方的错误处理路径因此是完整的。
///
/// `trust_user_header` 为 `false` 时，携带 `X-OO-User` 的请求会被明确拒绝（400），
/// 而不是被忽略。显示名按 id 稳定派生，避免引入用户表。
pub fn authenticate(parts: &Parts, trust_user_header: bool) -> Result<CurrentUser, AppError> {
    let requested = parts
        .headers
        .get("x-oo-user")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let Some(id) = requested else {
        return Ok(dev_user());
    };

    if !trust_user_header {
        return Err(AppError::BadRequest(format!(
            "X-OO-User 未被信任；如需在本地演练协作身份，请设置 {TRUST_USER_HEADER_ENV}=1"
        )));
    }

    if id.len() > 128 {
        return Err(AppError::BadRequest("X-OO-User 过长".into()));
    }

    Ok(CurrentUser {
        id: id.to_string(),
        display_name: format!("用户-{id}"),
    })
}

impl<S> FromRequestParts<S> for CurrentUser
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let state = AppState::from_ref(state);
        authenticate(parts, state.trust_user_header)
    }
}
