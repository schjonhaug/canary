//! The error type returned by HTTP handlers.
//!
//! Handlers return [`ApiResult`], use `?` on fallible calls and let
//! [`ApiError`] render the JSON [`ErrorResponse`] body the frontend expects.

use crate::models::ErrorResponse;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use std::borrow::Cow;

/// Result type for HTTP handlers.
pub type ApiResult<T> = Result<T, ApiError>;

/// An error that renders as a JSON [`ErrorResponse`].
///
/// Client errors carry a status, an optional machine-readable code that the
/// frontend translates, and a user-facing message. Internal errors keep their
/// source for the server log and only ever expose a fixed message, so database
/// and provider details never reach the client.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{message}")]
    Client {
        status: StatusCode,
        code: Option<&'static str>,
        message: Cow<'static, str>,
    },
    #[error("{message}: {source:#}")]
    Internal {
        message: &'static str,
        #[source]
        source: anyhow::Error,
    },
}

impl ApiError {
    /// A client error with a machine-readable `code`.
    pub fn new(
        status: StatusCode,
        code: &'static str,
        message: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::Client {
            status,
            code: Some(code),
            message: message.into(),
        }
    }

    /// A client error without a machine-readable code.
    pub fn uncoded(status: StatusCode, message: impl Into<Cow<'static, str>>) -> Self {
        Self::Client {
            status,
            code: None,
            message: message.into(),
        }
    }

    pub fn bad_request(code: &'static str, message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message)
    }

    pub fn unauthorized(code: &'static str, message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, code, message)
    }

    pub fn forbidden(code: &'static str, message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(StatusCode::FORBIDDEN, code, message)
    }

    pub fn not_found(code: &'static str, message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(StatusCode::NOT_FOUND, code, message)
    }

    pub fn conflict(code: &'static str, message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(StatusCode::CONFLICT, code, message)
    }

    /// An unexpected failure. `message` is shown to the client; `source` is
    /// only logged.
    pub fn internal(message: &'static str, source: impl Into<anyhow::Error>) -> Self {
        Self::Internal {
            message,
            source: source.into(),
        }
    }

    /// The HTTP status this error renders with.
    pub fn status(&self) -> StatusCode {
        match self {
            Self::Client { status, .. } => *status,
            Self::Internal { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(source: anyhow::Error) -> Self {
        Self::internal("Internal server error", source)
    }
}

/// Converts any error into an [`ApiError::Internal`] with a fixed public message.
pub trait OrInternal<T> {
    fn or_internal(self, message: &'static str) -> ApiResult<T>;
}

impl<T, E: Into<anyhow::Error>> OrInternal<T> for Result<T, E> {
    fn or_internal(self, message: &'static str) -> ApiResult<T> {
        self.map_err(|source| ApiError::internal(message, source))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Client {
                status,
                code,
                message,
            } => {
                let body = ErrorResponse {
                    error: message.into_owned(),
                    error_code: code.map(str::to_owned),
                };
                (status, Json(body)).into_response()
            }
            Self::Internal { message, source } => {
                tracing::error!(error = format!("{source:#}"), "{message}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(message)),
                )
                    .into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    async fn body_of(error: ApiError) -> (StatusCode, serde_json::Value) {
        let response = error.into_response();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn client_errors_render_code_and_message() {
        let (status, body) =
            body_of(ApiError::not_found("wallet_not_found", "Wallet not found")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "Wallet not found");
        assert_eq!(body["error_code"], "wallet_not_found");
    }

    #[tokio::test]
    async fn uncoded_errors_omit_the_code() {
        let (_, body) = body_of(ApiError::uncoded(StatusCode::BAD_REQUEST, "Bad input")).await;
        assert!(body.get("error_code").is_none());
    }

    #[tokio::test]
    async fn internal_errors_hide_their_source() {
        let source = anyhow::anyhow!("sensitive database detail");
        let (status, body) = body_of(ApiError::internal("Failed to load wallet", source)).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["error"], "Failed to load wallet");
        assert!(!body.to_string().contains("sensitive"));
    }

    #[test]
    fn or_internal_wraps_any_error() {
        let result: Result<(), std::io::Error> = Err(std::io::Error::other("disk"));
        let error = result.or_internal("Failed to read").unwrap_err();
        assert_eq!(error.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
