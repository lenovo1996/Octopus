//! `settings_get` / `settings_update` (T14).
//!
//! Settings v1 is one small validated struct (`SettingsV1` in the
//! persistence store): a version counter plus the UI font scale.
//! `settings_update` takes an expected version (optimistic concurrency —
//! a concurrent writer fails with `STALE_STATE`) and a whitelisted patch
//! (unknown fields are rejected, never ignored). Values are range-checked;
//! the store persists atomically and backs up corrupted files on load.

use serde::Deserialize;
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;

use crate::domain::{ApiResult, AppError, ErrorCode, RecoveryAction, RequestId};
use crate::persistence::{SettingsV1, Store};
use crate::services::RepoRegistry;

const MIN_FONT_SCALE: f32 = 0.875;
const MAX_FONT_SCALE: f32 = 1.25;

fn bad_request(message: impl Into<String>) -> AppError {
    AppError::new(
        ErrorCode::INVALID_ARGUMENT,
        message,
        RecoveryAction::InspectState,
        false,
    )
}

fn check_request_id(id: &str) -> Result<(), AppError> {
    if id.is_empty() || id.len() > 128 {
        return Err(bad_request("requestId must be 1..=128 characters"));
    }
    Ok(())
}

fn store_for(app: &AppHandle) -> Result<Store, AppError> {
    let dir = app.path().app_data_dir().map_err(|_| {
        AppError::new(
            ErrorCode::IO_ERROR,
            "App data directory is unavailable",
            RecoveryAction::RetryRead,
            false,
        )
    })?;
    Ok(Store::open(&dir))
}

fn stale_settings() -> AppError {
    AppError::new(
        ErrorCode::STALE_STATE,
        "Settings changed under you; reload and retry",
        RecoveryAction::Refresh,
        false,
    )
}

/// Pure patch validation: version match, non-empty whitelist, finite range.
/// The store write happens only after this returns `Ok`.
fn apply_patch(
    current: &SettingsV1,
    settings_version: u64,
    patch: &SettingsPatch,
) -> Result<f32, AppError> {
    if current.version != settings_version {
        return Err(stale_settings());
    }
    let scale = patch
        .font_scale
        .ok_or_else(|| bad_request("Empty patch: set at least one known settings field"))?;
    if !scale.is_finite() || scale < MIN_FONT_SCALE || scale > MAX_FONT_SCALE {
        return Err(bad_request("Font scale must be between 0.875 and 1.25"));
    }
    Ok(scale)
}

/// Whitelisted patch: every field optional, unknown fields rejected.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsPatch {
    pub font_scale: Option<f32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsGetRequest {
    pub request_id: RequestId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsUpdateRequest {
    pub request_id: RequestId,
    pub settings_version: u64,
    pub patch: SettingsPatch,
}

#[tauri::command]
pub async fn settings_get(
    app: AppHandle,
    _registry: State<'_, Mutex<RepoRegistry>>,
    request: SettingsGetRequest,
) -> Result<ApiResult<SettingsV1>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let store = store_for(&app)?;
        Ok::<SettingsV1, AppError>(store.settings().clone())
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn settings_update(
    app: AppHandle,
    _registry: State<'_, Mutex<RepoRegistry>>,
    request: SettingsUpdateRequest,
) -> Result<ApiResult<SettingsV1>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let mut store = store_for(&app)?;
        let scale = apply_patch(store.settings(), request.settings_version, &request.patch)?;
        Ok::<SettingsV1, AppError>(store.set_font_scale(scale))
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

pub mod prelude {
    pub use super::{settings_get, settings_update, SettingsGetRequest, SettingsUpdateRequest};
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("gitdock-t14-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    #[test]
    fn fresh_store_returns_default_settings() {
        let dir = temp_dir("fresh");
        let settings = Store::open(&dir).settings().clone();
        assert_eq!(settings.version, 1);
        assert_eq!(settings.font_scale, 1.0);
    }

    #[test]
    fn pre_settings_files_migrate_with_defaults() {
        let dir = temp_dir("migrate");
        // A T03-era file: schema 1, recents, no settings key.
        std::fs::write(
            dir.join("gitdock-settings.json"),
            r#"{"schemaVersion":1,"recentRepositories":[],"trustedRepositories":[]}"#,
        )
        .expect("write");
        let store = Store::open(&dir);
        assert_eq!(store.settings().font_scale, 1.0);
        assert_eq!(store.settings().version, 1);
    }

    #[test]
    fn corrupted_files_back_up_and_default() {
        let dir = temp_dir("corrupt");
        std::fs::write(dir.join("gitdock-settings.json"), b"not json{{{").expect("write");
        let store = Store::open(&dir);
        assert_eq!(store.settings().font_scale, 1.0);
        assert!(dir.join("gitdock-settings.json.corrupt").exists());
    }

    #[test]
    fn future_schema_fails_safe() {
        let dir = temp_dir("future");
        std::fs::write(
            dir.join("gitdock-settings.json"),
            r#"{"schemaVersion":99,"recentRepositories":[],"trustedRepositories":[]}"#,
        )
        .expect("write");
        let store = Store::open(&dir);
        assert_eq!(store.settings().font_scale, 1.0);
        assert!(dir.join("gitdock-settings.json.corrupt").exists());
    }

    #[test]
    fn font_scale_update_bumps_version() {
        let dir = temp_dir("update");
        let mut store = Store::open(&dir);
        let before = store.settings().version;
        let next = store.set_font_scale(1.125);
        assert_eq!(next.font_scale, 1.125);
        assert_eq!(next.version, before + 1);
        // Survives a reload.
        let again = Store::open(&dir);
        assert_eq!(again.settings().font_scale, 1.125);
    }

    #[tokio::test]
    async fn patch_rejects_unknown_fields() {
        let raw = serde_json::json!({
            "requestId": "r1",
            "settingsVersion": 1,
            "patch": {"fontScale": 1.0, "theme": "dark"}
        });
        assert!(serde_json::from_value::<SettingsUpdateRequest>(raw).is_err());
    }

    #[test]
    fn patch_accepts_rejects_and_detects_conflicts() {
        let current = SettingsV1::default();
        let ok = SettingsPatch {
            font_scale: Some(1.125),
        };
        assert_eq!(apply_patch(&current, 1, &ok), Ok(1.125));
        // Stale version fails before looking at values.
        assert_eq!(
            apply_patch(&current, 2, &ok).unwrap_err().code,
            ErrorCode::STALE_STATE
        );
        // Out-of-range and non-finite scales fail.
        for bad in [0.5, 2.0, f32::NAN, f32::INFINITY] {
            let patch = SettingsPatch {
                font_scale: Some(bad),
            };
            assert_eq!(
                apply_patch(&current, 1, &patch).unwrap_err().code,
                ErrorCode::INVALID_ARGUMENT,
                "scale {bad}"
            );
        }
        // Empty patch fails instead of no-op success.
        let empty = SettingsPatch { font_scale: None };
        assert_eq!(
            apply_patch(&current, 1, &empty).unwrap_err().code,
            ErrorCode::INVALID_ARGUMENT
        );
    }
}
