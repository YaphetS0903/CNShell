use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

pub type RelayResult<T> = Result<T, RelayError>;

#[derive(Debug, thiserror::Error)]
pub enum RelayError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Authentication(String),
    #[error("{0}")]
    PermissionDenied(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Unavailable(String),
    #[error("storage operation failed")]
    Storage(#[from] sqlx::Error),
    #[error("internal relay operation failed")]
    Internal,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorBody {
    code: &'static str,
    message: String,
}

impl IntoResponse for RelayError {
    fn into_response(self) -> Response {
        let (status, code) = match self {
            Self::Validation(_) => (StatusCode::BAD_REQUEST, "validation"),
            Self::Authentication(_) => (StatusCode::UNAUTHORIZED, "authentication"),
            Self::PermissionDenied(_) => (StatusCode::FORBIDDEN, "permissionDenied"),
            Self::NotFound(_) => (StatusCode::NOT_FOUND, "notFound"),
            Self::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            Self::Unavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
            Self::Storage(_) | Self::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };
        let message = match &self {
            Self::Storage(_) | Self::Internal => "团队服务暂时不可用".into(),
            _ => self.to_string(),
        };
        (status, Json(ErrorBody { code, message })).into_response()
    }
}
