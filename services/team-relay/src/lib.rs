mod email;
mod error;
mod metrics;
pub mod models;
mod store;
mod terminal;

pub use email::{
    AccountRegistrationMode, SmtpVerificationEmailSender, VerificationEmail,
    VerificationEmailSender,
};
pub use error::{RelayError, RelayResult};
use metrics::RelayMetrics;
pub use store::RelayStore;
use terminal::TerminalRelay;

use axum::{
    Json, Router,
    extract::{Path, Query, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    middleware,
    response::IntoResponse,
    routing::{delete, get, patch, post},
};
use models::*;
use serde::Deserialize;
use serde_json::json;
use tower_http::{limit::RequestBodyLimitLayer, trace::TraceLayer};

const MAX_HTTP_BODY_BYTES: usize = 256 * 1024;

#[derive(Clone)]
struct RelayState {
    store: RelayStore,
    terminal: TerminalRelay,
    metrics: RelayMetrics,
    registration: AccountRegistrationMode,
}

pub fn router(store: RelayStore) -> Router {
    router_with_shutdown(store).0
}

#[derive(Clone)]
pub struct RelayShutdownHandle {
    terminal: TerminalRelay,
}

impl RelayShutdownHandle {
    pub fn shutdown(&self) {
        self.terminal.shutdown();
    }
}

pub fn router_with_shutdown(store: RelayStore) -> (Router, RelayShutdownHandle) {
    router_with_registration_mode(store, AccountRegistrationMode::TrustedLocal)
}

pub fn router_with_registration_mode(
    store: RelayStore,
    registration: AccountRegistrationMode,
) -> (Router, RelayShutdownHandle) {
    let metrics = RelayMetrics::default();
    let state = RelayState {
        terminal: TerminalRelay::new(store.clone(), metrics.clone()),
        store,
        metrics: metrics.clone(),
        registration,
    };
    let shutdown = RelayShutdownHandle {
        terminal: state.terminal.clone(),
    };
    let router = Router::new()
        .route("/health", get(health))
        .route("/ready", get(readiness))
        .route("/metrics", get(prometheus_metrics))
        .route("/v1/accounts/register", post(register_account))
        .route("/v1/accounts/login", post(login))
        .route("/v1/accounts/verify-email", post(verify_email))
        .route(
            "/v1/accounts/resend-verification-email",
            post(resend_verification_email),
        )
        .route("/v1/workspaces/bootstrap", post(bootstrap_workspace))
        .route(
            "/v1/workspaces/{workspace_id}/invitations",
            post(create_workspace_invitation),
        )
        .route("/v1/invitations/accept", post(accept_workspace_invitation))
        .route("/v1/workspaces/{workspace_id}", get(workspace_snapshot))
        .route("/v1/workspaces/{workspace_id}/audit", get(workspace_audit))
        .route(
            "/v1/workspaces/{workspace_id}/permanent-deletion",
            delete(delete_workspace),
        )
        .route(
            "/v1/workspaces/{workspace_id}/members/{member_id}",
            patch(update_member),
        )
        .route(
            "/v1/workspaces/{workspace_id}/devices/{device_id}",
            delete(revoke_device),
        )
        .route("/v1/device-challenges", post(create_device_challenge))
        .route("/v1/device-sessions", post(create_device_session))
        .route("/v1/terminal/rooms", post(create_room))
        .route("/v1/terminal/invitations", get(list_room_invitations))
        .route(
            "/v1/terminal/rooms/{room_id}/invitation",
            post(route_room_invitation),
        )
        .route("/v1/terminal/rooms/{room_id}/join", post(join_room))
        .route(
            "/v1/terminal/rooms/{room_id}/participants/me",
            delete(leave_room),
        )
        .route("/v1/terminal/rooms/{room_id}", delete(close_room))
        .route(
            "/v1/terminal/rooms/{room_id}/control",
            post(grant_control).delete(revoke_control),
        )
        .route("/v1/terminal/ws/{room_id}", get(terminal_socket))
        .layer(RequestBodyLimitLayer::new(MAX_HTTP_BODY_BYTES))
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn_with_state(metrics, metrics::track_http))
        .with_state(state);
    (router, shutdown)
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "status": "ok" })))
}

async fn readiness(State(state): State<RelayState>) -> impl IntoResponse {
    let ready = state.store.readiness().await.is_ok();
    state.metrics.record_readiness(ready);
    if ready {
        return (StatusCode::OK, Json(json!({ "status": "ready" })));
    }
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "status": "unavailable" })),
    )
}

async fn prometheus_metrics(State(state): State<RelayState>) -> impl IntoResponse {
    let ready = state.store.readiness().await.is_ok();
    state.metrics.record_readiness(ready);
    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        state.metrics.render(ready),
    )
}

async fn register_account(
    State(state): State<RelayState>,
    Json(input): Json<RegisterAccountInput>,
) -> RelayResult<(StatusCode, Json<AccountRegistrationOutput>)> {
    let sender = match &state.registration {
        AccountRegistrationMode::TrustedLocal => None,
        AccountRegistrationMode::RequireEmail(sender) => Some(sender.clone()),
    };
    let result = state
        .store
        .register_account(input, sender.is_some())
        .await?;
    if let (Some(sender), Some(message)) = (sender, result.verification_email) {
        sender.send_verification(message).await?;
    }
    Ok((StatusCode::CREATED, Json(result.output)))
}

async fn login(
    State(state): State<RelayState>,
    Json(input): Json<LoginInput>,
) -> RelayResult<Json<AccountSessionOutput>> {
    Ok(Json(state.store.login(input).await?))
}

async fn verify_email(
    State(state): State<RelayState>,
    Json(input): Json<VerifyEmailInput>,
) -> RelayResult<Json<AccountSessionOutput>> {
    Ok(Json(state.store.verify_email(&input.token).await?))
}

async fn resend_verification_email(
    State(state): State<RelayState>,
    Json(input): Json<ResendVerificationEmailInput>,
) -> RelayResult<(StatusCode, Json<VerificationEmailAcceptedOutput>)> {
    if let AccountRegistrationMode::RequireEmail(sender) = &state.registration
        && let Some(message) = state.store.resend_verification_email(&input.email).await?
        && let Err(error) = sender.send_verification(message).await
    {
        tracing::warn!(error = %error, "verification email resend failed");
    }
    Ok((
        StatusCode::ACCEPTED,
        Json(VerificationEmailAcceptedOutput { accepted: true }),
    ))
}

async fn bootstrap_workspace(
    State(state): State<RelayState>,
    headers: HeaderMap,
    Json(input): Json<BootstrapWorkspaceInput>,
) -> RelayResult<(StatusCode, Json<DeviceSessionOutput>)> {
    let account = state
        .store
        .authenticate_account(bearer_token(&headers)?)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(state.store.bootstrap_workspace(&account, input).await?),
    ))
}

async fn create_workspace_invitation(
    State(state): State<RelayState>,
    Path(workspace_id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<CreateWorkspaceInvitationInput>,
) -> RelayResult<(StatusCode, Json<WorkspaceInvitationOutput>)> {
    let auth = state
        .store
        .authenticate_device(bearer_token(&headers)?)
        .await?;
    if auth.workspace_id != workspace_id {
        return Err(RelayError::PermissionDenied("设备不属于该工作区".into()));
    }
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .store
                .create_workspace_invitation(&auth, input)
                .await?,
        ),
    ))
}

async fn accept_workspace_invitation(
    State(state): State<RelayState>,
    headers: HeaderMap,
    Json(input): Json<AcceptWorkspaceInvitationInput>,
) -> RelayResult<(StatusCode, Json<DeviceSessionOutput>)> {
    let account = state
        .store
        .authenticate_account(bearer_token(&headers)?)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .store
                .accept_workspace_invitation(&account, input)
                .await?,
        ),
    ))
}

async fn workspace_snapshot(
    State(state): State<RelayState>,
    Path(workspace_id): Path<String>,
    headers: HeaderMap,
) -> RelayResult<Json<WorkspaceSnapshot>> {
    let auth = state
        .store
        .authenticate_device(bearer_token(&headers)?)
        .await?;
    Ok(Json(
        state.store.workspace_snapshot(&auth, &workspace_id).await?,
    ))
}

async fn workspace_audit(
    State(state): State<RelayState>,
    Path(workspace_id): Path<String>,
    headers: HeaderMap,
) -> RelayResult<Json<Vec<RelayAuditEvent>>> {
    let auth = state
        .store
        .authenticate_device(bearer_token(&headers)?)
        .await?;
    Ok(Json(state.store.list_audit(&auth, &workspace_id).await?))
}

async fn delete_workspace(
    State(state): State<RelayState>,
    Path(workspace_id): Path<String>,
    headers: HeaderMap,
) -> RelayResult<StatusCode> {
    let confirmation = headers
        .get("x-cnshell-confirm-workspace")
        .and_then(|value| value.to_str().ok());
    if confirmation != Some(workspace_id.as_str()) {
        return Err(RelayError::Validation(
            "永久删除需要精确确认工作区 ID".into(),
        ));
    }
    let auth = state
        .store
        .authenticate_device(bearer_token(&headers)?)
        .await?;
    state.store.delete_workspace(&auth, &workspace_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_member(
    State(state): State<RelayState>,
    Path((workspace_id, member_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<UpdateMemberInput>,
) -> RelayResult<Json<serde_json::Value>> {
    let auth = state
        .store
        .authenticate_device(bearer_token(&headers)?)
        .await?;
    let key_epoch = state
        .store
        .update_member(&auth, &workspace_id, &member_id, input)
        .await?;
    Ok(Json(json!({ "keyEpoch": key_epoch })))
}

async fn revoke_device(
    State(state): State<RelayState>,
    Path((workspace_id, device_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> RelayResult<Json<serde_json::Value>> {
    let auth = state
        .store
        .authenticate_device(bearer_token(&headers)?)
        .await?;
    let key_epoch = state
        .store
        .revoke_device(&auth, &workspace_id, &device_id)
        .await?;
    Ok(Json(json!({ "keyEpoch": key_epoch })))
}

async fn create_device_challenge(
    State(state): State<RelayState>,
    Json(input): Json<DeviceChallengeInput>,
) -> RelayResult<(StatusCode, Json<DeviceChallengeOutput>)> {
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .store
                .create_device_challenge(&input.workspace_id, &input.device_id)
                .await?,
        ),
    ))
}

async fn create_device_session(
    State(state): State<RelayState>,
    Json(input): Json<CreateDeviceSessionInput>,
) -> RelayResult<(StatusCode, Json<DeviceSessionOutput>)> {
    Ok((
        StatusCode::CREATED,
        Json(state.store.create_device_session(input).await?),
    ))
}

async fn create_room(
    State(state): State<RelayState>,
    headers: HeaderMap,
    Json(input): Json<CreateRoomInput>,
) -> RelayResult<(StatusCode, Json<RoomView>)> {
    let auth = state
        .store
        .authenticate_device(bearer_token(&headers)?)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(state.terminal.create_room(&auth, input).await?),
    ))
}

async fn route_room_invitation(
    State(state): State<RelayState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<RouteRoomInvitationInput>,
) -> RelayResult<StatusCode> {
    let auth = state
        .store
        .authenticate_device(bearer_token(&headers)?)
        .await?;
    state
        .terminal
        .route_invitation(&auth, &room_id, input)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_room_invitations(
    State(state): State<RelayState>,
    headers: HeaderMap,
) -> RelayResult<Json<Vec<RoutedRoomInvitation>>> {
    let auth = state
        .store
        .authenticate_device(bearer_token(&headers)?)
        .await?;
    Ok(Json(state.terminal.list_invitations(&auth).await?))
}

async fn join_room(
    State(state): State<RelayState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
) -> RelayResult<Json<RoomView>> {
    let auth = state
        .store
        .authenticate_device(bearer_token(&headers)?)
        .await?;
    Ok(Json(state.terminal.join_room(&auth, &room_id).await?))
}

async fn leave_room(
    State(state): State<RelayState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
) -> RelayResult<StatusCode> {
    let auth = state
        .store
        .authenticate_device(bearer_token(&headers)?)
        .await?;
    state.terminal.leave_room(&auth, &room_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn grant_control(
    State(state): State<RelayState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<GrantControlInput>,
) -> RelayResult<Json<ControlLeaseOutput>> {
    let auth = state
        .store
        .authenticate_device(bearer_token(&headers)?)
        .await?;
    Ok(Json(
        state.terminal.grant_control(&auth, &room_id, input).await?,
    ))
}

async fn revoke_control(
    State(state): State<RelayState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
) -> RelayResult<StatusCode> {
    let auth = state
        .store
        .authenticate_device(bearer_token(&headers)?)
        .await?;
    state.terminal.revoke_control(&auth, &room_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn close_room(
    State(state): State<RelayState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
) -> RelayResult<StatusCode> {
    let auth = state
        .store
        .authenticate_device(bearer_token(&headers)?)
        .await?;
    state.terminal.close_room(&auth, &room_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SocketQuery {
    #[serde(default)]
    after_sequence: u64,
}

async fn terminal_socket(
    State(state): State<RelayState>,
    Path(room_id): Path<String>,
    Query(query): Query<SocketQuery>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> RelayResult<impl IntoResponse> {
    let token = bearer_token(&headers)?.to_string();
    let auth = state.store.authenticate_device(&token).await?;
    state.terminal.authorize_room(&auth, &room_id).await?;
    Ok(upgrade.on_upgrade(move |socket| async move {
        state
            .terminal
            .handle_socket(socket, token, room_id, query.after_sequence)
            .await;
    }))
}

fn bearer_token(headers: &HeaderMap) -> RelayResult<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| RelayError::Authentication("缺少 Bearer 会话令牌".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn health_and_readiness_report_process_and_database_state() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("relay.sqlite");
        let database_url = format!("sqlite://{}?mode=rwc", database.display());
        let store = RelayStore::open(&database_url).await.unwrap();
        let metrics = RelayMetrics::default();
        let state = RelayState {
            terminal: TerminalRelay::new(store.clone(), metrics.clone()),
            store: store.clone(),
            metrics,
            registration: AccountRegistrationMode::TrustedLocal,
        };

        assert_eq!(health().await.into_response().status(), StatusCode::OK);
        assert_eq!(
            readiness(State(state.clone()))
                .await
                .into_response()
                .status(),
            StatusCode::OK
        );

        store.close_for_test().await;
        assert_eq!(
            readiness(State(state)).await.into_response().status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
}
