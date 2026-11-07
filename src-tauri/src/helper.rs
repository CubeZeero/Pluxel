//! macOS-only optional SMAppService transport. No root helper on Windows.
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
#[cfg(target_os = "macos")]
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize)]
pub struct HelperStatus {
    pub supported: bool,
    pub available: bool,
    pub enabled: bool,
    #[serde(default)]
    pub unlocked: bool,
    pub status: String,
}

#[cfg(target_os = "macos")]
fn call(operation: &str, payload: serde_json::Value) -> AppResult<serde_json::Value> {
    use std::ffi::{CStr, CString};
    unsafe extern "C" {
        fn pluxel_helper_call(
            operation: *const std::ffi::c_char,
            payload: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_char;
        fn pluxel_helper_free(value: *mut std::ffi::c_char);
    }
    let operation =
        CString::new(operation).map_err(|_| AppError::msg("Invalid helper operation"))?;
    let payload = CString::new(serde_json::to_string(&payload)?)
        .map_err(|_| AppError::msg("Invalid helper request"))?;
    let response = unsafe { pluxel_helper_call(operation.as_ptr(), payload.as_ptr()) };
    if response.is_null() {
        return Err(AppError::msg("Native helper returned no response"));
    }
    let text = unsafe { CStr::from_ptr(response) }
        .to_string_lossy()
        .into_owned();
    unsafe { pluxel_helper_free(response) };
    let value: serde_json::Value = serde_json::from_str(&text)?;
    if value["ok"].as_bool() != Some(true) {
        return Err(AppError::msg(
            value["error"].as_str().unwrap_or("Helper operation failed"),
        ));
    }
    Ok(value["value"].clone())
}

pub fn status() -> AppResult<HelperStatus> {
    #[cfg(target_os = "macos")]
    {
        Ok(serde_json::from_value(call(
            "status",
            serde_json::Value::Null,
        )?)?)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(HelperStatus {
            supported: false,
            available: false,
            enabled: false,
            unlocked: false,
            status: "unsupported".into(),
        })
    }
}

pub fn configure(register: bool) -> AppResult<HelperStatus> {
    #[cfg(target_os = "macos")]
    {
        let result = serde_json::from_value(call(
            if register { "register" } else { "unregister" },
            serde_json::Value::Null,
        )?)?;
        crate::installer::set_session_password(None);
        Ok(result)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = register;
        Err(AppError::msg("The helper is only supported on macOS"))
    }
}

pub fn authenticate() -> AppResult<()> {
    #[cfg(target_os = "macos")]
    {
        call("authenticate", serde_json::Value::Null)?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(AppError::msg("The helper is only supported on macOS"))
    }
}

/// Forget user-presence approval without unregistering the system service.
pub fn lock_session() -> AppResult<()> {
    #[cfg(target_os = "macos")]
    {
        call("lock", serde_json::Value::Null)?;
    }
    Ok(())
}

/// Return false only when the helper is genuinely disabled/unavailable. Once
/// enabled, authentication/IPC/policy failures must not silently downgrade to
/// the old password path or repeat a partially completed operation.
#[cfg(target_os = "macos")]
pub fn try_apply(pairs: &[(PathBuf, PathBuf)], removes: &[PathBuf]) -> AppResult<bool> {
    if !use_helper(&status()?)? {
        return Ok(false);
    }
    let pairs: Vec<_> = pairs.iter().map(|(source, destination)| serde_json::json!({"source": source, "destination": destination})).collect();
    call(
        "apply",
        serde_json::json!({"pairs": pairs, "removes": removes}),
    )?;
    crate::installer::set_session_password(None);
    Ok(true)
}

#[cfg(any(target_os = "macos", test))]
fn use_helper(status: &HelperStatus) -> AppResult<bool> {
    if !status.supported || !status.available {
        return Ok(false);
    }
    match status.status.as_str() {
        "enabled" if status.enabled => Ok(true),
        "notRegistered" if !status.enabled => Ok(false),
        _ => Err(AppError::msg("ヘルパーの承認が必要です。システム設定で許可するか、Pluxelの設定からヘルパーを解除してください。")),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn unbundled_test_runner_cannot_register_a_privileged_helper() {
        let status = super::status().unwrap();
        assert!(!status.available);
        assert!(!status.enabled);
        assert!(!status.unlocked);
    }

    #[test]
    fn pending_or_revoked_helper_cannot_silently_fall_back() {
        for state in ["requiresApproval", "notFound", "unknown"] {
            assert!(super::use_helper(&super::HelperStatus {
                supported: true,
                available: true,
                enabled: false,
                unlocked: false,
                status: state.into()
            })
            .is_err());
        }
        assert!(!super::use_helper(&super::HelperStatus {
            supported: true,
            available: true,
            enabled: false,
            unlocked: false,
            status: "notRegistered".into()
        })
        .unwrap());
        assert!(super::use_helper(&super::HelperStatus {
            supported: true,
            available: true,
            enabled: true,
            unlocked: false,
            status: "enabled".into()
        })
        .unwrap());
    }
}
