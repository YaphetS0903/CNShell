use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("未找到连接：{0}")]
    NotFound(String),
    #[error("连接参数无效：{0}")]
    Validation(String),
    #[error("主机指纹尚未信任：{fingerprint}")]
    HostKeyUnknown {
        fingerprint: String,
        algorithm: String,
    },
    #[error("主机指纹发生变化。已保存 {expected}，当前 {actual}")]
    HostKeyChanged { expected: String, actual: String },
    #[error("认证失败：{0}")]
    Authentication(String),
    #[error("权限不足：{0}")]
    PermissionDenied(String),
    #[error("远程操作失败：{0}")]
    Remote(String),
    #[error("本地存储失败：{0}")]
    Storage(String),
    #[error("系统能力不可用：{0}")]
    Unavailable(String),
    #[error("发生内部错误：{0}")]
    Internal(String),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorPayload {
    code: &'static str,
    message: String,
    fingerprint: Option<String>,
    algorithm: Option<String>,
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let (code, fingerprint, algorithm) = match self {
            Self::NotFound(_) => ("not_found", None, None),
            Self::Validation(_) => ("validation", None, None),
            Self::HostKeyUnknown {
                fingerprint,
                algorithm,
            } => (
                "host_key_unknown",
                Some(fingerprint.clone()),
                Some(algorithm.clone()),
            ),
            Self::HostKeyChanged { .. } => ("host_key_changed", None, None),
            Self::Authentication(_) => ("authentication", None, None),
            Self::PermissionDenied(_) => ("permission_denied", None, None),
            Self::Remote(_) => ("remote", None, None),
            Self::Storage(_) => ("storage", None, None),
            Self::Unavailable(_) => ("unavailable", None, None),
            Self::Internal(_) => ("internal", None, None),
        };
        ErrorPayload {
            code,
            message: self.to_string(),
            fingerprint,
            algorithm,
        }
        .serialize(serializer)
    }
}

impl From<sqlx::Error> for AppError {
    fn from(value: sqlx::Error) -> Self {
        Self::Storage(value.to_string())
    }
}

impl From<ssh2::Error> for AppError {
    fn from(value: ssh2::Error) -> Self {
        Self::Remote(value.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        if value.raw_os_error() == Some(28) {
            Self::Storage("磁盘空间不足，请释放本地空间后重试".into())
        } else {
            Self::Storage(value.to_string())
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;
