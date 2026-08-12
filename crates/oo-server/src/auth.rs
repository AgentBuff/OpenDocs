//! 认证。
//!
//! 当前只有一个固定的开发用户：把精力先放在模型与事务边界上。但**所有取用户身份
//! 的代码路径都已经走这里**，接入真正的 JWT 校验时只需要改 [`authenticate`] 一处，
//! 处理器不用动。

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::error::AppError;

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

/// 从请求中解析出调用者。
///
/// 未来这里要做的是：读取 `Authorization: Bearer <jwt>`，验签、校验过期时间，
/// 再把 claims 里的用户 id 取出来。目前先无条件返回开发用户，但保留了返回
/// [`AppError::Unauthorized`] 的能力，调用方的错误处理路径因此是完整的。
pub fn authenticate(_parts: &Parts) -> Result<CurrentUser, AppError> {
    Ok(dev_user())
}

impl<S: Send + Sync> FromRequestParts<S> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        authenticate(parts)
    }
}
