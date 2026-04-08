use crate::models::ErrorResponse;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use std::fmt::Display;

pub(crate) trait ApiResultExt<T> {
    fn to_api_result(self, not_found_msg: &str) -> Result<T, Response>;
    fn to_api_result_with_error(
        self,
        not_found_msg: &str,
        error_prefix: &str,
    ) -> Result<T, Response>;
    fn to_api_result_with_code(self, error_code: &str, not_found_msg: &str) -> Result<T, Response>;
    fn to_api_result_with_code_and_error(
        self,
        error_code: &str,
        not_found_msg: &str,
        error_prefix: &str,
    ) -> Result<T, Response>;
}

impl<T, E> ApiResultExt<T> for Result<Option<T>, E>
where
    E: Display,
{
    fn to_api_result(self, not_found_msg: &str) -> Result<T, Response> {
        self.to_api_result_with_error(not_found_msg, "Database error")
    }

    fn to_api_result_with_error(
        self,
        not_found_msg: &str,
        error_prefix: &str,
    ) -> Result<T, Response> {
        match self {
            Ok(Some(value)) => Ok(value),
            Ok(None) => Err(not_found_response(not_found_msg)),
            Err(error) => Err(internal_error_response(format!("{error_prefix}: {error}"))),
        }
    }

    fn to_api_result_with_code(self, error_code: &str, not_found_msg: &str) -> Result<T, Response> {
        self.to_api_result_with_code_and_error(error_code, not_found_msg, "Database error")
    }

    fn to_api_result_with_code_and_error(
        self,
        error_code: &str,
        not_found_msg: &str,
        error_prefix: &str,
    ) -> Result<T, Response> {
        match self {
            Ok(Some(value)) => Ok(value),
            Ok(None) => Err(coded_not_found_response(error_code, not_found_msg)),
            Err(error) => Err(internal_error_response(format!("{error_prefix}: {error}"))),
        }
    }
}

pub(crate) fn not_found_response(message: impl Into<String>) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::new(message.into())),
    )
        .into_response()
}

pub(crate) fn coded_not_found_response(error_code: &str, message: impl Into<String>) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::coded(error_code, message.into())),
    )
        .into_response()
}

pub(crate) fn internal_error_response(message: impl Into<String>) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse::new(message.into())),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::ApiResultExt;

    #[test]
    fn to_api_result_returns_value_for_some() {
        let result: Result<Option<i32>, &str> = Ok(Some(42));

        assert_eq!(result.to_api_result("missing").unwrap(), 42);
    }

    #[test]
    fn to_api_result_with_code_maps_missing_value() {
        let result: Result<Option<i32>, &str> = Ok(None);

        assert!(result
            .to_api_result_with_code("wallet_not_found", "Wallet not found")
            .is_err());
    }

    #[test]
    fn to_api_result_with_error_maps_database_error() {
        let result: Result<Option<i32>, &str> = Err("db down");

        assert!(result
            .to_api_result_with_error("User not found", "Failed to load user")
            .is_err());
    }
}
