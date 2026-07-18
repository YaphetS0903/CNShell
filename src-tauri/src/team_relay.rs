use crate::{
    db::Database,
    error::{AppError, AppResult},
    models::{
        AcceptTeamRelayInvitationInput, CreateTeamRelayInvitationInput,
        ResendTeamRelayVerificationInput, SaveTeamRelayProfileInput, TeamControlLease, TeamDevice,
        TeamRelayAccountInput, TeamRelayAccountRegistration, TeamRelayInvitation, TeamRelayProfile,
        TeamRelayTerminalInvitation, TeamRelayWorkspaceBinding, TeamTerminalInvitation,
        TeamTerminalRoom, TeamWorkspace, UpdateTeamRelayMemberInput, VerifyTeamRelayAccountInput,
    },
    team, team_share,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use ed25519_dalek::{Signer, SigningKey};
use futures_util::StreamExt;
use rand::{RngCore, rngs::OsRng};
use reqwest::{Client, RequestBuilder, StatusCode, Url, redirect::Policy};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use sqlx::{Row, Sqlite, Transaction};
use std::collections::HashSet;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};
use zeroize::{Zeroize, Zeroizing};

const KEYCHAIN_SERVICE: &str = "cn.cnshell.team-relay";
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const TOKEN_REFRESH_MARGIN_SECONDS: i64 = 30;
const MAX_MEMBERS: usize = 256;
const MAX_DEVICES: usize = 1024;

#[derive(Debug, Clone, sqlx::FromRow)]
struct StoredProfile {
    id: String,
    name: String,
    base_url: String,
    account_id: Option<String>,
    account_email: Option<String>,
    account_session_expires_at: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct StoredBinding {
    workspace_id: String,
    profile_id: String,
    account_id: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PendingAcceptance {
    token_hash: String,
    profile_id: String,
    device_id: String,
    device_name: String,
    encryption_public_key: String,
    signing_public_key: String,
    fingerprint: String,
    created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SecretSession {
    token: String,
    expires_at: String,
}

impl Drop for SecretSession {
    fn drop(&mut self) {
        self.token.zeroize();
    }
}

#[derive(Debug)]
struct RelayFailure {
    status: Option<StatusCode>,
    message: String,
}

impl RelayFailure {
    fn into_app(self) -> AppError {
        match self.status {
            Some(StatusCode::BAD_REQUEST) => AppError::Validation(self.message),
            Some(StatusCode::UNAUTHORIZED) => AppError::Authentication(self.message),
            Some(StatusCode::FORBIDDEN) => AppError::PermissionDenied(self.message),
            Some(StatusCode::NOT_FOUND) => AppError::NotFound(self.message),
            Some(StatusCode::CONFLICT) => {
                AppError::Remote(format!("团队服务冲突：{}", self.message))
            }
            Some(StatusCode::SERVICE_UNAVAILABLE) => AppError::Unavailable(self.message),
            Some(status) if status.is_server_error() => AppError::Unavailable(self.message),
            _ => AppError::Remote(self.message),
        }
    }

    fn can_retry_without_changing_identity(&self) -> bool {
        self.status.is_none() || self.status.is_some_and(|status| status.is_server_error())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RelayErrorBody {
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RegisterAccountRequest<'a> {
    email: &'a str,
    password: &'a str,
    display_name: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LoginRequest<'a> {
    email: &'a str,
    password: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountSessionResponse {
    account_id: String,
    email: String,
    token: String,
    expires_at: String,
}

impl Drop for AccountSessionResponse {
    fn drop(&mut self) {
        self.token.zeroize();
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountRegistrationResponse {
    verification_required: bool,
    verification_expires_at: Option<String>,
    account_session: Option<AccountSessionResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VerifyEmailRequest<'a> {
    token: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResendVerificationEmailRequest<'a> {
    email: &'a str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceRegistration {
    id: String,
    name: String,
    encryption_public_key: String,
    signing_public_key: String,
    fingerprint: String,
}

impl From<&PendingAcceptance> for DeviceRegistration {
    fn from(value: &PendingAcceptance) -> Self {
        Self {
            id: value.device_id.clone(),
            name: value.device_name.clone(),
            encryption_public_key: value.encryption_public_key.clone(),
            signing_public_key: value.signing_public_key.clone(),
            fingerprint: value.fingerprint.clone(),
        }
    }
}

impl From<&TeamDevice> for DeviceRegistration {
    fn from(value: &TeamDevice) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            encryption_public_key: value.encryption_public_key.clone(),
            signing_public_key: value.signing_public_key.clone(),
            fingerprint: value.fingerprint.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BootstrapWorkspaceRequest {
    workspace_id: String,
    workspace_name: String,
    member_id: String,
    device: DeviceRegistration,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceInvitationRequest<'a> {
    email: &'a str,
    role: &'a str,
    member_id: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceInvitationResponse {
    invitation_id: String,
    token: String,
    expires_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AcceptInvitationRequest<'a> {
    token: &'a str,
    device: &'a DeviceRegistration,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceSessionResponse {
    workspace_id: String,
    member_id: String,
    device_id: String,
    role: String,
    key_epoch: i64,
    token: String,
    expires_at: String,
}

impl Drop for DeviceSessionResponse {
    fn drop(&mut self) {
        self.token.zeroize();
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceChallengeRequest<'a> {
    workspace_id: &'a str,
    device_id: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceChallengeResponse {
    challenge_id: String,
    challenge: String,
    expires_at: String,
}

impl Drop for DeviceChallengeResponse {
    fn drop(&mut self) {
        self.challenge.zeroize();
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateDeviceSessionRequest<'a> {
    challenge_id: &'a str,
    challenge: &'a str,
    signature: &'a str,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceMemberSnapshot {
    id: String,
    display_name: String,
    role: String,
    status: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceDeviceSnapshot {
    id: String,
    member_id: String,
    name: String,
    encryption_public_key: String,
    signing_public_key: String,
    fingerprint: String,
    status: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceSnapshot {
    id: String,
    name: String,
    key_epoch: i64,
    members: Vec<WorkspaceMemberSnapshot>,
    devices: Vec<WorkspaceDeviceSnapshot>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateMemberRequest<'a> {
    role: &'a str,
    status: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateRoomRequest<'a> {
    room_id: &'a str,
    key_epoch: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RoomResponse {
    id: String,
    workspace_id: String,
    host_member_id: String,
    host_device_id: String,
    key_epoch: i64,
    status: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RouteRoomInvitationRequest<'a> {
    invitation: &'a TeamTerminalInvitation,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RoutedRoomInvitationResponse {
    room_id: String,
    invitation: TeamTerminalInvitation,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GrantControlRequest<'a> {
    device_id: &'a str,
    duration_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ControlLeaseResponse {
    lease_id: String,
    member_id: String,
    device_id: String,
    generation: u64,
    expires_at: String,
}

pub(crate) struct RelayDeviceContext {
    pub base_url: String,
    pub workspace_id: String,
    pub member_id: String,
    pub device_id: String,
    pub token: Zeroizing<String>,
    pub expires_at: String,
}

struct RelayApi {
    base_url: Url,
    client: Client,
}

impl RelayApi {
    fn new(base_url: &str) -> AppResult<Self> {
        Ok(Self {
            base_url: validate_base_url(base_url)?,
            client: Client::builder()
                .redirect(Policy::none())
                .connect_timeout(std::time::Duration::from_secs(10))
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|error| {
                    AppError::Unavailable(format!("团队服务客户端初始化失败：{error}"))
                })?,
        })
    }

    fn endpoint(&self, path: &str) -> Result<Url, RelayFailure> {
        self.base_url
            .join(path.trim_start_matches('/'))
            .map_err(|_| RelayFailure {
                status: None,
                message: "团队服务请求地址无效".into(),
            })
    }

    async fn register(
        &self,
        email: &str,
        password: &str,
        display_name: &str,
    ) -> Result<AccountRegistrationResponse, RelayFailure> {
        send_json(
            self.client
                .post(self.endpoint("v1/accounts/register")?)
                .json(&RegisterAccountRequest {
                    email,
                    password,
                    display_name,
                }),
        )
        .await
    }

    async fn login(
        &self,
        email: &str,
        password: &str,
    ) -> Result<AccountSessionResponse, RelayFailure> {
        send_json(
            self.client
                .post(self.endpoint("v1/accounts/login")?)
                .json(&LoginRequest { email, password }),
        )
        .await
    }

    async fn verify_email(&self, token: &str) -> Result<AccountSessionResponse, RelayFailure> {
        send_json(
            self.client
                .post(self.endpoint("v1/accounts/verify-email")?)
                .json(&VerifyEmailRequest { token }),
        )
        .await
    }

    async fn resend_verification_email(&self, email: &str) -> Result<(), RelayFailure> {
        send_empty(
            self.client
                .post(self.endpoint("v1/accounts/resend-verification-email")?)
                .json(&ResendVerificationEmailRequest { email }),
        )
        .await
    }

    async fn bootstrap(
        &self,
        account_token: &str,
        input: &BootstrapWorkspaceRequest,
    ) -> Result<DeviceSessionResponse, RelayFailure> {
        send_json(
            self.client
                .post(self.endpoint("v1/workspaces/bootstrap")?)
                .bearer_auth(account_token)
                .json(input),
        )
        .await
    }

    async fn create_invitation(
        &self,
        workspace_id: &str,
        device_token: &str,
        input: &WorkspaceInvitationRequest<'_>,
    ) -> Result<WorkspaceInvitationResponse, RelayFailure> {
        send_json(
            self.client
                .post(self.endpoint(&format!("v1/workspaces/{workspace_id}/invitations"))?)
                .bearer_auth(device_token)
                .json(input),
        )
        .await
    }

    async fn accept_invitation(
        &self,
        account_token: &str,
        input: &AcceptInvitationRequest<'_>,
    ) -> Result<DeviceSessionResponse, RelayFailure> {
        send_json(
            self.client
                .post(self.endpoint("v1/invitations/accept")?)
                .bearer_auth(account_token)
                .json(input),
        )
        .await
    }

    async fn challenge(
        &self,
        workspace_id: &str,
        device_id: &str,
    ) -> Result<DeviceChallengeResponse, RelayFailure> {
        send_json(
            self.client
                .post(self.endpoint("v1/device-challenges")?)
                .json(&DeviceChallengeRequest {
                    workspace_id,
                    device_id,
                }),
        )
        .await
    }

    async fn create_device_session(
        &self,
        input: &CreateDeviceSessionRequest<'_>,
    ) -> Result<DeviceSessionResponse, RelayFailure> {
        send_json(
            self.client
                .post(self.endpoint("v1/device-sessions")?)
                .json(input),
        )
        .await
    }

    async fn snapshot(
        &self,
        workspace_id: &str,
        device_token: &str,
    ) -> Result<WorkspaceSnapshot, RelayFailure> {
        send_json(
            self.client
                .get(self.endpoint(&format!("v1/workspaces/{workspace_id}"))?)
                .bearer_auth(device_token),
        )
        .await
    }

    async fn update_member(
        &self,
        workspace_id: &str,
        member_id: &str,
        device_token: &str,
        input: &UpdateMemberRequest<'_>,
    ) -> Result<(), RelayFailure> {
        send_empty(
            self.client
                .patch(self.endpoint(&format!("v1/workspaces/{workspace_id}/members/{member_id}"))?)
                .bearer_auth(device_token)
                .json(input),
        )
        .await
    }

    async fn revoke_device(
        &self,
        workspace_id: &str,
        device_id: &str,
        device_token: &str,
    ) -> Result<(), RelayFailure> {
        send_empty(
            self.client
                .delete(
                    self.endpoint(&format!("v1/workspaces/{workspace_id}/devices/{device_id}"))?,
                )
                .bearer_auth(device_token),
        )
        .await
    }

    async fn create_room(
        &self,
        device_token: &str,
        input: &CreateRoomRequest<'_>,
    ) -> Result<RoomResponse, RelayFailure> {
        send_json(
            self.client
                .post(self.endpoint("v1/terminal/rooms")?)
                .bearer_auth(device_token)
                .json(input),
        )
        .await
    }

    async fn route_room_invitation(
        &self,
        room_id: &str,
        device_token: &str,
        invitation: &TeamTerminalInvitation,
    ) -> Result<(), RelayFailure> {
        send_empty(
            self.client
                .post(self.endpoint(&format!("v1/terminal/rooms/{room_id}/invitation"))?)
                .bearer_auth(device_token)
                .json(&RouteRoomInvitationRequest { invitation }),
        )
        .await
    }

    async fn room_invitations(
        &self,
        device_token: &str,
    ) -> Result<Vec<RoutedRoomInvitationResponse>, RelayFailure> {
        send_json(
            self.client
                .get(self.endpoint("v1/terminal/invitations")?)
                .bearer_auth(device_token),
        )
        .await
    }

    async fn join_room(
        &self,
        room_id: &str,
        device_token: &str,
    ) -> Result<RoomResponse, RelayFailure> {
        send_json(
            self.client
                .post(self.endpoint(&format!("v1/terminal/rooms/{room_id}/join"))?)
                .bearer_auth(device_token),
        )
        .await
    }

    async fn leave_room(&self, room_id: &str, device_token: &str) -> Result<(), RelayFailure> {
        send_empty(
            self.client
                .delete(self.endpoint(&format!("v1/terminal/rooms/{room_id}/participants/me"))?)
                .bearer_auth(device_token),
        )
        .await
    }

    async fn grant_control(
        &self,
        room_id: &str,
        device_id: &str,
        duration_seconds: u64,
        device_token: &str,
    ) -> Result<ControlLeaseResponse, RelayFailure> {
        send_json(
            self.client
                .post(self.endpoint(&format!("v1/terminal/rooms/{room_id}/control"))?)
                .bearer_auth(device_token)
                .json(&GrantControlRequest {
                    device_id,
                    duration_seconds,
                }),
        )
        .await
    }

    async fn revoke_control(&self, room_id: &str, device_token: &str) -> Result<(), RelayFailure> {
        send_empty(
            self.client
                .delete(self.endpoint(&format!("v1/terminal/rooms/{room_id}/control"))?)
                .bearer_auth(device_token),
        )
        .await
    }

    async fn close_room(&self, room_id: &str, device_token: &str) -> Result<(), RelayFailure> {
        send_empty(
            self.client
                .delete(self.endpoint(&format!("v1/terminal/rooms/{room_id}"))?)
                .bearer_auth(device_token),
        )
        .await
    }
}

async fn send_json<T: DeserializeOwned>(request: RequestBuilder) -> Result<T, RelayFailure> {
    let response = request.send().await.map_err(network_failure)?;
    let status = response.status();
    let bytes = bounded_response(response).await?;
    if !status.is_success() {
        return Err(response_failure(status, &bytes));
    }
    serde_json::from_slice(&bytes).map_err(|_| RelayFailure {
        status: None,
        message: "团队服务返回了无法识别的数据".into(),
    })
}

async fn send_empty(request: RequestBuilder) -> Result<(), RelayFailure> {
    let response = request.send().await.map_err(network_failure)?;
    let status = response.status();
    let bytes = bounded_response(response).await?;
    if status.is_success() {
        Ok(())
    } else {
        Err(response_failure(status, &bytes))
    }
}

async fn bounded_response(response: reqwest::Response) -> Result<Vec<u8>, RelayFailure> {
    bounded_response_with_limit(response, MAX_RESPONSE_BYTES).await
}

async fn bounded_response_with_limit(
    response: reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, RelayFailure> {
    if response.content_length().unwrap_or(0) > max_bytes as u64 {
        return Err(RelayFailure {
            status: None,
            message: "团队服务响应超过 1 MiB 上限".into(),
        });
    }
    let mut bytes =
        Vec::with_capacity(response.content_length().unwrap_or(0).min(max_bytes as u64) as usize);
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(network_failure)?;
        if bytes.len().saturating_add(chunk.len()) > max_bytes {
            return Err(RelayFailure {
                status: None,
                message: "团队服务响应超过 1 MiB 上限".into(),
            });
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn response_failure(status: StatusCode, bytes: &[u8]) -> RelayFailure {
    let message = serde_json::from_slice::<RelayErrorBody>(bytes)
        .ok()
        .map(|body| body.message)
        .filter(|message| !message.is_empty() && message.len() <= 4096)
        .unwrap_or_else(|| format!("团队服务请求失败（HTTP {}）", status.as_u16()));
    RelayFailure {
        status: Some(status),
        message,
    }
}

fn network_failure(error: reqwest::Error) -> RelayFailure {
    RelayFailure {
        status: None,
        message: format!("无法连接团队服务：{error}"),
    }
}

fn validate_base_url(value: &str) -> AppResult<Url> {
    let mut url =
        Url::parse(value.trim()).map_err(|_| AppError::Validation("团队服务地址无效".into()))?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err(AppError::Validation(
            "团队服务必须使用 HTTPS；仅本机测试允许 HTTP".into(),
        ));
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        return Err(AppError::Validation(
            "团队服务地址不能包含账号、密码、子路径、查询参数或片段".into(),
        ));
    }
    url.set_path("/");
    Ok(url)
}

fn clean_name(value: &str, field: &str) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(AppError::Validation(format!("{field}无效")));
    }
    Ok(value.into())
}

fn validate_uuid(value: &str, field: &str) -> AppResult<()> {
    if value.len() > 64 || uuid::Uuid::parse_str(value).is_err() {
        return Err(AppError::Validation(format!("{field}无效")));
    }
    Ok(())
}

fn session_account(profile_id: &str) -> String {
    format!("profile:{profile_id}:account")
}

fn device_session_account(profile_id: &str, workspace_id: &str, device_id: &str) -> String {
    format!("profile:{profile_id}:workspace:{workspace_id}:device:{device_id}")
}

fn save_session(account: &str, token: &str, expires_at: &str) -> AppResult<()> {
    let mut encoded = serde_json::to_string(&SecretSession {
        token: token.into(),
        expires_at: expires_at.into(),
    })
    .map_err(|error| AppError::Internal(error.to_string()))?;
    let result = keyring::Entry::new(KEYCHAIN_SERVICE, account)
        .map_err(|error| {
            AppError::Storage(format!(
                "无法在{}中创建团队会话项：{error}",
                crate::platform::credential_store_name()
            ))
        })?
        .set_password(&encoded)
        .map_err(|error| AppError::Storage(format!("保存团队会话失败：{error}")));
    encoded.zeroize();
    result
}

fn load_session(account: &str) -> AppResult<Option<SecretSession>> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|error| {
        AppError::Storage(format!(
            "无法在{}中创建团队会话项：{error}",
            crate::platform::credential_store_name()
        ))
    })?;
    let mut encoded = match entry.get_password() {
        Ok(value) => value,
        Err(keyring::Error::NoEntry) => return Ok(None),
        Err(error) => {
            return Err(AppError::Storage(format!("读取团队会话失败：{error}")));
        }
    };
    let result = serde_json::from_str(&encoded).map_err(|_| {
        AppError::Storage(format!(
            "{}中的团队会话格式无效",
            crate::platform::credential_store_name()
        ))
    });
    encoded.zeroize();
    result.map(Some)
}

fn delete_session(account: &str) -> AppResult<()> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|error| {
        AppError::Storage(format!(
            "无法在{}中创建团队会话项：{error}",
            crate::platform::credential_store_name()
        ))
    })?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(AppError::Storage(format!("删除团队会话失败：{error}"))),
    }
}

fn session_is_usable(session: &SecretSession) -> bool {
    DateTime::parse_from_rfc3339(&session.expires_at)
        .map(|expires| {
            expires.with_timezone(&Utc)
                > Utc::now() + ChronoDuration::seconds(TOKEN_REFRESH_MARGIN_SECONDS)
        })
        .unwrap_or(false)
}

async fn stored_profile(db: &Database, id: &str) -> AppResult<StoredProfile> {
    validate_uuid(id, "团队服务配置 ID")?;
    sqlx::query_as::<_, StoredProfile>("SELECT id,name,base_url,account_id,account_email,account_session_expires_at,created_at,updated_at FROM team_relay_profiles WHERE id=?")
        .bind(id)
        .fetch_optional(&db.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("团队服务配置 {id}")))
}

async fn stored_binding(db: &Database, workspace_id: &str) -> AppResult<StoredBinding> {
    validate_uuid(workspace_id, "团队工作区 ID")?;
    sqlx::query_as::<_, StoredBinding>(
        "SELECT workspace_id,profile_id,account_id FROM team_relay_bindings WHERE workspace_id=?",
    )
    .bind(workspace_id)
    .fetch_optional(&db.pool)
    .await?
    .ok_or_else(|| AppError::Unavailable("该工作区尚未连接在线团队服务".into()))
}

fn profile_view(profile: StoredProfile, has_account_session: bool) -> TeamRelayProfile {
    TeamRelayProfile {
        id: profile.id,
        name: profile.name,
        base_url: profile.base_url,
        account_id: profile.account_id,
        account_email: profile.account_email,
        has_account_session,
        account_session_expires_at: profile.account_session_expires_at,
        created_at: profile.created_at,
        updated_at: profile.updated_at,
    }
}

pub async fn profiles(db: &Database) -> AppResult<Vec<TeamRelayProfile>> {
    let profiles = sqlx::query_as::<_, StoredProfile>("SELECT id,name,base_url,account_id,account_email,account_session_expires_at,created_at,updated_at FROM team_relay_profiles ORDER BY name COLLATE NOCASE")
        .fetch_all(&db.pool)
        .await?;
    let mut output = Vec::with_capacity(profiles.len());
    for profile in profiles {
        let has_session = load_session(&session_account(&profile.id))?
            .as_ref()
            .is_some_and(session_is_usable);
        output.push(profile_view(profile, has_session));
    }
    Ok(output)
}

pub async fn save_profile(
    db: &Database,
    input: SaveTeamRelayProfileInput,
) -> AppResult<TeamRelayProfile> {
    validate_uuid(&input.id, "团队服务配置 ID")?;
    let name = clean_name(&input.name, "团队服务名称")?;
    let base_url = validate_base_url(&input.base_url)?.to_string();
    let existing: Option<String> =
        sqlx::query_scalar("SELECT base_url FROM team_relay_profiles WHERE id=?")
            .bind(&input.id)
            .fetch_optional(&db.pool)
            .await?;
    if existing.as_deref().is_some_and(|value| value != base_url) {
        return Err(AppError::Validation(
            "已保存团队服务的地址不可直接修改，请新建配置后迁移工作区".into(),
        ));
    }
    let now = Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO team_relay_profiles(id,name,base_url,account_id,account_email,account_session_expires_at,created_at,updated_at) VALUES(?,?,?,NULL,NULL,NULL,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,updated_at=excluded.updated_at")
        .bind(&input.id)
        .bind(name)
        .bind(base_url)
        .bind(&now)
        .bind(&now)
        .execute(&db.pool)
        .await?;
    let profile = stored_profile(db, &input.id).await?;
    let has_session = load_session(&session_account(&input.id))?
        .as_ref()
        .is_some_and(session_is_usable);
    Ok(profile_view(profile, has_session))
}

pub async fn delete_profile(db: &Database, id: &str) -> AppResult<()> {
    let profile = stored_profile(db, id).await?;
    let binding_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM team_relay_bindings WHERE profile_id=?")
            .bind(id)
            .fetch_one(&db.pool)
            .await?;
    if binding_count != 0 {
        return Err(AppError::Validation(
            "该团队服务仍有关联工作区，不能删除".into(),
        ));
    }
    let pending = sqlx::query_as::<_, PendingAcceptance>("SELECT token_hash,profile_id,device_id,device_name,encryption_public_key,signing_public_key,fingerprint,created_at FROM team_relay_pending_acceptances WHERE profile_id=?")
        .bind(id)
        .fetch_all(&db.pool)
        .await?;
    sqlx::query("DELETE FROM team_relay_profiles WHERE id=?")
        .bind(id)
        .execute(&db.pool)
        .await?;
    delete_session(&session_account(&profile.id))?;
    for item in pending {
        team_share::delete_private_keys(&item.device_id);
    }
    Ok(())
}

pub async fn register_account(
    db: &Database,
    mut input: TeamRelayAccountInput,
) -> AppResult<TeamRelayAccountRegistration> {
    let display_name = clean_name(
        input.display_name.as_deref().unwrap_or_default(),
        "账号显示名称",
    )?;
    let profile = stored_profile(db, &input.profile_id).await?;
    let api = RelayApi::new(&profile.base_url)?;
    let response = api
        .register(&input.email, &input.password, &display_name)
        .await;
    input.password.zeroize();
    let response = response.map_err(RelayFailure::into_app)?;
    if response.verification_required {
        if response.account_session.is_some() {
            return Err(AppError::Remote(
                "团队服务在邮箱待验证状态返回了账号会话".into(),
            ));
        }
        let expires_at = response
            .verification_expires_at
            .as_deref()
            .ok_or_else(|| AppError::Remote("团队服务未返回邮箱验证到期时间".into()))?;
        DateTime::parse_from_rfc3339(expires_at)
            .map_err(|_| AppError::Remote("团队服务返回的邮箱验证到期时间无效".into()))?;
        return Ok(TeamRelayAccountRegistration {
            profile: profile_view(profile, false),
            verification_required: true,
            verification_expires_at: response.verification_expires_at,
        });
    }
    let session = response
        .account_session
        .ok_or_else(|| AppError::Remote("团队服务未返回注册账号会话".into()))?;
    let profile = save_account_response(db, profile, session).await?;
    Ok(TeamRelayAccountRegistration {
        profile,
        verification_required: false,
        verification_expires_at: None,
    })
}

pub async fn login_account(
    db: &Database,
    mut input: TeamRelayAccountInput,
) -> AppResult<TeamRelayProfile> {
    let profile = stored_profile(db, &input.profile_id).await?;
    let api = RelayApi::new(&profile.base_url)?;
    let response = api.login(&input.email, &input.password).await;
    input.password.zeroize();
    save_account_response(db, profile, response.map_err(RelayFailure::into_app)?).await
}

pub async fn verify_account_email(
    db: &Database,
    mut input: VerifyTeamRelayAccountInput,
) -> AppResult<TeamRelayProfile> {
    let profile = stored_profile(db, &input.profile_id).await?;
    validate_secret_token(&input.token, "邮箱验证令牌")?;
    let api = RelayApi::new(&profile.base_url)?;
    let response = api.verify_email(&input.token).await;
    input.token.zeroize();
    let response = response.map_err(RelayFailure::into_app)?;
    save_account_response(db, profile, response).await
}

pub async fn resend_account_verification(
    db: &Database,
    input: ResendTeamRelayVerificationInput,
) -> AppResult<()> {
    let profile = stored_profile(db, &input.profile_id).await?;
    if input.email.len() > 254 || !input.email.contains('@') {
        return Err(AppError::Validation("邮箱格式无效".into()));
    }
    RelayApi::new(&profile.base_url)?
        .resend_verification_email(input.email.trim())
        .await
        .map_err(RelayFailure::into_app)
}

async fn save_account_response(
    db: &Database,
    profile: StoredProfile,
    response: AccountSessionResponse,
) -> AppResult<TeamRelayProfile> {
    validate_uuid(&response.account_id, "团队账号 ID")?;
    if response.email.len() > 254 || !response.email.contains('@') {
        return Err(AppError::Remote("团队服务返回的账号邮箱无效".into()));
    }
    DateTime::parse_from_rfc3339(&response.expires_at)
        .map_err(|_| AppError::Remote("团队服务返回的账号会话到期时间无效".into()))?;
    let binding_accounts: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT account_id FROM team_relay_bindings WHERE profile_id=?",
    )
    .bind(&profile.id)
    .fetch_all(&db.pool)
    .await?;
    if binding_accounts
        .iter()
        .any(|account_id| account_id != &response.account_id)
    {
        return Err(AppError::PermissionDenied(
            "该配置已有其他账号的在线工作区，不能切换账号".into(),
        ));
    }
    save_session(
        &session_account(&profile.id),
        &response.token,
        &response.expires_at,
    )?;
    let now = Utc::now().to_rfc3339();
    if let Err(error) = sqlx::query("UPDATE team_relay_profiles SET account_id=?,account_email=?,account_session_expires_at=?,updated_at=? WHERE id=?")
        .bind(&response.account_id)
        .bind(response.email.trim().to_ascii_lowercase())
        .bind(&response.expires_at)
        .bind(&now)
        .bind(&profile.id)
        .execute(&db.pool)
        .await
    {
        let _ = delete_session(&session_account(&profile.id));
        return Err(error.into());
    }
    Ok(profile_view(stored_profile(db, &profile.id).await?, true))
}

pub async fn logout_account(db: &Database, profile_id: &str) -> AppResult<()> {
    let profile = stored_profile(db, profile_id).await?;
    let device_rows = sqlx::query("SELECT b.workspace_id,w.local_device_id FROM team_relay_bindings b JOIN team_workspaces w ON w.id=b.workspace_id WHERE b.profile_id=?")
        .bind(profile_id)
        .fetch_all(&db.pool)
        .await?;
    delete_session(&session_account(&profile.id))?;
    for row in device_rows {
        let workspace_id: String = row.get(0);
        let device_id: Option<String> = row.get(1);
        if let Some(device_id) = device_id {
            delete_session(&device_session_account(
                profile_id,
                &workspace_id,
                &device_id,
            ))?;
        }
    }
    let now = Utc::now().to_rfc3339();
    let mut transaction = db.pool.begin().await?;
    sqlx::query(
        "UPDATE team_relay_profiles SET account_session_expires_at=NULL,updated_at=? WHERE id=?",
    )
    .bind(&now)
    .bind(profile_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query("UPDATE team_relay_bindings SET device_session_expires_at=NULL,updated_at=? WHERE profile_id=?")
        .bind(&now)
        .bind(profile_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}

fn account_session(profile: &StoredProfile) -> AppResult<SecretSession> {
    let session = load_session(&session_account(&profile.id))?
        .ok_or_else(|| AppError::Authentication("请先登录在线团队账号".into()))?;
    if !session_is_usable(&session) {
        return Err(AppError::Authentication(
            "在线团队账号会话已过期，请重新登录".into(),
        ));
    }
    Ok(session)
}

pub async fn bindings(db: &Database) -> AppResult<Vec<TeamRelayWorkspaceBinding>> {
    let rows = sqlx::query("SELECT b.workspace_id,b.profile_id,p.name,p.base_url,b.account_id,b.device_session_expires_at,b.last_synced_at FROM team_relay_bindings b JOIN team_relay_profiles p ON p.id=b.profile_id ORDER BY p.name,b.workspace_id")
        .fetch_all(&db.pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| TeamRelayWorkspaceBinding {
            workspace_id: row.get(0),
            profile_id: row.get(1),
            profile_name: row.get(2),
            base_url: row.get(3),
            account_id: row.get(4),
            device_session_expires_at: row.get(5),
            last_synced_at: row.get(6),
        })
        .collect())
}

pub async fn publish_workspace(
    db: &Database,
    workspace_id: &str,
    profile_id: &str,
) -> AppResult<TeamRelayWorkspaceBinding> {
    validate_uuid(workspace_id, "团队工作区 ID")?;
    if sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM team_relay_bindings WHERE workspace_id=?")
        .bind(workspace_id)
        .fetch_one(&db.pool)
        .await?
        != 0
    {
        return Err(AppError::Validation("该工作区已经连接在线团队服务".into()));
    }
    let profile = stored_profile(db, profile_id).await?;
    let account_id = profile
        .account_id
        .clone()
        .ok_or_else(|| AppError::Authentication("请先登录在线团队账号".into()))?;
    let account = account_session(&profile)?;
    let workspace = team::list_workspaces(db)
        .await?
        .into_iter()
        .find(|workspace| workspace.id == workspace_id)
        .ok_or_else(|| AppError::NotFound(format!("团队工作区 {workspace_id}")))?;
    if workspace.local_role != "owner" {
        return Err(AppError::PermissionDenied(
            "只有本地 Owner 可以发布工作区".into(),
        ));
    }
    let active_members: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM team_members WHERE workspace_id=? AND status='active'",
    )
    .bind(workspace_id)
    .fetch_one(&db.pool)
    .await?;
    let active_devices: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM team_devices WHERE workspace_id=? AND status='active'",
    )
    .bind(workspace_id)
    .fetch_one(&db.pool)
    .await?;
    if active_members != 1 || active_devices != 1 {
        return Err(AppError::Validation(
            "发布前工作区只能包含本机 Owner 和一台本机设备；其他成员请改用在线邀请加入".into(),
        ));
    }
    let device = local_device(db, workspace_id).await?;
    let api = RelayApi::new(&profile.base_url)?;
    let request = BootstrapWorkspaceRequest {
        workspace_id: workspace.id.clone(),
        workspace_name: workspace.name,
        member_id: workspace.local_member_id.clone(),
        device: DeviceRegistration::from(&device),
    };
    let session = match api.bootstrap(&account.token, &request).await {
        Ok(session) => session,
        Err(error) if error.status == Some(StatusCode::CONFLICT) => {
            refresh_device_session(&api, workspace_id, &device.id).await?
        }
        Err(error) => return Err(error.into_app()),
    };
    validate_device_session(
        &session,
        workspace_id,
        &workspace.local_member_id,
        &device.id,
    )?;
    save_device_session(profile_id, &session)?;
    let snapshot = api
        .snapshot(workspace_id, &session.token)
        .await
        .map_err(RelayFailure::into_app)?;
    apply_existing_snapshot(db, &profile, &account_id, &session, snapshot, true).await?;
    binding_view(db, workspace_id).await
}

pub async fn sync_workspace(db: &Database, workspace_id: &str) -> AppResult<TeamWorkspace> {
    let binding = stored_binding(db, workspace_id).await?;
    let profile = stored_profile(db, &binding.profile_id).await?;
    let device = local_device(db, workspace_id).await?;
    let api = RelayApi::new(&profile.base_url)?;
    let session = ensure_device_session(db, &api, &binding, &device.id).await?;
    let snapshot = api
        .snapshot(workspace_id, &session.token)
        .await
        .map_err(RelayFailure::into_app)?;
    apply_existing_snapshot(db, &profile, &binding.account_id, &session, snapshot, false).await?;
    team::list_workspaces(db)
        .await?
        .into_iter()
        .find(|workspace| workspace.id == workspace_id)
        .ok_or_else(|| AppError::NotFound(format!("团队工作区 {workspace_id}")))
}

pub async fn create_invitation(
    db: &Database,
    input: CreateTeamRelayInvitationInput,
) -> AppResult<TeamRelayInvitation> {
    if !matches!(input.role.as_str(), "admin" | "operator" | "viewer") {
        return Err(AppError::Validation(
            "在线邀请角色必须是 Admin、Operator 或 Viewer".into(),
        ));
    }
    let binding = stored_binding(db, &input.workspace_id).await?;
    let profile = stored_profile(db, &binding.profile_id).await?;
    let device = local_device(db, &input.workspace_id).await?;
    let api = RelayApi::new(&profile.base_url)?;
    let session = ensure_device_session(db, &api, &binding, &device.id).await?;
    let member_id = uuid::Uuid::new_v4().to_string();
    let response = api
        .create_invitation(
            &input.workspace_id,
            &session.token,
            &WorkspaceInvitationRequest {
                email: &input.email,
                role: &input.role,
                member_id: &member_id,
            },
        )
        .await
        .map_err(RelayFailure::into_app)?;
    Ok(TeamRelayInvitation {
        invitation_id: response.invitation_id,
        token: response.token,
        member_id,
        email: input.email.trim().to_ascii_lowercase(),
        role: input.role,
        expires_at: response.expires_at,
    })
}

pub async fn accept_invitation(
    db: &Database,
    input: AcceptTeamRelayInvitationInput,
) -> AppResult<TeamWorkspace> {
    let profile = stored_profile(db, &input.profile_id).await?;
    let account_id = profile
        .account_id
        .clone()
        .ok_or_else(|| AppError::Authentication("请先登录在线团队账号".into()))?;
    let account = account_session(&profile)?;
    let workspace_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM team_workspaces")
        .fetch_one(&db.pool)
        .await?;
    if workspace_count >= 32 {
        return Err(AppError::Validation("最多保存 32 个团队工作区".into()));
    }
    let pending = pending_acceptance(db, &profile.id, &input.token, &input.device_name).await?;
    let registration = DeviceRegistration::from(&pending);
    let api = RelayApi::new(&profile.base_url)?;
    let response = api
        .accept_invitation(
            &account.token,
            &AcceptInvitationRequest {
                token: &input.token,
                device: &registration,
            },
        )
        .await;
    let session = match response {
        Ok(session) => session,
        Err(error) => {
            if !error.can_retry_without_changing_identity() {
                cleanup_pending_acceptance(db, &pending).await?;
            }
            return Err(error.into_app());
        }
    };
    validate_device_session(
        &session,
        &session.workspace_id.clone(),
        &session.member_id.clone(),
        &registration.id,
    )?;
    save_device_session(&profile.id, &session)?;
    let snapshot = api
        .snapshot(&session.workspace_id, &session.token)
        .await
        .map_err(RelayFailure::into_app)?;
    materialize_snapshot(db, &profile, &account_id, &session, snapshot, &pending).await?;
    sqlx::query("DELETE FROM team_relay_pending_acceptances WHERE token_hash=?")
        .bind(&pending.token_hash)
        .execute(&db.pool)
        .await?;
    team::list_workspaces(db)
        .await?
        .into_iter()
        .find(|workspace| workspace.id == session.workspace_id)
        .ok_or_else(|| AppError::NotFound(format!("团队工作区 {}", session.workspace_id)))
}

pub async fn update_member(
    db: &Database,
    input: UpdateTeamRelayMemberInput,
) -> AppResult<TeamWorkspace> {
    validate_uuid(&input.member_id, "团队成员 ID")?;
    if !matches!(
        (input.role.as_str(), input.status.as_str()),
        (
            "owner" | "admin" | "operator" | "viewer",
            "active" | "removed"
        )
    ) {
        return Err(AppError::Validation("团队成员角色或状态无效".into()));
    }
    let binding = stored_binding(db, &input.workspace_id).await?;
    let profile = stored_profile(db, &binding.profile_id).await?;
    let device = local_device(db, &input.workspace_id).await?;
    let api = RelayApi::new(&profile.base_url)?;
    let session = ensure_device_session(db, &api, &binding, &device.id).await?;
    api.update_member(
        &input.workspace_id,
        &input.member_id,
        &session.token,
        &UpdateMemberRequest {
            role: &input.role,
            status: &input.status,
        },
    )
    .await
    .map_err(RelayFailure::into_app)?;
    sync_workspace(db, &input.workspace_id).await
}

pub async fn revoke_device(
    db: &Database,
    workspace_id: &str,
    device_id: &str,
) -> AppResult<TeamWorkspace> {
    validate_uuid(device_id, "团队设备 ID")?;
    let binding = stored_binding(db, workspace_id).await?;
    let profile = stored_profile(db, &binding.profile_id).await?;
    let local = local_device(db, workspace_id).await?;
    if local.id == device_id {
        return Err(AppError::Validation(
            "不能在当前设备上撤销本机在线身份，请先由另一台 Owner/Admin 设备完成交接".into(),
        ));
    }
    let api = RelayApi::new(&profile.base_url)?;
    let session = ensure_device_session(db, &api, &binding, &local.id).await?;
    api.revoke_device(workspace_id, device_id, &session.token)
        .await
        .map_err(RelayFailure::into_app)?;
    sync_workspace(db, workspace_id).await
}

pub(crate) async fn device_context(
    db: &Database,
    workspace_id: &str,
) -> AppResult<RelayDeviceContext> {
    let binding = stored_binding(db, workspace_id).await?;
    let profile = stored_profile(db, &binding.profile_id).await?;
    let device = local_device(db, workspace_id).await?;
    let api = RelayApi::new(&profile.base_url)?;
    let session = ensure_device_session(db, &api, &binding, &device.id).await?;
    if session.workspace_id != workspace_id
        || session.member_id != device.member_id
        || session.device_id != device.id
    {
        return Err(AppError::Remote("在线团队设备会话与本机身份不匹配".into()));
    }
    Ok(RelayDeviceContext {
        base_url: profile.base_url,
        workspace_id: workspace_id.into(),
        member_id: device.member_id,
        device_id: device.id,
        token: Zeroizing::new(session.token.clone()),
        expires_at: session.expires_at.clone(),
    })
}

pub(crate) async fn terminal_create_room(
    db: &Database,
    room: &TeamTerminalRoom,
) -> AppResult<RelayDeviceContext> {
    let context = device_context(db, &room.workspace_id).await?;
    let response = RelayApi::new(&context.base_url)?
        .create_room(
            &context.token,
            &CreateRoomRequest {
                room_id: &room.id,
                key_epoch: room.key_epoch,
            },
        )
        .await
        .map_err(RelayFailure::into_app)?;
    validate_room_response(&response, room, &context)?;
    Ok(context)
}

pub(crate) async fn terminal_route_invitation(
    db: &Database,
    invitation: &TeamTerminalInvitation,
) -> AppResult<()> {
    let context = device_context(db, &invitation.workspace_id).await?;
    if invitation.host_member_id != context.member_id
        || invitation.host_device_id != context.device_id
    {
        return Err(AppError::PermissionDenied(
            "只有当前在线主持设备可以路由房间邀请".into(),
        ));
    }
    RelayApi::new(&context.base_url)?
        .route_room_invitation(&invitation.room_id, &context.token, invitation)
        .await
        .map_err(RelayFailure::into_app)
}

pub async fn terminal_invitations(
    db: &Database,
    workspace_id: &str,
) -> AppResult<Vec<TeamRelayTerminalInvitation>> {
    let context = device_context(db, workspace_id).await?;
    let invitations = RelayApi::new(&context.base_url)?
        .room_invitations(&context.token)
        .await
        .map_err(RelayFailure::into_app)?;
    if invitations.len() > 256 {
        return Err(AppError::Remote(
            "团队服务返回的待处理房间邀请超过 256 条上限".into(),
        ));
    }
    invitations
        .into_iter()
        .map(|routed| {
            if routed.room_id != routed.invitation.room_id
                || routed.invitation.workspace_id != workspace_id
                || routed.invitation.recipient_member_id != context.member_id
                || routed.invitation.recipient_device_id != context.device_id
            {
                return Err(AppError::Remote(
                    "团队服务返回了不属于当前工作区或设备的房间邀请".into(),
                ));
            }
            Ok(TeamRelayTerminalInvitation {
                room_id: routed.room_id,
                invitation: routed.invitation,
            })
        })
        .collect()
}

pub(crate) async fn terminal_join_room(
    db: &Database,
    invitation: &TeamTerminalInvitation,
) -> AppResult<RelayDeviceContext> {
    let context = device_context(db, &invitation.workspace_id).await?;
    let response = RelayApi::new(&context.base_url)?
        .join_room(&invitation.room_id, &context.token)
        .await
        .map_err(RelayFailure::into_app)?;
    if response.id != invitation.room_id
        || response.workspace_id != invitation.workspace_id
        || response.host_member_id != invitation.host_member_id
        || response.host_device_id != invitation.host_device_id
        || response.key_epoch != invitation.key_epoch
        || response.status != "active"
    {
        return Err(AppError::Remote(
            "团队服务加入房间响应与端到端邀请不匹配".into(),
        ));
    }
    Ok(context)
}

pub(crate) async fn terminal_leave_room(
    db: &Database,
    workspace_id: &str,
    room_id: &str,
) -> AppResult<()> {
    let context = device_context(db, workspace_id).await?;
    RelayApi::new(&context.base_url)?
        .leave_room(room_id, &context.token)
        .await
        .map_err(RelayFailure::into_app)
}

pub(crate) async fn terminal_grant_control(
    db: &Database,
    workspace_id: &str,
    room_id: &str,
    device_id: &str,
    duration_seconds: u64,
) -> AppResult<TeamControlLease> {
    validate_uuid(room_id, "在线团队房间 ID")?;
    validate_uuid(device_id, "在线团队设备 ID")?;
    let context = device_context(db, workspace_id).await?;
    let response = RelayApi::new(&context.base_url)?
        .grant_control(room_id, device_id, duration_seconds, &context.token)
        .await
        .map_err(RelayFailure::into_app)?;
    let lease = TeamControlLease {
        id: response.lease_id,
        member_id: response.member_id,
        device_id: response.device_id,
        generation: response.generation,
        expires_at: response.expires_at,
    };
    if lease.device_id != device_id
        || lease.generation == 0
        || uuid::Uuid::parse_str(&lease.id).is_err()
        || DateTime::parse_from_rfc3339(&lease.expires_at).is_err()
    {
        return Err(AppError::Remote(
            "团队服务返回的控制租约身份或时间无效".into(),
        ));
    }
    Ok(lease)
}

pub(crate) async fn terminal_revoke_control(
    db: &Database,
    workspace_id: &str,
    room_id: &str,
) -> AppResult<()> {
    let context = device_context(db, workspace_id).await?;
    RelayApi::new(&context.base_url)?
        .revoke_control(room_id, &context.token)
        .await
        .map_err(RelayFailure::into_app)
}

pub(crate) async fn terminal_close_room(
    db: &Database,
    workspace_id: &str,
    room_id: &str,
) -> AppResult<()> {
    let context = device_context(db, workspace_id).await?;
    RelayApi::new(&context.base_url)?
        .close_room(room_id, &context.token)
        .await
        .map_err(RelayFailure::into_app)
}

fn validate_room_response(
    response: &RoomResponse,
    room: &TeamTerminalRoom,
    context: &RelayDeviceContext,
) -> AppResult<()> {
    if response.id != room.id
        || response.workspace_id != room.workspace_id
        || response.host_member_id != context.member_id
        || response.host_device_id != context.device_id
        || response.key_epoch != room.key_epoch
        || response.status != "active"
    {
        return Err(AppError::Remote(
            "团队服务创建房间响应与本机主持身份不匹配".into(),
        ));
    }
    Ok(())
}

async fn binding_view(db: &Database, workspace_id: &str) -> AppResult<TeamRelayWorkspaceBinding> {
    bindings(db)
        .await?
        .into_iter()
        .find(|binding| binding.workspace_id == workspace_id)
        .ok_or_else(|| AppError::Unavailable("在线团队工作区绑定未保存".into()))
}

async fn local_device(db: &Database, workspace_id: &str) -> AppResult<TeamDevice> {
    let device = sqlx::query_as::<_, TeamDevice>("SELECT d.id,d.workspace_id,d.member_id,d.name,d.encryption_public_key,d.signing_public_key,d.fingerprint,d.is_local,d.status,d.created_at,d.updated_at,d.revoked_at FROM team_workspaces w JOIN team_devices d ON d.id=w.local_device_id AND d.workspace_id=w.id WHERE w.id=? AND d.is_local=1 AND d.status='active'")
        .bind(workspace_id)
        .fetch_optional(&db.pool)
        .await?
        .ok_or_else(|| AppError::Unavailable("请先为工作区创建本机设备身份".into()))?;
    team_share::validated_device_keys(&device)?;
    Ok(device)
}

fn validate_device_session(
    session: &DeviceSessionResponse,
    workspace_id: &str,
    member_id: &str,
    device_id: &str,
) -> AppResult<()> {
    if session.workspace_id != workspace_id
        || session.member_id != member_id
        || session.device_id != device_id
        || !matches!(
            session.role.as_str(),
            "owner" | "admin" | "operator" | "viewer"
        )
        || session.key_epoch < 1
        || DateTime::parse_from_rfc3339(&session.expires_at).is_err()
    {
        return Err(AppError::Remote(
            "团队服务返回的设备会话身份或到期时间不匹配".into(),
        ));
    }
    Ok(())
}

fn save_device_session(profile_id: &str, session: &DeviceSessionResponse) -> AppResult<()> {
    save_session(
        &device_session_account(profile_id, &session.workspace_id, &session.device_id),
        &session.token,
        &session.expires_at,
    )
}

async fn ensure_device_session(
    db: &Database,
    api: &RelayApi,
    binding: &StoredBinding,
    device_id: &str,
) -> AppResult<DeviceSessionResponse> {
    let account = device_session_account(&binding.profile_id, &binding.workspace_id, device_id);
    if let Some(session) = load_session(&account)?.filter(session_is_usable) {
        return Ok(DeviceSessionResponse {
            workspace_id: binding.workspace_id.clone(),
            member_id: sqlx::query_scalar("SELECT local_member_id FROM team_workspaces WHERE id=?")
                .bind(&binding.workspace_id)
                .fetch_one(&db.pool)
                .await?,
            device_id: device_id.into(),
            role: String::new(),
            key_epoch: 0,
            token: session.token.clone(),
            expires_at: session.expires_at.clone(),
        });
    }
    let session = refresh_device_session(api, &binding.workspace_id, device_id).await?;
    save_device_session(&binding.profile_id, &session)?;
    sqlx::query("UPDATE team_relay_bindings SET device_session_expires_at=?,updated_at=? WHERE workspace_id=?")
        .bind(&session.expires_at)
        .bind(Utc::now().to_rfc3339())
        .bind(&binding.workspace_id)
        .execute(&db.pool)
        .await?;
    Ok(session)
}

async fn refresh_device_session(
    api: &RelayApi,
    workspace_id: &str,
    device_id: &str,
) -> AppResult<DeviceSessionResponse> {
    let challenge = api
        .challenge(workspace_id, device_id)
        .await
        .map_err(RelayFailure::into_app)?;
    DateTime::parse_from_rfc3339(&challenge.expires_at)
        .map_err(|_| AppError::Remote("团队服务返回的设备挑战到期时间无效".into()))?;
    let signing_secret = Zeroizing::new(team_share::load_private_key(device_id, "ed25519")?);
    let signing_key = SigningKey::from_bytes(&signing_secret);
    let payload = format!(
        "cnshell-relay-device-session-v1\0{}\0{}",
        challenge.challenge_id, challenge.challenge
    );
    let mut signature = format!(
        "ed25519:{}",
        URL_SAFE_NO_PAD.encode(signing_key.sign(payload.as_bytes()).to_bytes())
    );
    let response = api
        .create_device_session(&CreateDeviceSessionRequest {
            challenge_id: &challenge.challenge_id,
            challenge: &challenge.challenge,
            signature: &signature,
        })
        .await;
    signature.zeroize();
    let response = response.map_err(RelayFailure::into_app)?;
    validate_device_session(
        &response,
        workspace_id,
        &response.member_id.clone(),
        device_id,
    )?;
    Ok(response)
}

fn validate_snapshot(
    snapshot: &WorkspaceSnapshot,
    workspace_id: &str,
    local_member_id: &str,
    local_device_id: &str,
    expected_local_keys: (&str, &str, &str),
) -> AppResult<()> {
    validate_uuid(&snapshot.id, "服务端工作区 ID")?;
    if snapshot.id != workspace_id || snapshot.key_epoch < 1 {
        return Err(AppError::Remote(
            "团队服务快照的工作区或密钥 epoch 不匹配".into(),
        ));
    }
    clean_name(&snapshot.name, "服务端工作区名称")?;
    if snapshot.members.is_empty()
        || snapshot.members.len() > MAX_MEMBERS
        || snapshot.devices.is_empty()
        || snapshot.devices.len() > MAX_DEVICES
    {
        return Err(AppError::Remote("团队服务快照数量超过客户端上限".into()));
    }
    let mut member_ids = HashSet::new();
    for member in &snapshot.members {
        validate_uuid(&member.id, "服务端成员 ID")?;
        clean_name(&member.display_name, "服务端成员名称")?;
        if !member_ids.insert(member.id.as_str())
            || !matches!(
                member.role.as_str(),
                "owner" | "admin" | "operator" | "viewer"
            )
            || !matches!(member.status.as_str(), "active" | "removed")
        {
            return Err(AppError::Remote("团队服务成员快照无效".into()));
        }
    }
    if !snapshot
        .members
        .iter()
        .any(|member| member.id == local_member_id && member.status == "active")
    {
        return Err(AppError::PermissionDenied(
            "本机成员已被在线工作区移除".into(),
        ));
    }
    let mut device_ids = HashSet::new();
    for device in &snapshot.devices {
        validate_uuid(&device.id, "服务端设备 ID")?;
        clean_name(&device.name, "服务端设备名称")?;
        if !device_ids.insert(device.id.as_str())
            || !member_ids.contains(device.member_id.as_str())
            || !matches!(device.status.as_str(), "active" | "revoked")
        {
            return Err(AppError::Remote("团队服务设备快照无效".into()));
        }
        let candidate = TeamDevice {
            id: device.id.clone(),
            workspace_id: snapshot.id.clone(),
            member_id: device.member_id.clone(),
            name: device.name.clone(),
            encryption_public_key: device.encryption_public_key.clone(),
            signing_public_key: device.signing_public_key.clone(),
            fingerprint: device.fingerprint.clone(),
            is_local: device.id == local_device_id,
            status: device.status.clone(),
            created_at: String::new(),
            updated_at: String::new(),
            revoked_at: None,
        };
        team_share::validated_device_keys(&candidate)?;
    }
    if !snapshot.devices.iter().any(|device| {
        device.id == local_device_id
            && device.member_id == local_member_id
            && device.status == "active"
            && device.encryption_public_key == expected_local_keys.0
            && device.signing_public_key == expected_local_keys.1
            && device.fingerprint == expected_local_keys.2
    }) {
        return Err(AppError::PermissionDenied(format!(
            "本机设备已被撤销，或服务端公钥与本机{}中的设备身份不匹配",
            crate::platform::credential_store_name()
        )));
    }
    Ok(())
}

async fn apply_existing_snapshot(
    db: &Database,
    profile: &StoredProfile,
    account_id: &str,
    session: &DeviceSessionResponse,
    snapshot: WorkspaceSnapshot,
    create_binding: bool,
) -> AppResult<()> {
    let row = sqlx::query("SELECT w.local_member_id,d.id,d.encryption_public_key,d.signing_public_key,d.fingerprint FROM team_workspaces w JOIN team_devices d ON d.id=w.local_device_id AND d.workspace_id=w.id WHERE w.id=? AND d.is_local=1 AND d.status='active'")
        .bind(&snapshot.id)
        .fetch_optional(&db.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("团队工作区 {}", snapshot.id)))?;
    let local_member_id: String = row.get(0);
    let local_device_id: String = row.get(1);
    let expected_encryption_key: String = row.get(2);
    let expected_signing_key: String = row.get(3);
    let expected_fingerprint: String = row.get(4);
    validate_snapshot(
        &snapshot,
        &snapshot.id,
        &local_member_id,
        &local_device_id,
        (
            &expected_encryption_key,
            &expected_signing_key,
            &expected_fingerprint,
        ),
    )?;
    if session.workspace_id != snapshot.id
        || session.member_id != local_member_id
        || session.device_id != local_device_id
    {
        return Err(AppError::Remote("设备会话与本地工作区身份不匹配".into()));
    }
    let now = Utc::now().to_rfc3339();
    let mut transaction = db.pool.begin().await?;
    apply_snapshot_rows(
        &mut transaction,
        &snapshot,
        &local_member_id,
        &local_device_id,
        &now,
    )
    .await?;
    if create_binding {
        sqlx::query("INSERT INTO team_relay_bindings(workspace_id,profile_id,account_id,device_session_expires_at,last_synced_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?)")
            .bind(&snapshot.id)
            .bind(&profile.id)
            .bind(account_id)
            .bind(&session.expires_at)
            .bind(&now)
            .bind(&now)
            .bind(&now)
            .execute(&mut *transaction)
            .await?;
    } else {
        sqlx::query("UPDATE team_relay_bindings SET device_session_expires_at=?,last_synced_at=?,updated_at=? WHERE workspace_id=? AND profile_id=? AND account_id=?")
            .bind(&session.expires_at)
            .bind(&now)
            .bind(&now)
            .bind(&snapshot.id)
            .bind(&profile.id)
            .bind(account_id)
            .execute(&mut *transaction)
            .await?;
    }
    transaction.commit().await?;
    Ok(())
}

async fn materialize_snapshot(
    db: &Database,
    profile: &StoredProfile,
    account_id: &str,
    session: &DeviceSessionResponse,
    snapshot: WorkspaceSnapshot,
    pending: &PendingAcceptance,
) -> AppResult<()> {
    validate_snapshot(
        &snapshot,
        &session.workspace_id,
        &session.member_id,
        &session.device_id,
        (
            &pending.encryption_public_key,
            &pending.signing_public_key,
            &pending.fingerprint,
        ),
    )?;
    if pending.device_id != session.device_id {
        return Err(AppError::Remote(
            "邀请响应使用了非本机生成的设备身份".into(),
        ));
    }
    let now = Utc::now().to_rfc3339();
    let mut transaction = db.pool.begin().await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM team_workspaces")
        .fetch_one(&mut *transaction)
        .await?;
    if count >= 32 {
        return Err(AppError::Validation("最多保存 32 个团队工作区".into()));
    }
    let collision: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM team_workspaces WHERE id=?")
        .bind(&snapshot.id)
        .fetch_one(&mut *transaction)
        .await?;
    if collision != 0 {
        return Err(AppError::Validation(
            "邀请对应的工作区 ID 已在本机存在，未覆盖本地数据".into(),
        ));
    }
    sqlx::query("INSERT INTO team_workspaces(id,name,local_member_id,key_epoch,created_at,updated_at,local_device_id) VALUES(?,?,?,?,?,?,?)")
        .bind(&snapshot.id)
        .bind(&snapshot.name)
        .bind(&session.member_id)
        .bind(snapshot.key_epoch)
        .bind(&now)
        .bind(&now)
        .bind(&session.device_id)
        .execute(&mut *transaction)
        .await?;
    insert_snapshot_rows(
        &mut transaction,
        &snapshot,
        &session.member_id,
        &session.device_id,
        &now,
    )
    .await?;
    sqlx::query("INSERT INTO team_relay_bindings(workspace_id,profile_id,account_id,device_session_expires_at,last_synced_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?)")
        .bind(&snapshot.id)
        .bind(&profile.id)
        .bind(account_id)
        .bind(&session.expires_at)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(&mut *transaction)
        .await?;
    team::audit(
        &mut transaction,
        &snapshot.id,
        &session.member_id,
        "relay-workspace-accepted",
        "workspace",
        &snapshot.id,
    )
    .await?;
    transaction.commit().await?;
    Ok(())
}

async fn apply_snapshot_rows(
    transaction: &mut Transaction<'_, Sqlite>,
    snapshot: &WorkspaceSnapshot,
    local_member_id: &str,
    local_device_id: &str,
    now: &str,
) -> AppResult<()> {
    sqlx::query("UPDATE team_workspaces SET name=?,local_member_id=?,local_device_id=?,key_epoch=?,updated_at=? WHERE id=?")
        .bind(&snapshot.name)
        .bind(local_member_id)
        .bind(local_device_id)
        .bind(snapshot.key_epoch)
        .bind(now)
        .bind(&snapshot.id)
        .execute(&mut **transaction)
        .await?;
    sqlx::query("UPDATE team_members SET status='removed',removed_at=COALESCE(removed_at,?),updated_at=? WHERE workspace_id=?")
        .bind(now)
        .bind(now)
        .bind(&snapshot.id)
        .execute(&mut **transaction)
        .await?;
    sqlx::query("UPDATE team_devices SET is_local=0,status='revoked',revoked_at=COALESCE(revoked_at,?),updated_at=? WHERE workspace_id=?")
        .bind(now)
        .bind(now)
        .bind(&snapshot.id)
        .execute(&mut **transaction)
        .await?;
    insert_snapshot_rows(transaction, snapshot, local_member_id, local_device_id, now).await
}

async fn insert_snapshot_rows(
    transaction: &mut Transaction<'_, Sqlite>,
    snapshot: &WorkspaceSnapshot,
    _local_member_id: &str,
    local_device_id: &str,
    now: &str,
) -> AppResult<()> {
    for member in &snapshot.members {
        let existing_workspace: Option<String> =
            sqlx::query_scalar("SELECT workspace_id FROM team_members WHERE id=?")
                .bind(&member.id)
                .fetch_optional(&mut **transaction)
                .await?;
        if existing_workspace
            .as_deref()
            .is_some_and(|workspace| workspace != snapshot.id)
        {
            return Err(AppError::Storage("服务端成员 ID 与另一工作区冲突".into()));
        }
        sqlx::query("INSERT INTO team_members(id,workspace_id,display_name,role,status,joined_at,updated_at,removed_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET display_name=excluded.display_name,role=excluded.role,status=excluded.status,updated_at=excluded.updated_at,removed_at=excluded.removed_at")
            .bind(&member.id)
            .bind(&snapshot.id)
            .bind(&member.display_name)
            .bind(&member.role)
            .bind(&member.status)
            .bind(now)
            .bind(now)
            .bind((member.status == "removed").then_some(now))
            .execute(&mut **transaction)
            .await?;
    }
    for device in &snapshot.devices {
        let existing_workspace: Option<String> =
            sqlx::query_scalar("SELECT workspace_id FROM team_devices WHERE id=?")
                .bind(&device.id)
                .fetch_optional(&mut **transaction)
                .await?;
        if existing_workspace
            .as_deref()
            .is_some_and(|workspace| workspace != snapshot.id)
        {
            return Err(AppError::Storage("服务端设备 ID 与另一工作区冲突".into()));
        }
        let is_local = i64::from(device.id == local_device_id);
        sqlx::query("INSERT INTO team_devices(id,workspace_id,member_id,name,encryption_public_key,signing_public_key,fingerprint,is_local,status,created_at,updated_at,revoked_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET member_id=excluded.member_id,name=excluded.name,encryption_public_key=excluded.encryption_public_key,signing_public_key=excluded.signing_public_key,fingerprint=excluded.fingerprint,is_local=excluded.is_local,status=excluded.status,updated_at=excluded.updated_at,revoked_at=excluded.revoked_at")
            .bind(&device.id)
            .bind(&snapshot.id)
            .bind(&device.member_id)
            .bind(&device.name)
            .bind(&device.encryption_public_key)
            .bind(&device.signing_public_key)
            .bind(&device.fingerprint)
            .bind(is_local)
            .bind(&device.status)
            .bind(now)
            .bind(now)
            .bind((device.status == "revoked").then_some(now))
            .execute(&mut **transaction)
            .await?;
    }
    Ok(())
}

async fn pending_acceptance(
    db: &Database,
    profile_id: &str,
    token: &str,
    device_name: &str,
) -> AppResult<PendingAcceptance> {
    validate_secret_token(token, "在线团队邀请令牌")?;
    cleanup_expired_pending(db).await?;
    let token_hash = invitation_token_hash(token);
    if let Some(pending) = sqlx::query_as::<_, PendingAcceptance>("SELECT token_hash,profile_id,device_id,device_name,encryption_public_key,signing_public_key,fingerprint,created_at FROM team_relay_pending_acceptances WHERE token_hash=?")
        .bind(&token_hash)
        .fetch_optional(&db.pool)
        .await?
    {
        if pending.profile_id != profile_id {
            return Err(AppError::Validation(
                "该邀请已使用另一团队服务配置开始接受".into(),
            ));
        }
        return Ok(pending);
    }
    let device_name = clean_name(device_name, "设备名称")?;
    let device_id = uuid::Uuid::new_v4().to_string();
    let mut encryption_secret = Zeroizing::new([0_u8; 32]);
    let mut signing_secret = Zeroizing::new([0_u8; 32]);
    OsRng.fill_bytes(&mut *encryption_secret);
    OsRng.fill_bytes(&mut *signing_secret);
    let encryption_private = StaticSecret::from(*encryption_secret);
    let encryption_public = X25519PublicKey::from(&encryption_private).to_bytes();
    let signing_key = SigningKey::from_bytes(&signing_secret);
    let signing_public = signing_key.verifying_key().to_bytes();
    team_share::save_private_key(&device_id, "x25519", &encryption_secret)?;
    if let Err(error) = team_share::save_private_key(&device_id, "ed25519", &signing_secret) {
        team_share::delete_private_keys(&device_id);
        return Err(error);
    }
    let pending = PendingAcceptance {
        token_hash,
        profile_id: profile_id.into(),
        device_id: device_id.clone(),
        device_name,
        encryption_public_key: team_share::encode_key("x25519", &encryption_public),
        signing_public_key: team_share::encode_key("ed25519", &signing_public),
        fingerprint: team_share::device_fingerprint(&encryption_public, &signing_public),
        created_at: Utc::now().to_rfc3339(),
    };
    if let Err(error) = sqlx::query("INSERT INTO team_relay_pending_acceptances(token_hash,profile_id,device_id,device_name,encryption_public_key,signing_public_key,fingerprint,created_at) VALUES(?,?,?,?,?,?,?,?)")
        .bind(&pending.token_hash)
        .bind(&pending.profile_id)
        .bind(&pending.device_id)
        .bind(&pending.device_name)
        .bind(&pending.encryption_public_key)
        .bind(&pending.signing_public_key)
        .bind(&pending.fingerprint)
        .bind(&pending.created_at)
        .execute(&db.pool)
        .await
    {
        team_share::delete_private_keys(&device_id);
        return Err(error.into());
    }
    Ok(pending)
}

fn validate_secret_token(token: &str, field: &str) -> AppResult<()> {
    if token.len() != 43
        || URL_SAFE_NO_PAD
            .decode(token)
            .ok()
            .is_none_or(|bytes| bytes.len() != 32)
    {
        return Err(AppError::Validation(format!("{field}格式无效")));
    }
    Ok(())
}

fn invitation_token_hash(token: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"cnshell-team-relay-client-invitation-v1\0");
    digest.update(token.as_bytes());
    format!("sha256:{:x}", digest.finalize())
}

async fn cleanup_expired_pending(db: &Database) -> AppResult<()> {
    let cutoff = (Utc::now() - ChronoDuration::hours(25)).to_rfc3339();
    let expired = sqlx::query_as::<_, PendingAcceptance>("SELECT token_hash,profile_id,device_id,device_name,encryption_public_key,signing_public_key,fingerprint,created_at FROM team_relay_pending_acceptances WHERE created_at<?")
        .bind(&cutoff)
        .fetch_all(&db.pool)
        .await?;
    sqlx::query("DELETE FROM team_relay_pending_acceptances WHERE created_at<?")
        .bind(&cutoff)
        .execute(&db.pool)
        .await?;
    for pending in expired {
        team_share::delete_private_keys(&pending.device_id);
    }
    Ok(())
}

async fn cleanup_pending_acceptance(db: &Database, pending: &PendingAcceptance) -> AppResult<()> {
    sqlx::query("DELETE FROM team_relay_pending_acceptances WHERE token_hash=?")
        .bind(&pending.token_hash)
        .execute(&db.pool)
        .await?;
    team_share::delete_private_keys(&pending.device_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cnshell_team_relay::{
        AccountRegistrationMode, RelayResult, RelayStore, VerificationEmail,
        VerificationEmailSender, router, router_with_registration_mode,
    };
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::task::JoinHandle;

    struct TestRelay {
        base_url: String,
        handle: JoinHandle<()>,
        _directory: tempfile::TempDir,
    }

    #[derive(Clone, Default)]
    struct CapturingEmailSender {
        messages: Arc<Mutex<Vec<VerificationEmail>>>,
    }

    #[async_trait::async_trait]
    impl VerificationEmailSender for CapturingEmailSender {
        async fn send_verification(&self, message: VerificationEmail) -> RelayResult<()> {
            self.messages.lock().unwrap().push(message);
            Ok(())
        }
    }

    impl Drop for TestRelay {
        fn drop(&mut self) {
            self.handle.abort();
        }
    }

    async fn start_relay() -> TestRelay {
        let directory = tempdir().unwrap();
        let database_url = format!(
            "sqlite://{}?mode=rwc",
            directory.path().join("relay.sqlite").display()
        );
        let store = RelayStore::open(&database_url).await.unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, router(store)).await.unwrap();
        });
        TestRelay {
            base_url: format!("http://{address}"),
            handle,
            _directory: directory,
        }
    }

    async fn start_verified_relay() -> (TestRelay, CapturingEmailSender) {
        let directory = tempdir().unwrap();
        let database_url = format!(
            "sqlite://{}?mode=rwc",
            directory.path().join("relay.sqlite").display()
        );
        let store = RelayStore::open(&database_url).await.unwrap();
        let sender = CapturingEmailSender::default();
        let (app, _) = router_with_registration_mode(
            store,
            AccountRegistrationMode::RequireEmail(Arc::new(sender.clone())),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (
            TestRelay {
                base_url: format!("http://{address}"),
                handle,
                _directory: directory,
            },
            sender,
        )
    }

    #[test]
    fn endpoint_validation_requires_tls_except_loopback() {
        assert!(validate_base_url("https://relay.example.com").is_ok());
        assert!(validate_base_url("http://127.0.0.1:8787").is_ok());
        assert!(validate_base_url("http://relay.example.com").is_err());
        assert!(validate_base_url("https://relay.example.com/path").is_err());
        assert!(validate_base_url("https://user@relay.example.com").is_err());
    }

    #[tokio::test]
    async fn client_registration_waits_for_email_verification_before_saving_session() {
        let (relay, sender) = start_verified_relay().await;
        let directory = tempdir().unwrap();
        let db = Database::open(&directory.path().join("cnshell.sqlite"))
            .await
            .unwrap();
        let profile_id = uuid::Uuid::new_v4().to_string();
        save_profile(
            &db,
            SaveTeamRelayProfileInput {
                id: profile_id.clone(),
                name: "Verified relay".into(),
                base_url: relay.base_url.clone(),
            },
        )
        .await
        .unwrap();
        let registration = register_account(
            &db,
            TeamRelayAccountInput {
                profile_id: profile_id.clone(),
                email: "verified-client@example.com".into(),
                password: "correct horse battery staple".into(),
                display_name: Some("Verified Client".into()),
            },
        )
        .await
        .unwrap();
        assert!(registration.verification_required);
        assert!(!registration.profile.has_account_session);
        let token = sender.messages.lock().unwrap()[0].token.clone();

        let profile = verify_account_email(
            &db,
            VerifyTeamRelayAccountInput {
                profile_id: profile_id.clone(),
                token,
            },
        )
        .await
        .unwrap();
        assert!(profile.has_account_session);
        assert_eq!(
            profile.account_email.as_deref(),
            Some("verified-client@example.com")
        );
        logout_account(&db, &profile_id).await.unwrap();
    }

    #[tokio::test]
    async fn chunked_relay_response_is_bounded_while_streaming() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                let count = socket.read(&mut buffer).await.unwrap();
                request.extend_from_slice(&buffer[..count]);
            }
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\nhello\r\n5\r\nworld\r\n0\r\n\r\n")
                .await
                .unwrap();
        });
        let response = Client::new()
            .get(format!("http://{address}/oversized"))
            .send()
            .await
            .unwrap();
        let error = bounded_response_with_limit(response, 8).await.unwrap_err();
        assert!(error.message.contains("1 MiB"));
    }

    #[tokio::test]
    async fn profiles_never_store_tokens_in_sqlite() {
        let directory = tempdir().unwrap();
        let db = Database::open(&directory.path().join("cnshell.sqlite"))
            .await
            .unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let saved = save_profile(
            &db,
            SaveTeamRelayProfileInput {
                id: id.clone(),
                name: "Local relay".into(),
                base_url: "http://127.0.0.1:8787".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(saved.base_url, "http://127.0.0.1:8787/");
        let schema: String = sqlx::query_scalar(
            "SELECT group_concat(sql, ' ') FROM sqlite_master WHERE name LIKE 'team_relay_%'",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert!(!schema.contains("token TEXT"));
        delete_profile(&db, &id).await.unwrap();
    }

    #[tokio::test]
    async fn relay_accounts_invitation_sync_and_device_refresh_round_trip() {
        let relay = start_relay().await;
        let owner_directory = tempdir().unwrap();
        let guest_directory = tempdir().unwrap();
        let owner_path = owner_directory.path().join("cnshell.sqlite");
        let guest_path = guest_directory.path().join("cnshell.sqlite");
        let owner_db = Database::open(&owner_path).await.unwrap();
        let guest_db = Database::open(&guest_path).await.unwrap();
        let owner_profile_id = uuid::Uuid::new_v4().to_string();
        let guest_profile_id = uuid::Uuid::new_v4().to_string();
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let owner_email = format!("owner-{suffix}@example.com");
        let guest_email = format!("guest-{suffix}@example.com");
        let password = "correct horse battery staple";

        save_profile(
            &owner_db,
            SaveTeamRelayProfileInput {
                id: owner_profile_id.clone(),
                name: "Loopback".into(),
                base_url: relay.base_url.clone(),
            },
        )
        .await
        .unwrap();
        register_account(
            &owner_db,
            TeamRelayAccountInput {
                profile_id: owner_profile_id.clone(),
                email: owner_email,
                password: password.into(),
                display_name: Some("Alice".into()),
            },
        )
        .await
        .unwrap();
        let workspace = team::create_workspace(
            &owner_db,
            crate::models::CreateTeamWorkspaceInput {
                name: "Online Ops".into(),
                owner_name: "Alice".into(),
            },
        )
        .await
        .unwrap();
        let owner_device = team_share::ensure_local_device(&owner_db, &workspace.id, "Alice Mac")
            .await
            .unwrap();
        let published = publish_workspace(&owner_db, &workspace.id, &owner_profile_id)
            .await
            .unwrap();
        assert_eq!(published.workspace_id, workspace.id);
        let invitation = create_invitation(
            &owner_db,
            CreateTeamRelayInvitationInput {
                workspace_id: workspace.id.clone(),
                email: guest_email.clone(),
                role: "viewer".into(),
            },
        )
        .await
        .unwrap();

        save_profile(
            &guest_db,
            SaveTeamRelayProfileInput {
                id: guest_profile_id.clone(),
                name: "Loopback".into(),
                base_url: relay.base_url.clone(),
            },
        )
        .await
        .unwrap();
        register_account(
            &guest_db,
            TeamRelayAccountInput {
                profile_id: guest_profile_id.clone(),
                email: guest_email,
                password: password.into(),
                display_name: Some("Bob".into()),
            },
        )
        .await
        .unwrap();
        let accepted = accept_invitation(
            &guest_db,
            AcceptTeamRelayInvitationInput {
                profile_id: guest_profile_id.clone(),
                token: invitation.token,
                device_name: "Bob Mac".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(accepted.id, workspace.id);
        assert_eq!(accepted.local_role, "viewer");
        let guest_device = local_device(&guest_db, &workspace.id).await.unwrap();

        let synced_owner = sync_workspace(&owner_db, &workspace.id).await.unwrap();
        assert_eq!(synced_owner.key_epoch, 2);
        let guest_member = team::list_members(&owner_db, &workspace.id)
            .await
            .unwrap()
            .into_iter()
            .find(|member| member.display_name == "Bob")
            .unwrap();
        update_member(
            &owner_db,
            UpdateTeamRelayMemberInput {
                workspace_id: workspace.id.clone(),
                member_id: guest_member.id,
                role: "operator".into(),
                status: "active".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            sync_workspace(&guest_db, &workspace.id)
                .await
                .unwrap()
                .local_role,
            "operator"
        );

        delete_session(&device_session_account(
            &owner_profile_id,
            &workspace.id,
            &owner_device.id,
        ))
        .unwrap();
        assert_eq!(
            sync_workspace(&owner_db, &workspace.id)
                .await
                .unwrap()
                .key_epoch,
            3
        );

        let account_secret = load_session(&session_account(&owner_profile_id))
            .unwrap()
            .unwrap();
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&owner_db.pool)
            .await
            .unwrap();
        let bytes = std::fs::read(&owner_path).unwrap();
        assert!(
            !bytes
                .windows(password.len())
                .any(|value| value == password.as_bytes())
        );
        assert!(
            !bytes
                .windows(account_secret.token.len())
                .any(|value| value == account_secret.token.as_bytes())
        );

        logout_account(&owner_db, &owner_profile_id).await.unwrap();
        logout_account(&guest_db, &guest_profile_id).await.unwrap();
        team_share::delete_private_keys(&owner_device.id);
        team_share::delete_private_keys(&guest_device.id);
    }
}
