//! Tauri commands driving the OAuth login flows (currently just the
//! Copilot GitHub device-code path). The frontend calls
//! `start_copilot_device_login` to get a user code + verification URL,
//! displays them, and then `poll_copilot_device_login` blocks until the
//! user finishes the github.com flow. On success we store the access
//! token in the DPAPI-wrapped `TokenAccountStore`.

use std::sync::Arc;
use std::time::Duration;

use codexbar::providers::copilot::oauth::device_flow::{
    poll_for_token, request_device_code, DeviceCodeResponse, DeviceFlowConfig, DeviceFlowError,
    TokioSleeper,
};
use codexbar::providers::copilot::oauth::device_flow_transport::ReqwestDeviceFlowClient;
use codexbar::secrets::token_account::{TokenAccountStore, TokenKind};
use parking_lot::Mutex;
use serde::Serialize;
use tauri::State;

use crate::secrets_commands::TokenAccountHandle;

#[derive(Default)]
pub struct CopilotLoginRegistry {
    sessions: Mutex<Vec<CopilotSession>>,
}

#[derive(Clone)]
struct CopilotSession {
    id: String,
    enterprise_host: Option<String>,
    device_code: String,
    interval_secs: u64,
}

pub struct CopilotLoginHandle(pub Arc<CopilotLoginRegistry>);

#[derive(Debug, Serialize)]
pub struct CopilotDeviceCodeDto {
    pub session_id: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub expires_in_secs: i64,
    pub interval_secs: u64,
}

#[derive(Debug, Serialize)]
pub struct CopilotLoginResultDto {
    pub account_id: String,
    pub label: String,
}

#[tauri::command]
pub async fn start_copilot_device_login(
    enterprise_host: Option<String>,
    registry: State<'_, CopilotLoginHandle>,
) -> Result<CopilotDeviceCodeDto, String> {
    let host = enterprise_host.and_then(|h| {
        let trimmed = h.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    let config = DeviceFlowConfig {
        enterprise_host: host.clone(),
        ..DeviceFlowConfig::default()
    };
    let http = ReqwestDeviceFlowClient::new().map_err(|e| e.to_string())?;
    let response: DeviceCodeResponse =
        request_device_code(&http, &config).await.map_err(|e| e.to_string())?;
    let session_id = format!(
        "copilot-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    );
    registry.0.sessions.lock().push(CopilotSession {
        id: session_id.clone(),
        enterprise_host: host,
        device_code: response.device_code.clone(),
        interval_secs: response.interval.max(1),
    });
    Ok(CopilotDeviceCodeDto {
        session_id,
        user_code: response.user_code,
        verification_uri: response.verification_uri,
        verification_uri_complete: response.verification_uri_complete,
        expires_in_secs: response.expires_in,
        interval_secs: response.interval,
    })
}

#[tauri::command]
pub async fn poll_copilot_device_login(
    session_id: String,
    registry: State<'_, CopilotLoginHandle>,
    tokens: State<'_, TokenAccountHandle>,
) -> Result<CopilotLoginResultDto, String> {
    let session = registry
        .0
        .sessions
        .lock()
        .iter()
        .find(|s| s.id == session_id)
        .cloned()
        .ok_or_else(|| "no active Copilot login session".to_string())?;

    let config = DeviceFlowConfig {
        enterprise_host: session.enterprise_host.clone(),
        ..DeviceFlowConfig::default()
    };
    let http = ReqwestDeviceFlowClient::new().map_err(|e| e.to_string())?;
    let sleeper = TokioSleeper;
    let token = poll_for_token(
        &http,
        &sleeper,
        &config,
        &session.device_code,
        session.interval_secs,
    )
    .await
    .map_err(map_device_flow_error)?;

    let store: Arc<TokenAccountStore> = tokens.0.clone();
    let label = match &session.enterprise_host {
        Some(host) => format!("GitHub ({host})"),
        None => "GitHub.com".into(),
    };
    let account = tokio::task::spawn_blocking({
        let store = store.clone();
        let token_value = token.access_token.clone();
        let label = label.clone();
        move || store.add("copilot", TokenKind::OauthToken, label, token_value)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    // Also mark the new account active so the next refresh tick uses it.
    let _ = tokio::task::spawn_blocking({
        let store = store.clone();
        let id = account.id.clone();
        move || store.set_active("copilot", &id)
    })
    .await;

    // Forget the session so it cannot be polled again with a stale code.
    registry.0.sessions.lock().retain(|s| s.id != session_id);

    Ok(CopilotLoginResultDto {
        account_id: account.id,
        label: account.label,
    })
}

fn map_device_flow_error(err: DeviceFlowError) -> String {
    match err {
        DeviceFlowError::Expired => {
            "Login expired. Start a new device-code login.".into()
        }
        DeviceFlowError::AccessDenied => "Login was denied on GitHub.".into(),
        DeviceFlowError::IncorrectDeviceCode => {
            "GitHub did not recognise the device code. Try again.".into()
        }
        DeviceFlowError::GithubError(code) => format!("GitHub returned error `{code}`."),
        DeviceFlowError::Transport(msg) => format!("Network error: {msg}"),
        DeviceFlowError::Decode(msg) => format!("Could not decode GitHub response: {msg}"),
    }
}

/// Bounded sleep so the React side can show a cancel button between
/// polls if the user wants out. Currently unused but reserved for the
/// next iteration of the login UI.
pub async fn _polling_tick() {
    tokio::time::sleep(Duration::from_secs(1)).await;
}

// ─── Factory WorkOS login (paste refresh token) ─────────────────────
//
// Factory does not expose a public OAuth client we can drive headlessly
// the way GitHub does for Copilot. The flow that works on Windows:
//
// 1. User clicks "Sign in with Factory" → we open
//    https://app.factory.ai in their default browser.
// 2. User finishes the WorkOS login.
// 3. User opens DevTools → Application → Cookies on app.factory.ai,
//    copies the `wos-session` value, pastes it into the form we render.
// 4. We POST that cookie to api.workos.com/user_management/authenticate
//    (the same path the strategy uses) and stash the returned
//    access_token + refresh_token in the DPAPI-wrapped TokenAccountStore.
//
// This is two Tauri commands: one to open the sign-in URL, one to
// finish the flow with the pasted cookie value.

use codexbar::providers::factory::api::transport::ReqwestFactoryClient as FactoryWorkOSClient;
use codexbar::providers::factory::api::workos_refresh::exchange_cookie;

#[derive(Debug, Serialize)]
pub struct FactoryLoginResultDto {
    pub bearer_account_id: String,
    pub refresh_account_id: Option<String>,
}

#[tauri::command]
pub async fn complete_factory_workos_login(
    cookie_value: String,
    tokens: State<'_, TokenAccountHandle>,
) -> Result<FactoryLoginResultDto, String> {
    let cookie = cookie_value.trim().to_string();
    if cookie.is_empty() {
        return Err("Paste the wos-session cookie value first.".into());
    }
    // Accept either the bare value or a `name=value` pair. WorkOS
    // expects the canonical cookie header form on its side.
    let cookie_header = if cookie.contains('=') {
        cookie.clone()
    } else {
        format!("wos-session={cookie}")
    };

    let client = FactoryWorkOSClient::new().map_err(|e| e.to_string())?;
    let auth = exchange_cookie(&client, &cookie_header, None)
        .await
        .map_err(map_workos_error)?;

    let store = tokens.0.clone();
    let bearer_value = auth.access_token.clone();
    let refresh_value = auth.refresh_token.clone();
    let bearer_account = tokio::task::spawn_blocking({
        let store = store.clone();
        move || store.add("factory", TokenKind::OauthToken, "WorkOS bearer", bearer_value)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    let _ = tokio::task::spawn_blocking({
        let store = store.clone();
        let id = bearer_account.id.clone();
        move || store.set_active("factory", &id)
    })
    .await;

    let refresh_account_id = if let Some(rt) = refresh_value {
        let acct = tokio::task::spawn_blocking({
            let store = store.clone();
            move || store.add("factory", TokenKind::OauthToken, "WorkOS refresh", rt)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
        Some(acct.id)
    } else {
        None
    };

    Ok(FactoryLoginResultDto {
        bearer_account_id: bearer_account.id,
        refresh_account_id,
    })
}

fn map_workos_error(err: ProviderFetchError) -> String {
    match err {
        ProviderFetchError::Unauthorized => {
            "WorkOS rejected the cookie. Sign in to app.factory.ai again and copy the wos-session value.".into()
        }
        ProviderFetchError::UserConfigInvalid(msg) => msg,
        ProviderFetchError::NoCookies(_) => {
            "Cookie value was empty after trimming.".into()
        }
        other => format!("WorkOS auth failed: {other}"),
    }
}

use codexbar::providers::errors::ProviderFetchError;
