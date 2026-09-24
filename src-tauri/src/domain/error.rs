//! Typed error/result envelope shared by every Tauri command.
//! JSON shape: `{ok:true,data,requestId}` or `{ok:false,error,requestId}`
//! with camelCase fields, matching src/lib/ipc/types.ts.

use serde::{Deserialize, Serialize};

pub type RequestId = String;
pub type OperationId = String;

/// Every variant of docs/04-ipc-contracts.md §1. Serializes as SCREAMING_SNAKE.
/// The allow is intentional: variant names double as the wire format.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    GIT_NOT_FOUND,
    GIT_VERSION_UNSUPPORTED,
    NOT_REPOSITORY,
    BARE_REPOSITORY,
    REPO_UNAVAILABLE,
    INVALID_ARGUMENT,
    PATH_INVALID,
    REF_INVALID,
    TRUST_REQUIRED,
    REPO_BUSY,
    REPO_LOCKED,
    STALE_STATE,
    DIRTY_WORKTREE,
    EMPTY_INDEX,
    IDENTITY_MISSING,
    CONFLICTS_PRESENT,
    DIVERGED,
    AUTH_REQUIRED,
    NETWORK_ERROR,
    OFFLINE,
    HOOK_FAILED,
    SIGNING_FAILED,
    OUTPUT_LIMIT,
    UNSUPPORTED,
    CANCELLED,
    TIMEOUT,
    IO_ERROR,
    GIT_ERROR,
}

/// Recovery actions, serialized camelCase to match the TS contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryAction {
    Refresh,
    RetryRead,
    ConfigureGit,
    Authenticate,
    ResolveConflict,
    ChooseRepository,
    InspectState,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: ErrorCode,
    /// Plain text, user-safe. Never carries secrets or raw stderr.
    pub message: String,
    pub recovery: RecoveryAction,
    /// Mutations must never auto-retry.
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<OperationId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic_id: Option<String>,
}

impl AppError {
    pub fn new(
        code: ErrorCode,
        message: impl Into<String>,
        recovery: RecoveryAction,
        retryable: bool,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            recovery,
            retryable,
            operation_id: None,
            diagnostic_id: None,
        }
    }
}

/// Untagged so the wire shape is exactly the TS `ApiResult<T>` union.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ApiResult<T> {
    Ok {
        ok: bool,
        data: T,
        #[serde(rename = "requestId")]
        request_id: RequestId,
    },
    Err {
        ok: bool,
        error: AppError,
        #[serde(rename = "requestId")]
        request_id: RequestId,
    },
}

impl<T> ApiResult<T> {
    pub fn ok(data: T, request_id: RequestId) -> Self {
        Self::Ok {
            ok: true,
            data,
            request_id,
        }
    }

    pub fn err(error: AppError, request_id: RequestId) -> Self {
        Self::Err {
            ok: false,
            error,
            request_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn ok_envelope_uses_camel_case_request_id() {
        let result: ApiResult<Value> = ApiResult::ok(json!({"a": 1}), "req-1".to_string());
        let value = serde_json::to_value(&result).expect("serialize");
        assert_eq!(
            value,
            json!({"ok": true, "data": {"a": 1}, "requestId": "req-1"})
        );
    }

    #[test]
    fn err_envelope_matches_ts_contract_shape() {
        let error = AppError::new(
            ErrorCode::GIT_NOT_FOUND,
            "Git executable not found",
            RecoveryAction::ConfigureGit,
            false,
        );
        let result: ApiResult<Value> = ApiResult::err(error, "req-2".to_string());
        let value = serde_json::to_value(&result).expect("serialize");
        assert_eq!(
            value,
            json!({
                "ok": false,
                "error": {
                    "code": "GIT_NOT_FOUND",
                    "message": "Git executable not found",
                    "recovery": "configureGit",
                    "retryable": false
                },
                "requestId": "req-2"
            })
        );
    }

    #[test]
    fn all_error_codes_round_trip() {
        let codes = [
            ErrorCode::GIT_NOT_FOUND,
            ErrorCode::GIT_VERSION_UNSUPPORTED,
            ErrorCode::NOT_REPOSITORY,
            ErrorCode::BARE_REPOSITORY,
            ErrorCode::REPO_UNAVAILABLE,
            ErrorCode::INVALID_ARGUMENT,
            ErrorCode::PATH_INVALID,
            ErrorCode::REF_INVALID,
            ErrorCode::TRUST_REQUIRED,
            ErrorCode::REPO_BUSY,
            ErrorCode::REPO_LOCKED,
            ErrorCode::STALE_STATE,
            ErrorCode::DIRTY_WORKTREE,
            ErrorCode::EMPTY_INDEX,
            ErrorCode::IDENTITY_MISSING,
            ErrorCode::CONFLICTS_PRESENT,
            ErrorCode::DIVERGED,
            ErrorCode::AUTH_REQUIRED,
            ErrorCode::NETWORK_ERROR,
            ErrorCode::HOOK_FAILED,
            ErrorCode::SIGNING_FAILED,
            ErrorCode::OUTPUT_LIMIT,
            ErrorCode::UNSUPPORTED,
            ErrorCode::CANCELLED,
            ErrorCode::TIMEOUT,
            ErrorCode::IO_ERROR,
            ErrorCode::GIT_ERROR,
        ];
        assert_eq!(codes.len(), 27);
        for code in codes {
            let json = serde_json::to_value(code).expect("serialize code");
            let back: ErrorCode = serde_json::from_value(json).expect("deserialize code");
            assert_eq!(back, code);
        }
    }
}
