use crate::{
    db::Database,
    error::{AppError, AppResult},
    models::{
        AiAssistantResult, AiPreviewInput, AiProviderProfile, AiRequestPreview, SaveAiProviderInput,
    },
};
use chrono::{Duration as ChronoDuration, Utc};
use parking_lot::Mutex;
use regex::Regex;
use reqwest::{Client, StatusCode, Url, redirect::Policy};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use uuid::Uuid;
use zeroize::Zeroizing;

const PROVIDERS_KEY: &str = "cnshell.ai.providers";
const KEYCHAIN_SERVICE: &str = "cn.cnshell.ai";
const MAX_INPUT_BYTES: usize = 64 * 1024;
const MAX_OUTPUT_BYTES: usize = 64 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAiProvider {
    id: String,
    name: String,
    endpoint: String,
    model: String,
}

#[derive(Clone)]
pub(crate) struct PendingAiRequest {
    provider: StoredAiProvider,
    kind: String,
    content: String,
    expires_at: chrono::DateTime<Utc>,
}

#[derive(Clone, Default)]
pub struct AiManager {
    pending: Arc<Mutex<HashMap<String, PendingAiRequest>>>,
}

impl AiManager {
    pub async fn preview(
        &self,
        db: &Database,
        input: AiPreviewInput,
    ) -> AppResult<AiRequestPreview> {
        validate_kind_content(&input.kind, &input.content)?;
        let provider = stored_providers(db)
            .await?
            .into_iter()
            .find(|item| item.id == input.provider_id)
            .ok_or_else(|| AppError::NotFound(format!("AI Provider {}", input.provider_id)))?;
        let (content, redactions) = redact(&input.content)?;
        let request_id = Uuid::new_v4().to_string();
        let expires_at = Utc::now() + ChronoDuration::minutes(10);
        let mut pending = self.pending.lock();
        pending.retain(|_, request| request.expires_at > Utc::now());
        if pending.len() >= 32 {
            return Err(AppError::Unavailable(
                "待确认的 AI 请求过多，请稍后重试".into(),
            ));
        }
        pending.insert(
            request_id.clone(),
            PendingAiRequest {
                provider: provider.clone(),
                kind: input.kind.clone(),
                content: content.clone(),
                expires_at,
            },
        );
        Ok(AiRequestPreview {
            request_id,
            provider_name: provider.name,
            endpoint: provider.endpoint,
            model: provider.model,
            kind: input.kind,
            redacted_content: content,
            redactions,
            expires_at: expires_at.to_rfc3339(),
        })
    }

    pub fn take(&self, request_id: &str) -> AppResult<PendingAiRequest> {
        let request = self
            .pending
            .lock()
            .remove(request_id)
            .ok_or_else(|| AppError::NotFound("AI 预览已过期或已使用".into()))?;
        if request.expires_at <= Utc::now() {
            return Err(AppError::Unavailable("AI 预览已过期，请重新预览".into()));
        }
        Ok(request)
    }
}

pub async fn providers(db: &Database) -> AppResult<Vec<AiProviderProfile>> {
    Ok(stored_providers(db)
        .await?
        .into_iter()
        .map(|provider| AiProviderProfile {
            has_api_key: load_api_key(&provider.id).ok().flatten().is_some(),
            id: provider.id,
            name: provider.name,
            endpoint: provider.endpoint,
            model: provider.model,
        })
        .collect())
}

pub async fn save_provider(
    db: &Database,
    input: SaveAiProviderInput,
) -> AppResult<AiProviderProfile> {
    validate_id(&input.id)?;
    if input.name.trim().is_empty() || input.name.len() > 256 {
        return Err(AppError::Validation("AI Provider 名称无效".into()));
    }
    let endpoint = validate_endpoint(&input.endpoint)?.to_string();
    if input.model.trim().is_empty() || input.model.len() > 256 {
        return Err(AppError::Validation(
            "模型名称不能为空且不能超过 256 字符".into(),
        ));
    }
    if let Some(api_key) = input.api_key.as_deref() {
        if api_key.is_empty() {
            delete_api_key(&input.id)?;
        } else if api_key.len() > 4096 || api_key.contains(['\n', '\r', '\0']) {
            return Err(AppError::Validation("API Key 无效".into()));
        } else {
            save_api_key(&input.id, api_key)?;
        }
    }
    let provider = StoredAiProvider {
        id: input.id,
        name: input.name.trim().into(),
        endpoint,
        model: input.model.trim().into(),
    };
    let mut providers = stored_providers(db).await?;
    if let Some(existing) = providers.iter_mut().find(|item| item.id == provider.id) {
        *existing = provider.clone();
    } else {
        providers.push(provider.clone());
    }
    db.save_named_state(
        PROVIDERS_KEY,
        &serde_json::to_value(&providers).map_err(|error| AppError::Internal(error.to_string()))?,
    )
    .await?;
    Ok(AiProviderProfile {
        has_api_key: load_api_key(&provider.id)?.is_some(),
        id: provider.id,
        name: provider.name,
        endpoint: provider.endpoint,
        model: provider.model,
    })
}

pub async fn delete_provider(db: &Database, id: &str) -> AppResult<()> {
    validate_id(id)?;
    let mut providers = stored_providers(db).await?;
    providers.retain(|item| item.id != id);
    db.save_named_state(
        PROVIDERS_KEY,
        &serde_json::to_value(&providers).map_err(|error| AppError::Internal(error.to_string()))?,
    )
    .await?;
    delete_api_key(id)
}

pub async fn execute(
    request_id: String,
    request: PendingAiRequest,
    cancelled: Arc<AtomicBool>,
) -> AppResult<AiAssistantResult> {
    let endpoint = chat_completions_url(&request.provider.endpoint)?;
    let api_key = load_api_key(&request.provider.id)?.map(Zeroizing::new);
    let body = serde_json::json!({
        "model": request.provider.model,
        "messages": [
            {"role": "system", "content": system_instruction(&request.kind)},
            {"role": "user", "content": request.content}
        ],
        "temperature": 0.2
    });
    let client = Client::builder()
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(90))
        .build()
        .map_err(|error| AppError::Unavailable(format!("AI 客户端初始化失败：{error}")))?;
    let mut request_builder = client.post(endpoint).json(&body);
    if let Some(api_key) = api_key.as_ref() {
        request_builder = request_builder.bearer_auth(api_key.as_str());
    }
    let response = tokio::select! {
        response = request_builder.send() => response.map_err(ai_network_error)?,
        _ = wait_cancelled(&cancelled) => return Err(AppError::Unavailable("AI 请求已取消".into())),
    };
    if !response.status().is_success() {
        return Err(ai_http_error(response.status()));
    }
    if response.content_length().unwrap_or(0) > MAX_OUTPUT_BYTES as u64 {
        return Err(AppError::Validation("AI 响应超过 64 KB".into()));
    }
    let bytes = response.bytes().await.map_err(ai_network_error)?;
    if bytes.len() > MAX_OUTPUT_BYTES {
        return Err(AppError::Validation("AI 响应超过 64 KB".into()));
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| AppError::Remote("AI Provider 返回了无效 JSON".into()))?;
    let content = value
        .pointer("/choices/0/message/content")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::Remote("AI Provider 响应中没有文本结果".into()))?;
    Ok(AiAssistantResult {
        request_id,
        kind: request.kind,
        model: request.provider.model,
        content: content.trim().into(),
    })
}

async fn stored_providers(db: &Database) -> AppResult<Vec<StoredAiProvider>> {
    Ok(db
        .load_named_state(PROVIDERS_KEY)
        .await?
        .unwrap_or_default())
}

fn validate_endpoint(value: &str) -> AppResult<Url> {
    let mut url =
        Url::parse(value.trim()).map_err(|_| AppError::Validation("AI endpoint 无效".into()))?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err(AppError::Validation(
            "AI endpoint 必须使用 HTTPS；仅本机模型允许 HTTP".into(),
        ));
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AppError::Validation(
            "AI endpoint 不能内嵌凭据、查询参数或片段".into(),
        ));
    }
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}

fn chat_completions_url(endpoint: &str) -> AppResult<Url> {
    validate_endpoint(endpoint)?
        .join("chat/completions")
        .map_err(|_| AppError::Validation("AI 请求地址无效".into()))
}

fn validate_kind_content(kind: &str, content: &str) -> AppResult<()> {
    if !["command", "explain", "summarize"].contains(&kind) {
        return Err(AppError::Validation("AI 请求类型无效".into()));
    }
    if content.trim().is_empty() || content.len() > MAX_INPUT_BYTES {
        return Err(AppError::Validation("AI 输入必须为 1～64 KB".into()));
    }
    Ok(())
}

fn validate_id(id: &str) -> AppResult<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'-' || value == b'_')
    {
        return Err(AppError::Validation("AI Provider ID 无效".into()));
    }
    Ok(())
}

fn redact(input: &str) -> AppResult<(String, Vec<String>)> {
    let mut value = input.to_owned();
    let mut categories = Vec::new();
    for (pattern, replacement, category) in [
        (
            r"(?s)-----BEGIN [^-\n]{0,64}PRIVATE KEY-----.*?-----END [^-\n]{0,64}PRIVATE KEY-----",
            "[REDACTED_PRIVATE_KEY]",
            "privateKey",
        ),
        (
            r"(?i)\b(?:bearer\s+|sk-)[A-Za-z0-9._-]{12,}\b",
            "[REDACTED_TOKEN]",
            "token",
        ),
        (
            r"(?i)\b(?:password|passwd|secret|token|api[_-]?key)\s*[:=]\s*[^\s,;]+",
            "[REDACTED_SECRET]",
            "secret",
        ),
        (r"\b(?:[0-9]{1,3}\.){3}[0-9]{1,3}\b", "[IP]", "ip"),
        (
            r"\b[A-Za-z0-9_-]+@(?:[A-Za-z0-9-]+\.)+[A-Za-z]{2,}\b",
            "[USER]@[HOST]",
            "userHost",
        ),
        (
            r"\b(?:[A-Za-z0-9-]+\.)+[A-Za-z]{2,}\b",
            "[HOST]",
            "hostname",
        ),
        (
            r#"(?m)(^|[\s='\"])(/[A-Za-z0-9._~+@%:,/-]+)"#,
            "$1[PATH]",
            "path",
        ),
    ] {
        let regex = Regex::new(pattern).map_err(|error| AppError::Internal(error.to_string()))?;
        if regex.is_match(&value) {
            value = regex.replace_all(&value, replacement).into_owned();
            categories.push(category.into());
        }
    }
    Ok((value, categories))
}

fn system_instruction(kind: &str) -> &'static str {
    match kind {
        "command" => {
            "Generate one shell command for the described task. Return the command first, then a short risk note. Never claim it was executed."
        }
        "explain" => {
            "Explain the selected terminal error concisely. Separate likely cause from safe, read-only diagnostic steps. Never claim access to the host."
        }
        _ => {
            "Summarize the selected log. Highlight timestamps, failures, likely causes, and safe follow-up checks. Do not invent missing events."
        }
    }
}

async fn wait_cancelled(cancelled: &Arc<AtomicBool>) {
    while !cancelled.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn save_api_key(id: &str, api_key: &str) -> AppResult<()> {
    keyring::Entry::new(KEYCHAIN_SERVICE, id)
        .map_err(|error| AppError::Storage(error.to_string()))?
        .set_password(api_key)
        .map_err(|error| AppError::Storage(format!("AI API Key 保存失败：{error}")))
}

fn load_api_key(id: &str) -> AppResult<Option<String>> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, id)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(AppError::Storage(format!("AI API Key 读取失败：{error}"))),
    }
}

fn delete_api_key(id: &str) -> AppResult<()> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, id)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(AppError::Storage(format!("AI API Key 删除失败：{error}"))),
    }
}

fn ai_network_error(error: reqwest::Error) -> AppError {
    if error.is_timeout() {
        AppError::Unavailable("AI 请求超时".into())
    } else {
        AppError::Unavailable(format!("AI 网络请求失败：{error}"))
    }
}

fn ai_http_error(status: StatusCode) -> AppError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            AppError::Authentication(format!("AI Provider 认证失败（HTTP {status}）"))
        }
        StatusCode::TOO_MANY_REQUESTS => {
            AppError::Unavailable("AI Provider 请求过多，请稍后重试".into())
        }
        _ => AppError::Remote(format!("AI Provider 请求失败（HTTP {status}）")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_secrets_hosts_ips_and_paths() {
        let (value, categories) =
            redact("password=hunter2 curl https://api.example.test 10.0.0.7 /Users/alice/key")
                .unwrap();
        assert!(!value.contains("hunter2"));
        assert!(!value.contains("api.example.test"));
        assert!(!value.contains("10.0.0.7"));
        assert!(!value.contains("/Users/alice/key"));
        assert!(categories.contains(&"secret".into()));
    }

    #[test]
    fn endpoints_are_scoped_to_explicit_compatible_api() {
        assert_eq!(
            chat_completions_url("https://api.openai.com/v1")
                .unwrap()
                .as_str(),
            "https://api.openai.com/v1/chat/completions"
        );
        assert!(validate_endpoint("http://remote.example.test/v1").is_err());
        assert!(validate_endpoint("http://127.0.0.1:11434/v1").is_ok());
    }

    #[test]
    fn preview_kind_and_size_are_bounded() {
        assert!(validate_kind_content("command", "list files").is_ok());
        assert!(validate_kind_content("unknown", "text").is_err());
        assert!(validate_kind_content("explain", "").is_err());
    }
}
