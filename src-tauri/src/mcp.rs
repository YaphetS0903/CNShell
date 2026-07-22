use crate::{
    db::Database,
    error::{AppError, AppResult},
    mcp_protocol::{
        BROKER_PROTOCOL_VERSION, BrokerRequest, BrokerResponse, DiscoveryDocument,
        MAX_BROKER_MESSAGE_BYTES,
    },
    models::{
        McpApproval, McpApprovalRule, McpAuditEvent, McpClient, McpClientConfig,
        McpClientGrantInput, McpLocalGrant, McpSettings, McpStatus,
    },
    ssh::SessionManager,
};
use chrono::{Duration as ChronoDuration, Utc};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::OpenOptions,
    io::Write,
    path::{Component, Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const KEYCHAIN_SERVICE: &str = "cn.cnshell.mcp";
const BUNDLED_SIDECAR_SHA256: &str = env!("CNSHELL_MCP_SIDECAR_SHA256");
const APPROVAL_TTL: Duration = Duration::from_secs(120);
const SESSION_IDLE_TTL: Duration = Duration::from_secs(15 * 60);
const SESSION_MAX_TTL: Duration = Duration::from_secs(8 * 60 * 60);
const MAX_PENDING_PER_CLIENT: usize = 10;
const MAX_PENDING_GLOBAL: usize = 50;
const MAX_AUDIT_EVENTS: i64 = 4_096;
const MAX_MCP_DIRECTORY_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_MCP_RESOURCE_CONNECTIONS: usize = 256;
const MAX_MCP_RESOURCE_AUDIT_EVENTS: i64 = 100;
const MAX_MCP_APPROVAL_RULES_PER_CLIENT: i64 = 256;
const RESOURCE_CONNECTIONS_OPERATION: &str = "resource:connections";
const RESOURCE_AUDIT_OPERATION: &str = "resource:audit-recent";

pub const READ_TOOLS: &[&str] = &[
    "cnshell_list_connections",
    "cnshell_open_session",
    "cnshell_close_session",
    "cnshell_file_list",
    "cnshell_file_read",
    "cnshell_system_info",
];

pub const WRITE_TOOLS: &[&str] = &[
    "cnshell_run_command",
    "cnshell_file_write",
    "cnshell_file_mkdir",
    "cnshell_file_rename",
    "cnshell_file_delete",
    "cnshell_file_upload",
    "cnshell_file_download",
];

fn known_tool(tool: &str) -> bool {
    READ_TOOLS.contains(&tool) || WRITE_TOOLS.contains(&tool)
}

fn known_broker_operation(operation: &str) -> bool {
    known_tool(operation)
        || matches!(
            operation,
            RESOURCE_CONNECTIONS_OPERATION | RESOURCE_AUDIT_OPERATION
        )
}

#[derive(Clone)]
pub struct McpManager {
    inner: Arc<Mutex<McpRuntime>>,
    data_dir: PathBuf,
}

impl Drop for McpManager {
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) == 1 {
            let _ = remove_discovery(&self.discovery_path());
        }
    }
}

#[derive(Default)]
struct McpRuntime {
    running: bool,
    address: Option<String>,
    generation: Option<String>,
    cancellation: Option<CancellationToken>,
    sessions: HashMap<String, McpSession>,
    approvals: HashMap<String, PendingApproval>,
    rate_limits: HashMap<String, RateLimitState>,
    in_flight: HashMap<String, usize>,
    active_requests: HashMap<String, ActiveRequest>,
    transfer_targets: HashSet<String>,
    session_rules: HashSet<ApprovalRuleKey>,
    confirmed_connections: HashSet<(String, String)>,
    auth_failures: HashMap<String, AuthFailureState>,
}

struct McpSession {
    client_id: String,
    connection_id: String,
    created_at: Instant,
    last_used_at: Instant,
}

struct PendingApproval {
    view: McpApproval,
    rule_key: Option<ApprovalRuleKey>,
    decision: Option<oneshot::Sender<ApprovalDecision>>,
}

struct ActiveRequest {
    client_id: String,
    cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
struct RateLimitState {
    reads: VecDeque<Instant>,
    writes: VecDeque<Instant>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct ApprovalRuleKey {
    client_id: String,
    connection_id: String,
    tool: String,
    target_key: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ApprovalDecision {
    Reject,
    Once,
    Session,
    Persistent,
}

impl ApprovalDecision {
    fn approved(self) -> bool {
        self != Self::Reject
    }

    fn audit_outcome(self) -> &'static str {
        match self {
            Self::Reject => "rejected",
            Self::Once => "approved-once",
            Self::Session => "approved-session",
            Self::Persistent => "approved-rule",
        }
    }
}

#[derive(Default)]
struct ApprovalPolicy {
    target_key: Option<String>,
    can_allow_session: bool,
    persistent_command: Option<String>,
}

#[derive(Clone, Copy)]
struct AuthFailureState {
    failures: u32,
    retry_after: Instant,
}

#[derive(Clone)]
struct BrokerContext {
    app: AppHandle,
    db: Database,
    sessions: SessionManager,
    manager: McpManager,
    broker_token: String,
    generation: String,
}

struct ExecutionPermit {
    manager: McpManager,
    client_id: String,
}

struct TransferTargetPermit {
    manager: McpManager,
    key: String,
}

struct RequestPermit {
    manager: McpManager,
    request_id: String,
}

impl Drop for RequestPermit {
    fn drop(&mut self) {
        self.manager
            .inner
            .lock()
            .active_requests
            .remove(&self.request_id);
    }
}

impl Drop for ExecutionPermit {
    fn drop(&mut self) {
        let mut runtime = self.manager.inner.lock();
        if let Some(count) = runtime.in_flight.get_mut(&self.client_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                runtime.in_flight.remove(&self.client_id);
            }
        }
    }
}

impl Drop for TransferTargetPermit {
    fn drop(&mut self) {
        self.manager.inner.lock().transfer_targets.remove(&self.key);
    }
}

impl McpManager {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            inner: Arc::new(Mutex::new(McpRuntime::default())),
            data_dir,
        }
    }

    pub fn discovery_path(&self) -> PathBuf {
        self.data_dir.join("mcp-broker.json")
    }

    pub async fn settings(db: &Database) -> AppResult<McpSettings> {
        let value = sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key='mcp'")
            .fetch_optional(&db.pool)
            .await?;
        value
            .map(|value| {
                serde_json::from_str(&value).map_err(|error| AppError::Storage(error.to_string()))
            })
            .transpose()
            .map(|value| value.unwrap_or_default())
    }

    pub async fn save_settings(db: &Database, settings: &McpSettings) -> AppResult<()> {
        let value = serde_json::to_string(settings)
            .map_err(|error| AppError::Storage(error.to_string()))?;
        sqlx::query("INSERT INTO settings(key,value) VALUES('mcp',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value")
            .bind(value)
            .execute(&db.pool)
            .await?;
        Ok(())
    }

    pub async fn start(
        &self,
        app: AppHandle,
        db: Database,
        sessions: SessionManager,
    ) -> AppResult<McpStatus> {
        if self.inner.lock().running {
            return self.status(&db).await;
        }
        std::fs::create_dir_all(&self.data_dir)?;
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|error| {
                AppError::Unavailable(format!("MCP Broker 无法监听本机端口：{error}"))
            })?;
        let address = listener.local_addr()?.to_string();
        let generation = Uuid::new_v4().to_string();
        let broker_token = random_secret();
        write_discovery(
            &self.discovery_path(),
            &DiscoveryDocument {
                schema_version: BROKER_PROTOCOL_VERSION,
                address: address.clone(),
                generation: generation.clone(),
                broker_token: broker_token.clone(),
                process_id: std::process::id(),
                created_at: Utc::now().to_rfc3339(),
            },
        )?;
        let cancellation = CancellationToken::new();
        {
            let mut runtime = self.inner.lock();
            runtime.running = true;
            runtime.address = Some(address.clone());
            runtime.generation = Some(generation.clone());
            runtime.cancellation = Some(cancellation.clone());
        }
        let context = BrokerContext {
            app: app.clone(),
            db: db.clone(),
            sessions,
            manager: self.clone(),
            broker_token,
            generation,
        };
        tauri::async_runtime::spawn(async move {
            run_listener(listener, context, cancellation).await;
        });
        let cleanup_app = app.clone();
        let cleanup_db = db.clone();
        tauri::async_runtime::spawn(async move {
            retry_revoked_client_cleanup(&cleanup_app, &cleanup_db).await;
        });
        append_audit(
            &db,
            AuditInput::event("broker", "info", "started").target("loopback"),
        )
        .await?;
        self.status(&db).await
    }

    pub async fn set_enabled(
        &self,
        app: AppHandle,
        db: Database,
        sessions: SessionManager,
        enabled: bool,
    ) -> AppResult<McpStatus> {
        let mut settings = Self::settings(&db).await?;
        settings.enabled = enabled;
        Self::save_settings(&db, &settings).await?;
        if enabled {
            self.start(app, db.clone(), sessions).await?;
        } else {
            self.stop(&sessions, &db).await?;
        }
        self.status(&db).await
    }

    pub async fn stop(&self, sessions: &SessionManager, db: &Database) -> AppResult<()> {
        let (was_running, cancellation, session_ids, approvals) = {
            let mut runtime = self.inner.lock();
            let was_running = runtime.running;
            let cancellation = runtime.cancellation.take();
            let session_ids = runtime.sessions.keys().cloned().collect::<Vec<_>>();
            let approvals = runtime.approvals.drain().collect::<Vec<_>>();
            runtime.sessions.clear();
            runtime.rate_limits.clear();
            for request in runtime.active_requests.values() {
                request
                    .cancelled
                    .store(true, std::sync::atomic::Ordering::Release);
            }
            runtime.active_requests.clear();
            runtime.in_flight.clear();
            runtime.transfer_targets.clear();
            runtime.session_rules.clear();
            runtime.confirmed_connections.clear();
            runtime.auth_failures.clear();
            runtime.running = false;
            runtime.address = None;
            runtime.generation = None;
            (was_running, cancellation, session_ids, approvals)
        };
        if let Some(cancellation) = cancellation {
            cancellation.cancel();
        }
        for session_id in session_ids {
            sessions.remove_external(&session_id);
        }
        for (_, mut pending) in approvals {
            if let Some(decision) = pending.decision.take() {
                let _ = decision.send(ApprovalDecision::Reject);
            }
        }
        remove_discovery(&self.discovery_path())?;
        if was_running {
            append_audit(
                db,
                AuditInput::event("broker", "info", "stopped").target("loopback"),
            )
            .await?;
        }
        Ok(())
    }

    pub async fn status(&self, db: &Database) -> AppResult<McpStatus> {
        let settings = Self::settings(db).await?;
        let client_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM mcp_clients WHERE status='active'")
                .fetch_one(&db.pool)
                .await?;
        let runtime = self.inner.lock();
        Ok(McpStatus {
            enabled: settings.enabled,
            running: runtime.running,
            address: runtime.address.clone(),
            generation: runtime.generation.clone(),
            client_count: client_count.max(0) as usize,
            session_count: runtime.sessions.len(),
            pending_approval_count: runtime.approvals.len(),
            message: if runtime.running {
                "MCP Broker 仅监听本机，外部请求受 CNshell 授权控制。".into()
            } else if settings.enabled {
                "MCP 已启用，但 Broker 尚未启动。".into()
            } else {
                "MCP 默认关闭；启用前不会监听端口。".into()
            },
        })
    }

    fn authorize_rate(&self, client_id: &str, write: bool) -> AppResult<()> {
        let now = Instant::now();
        let mut runtime = self.inner.lock();
        let state = runtime.rate_limits.entry(client_id.into()).or_default();
        let queue = if write {
            &mut state.writes
        } else {
            &mut state.reads
        };
        while queue
            .front()
            .is_some_and(|timestamp| now.duration_since(*timestamp) >= Duration::from_secs(60))
        {
            queue.pop_front();
        }
        let limit = if write { 10 } else { 60 };
        if queue.len() >= limit {
            return Err(AppError::Unavailable(
                "MCP 客户端请求过于频繁，请稍后再试".into(),
            ));
        }
        queue.push_back(now);
        Ok(())
    }

    fn check_auth_backoff(&self, client_id: &str) -> AppResult<()> {
        if self
            .inner
            .lock()
            .auth_failures
            .get(client_id)
            .is_some_and(|state| Instant::now() < state.retry_after)
        {
            return Err(AppError::Unavailable(
                "MCP 客户端认证失败次数过多，请稍后重试".into(),
            ));
        }
        Ok(())
    }

    fn record_auth_failure(&self, client_id: &str) {
        let mut runtime = self.inner.lock();
        let failures = runtime
            .auth_failures
            .get(client_id)
            .map_or(1, |state| state.failures.saturating_add(1));
        let delay_ms = 100_u64.saturating_mul(1_u64 << failures.min(6));
        runtime.auth_failures.insert(
            client_id.into(),
            AuthFailureState {
                failures,
                retry_after: Instant::now() + Duration::from_millis(delay_ms.min(5_000)),
            },
        );
    }

    fn clear_auth_failure(&self, client_id: &str) {
        self.inner.lock().auth_failures.remove(client_id);
    }

    fn acquire_execution(&self, client_id: &str) -> AppResult<ExecutionPermit> {
        let mut runtime = self.inner.lock();
        let count = runtime.in_flight.entry(client_id.into()).or_default();
        if *count >= 2 {
            return Err(AppError::Unavailable(
                "该 MCP 客户端已有 2 个请求正在执行，请稍后重试".into(),
            ));
        }
        *count += 1;
        Ok(ExecutionPermit {
            manager: self.clone(),
            client_id: client_id.into(),
        })
    }

    fn acquire_transfer_target(&self, target: &Path) -> AppResult<TransferTargetPermit> {
        let key = format!(
            "sha256:{:x}",
            Sha256::digest(target.to_string_lossy().as_bytes())
        );
        let mut runtime = self.inner.lock();
        if !runtime.transfer_targets.insert(key.clone()) {
            return Err(AppError::Unavailable(
                "同一本地 MCP 目标已有传输正在执行".into(),
            ));
        }
        Ok(TransferTargetPermit {
            manager: self.clone(),
            key,
        })
    }

    fn open_session(
        &self,
        sessions: &SessionManager,
        client_id: &str,
        profile: crate::models::ConnectionProfile,
    ) -> AppResult<String> {
        self.remove_expired_sessions(sessions);
        let mut runtime = self.inner.lock();
        let count = runtime
            .sessions
            .values()
            .filter(|session| session.client_id == client_id)
            .count();
        if count >= 4 {
            return Err(AppError::Unavailable(
                "每个 MCP 客户端最多打开 4 个会话".into(),
            ));
        }
        let id = format!("mcp-{}", Uuid::new_v4());
        sessions.insert_external(id.clone(), profile.clone());
        runtime.sessions.insert(
            id.clone(),
            McpSession {
                client_id: client_id.into(),
                connection_id: profile.id,
                created_at: Instant::now(),
                last_used_at: Instant::now(),
            },
        );
        Ok(id)
    }

    fn session(
        &self,
        sessions: &SessionManager,
        client_id: &str,
        session_id: &str,
    ) -> AppResult<String> {
        self.remove_expired_sessions(sessions);
        let mut runtime = self.inner.lock();
        let session = runtime
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AppError::NotFound(format!("MCP 会话 {session_id}")))?;
        if session.client_id != client_id {
            return Err(AppError::PermissionDenied(
                "MCP 会话不属于当前客户端".into(),
            ));
        }
        session.last_used_at = Instant::now();
        Ok(session.connection_id.clone())
    }

    fn close_session(
        &self,
        sessions: &SessionManager,
        client_id: &str,
        session_id: &str,
    ) -> AppResult<()> {
        let removed = {
            let mut runtime = self.inner.lock();
            if runtime
                .sessions
                .get(session_id)
                .is_some_and(|session| session.client_id != client_id)
            {
                return Err(AppError::PermissionDenied(
                    "MCP 会话不属于当前客户端".into(),
                ));
            }
            runtime.sessions.remove(session_id).is_some()
        };
        if !removed {
            return Err(AppError::NotFound(format!("MCP 会话 {session_id}")));
        }
        sessions.remove_external(session_id);
        Ok(())
    }

    fn remove_expired_sessions(&self, sessions: &SessionManager) {
        let expired = {
            let runtime = self.inner.lock();
            runtime
                .sessions
                .iter()
                .filter(|(_, session)| {
                    session.last_used_at.elapsed() >= SESSION_IDLE_TTL
                        || session.created_at.elapsed() >= SESSION_MAX_TTL
                })
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>()
        };
        if expired.is_empty() {
            return;
        }
        let mut runtime = self.inner.lock();
        for id in expired {
            runtime.sessions.remove(&id);
            sessions.remove_external(&id);
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Approval records deliberately carry the complete user-visible request context"
    )]
    async fn request_approval(
        &self,
        app: &AppHandle,
        db: &Database,
        client: &McpClient,
        connection_id: &str,
        connection_name: &str,
        request_id: &str,
        tool: &str,
        risk: &str,
        target: &str,
        preview: &str,
        policy: ApprovalPolicy,
    ) -> AppResult<ApprovalDecision> {
        let rule_key = policy
            .target_key
            .as_ref()
            .map(|target_key| ApprovalRuleKey {
                client_id: client.id.clone(),
                connection_id: connection_id.into(),
                tool: tool.into(),
                target_key: target_key.clone(),
            });
        if let Some(rule_key) = rule_key.as_ref() {
            if policy.can_allow_session && self.inner.lock().session_rules.contains(rule_key) {
                return Ok(ApprovalDecision::Session);
            }
            if policy.persistent_command.is_some() && persistent_rule_exists(db, rule_key).await? {
                return Ok(ApprovalDecision::Persistent);
            }
        }
        let (sender, receiver) = oneshot::channel();
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let view = McpApproval {
            id: id.clone(),
            request_id: request_id.into(),
            client_id: client.id.clone(),
            client_name: client.name.clone(),
            connection_id: connection_id.into(),
            connection_name: connection_name.into(),
            tool: tool.into(),
            risk: risk.into(),
            target: target.into(),
            preview: preview.into(),
            can_allow_session: policy.can_allow_session,
            can_save_rule: policy.persistent_command.is_some(),
            created_at: now.to_rfc3339(),
            expires_at: (now + ChronoDuration::seconds(APPROVAL_TTL.as_secs() as i64)).to_rfc3339(),
        };
        {
            let mut runtime = self.inner.lock();
            if runtime.approvals.len() >= MAX_PENDING_GLOBAL
                || runtime
                    .approvals
                    .values()
                    .filter(|pending| pending.view.client_id == client.id)
                    .count()
                    >= MAX_PENDING_PER_CLIENT
            {
                return Err(AppError::Unavailable("MCP 待审批请求已达到上限".into()));
            }
            runtime.approvals.insert(
                id.clone(),
                PendingApproval {
                    view,
                    rule_key: rule_key.clone(),
                    decision: Some(sender),
                },
            );
        }
        let _ = app.emit("mcp-approval-changed", ());
        let decision = tokio::time::timeout(APPROVAL_TTL, receiver).await;
        self.inner.lock().approvals.remove(&id);
        let _ = app.emit("mcp-approval-changed", ());
        let decision = match decision {
            Ok(Ok(decision)) => decision,
            _ => ApprovalDecision::Reject,
        };
        if let Some(rule_key) = rule_key {
            match decision {
                ApprovalDecision::Session if policy.can_allow_session => {
                    self.inner.lock().session_rules.insert(rule_key);
                }
                ApprovalDecision::Persistent
                    if let Some(command) = policy.persistent_command.as_deref() =>
                {
                    save_persistent_rule(db, &rule_key, command).await?;
                }
                _ => {}
            }
        }
        Ok(decision)
    }

    pub fn approvals(&self) -> Vec<McpApproval> {
        let mut approvals = self
            .inner
            .lock()
            .approvals
            .values()
            .map(|pending| pending.view.clone())
            .collect::<Vec<_>>();
        approvals.sort_by(|left, right| left.created_at.cmp(&right.created_at));
        approvals
    }

    pub fn decide(&self, id: &str, decision: &str) -> AppResult<()> {
        let mut runtime = self.inner.lock();
        let pending = runtime
            .approvals
            .get_mut(id)
            .ok_or_else(|| AppError::NotFound(format!("MCP 审批 {id}")))?;
        let selected = match decision {
            "reject" => ApprovalDecision::Reject,
            "once" => ApprovalDecision::Once,
            "session" if pending.view.can_allow_session && pending.rule_key.is_some() => {
                ApprovalDecision::Session
            }
            "persistent" if pending.view.can_save_rule && pending.rule_key.is_some() => {
                ApprovalDecision::Persistent
            }
            _ => return Err(AppError::Validation("MCP 审批决定无效".into())),
        };
        let sender = pending
            .decision
            .take()
            .ok_or_else(|| AppError::Unavailable("MCP 审批已经处理".into()))?;
        sender
            .send(selected)
            .map_err(|_| AppError::Unavailable("MCP 请求已经断开或过期".into()))
    }

    fn cancel_request(&self, request_id: &str) {
        let mut runtime = self.inner.lock();
        if let Some(request) = runtime.active_requests.get(request_id) {
            request
                .cancelled
                .store(true, std::sync::atomic::Ordering::Release);
        }
        let approval_ids = runtime
            .approvals
            .iter()
            .filter(|(_, pending)| pending.view.request_id == request_id)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in approval_ids {
            if let Some(mut pending) = runtime.approvals.remove(&id)
                && let Some(decision) = pending.decision.take()
            {
                let _ = decision.send(ApprovalDecision::Reject);
            }
        }
    }

    fn register_request(
        &self,
        request_id: &str,
        client_id: &str,
        cancelled: Arc<AtomicBool>,
    ) -> AppResult<RequestPermit> {
        let mut runtime = self.inner.lock();
        if runtime.active_requests.contains_key(request_id) {
            return Err(AppError::Validation("MCP requestId 不能重复".into()));
        }
        runtime.active_requests.insert(
            request_id.into(),
            ActiveRequest {
                client_id: client_id.into(),
                cancelled,
            },
        );
        Ok(RequestPermit {
            manager: self.clone(),
            request_id: request_id.into(),
        })
    }

    fn cancel_client_requests(&self, client_id: &str) {
        let runtime = self.inner.lock();
        for request in runtime.active_requests.values() {
            if request.client_id == client_id {
                request
                    .cancelled
                    .store(true, std::sync::atomic::Ordering::Release);
            }
        }
    }

    pub fn invalidate_client_authorizations(&self, sessions: &SessionManager, client_id: &str) {
        let mut runtime = self.inner.lock();
        let session_ids = runtime
            .sessions
            .iter()
            .filter(|(_, session)| session.client_id == client_id)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in session_ids {
            runtime.sessions.remove(&id);
            sessions.remove_external(&id);
        }
        let approval_ids = runtime
            .approvals
            .iter()
            .filter(|(_, pending)| pending.view.client_id == client_id)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in approval_ids {
            if let Some(mut pending) = runtime.approvals.remove(&id)
                && let Some(decision) = pending.decision.take()
            {
                let _ = decision.send(ApprovalDecision::Reject);
            }
        }
        for request in runtime.active_requests.values() {
            if request.client_id == client_id {
                request
                    .cancelled
                    .store(true, std::sync::atomic::Ordering::Release);
            }
        }
        runtime
            .session_rules
            .retain(|key| key.client_id != client_id);
        runtime
            .confirmed_connections
            .retain(|(id, _)| id != client_id);
        runtime.rate_limits.remove(client_id);
    }
}

async fn run_listener(
    listener: TcpListener,
    context: BrokerContext,
    cancellation: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = cancellation.cancelled() => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, address)) if address.ip().is_loopback() => {
                    let context = context.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = handle_stream(stream, context).await;
                    });
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    }
}

async fn handle_stream(mut stream: TcpStream, context: BrokerContext) -> AppResult<()> {
    let length = tokio::time::timeout(Duration::from_secs(10), stream.read_u32())
        .await
        .map_err(|_| AppError::Unavailable("MCP Broker 请求读取超时".into()))??
        as usize;
    if length == 0 || length > MAX_BROKER_MESSAGE_BYTES {
        return Err(AppError::Validation("MCP Broker 请求大小无效".into()));
    }
    let mut bytes = vec![0_u8; length];
    tokio::time::timeout(Duration::from_secs(10), stream.read_exact(&mut bytes))
        .await
        .map_err(|_| AppError::Unavailable("MCP Broker 请求正文读取超时".into()))??;
    let request = serde_json::from_slice::<BrokerRequest>(&bytes)
        .map_err(|_| AppError::Validation("MCP Broker 请求格式无效".into()))?;
    let request_id = request.request_id.clone();
    let cancelled = Arc::new(AtomicBool::new(false));
    let execution = execute_request(&context, request, cancelled.clone());
    tokio::pin!(execution);
    let response = tokio::select! {
        result = &mut execution => match result {
            Ok(result) => BrokerResponse::success(request_id.clone(), result),
            Err(error) => BrokerResponse::error(request_id.clone(), app_error_code(&error), safe_error(&error)),
        },
        disconnected = stream.read_u8() => {
            cancelled.store(true, std::sync::atomic::Ordering::Release);
            context.manager.cancel_request(&request_id);
            let _ = disconnected;
            let _ = execution.await;
            return Ok(());
        }
    };
    let encoded =
        serde_json::to_vec(&response).map_err(|error| AppError::Internal(error.to_string()))?;
    if encoded.len() > MAX_BROKER_MESSAGE_BYTES {
        return Err(AppError::Internal("MCP Broker 响应超过限制".into()));
    }
    stream.write_u32(encoded.len() as u32).await?;
    stream.write_all(&encoded).await?;
    stream.shutdown().await?;
    Ok(())
}

async fn execute_request(
    context: &BrokerContext,
    request: BrokerRequest,
    cancelled: Arc<AtomicBool>,
) -> AppResult<Value> {
    validate_broker_request(context, &request)?;
    context.manager.check_auth_backoff(&request.client_id)?;
    let client = match authenticate_client(&context.db, &request).await {
        Ok(client) => {
            context.manager.clear_auth_failure(&request.client_id);
            client
        }
        Err(error) => {
            context.manager.record_auth_failure(&request.client_id);
            let _ = append_audit(
                &context.db,
                AuditInput::event("authentication", "medium", "failed").client(&request.client_id),
            )
            .await;
            return Err(error);
        }
    };
    let _execution = context.manager.acquire_execution(&client.id)?;
    let _request =
        context
            .manager
            .register_request(&request.request_id, &client.id, cancelled.clone())?;
    context
        .manager
        .authorize_rate(&client.id, WRITE_TOOLS.contains(&request.tool.as_str()))?;
    let started = Instant::now();
    let tool = request.tool.clone();
    let request_id = request.request_id.clone();
    let result = execute_operation(context, &client, &request, cancelled).await;
    let outcome = if result.is_ok() {
        "completed"
    } else {
        "failed"
    };
    let audit = AuditInput::request(&request_id, &client.id, &tool, outcome)
        .duration(started.elapsed().as_millis() as i64);
    let audit = match result.as_ref() {
        Ok(value) => audit
            .transferred_bytes(
                value
                    .get("transferredBytes")
                    .and_then(Value::as_i64)
                    .unwrap_or_default(),
            )
            .truncated(
                value
                    .get("truncated")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            ),
        Err(_) => audit,
    };
    if append_audit(&context.db, audit).await.is_err() {
        tracing::error!("MCP 请求审计写入失败");
    }
    result
}

fn validate_broker_request(context: &BrokerContext, request: &BrokerRequest) -> AppResult<()> {
    if request.protocol_version != BROKER_PROTOCOL_VERSION
        || request.generation != context.generation
        || request.request_id.is_empty()
        || request.request_id.len() > 128
        || request.client_id.is_empty()
        || request.client_id.len() > 128
        || request.client_name.is_empty()
        || request.client_name.len() > 128
        || request.tool.len() > 128
        || !known_broker_operation(&request.tool)
    {
        return Err(AppError::Validation("MCP Broker 请求字段无效".into()));
    }
    if !constant_time_equal(&request.broker_token, &context.broker_token) {
        return Err(AppError::PermissionDenied("MCP Broker 身份无效".into()));
    }
    Ok(())
}

async fn authenticate_client(db: &Database, request: &BrokerRequest) -> AppResult<McpClient> {
    let client = get_client(db, &request.client_id).await?;
    if client.status != "active" {
        return Err(AppError::PermissionDenied("MCP 客户端已撤销".into()));
    }
    let expected_secret_sha256 = sqlx::query_scalar::<_, Option<String>>(
        "SELECT client_secret_sha256 FROM mcp_clients WHERE id=?",
    )
    .bind(&request.client_id)
    .fetch_one(&db.pool)
    .await?
    .ok_or_else(|| AppError::PermissionDenied("MCP 客户端尚未生成配置".into()))?;
    let supplied_secret_sha256 = format!(
        "sha256:{:x}",
        Sha256::digest(request.client_secret.as_bytes())
    );
    if !constant_time_equal(&expected_secret_sha256, &supplied_secret_sha256) {
        return Err(AppError::PermissionDenied("MCP 客户端身份无效".into()));
    }
    match (
        client.executable_path.as_deref(),
        client.executable_sha256.as_deref(),
        request.executable_path.as_deref(),
        request.executable_sha256.as_deref(),
    ) {
        (Some(expected_path), Some(expected_digest), Some(path), Some(digest))
            if expected_path == path && expected_digest == digest => {}
        _ => {
            return Err(AppError::PermissionDenied(
                "MCP sidecar 尚未登记或路径/摘要已变化，请在 CNshell 中重新生成客户端配置".into(),
            ));
        }
    }
    sqlx::query("UPDATE mcp_clients SET last_used_at=? WHERE id=?")
        .bind(Utc::now().to_rfc3339())
        .bind(&request.client_id)
        .execute(&db.pool)
        .await?;
    Ok(client)
}

async fn execute_tool(
    context: &BrokerContext,
    client: &McpClient,
    request: &BrokerRequest,
    cancelled: Arc<AtomicBool>,
) -> AppResult<Value> {
    match request.tool.as_str() {
        "cnshell_list_connections" => list_connections(context, client, request).await,
        "cnshell_open_session" => open_session(context, client, request).await,
        "cnshell_close_session" => close_session(context, client, request).await,
        "cnshell_file_list" => file_list(context, client, request, cancelled).await,
        "cnshell_file_read" => file_read(context, client, request, cancelled).await,
        "cnshell_system_info" => system_info(context, client, request, cancelled).await,
        "cnshell_run_command" => run_command(context, client, request, cancelled).await,
        "cnshell_file_write" => file_write(context, client, request, cancelled).await,
        "cnshell_file_mkdir" => file_mkdir(context, client, request, cancelled).await,
        "cnshell_file_rename" => file_rename(context, client, request, cancelled).await,
        "cnshell_file_delete" => file_delete(context, client, request, cancelled).await,
        "cnshell_file_upload" => file_upload(context, client, request, cancelled).await,
        "cnshell_file_download" => file_download(context, client, request, cancelled).await,
        _ => Err(AppError::Validation("未知 MCP 工具".into())),
    }
}

async fn execute_operation(
    context: &BrokerContext,
    client: &McpClient,
    request: &BrokerRequest,
    cancelled: Arc<AtomicBool>,
) -> AppResult<Value> {
    match request.tool.as_str() {
        RESOURCE_CONNECTIONS_OPERATION => resource_connections(&context.db, client).await,
        RESOURCE_AUDIT_OPERATION => resource_recent_audit(&context.db, client).await,
        _ => execute_tool(context, client, request, cancelled).await,
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ListConnectionsArgs {
    cursor: Option<String>,
    limit: Option<usize>,
    tag: Option<String>,
}

async fn list_connections(
    context: &BrokerContext,
    client: &McpClient,
    request: &BrokerRequest,
) -> AppResult<Value> {
    let args: ListConnectionsArgs = arguments(request)?;
    let limit = args.limit.unwrap_or(100).clamp(1, 100);
    let offset = decode_cursor(args.cursor.as_deref())?;
    let allowed =
        granted_connection_ids(&context.db, &client.id, "cnshell_list_connections").await?;
    let mut connections = context
        .db
        .list_connections()
        .await?
        .into_iter()
        .filter(|profile| profile.protocol == "ssh" && allowed.contains(&profile.id))
        .filter(|profile| {
            args.tag
                .as_ref()
                .is_none_or(|tag| profile.tags.iter().any(|value| value == tag))
        })
        .map(|profile| {
            let mut value = json!({
                "id": profile.id,
                "name": profile.name,
                "protocol": profile.protocol,
                "tags": profile.tags,
                "hasCredential": profile.has_credential,
            });
            if client.show_hostnames {
                value["host"] = Value::String(profile.host);
                value["username"] = Value::String(profile.username);
                value["port"] = Value::Number(profile.port.into());
            }
            value
        })
        .collect::<Vec<_>>();
    connections.sort_by(|left, right| {
        left["name"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["name"].as_str().unwrap_or_default())
    });
    let total = connections.len();
    let page = connections
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    Ok(json!({
        "requestId": request.request_id,
        "connections": page,
        "nextCursor": (offset + limit < total).then(|| encode_cursor(offset + limit)),
    }))
}

async fn resource_connections(db: &Database, client: &McpClient) -> AppResult<Value> {
    let allowed = granted_connection_ids(db, &client.id, "cnshell_list_connections").await?;
    let mut connections = db
        .list_connections()
        .await?
        .into_iter()
        .filter(|profile| profile.protocol == "ssh" && allowed.contains(&profile.id))
        .map(|profile| {
            let mut value = json!({
                "id": profile.id,
                "name": profile.name,
                "protocol": profile.protocol,
                "tags": profile.tags,
            });
            if client.show_hostnames {
                value["host"] = Value::String(profile.host);
                value["username"] = Value::String(profile.username);
                value["port"] = Value::Number(profile.port.into());
            }
            value
        })
        .collect::<Vec<_>>();
    connections.sort_by(|left, right| {
        left["name"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["name"].as_str().unwrap_or_default())
    });
    let total = connections.len();
    connections.truncate(MAX_MCP_RESOURCE_CONNECTIONS);
    Ok(json!({
        "connections": connections,
        "truncated": total > MAX_MCP_RESOURCE_CONNECTIONS,
    }))
}

async fn resource_recent_audit(db: &Database, client: &McpClient) -> AppResult<Value> {
    let rows = sqlx::query(
        "SELECT connection_id,tool,risk,outcome,duration_ms,transferred_bytes,truncated,created_at \
         FROM mcp_audit_events WHERE client_id=? ORDER BY created_at DESC,rowid DESC LIMIT ?",
    )
    .bind(&client.id)
    .bind(MAX_MCP_RESOURCE_AUDIT_EVENTS)
    .fetch_all(&db.pool)
    .await?;
    let events = rows
        .into_iter()
        .map(|row| {
            json!({
                "connectionId": row.get::<Option<String>, _>("connection_id"),
                "tool": row.get::<String, _>("tool"),
                "risk": row.get::<String, _>("risk"),
                "outcome": row.get::<String, _>("outcome"),
                "durationMs": row.get::<Option<i64>, _>("duration_ms"),
                "transferredBytes": row.get::<Option<i64>, _>("transferred_bytes"),
                "truncated": row.get::<i64, _>("truncated") != 0,
                "createdAt": row.get::<String, _>("created_at"),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({ "events": events }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OpenSessionArgs {
    connection_id: String,
}

async fn open_session(
    context: &BrokerContext,
    client: &McpClient,
    request: &BrokerRequest,
) -> AppResult<Value> {
    let args: OpenSessionArgs = arguments(request)?;
    require_tool_grant(
        &context.db,
        &client.id,
        &args.connection_id,
        "cnshell_open_session",
        None,
    )
    .await?;
    let profile = context.db.get_connection(&args.connection_id).await?;
    if profile.protocol != "ssh" {
        return Err(AppError::Validation("MCP 首版只支持 SSH 连接".into()));
    }
    let confirmation_key = (client.id.clone(), args.connection_id.clone());
    if !context
        .manager
        .inner
        .lock()
        .confirmed_connections
        .contains(&confirmation_key)
    {
        let decision = context
            .manager
            .request_approval(
                &context.app,
                &context.db,
                client,
                &args.connection_id,
                &profile.name,
                &request.request_id,
                &request.tool,
                "low",
                "打开短期 SSH 会话",
                "允许此 MCP 客户端在本次 CNshell 运行期间打开该连接的短期会话。连接密码、私钥和备注不会提供给客户端。",
                ApprovalPolicy::default(),
            )
            .await?;
        audit_approval(
            &context.db,
            request,
            client,
            &args.connection_id,
            "low",
            "session-open",
            decision,
        )
        .await?;
        if !decision.approved() {
            return Err(AppError::PermissionDenied(
                "用户拒绝或未及时批准 MCP 短期会话".into(),
            ));
        }
        context
            .manager
            .inner
            .lock()
            .confirmed_connections
            .insert(confirmation_key);
    }
    let session_id = context
        .manager
        .open_session(&context.sessions, &client.id, profile)?;
    Ok(json!({
        "requestId": request.request_id,
        "sessionId": session_id,
        "connectionId": args.connection_id,
        "expiresInSeconds": SESSION_MAX_TTL.as_secs(),
        "capabilities": ["sftp", "exec", "systemInfo"],
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionArgs {
    session_id: String,
}

async fn close_session(
    context: &BrokerContext,
    client: &McpClient,
    request: &BrokerRequest,
) -> AppResult<Value> {
    let args: SessionArgs = arguments(request)?;
    context
        .manager
        .close_session(&context.sessions, &client.id, &args.session_id)?;
    Ok(json!({"requestId": request.request_id, "closed": true}))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileListArgs {
    session_id: String,
    path: String,
    cursor: Option<String>,
    limit: Option<usize>,
    #[serde(default)]
    show_hidden: bool,
}

async fn file_list(
    context: &BrokerContext,
    client: &McpClient,
    request: &BrokerRequest,
    cancelled: Arc<AtomicBool>,
) -> AppResult<Value> {
    let args: FileListArgs = arguments(request)?;
    let connection_id = context
        .manager
        .session(&context.sessions, &client.id, &args.session_id)?;
    let path = normalize_remote_path(&args.path)?;
    let root = require_tool_grant(
        &context.db,
        &client.id,
        &connection_id,
        &request.tool,
        Some(&path),
    )
    .await?;
    crate::sftp::validate_mcp_remote_path_boundary(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id.clone(),
        path.clone(),
        root.clone(),
        false,
    )
    .await?;
    let offset = decode_cursor(args.cursor.as_deref())?;
    let limit = args.limit.unwrap_or(100).clamp(1, 500);
    let entries = crate::sftp::list_bounded(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id,
        path.clone(),
        root.clone(),
        args.show_hidden,
        cancelled,
    )
    .await?;
    let total = entries.len();
    let page = entries
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    bounded_file_list_response(&request.request_id, &path, page, offset, total)
}

fn bounded_file_list_response(
    request_id: &str,
    path: &str,
    mut entries: Vec<crate::models::RemoteFile>,
    offset: usize,
    total: usize,
) -> AppResult<Value> {
    loop {
        let next_offset = offset.saturating_add(entries.len());
        let response = json!({
            "requestId": request_id,
            "path": path,
            "entries": entries.clone(),
            "nextCursor": (next_offset < total).then(|| encode_cursor(next_offset)),
        });
        let encoded =
            serde_json::to_vec(&response).map_err(|error| AppError::Internal(error.to_string()))?;
        if encoded.len() <= MAX_MCP_DIRECTORY_RESPONSE_BYTES {
            return Ok(response);
        }
        if entries.len() <= 1 {
            return Err(AppError::Validation(
                "MCP 目录项响应超过 512 KiB，请缩小目录或使用更短的文件名".into(),
            ));
        }
        // Keep the response bounded even when a directory contains unusually
        // long names or metadata. The shortened page gets a valid cursor.
        entries.pop();
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileReadArgs {
    session_id: String,
    path: String,
    offset: Option<u64>,
    max_bytes: Option<usize>,
}

async fn file_read(
    context: &BrokerContext,
    client: &McpClient,
    request: &BrokerRequest,
    cancelled: Arc<AtomicBool>,
) -> AppResult<Value> {
    let args: FileReadArgs = arguments(request)?;
    let connection_id = context
        .manager
        .session(&context.sessions, &client.id, &args.session_id)?;
    let path = normalize_remote_path(&args.path)?;
    let root = require_tool_grant(
        &context.db,
        &client.id,
        &connection_id,
        &request.tool,
        Some(&path),
    )
    .await?;
    crate::sftp::validate_mcp_remote_path_boundary(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id.clone(),
        path.clone(),
        root.clone(),
        false,
    )
    .await?;
    let offset = args.offset.unwrap_or(0);
    let max_bytes = args.max_bytes.unwrap_or(64 * 1024).clamp(1, 256 * 1024);
    let file = crate::sftp::read_text_range(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id,
        path.clone(),
        root,
        offset,
        max_bytes,
        cancelled,
    )
    .await?;
    Ok(json!({
        "requestId": request.request_id,
        "path": path,
        "content": file.content,
        "offset": offset,
        "nextOffset": file.next_offset,
        "size": file.size,
        "sha256": file.sha256,
        "modifiedAt": file.modified_at,
        "truncated": file.next_offset.is_some(),
    }))
}

async fn system_info(
    context: &BrokerContext,
    client: &McpClient,
    request: &BrokerRequest,
    cancelled: Arc<AtomicBool>,
) -> AppResult<Value> {
    let args: SessionArgs = arguments(request)?;
    let connection_id = context
        .manager
        .session(&context.sessions, &client.id, &args.session_id)?;
    require_tool_grant(&context.db, &client.id, &connection_id, &request.tool, None).await?;
    let info = crate::monitor::system_info_cancelable(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id,
        cancelled,
    )
    .await?;
    Ok(json!({"requestId": request.request_id, "system": info}))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunCommandArgs {
    session_id: String,
    command: String,
    timeout_seconds: Option<u64>,
}

async fn run_command(
    context: &BrokerContext,
    client: &McpClient,
    request: &BrokerRequest,
    cancelled: Arc<AtomicBool>,
) -> AppResult<Value> {
    let args: RunCommandArgs = arguments(request)?;
    if args.command.is_empty() || args.command.len() > 16 * 1024 || args.command.contains('\0') {
        return Err(AppError::Validation(
            "MCP 命令必须是 16 KiB 以内的非空文本".into(),
        ));
    }
    let connection_id = context
        .manager
        .session(&context.sessions, &client.id, &args.session_id)?;
    require_tool_grant(&context.db, &client.id, &connection_id, &request.tool, None).await?;
    let profile = context.db.get_connection(&connection_id).await?;
    let risk = command_risk(&args.command);
    let approved = context
        .manager
        .request_approval(
            &context.app,
            &context.db,
            client,
            &connection_id,
            &profile.name,
            &request.request_id,
            &request.tool,
            risk,
            "远端命令",
            &args.command,
            ApprovalPolicy {
                target_key: Some(command_summary(&args.command)),
                can_allow_session: risk != "high",
                persistent_command: can_save_command_rule(&args.command)
                    .then(|| args.command.clone()),
            },
        )
        .await?;
    audit_approval(
        &context.db,
        request,
        client,
        &connection_id,
        risk,
        &command_summary(&args.command),
        approved,
    )
    .await?;
    if !approved.approved() {
        return Err(AppError::PermissionDenied(
            "用户拒绝或未及时批准 MCP 命令".into(),
        ));
    }
    let timeout = Duration::from_secs(args.timeout_seconds.unwrap_or(30).clamp(1, 600));
    let result = crate::ssh::execute_pooled_command(
        &context.db,
        &context.sessions,
        &profile,
        &args.command,
        cancelled,
        timeout,
        1024 * 1024,
    )
    .await?;
    Ok(json!({
        "requestId": request.request_id,
        "exitCode": result.exit_code,
        "stdout": result.stdout,
        "stderr": result.stderr,
        "truncated": result.truncated,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileWriteArgs {
    session_id: String,
    path: String,
    content: String,
    expected_sha256: Option<String>,
}

async fn file_write(
    context: &BrokerContext,
    client: &McpClient,
    request: &BrokerRequest,
    cancelled: Arc<AtomicBool>,
) -> AppResult<Value> {
    let args: FileWriteArgs = arguments(request)?;
    if args.content.len() > 256 * 1024 {
        return Err(AppError::Validation(
            "MCP 单次文本写入不能超过 256 KiB".into(),
        ));
    }
    let connection_id = context
        .manager
        .session(&context.sessions, &client.id, &args.session_id)?;
    let path = normalize_remote_path(&args.path)?;
    let root = require_tool_grant(
        &context.db,
        &client.id,
        &connection_id,
        &request.tool,
        Some(&path),
    )
    .await?;
    crate::sftp::validate_mcp_remote_path_boundary(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id.clone(),
        path.clone(),
        root.clone(),
        true,
    )
    .await?;
    let profile = context.db.get_connection(&connection_id).await?;
    let preview = file_write_preview(context, &args, &root, cancelled.clone()).await?;
    let approved = context
        .manager
        .request_approval(
            &context.app,
            &context.db,
            client,
            &connection_id,
            &profile.name,
            &request.request_id,
            &request.tool,
            "medium",
            &path,
            &preview,
            ApprovalPolicy::default(),
        )
        .await?;
    audit_approval(
        &context.db,
        request,
        client,
        &connection_id,
        "medium",
        &path_summary(&path),
        approved,
    )
    .await?;
    if !approved.approved() {
        return Err(AppError::PermissionDenied(
            "用户拒绝或未及时批准 MCP 文件写入".into(),
        ));
    }
    ensure_not_cancelled(&cancelled)?;
    crate::sftp::validate_mcp_remote_path_boundary(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id.clone(),
        path.clone(),
        root.clone(),
        true,
    )
    .await?;
    let saved = crate::sftp::write_text_atomic(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id,
        path.clone(),
        root,
        args.content,
        args.expected_sha256,
        cancelled,
    )
    .await?;
    Ok(json!({
        "requestId": request.request_id,
        "path": path,
        "sha256": saved.sha256,
        "created": saved.created,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FilePathArgs {
    session_id: String,
    path: String,
}

async fn file_mkdir(
    context: &BrokerContext,
    client: &McpClient,
    request: &BrokerRequest,
    cancelled: Arc<AtomicBool>,
) -> AppResult<Value> {
    let args: FilePathArgs = arguments(request)?;
    let connection_id = context
        .manager
        .session(&context.sessions, &client.id, &args.session_id)?;
    let path = normalize_remote_path(&args.path)?;
    if path == "/" {
        return Err(AppError::Validation("禁止创建远端根目录".into()));
    }
    let root = require_tool_grant(
        &context.db,
        &client.id,
        &connection_id,
        &request.tool,
        Some(&path),
    )
    .await?;
    crate::sftp::validate_mcp_remote_path_boundary(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id.clone(),
        path.clone(),
        root.clone(),
        true,
    )
    .await?;
    approve_path_operation(context, client, request, &connection_id, &path, "low").await?;
    ensure_not_cancelled(&cancelled)?;
    crate::sftp::validate_mcp_remote_path_boundary(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id.clone(),
        path.clone(),
        root.clone(),
        true,
    )
    .await?;
    crate::sftp::mkdir_for_mcp(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id,
        path.clone(),
        root,
        cancelled,
    )
    .await?;
    Ok(json!({"requestId": request.request_id, "path": path, "created": true}))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileRenameArgs {
    session_id: String,
    from: String,
    to: String,
    expected_sha256: Option<String>,
}

async fn file_rename(
    context: &BrokerContext,
    client: &McpClient,
    request: &BrokerRequest,
    cancelled: Arc<AtomicBool>,
) -> AppResult<Value> {
    let args: FileRenameArgs = arguments(request)?;
    let connection_id = context
        .manager
        .session(&context.sessions, &client.id, &args.session_id)?;
    let source = normalize_remote_path(&args.from)?;
    let destination = normalize_remote_path(&args.to)?;
    if source == "/" || destination == "/" {
        return Err(AppError::Validation("禁止重命名远端根目录".into()));
    }
    let source_root = require_tool_grant(
        &context.db,
        &client.id,
        &connection_id,
        &request.tool,
        Some(&source),
    )
    .await?;
    ensure_not_cancelled(&cancelled)?;
    let destination_root = require_tool_grant(
        &context.db,
        &client.id,
        &connection_id,
        &request.tool,
        Some(&destination),
    )
    .await?;
    crate::sftp::validate_mcp_remote_path_boundary(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id.clone(),
        source.clone(),
        source_root.clone(),
        false,
    )
    .await?;
    crate::sftp::validate_mcp_remote_path_boundary(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id.clone(),
        destination.clone(),
        destination_root.clone(),
        true,
    )
    .await?;
    approve_path_operation(
        context,
        client,
        request,
        &connection_id,
        &format!("{source} -> {destination}"),
        "medium",
    )
    .await?;
    ensure_not_cancelled(&cancelled)?;
    crate::sftp::validate_mcp_remote_path_boundary(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id.clone(),
        source.clone(),
        source_root.clone(),
        false,
    )
    .await?;
    crate::sftp::validate_mcp_remote_path_boundary(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id.clone(),
        destination.clone(),
        destination_root.clone(),
        true,
    )
    .await?;
    crate::sftp::rename_for_mcp(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id,
        source.clone(),
        destination.clone(),
        source_root,
        destination_root,
        args.expected_sha256,
        cancelled,
    )
    .await?;
    Ok(json!({
        "requestId": request.request_id,
        "from": source,
        "to": destination,
        "renamed": true,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileDeleteArgs {
    session_id: String,
    path: String,
    #[serde(default)]
    recursive: bool,
}

async fn file_delete(
    context: &BrokerContext,
    client: &McpClient,
    request: &BrokerRequest,
    cancelled: Arc<AtomicBool>,
) -> AppResult<Value> {
    let args: FileDeleteArgs = arguments(request)?;
    let connection_id = context
        .manager
        .session(&context.sessions, &client.id, &args.session_id)?;
    let path = normalize_remote_path(&args.path)?;
    if path == "/" {
        return Err(AppError::Validation("禁止删除远端根目录".into()));
    }
    let root = require_tool_grant(
        &context.db,
        &client.id,
        &connection_id,
        &request.tool,
        Some(&path),
    )
    .await?;
    if path == root {
        return Err(AppError::PermissionDenied(
            "禁止删除 MCP 授权根目录本身".into(),
        ));
    }
    crate::sftp::validate_mcp_remote_path_boundary(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id.clone(),
        path.clone(),
        root.clone(),
        false,
    )
    .await?;
    approve_path_operation(
        context,
        client,
        request,
        &connection_id,
        &format!("{}{}", path, if args.recursive { "（递归）" } else { "" }),
        if args.recursive { "high" } else { "medium" },
    )
    .await?;
    ensure_not_cancelled(&cancelled)?;
    crate::sftp::validate_mcp_remote_path_boundary(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id.clone(),
        path.clone(),
        root.clone(),
        false,
    )
    .await?;
    crate::sftp::delete_for_mcp(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id,
        path.clone(),
        root.clone(),
        args.recursive,
        cancelled,
    )
    .await?;
    Ok(json!({"requestId": request.request_id, "path": path, "deleted": true}))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileTransferArgs {
    session_id: String,
    local_grant_id: String,
    #[serde(default)]
    relative_path: String,
    remote_path: String,
    #[serde(default = "default_conflict_policy")]
    conflict_policy: String,
}

fn default_conflict_policy() -> String {
    "rename".into()
}

async fn file_upload(
    context: &BrokerContext,
    client: &McpClient,
    request: &BrokerRequest,
    cancelled: Arc<AtomicBool>,
) -> AppResult<Value> {
    let args: FileTransferArgs = arguments(request)?;
    validate_conflict_policy(&args.conflict_policy)?;
    let connection_id = context
        .manager
        .session(&context.sessions, &client.id, &args.session_id)?;
    let remote_path = normalize_remote_path(&args.remote_path)?;
    let root = require_tool_grant(
        &context.db,
        &client.id,
        &connection_id,
        &request.tool,
        Some(&remote_path),
    )
    .await?;
    crate::sftp::validate_mcp_remote_path_boundary(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id.clone(),
        remote_path.clone(),
        root.clone(),
        true,
    )
    .await?;
    let grant = active_local_grant(&context.db, &client.id, &args.local_grant_id, "upload").await?;
    let access = crate::bookmark::access_mcp_local_grant(&grant.id)?;
    let local_path = resolve_local_grant_path(access.path(), &args.relative_path, false)?;
    let metadata = std::fs::symlink_metadata(&local_path)?;
    let kind = if metadata.is_dir() {
        "directory"
    } else if metadata.is_file() {
        "file"
    } else {
        return Err(AppError::Validation("MCP 只能上传普通文件或文件夹".into()));
    };
    let preview = format!("上传 {} ({kind})\n到 {remote_path}", grant.display_name);
    approve_transfer(
        context,
        client,
        request,
        &connection_id,
        &remote_path,
        &preview,
    )
    .await?;
    ensure_not_cancelled(&cancelled)?;
    crate::sftp::validate_mcp_remote_path_boundary(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id.clone(),
        remote_path.clone(),
        root.clone(),
        true,
    )
    .await?;
    let _target = context
        .manager
        .acquire_transfer_target(Path::new(&remote_path))?;
    if kind == "directory" {
        crate::sftp::validate_local_directory_tree_for_mcp(&local_path, &cancelled)?;
    }
    consume_local_grant(&context.db, &grant).await?;
    let (final_remote_path, transferred_bytes) = if kind == "directory" {
        let result = crate::sftp::transfer_directory(
            context.db.clone(),
            context.sessions.clone(),
            args.session_id,
            "upload".into(),
            local_path.to_string_lossy().into_owned(),
            remote_path.clone(),
            args.conflict_policy,
            cancelled,
            Some(root),
        )
        .await?;
        (result, 0_u64)
    } else {
        let result = crate::sftp::transfer_file_direct(
            context.db.clone(),
            context.sessions.clone(),
            args.session_id,
            "upload".into(),
            local_path.to_string_lossy().into_owned(),
            remote_path.clone(),
            args.conflict_policy,
            cancelled,
            root,
        )
        .await?;
        (result.final_path, result.transferred_bytes)
    };
    drop(access);
    Ok(json!({
        "requestId": request.request_id,
        "localGrantId": grant.id,
        "relativePath": args.relative_path,
        "remotePath": final_remote_path,
        "kind": kind,
        "transferredBytes": transferred_bytes,
    }))
}

async fn file_download(
    context: &BrokerContext,
    client: &McpClient,
    request: &BrokerRequest,
    cancelled: Arc<AtomicBool>,
) -> AppResult<Value> {
    let args: FileTransferArgs = arguments(request)?;
    validate_conflict_policy(&args.conflict_policy)?;
    if args.relative_path.is_empty() {
        return Err(AppError::Validation("MCP 下载必须提供目标相对路径".into()));
    }
    let connection_id = context
        .manager
        .session(&context.sessions, &client.id, &args.session_id)?;
    let remote_path = normalize_remote_path(&args.remote_path)?;
    let root = require_tool_grant(
        &context.db,
        &client.id,
        &connection_id,
        &request.tool,
        Some(&remote_path),
    )
    .await?;
    crate::sftp::validate_mcp_remote_path_boundary(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id.clone(),
        remote_path.clone(),
        root.clone(),
        false,
    )
    .await?;
    let grant =
        active_local_grant(&context.db, &client.id, &args.local_grant_id, "download").await?;
    let access = crate::bookmark::access_mcp_local_grant(&grant.id)?;
    let local_path = resolve_local_grant_path(access.path(), &args.relative_path, true)?;
    let kind = crate::sftp::path_kind(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id.clone(),
        remote_path.clone(),
        root.clone(),
    )
    .await?;
    let preview = format!(
        "下载 {remote_path} ({kind})\n到授权目录内的 {}",
        args.relative_path
    );
    approve_transfer(
        context,
        client,
        request,
        &connection_id,
        &remote_path,
        &preview,
    )
    .await?;
    ensure_not_cancelled(&cancelled)?;
    crate::sftp::validate_mcp_remote_path_boundary(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id.clone(),
        remote_path.clone(),
        root.clone(),
        false,
    )
    .await?;
    let _target = context.manager.acquire_transfer_target(&local_path)?;
    if kind == "directory" {
        crate::sftp::validate_remote_directory_tree_for_mcp(
            context.db.clone(),
            context.sessions.clone(),
            args.session_id.clone(),
            remote_path.clone(),
            cancelled.clone(),
        )
        .await?;
    }
    consume_local_grant(&context.db, &grant).await?;
    let transferred_bytes = if kind == "directory" {
        crate::sftp::transfer_directory(
            context.db.clone(),
            context.sessions.clone(),
            args.session_id,
            "download".into(),
            remote_path.clone(),
            local_path.to_string_lossy().into_owned(),
            args.conflict_policy,
            cancelled,
            Some(root),
        )
        .await?;
        0_u64
    } else {
        crate::sftp::transfer_file_direct(
            context.db.clone(),
            context.sessions.clone(),
            args.session_id,
            "download".into(),
            remote_path.clone(),
            local_path.to_string_lossy().into_owned(),
            args.conflict_policy,
            cancelled,
            root,
        )
        .await?
        .transferred_bytes
    };
    drop(access);
    Ok(json!({
        "requestId": request.request_id,
        "localGrantId": grant.id,
        "relativePath": args.relative_path,
        "remotePath": remote_path,
        "kind": kind,
        "transferredBytes": transferred_bytes,
    }))
}

fn validate_conflict_policy(value: &str) -> AppResult<()> {
    if ["overwrite", "skip", "rename"].contains(&value) {
        Ok(())
    } else {
        Err(AppError::Validation(
            "MCP 冲突策略必须是 overwrite、skip 或 rename".into(),
        ))
    }
}

async fn active_local_grant(
    db: &Database,
    client_id: &str,
    id: &str,
    direction: &str,
) -> AppResult<McpLocalGrant> {
    if id.is_empty() || id.len() > 128 {
        return Err(AppError::Validation("MCP 本地授权 ID 无效".into()));
    }
    let row = sqlx::query("SELECT id,client_id,direction,display_name,path_hint,persistent,created_at,expires_at,revoked_at FROM mcp_local_grants WHERE id=? AND client_id=? AND direction=? AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>?)")
        .bind(id).bind(client_id).bind(direction).bind(Utc::now().to_rfc3339())
        .fetch_optional(&db.pool).await?
        .ok_or_else(|| AppError::PermissionDenied("MCP 本地路径授权不存在、已过期或方向不匹配".into()))?;
    Ok(local_grant_from_row(row))
}

async fn consume_local_grant(db: &Database, grant: &McpLocalGrant) -> AppResult<()> {
    if grant.persistent {
        return Ok(());
    }
    let affected =
        sqlx::query("UPDATE mcp_local_grants SET revoked_at=? WHERE id=? AND revoked_at IS NULL")
            .bind(Utc::now().to_rfc3339())
            .bind(&grant.id)
            .execute(&db.pool)
            .await?
            .rows_affected();
    if affected != 1 {
        return Err(AppError::PermissionDenied(
            "MCP 一次性本地授权已经使用".into(),
        ));
    }
    crate::bookmark::delete_mcp_local_grant(&grant.id)
}

fn resolve_local_grant_path(root: &Path, relative: &str, for_download: bool) -> AppResult<PathBuf> {
    if relative.len() > 16 * 1024 || relative.contains('\0') || Path::new(relative).is_absolute() {
        return Err(AppError::Validation("MCP 本地相对路径无效".into()));
    }
    let mut components = Vec::new();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(value) => components.push(value.to_owned()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::Validation(
                    "MCP 本地路径不能包含父目录或绝对路径".into(),
                ));
            }
        }
    }
    let root_metadata = std::fs::symlink_metadata(root)?;
    if root_metadata.is_file() {
        if for_download || !components.is_empty() {
            return Err(AppError::Validation("文件授权不接受子路径".into()));
        }
        return Ok(root.to_path_buf());
    }
    if !root_metadata.is_dir() {
        return Err(AppError::PermissionDenied("MCP 本地授权根类型无效".into()));
    }
    let canonical_root = root.canonicalize()?;
    let mut candidate = root.to_path_buf();
    for component in components {
        candidate.push(component);
    }
    let boundary = if candidate.exists() {
        candidate.canonicalize()?
    } else {
        candidate
            .parent()
            .ok_or_else(|| AppError::Validation("MCP 本地目标路径无效".into()))?
            .canonicalize()?
    };
    if !boundary.starts_with(&canonical_root) {
        return Err(AppError::PermissionDenied(
            "MCP 本地路径越过授权目录".into(),
        ));
    }
    reject_local_link_components(
        root,
        if candidate.exists() {
            &candidate
        } else {
            candidate.parent().unwrap_or(root)
        },
    )?;
    if !for_download && !candidate.exists() {
        return Err(AppError::NotFound("MCP 上传源不存在".into()));
    }
    Ok(candidate)
}

fn reject_local_link_components(root: &Path, target: &Path) -> AppResult<()> {
    let relative = target
        .strip_prefix(root)
        .map_err(|_| AppError::PermissionDenied("MCP 本地路径越界".into()))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        let metadata = std::fs::symlink_metadata(&current)?;
        if metadata.file_type().is_symlink() || local_metadata_is_reparse_point(&metadata) {
            return Err(AppError::PermissionDenied(
                "MCP 本地路径不能经过符号链接或重解析点".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn local_metadata_is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(target_os = "windows"))]
fn local_metadata_is_reparse_point(_metadata: &std::fs::Metadata) -> bool {
    false
}

async fn approve_transfer(
    context: &BrokerContext,
    client: &McpClient,
    request: &BrokerRequest,
    connection_id: &str,
    target: &str,
    preview: &str,
) -> AppResult<()> {
    let profile = context.db.get_connection(connection_id).await?;
    let approved = context
        .manager
        .request_approval(
            &context.app,
            &context.db,
            client,
            connection_id,
            &profile.name,
            &request.request_id,
            &request.tool,
            "medium",
            target,
            preview,
            ApprovalPolicy::default(),
        )
        .await?;
    audit_approval(
        &context.db,
        request,
        client,
        connection_id,
        "medium",
        &path_summary(target),
        approved,
    )
    .await?;
    if approved.approved() {
        Ok(())
    } else {
        Err(AppError::PermissionDenied(
            "用户拒绝或未及时批准 MCP 文件传输".into(),
        ))
    }
}

async fn approve_path_operation(
    context: &BrokerContext,
    client: &McpClient,
    request: &BrokerRequest,
    connection_id: &str,
    target: &str,
    risk: &str,
) -> AppResult<()> {
    let profile = context.db.get_connection(connection_id).await?;
    let approved = context
        .manager
        .request_approval(
            &context.app,
            &context.db,
            client,
            connection_id,
            &profile.name,
            &request.request_id,
            &request.tool,
            risk,
            target,
            target,
            ApprovalPolicy::default(),
        )
        .await?;
    audit_approval(
        &context.db,
        request,
        client,
        connection_id,
        risk,
        &path_summary(target),
        approved,
    )
    .await?;
    if approved.approved() {
        Ok(())
    } else {
        Err(AppError::PermissionDenied(
            "用户拒绝或未及时批准 MCP 文件操作".into(),
        ))
    }
}

async fn require_tool_grant(
    db: &Database,
    client_id: &str,
    connection_id: &str,
    tool: &str,
    path: Option<&str>,
) -> AppResult<String> {
    let roots = sqlx::query_scalar::<_, String>(
        "SELECT remote_root FROM mcp_grants WHERE client_id=? AND connection_id=? AND tool=? AND (expires_at IS NULL OR expires_at>?)",
    )
    .bind(client_id)
    .bind(connection_id)
    .bind(tool)
    .bind(Utc::now().to_rfc3339())
    .fetch_all(&db.pool)
    .await?;
    let matched = roots
        .into_iter()
        .find(|root| path.is_none_or(|path| remote_path_is_within(path, root)));
    matched.ok_or_else(|| {
        AppError::PermissionDenied(format!("MCP 客户端没有连接 {connection_id} 的 {tool} 权限"))
    })
}

fn arguments<T: for<'de> Deserialize<'de>>(request: &BrokerRequest) -> AppResult<T> {
    serde_json::from_value(Value::Object(request.arguments.clone()))
        .map_err(|error| AppError::Validation(format!("MCP 工具参数无效：{error}")))
}

fn normalize_remote_path(path: &str) -> AppResult<String> {
    if path.is_empty() || path.len() > 32 * 1024 || path.contains('\0') || !path.starts_with('/') {
        return Err(AppError::Validation(
            "MCP 远端路径必须是长度受限的绝对路径".into(),
        ));
    }
    let mut parts = Vec::new();
    for component in Path::new(path).components() {
        match component {
            Component::RootDir => {}
            Component::Normal(value) => {
                let value = value
                    .to_str()
                    .ok_or_else(|| AppError::Validation("MCP 路径必须是 UTF-8".into()))?;
                if value == "." || value == ".." {
                    return Err(AppError::Validation("MCP 路径不能包含父目录跳转".into()));
                }
                parts.push(value);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) => {
                return Err(AppError::Validation("MCP 远端路径无效".into()));
            }
        }
    }
    Ok(if parts.is_empty() {
        "/".into()
    } else {
        format!("/{}", parts.join("/"))
    })
}

fn remote_path_is_within(path: &str, root: &str) -> bool {
    root == "/"
        || path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn command_risk(command: &str) -> &'static str {
    let lower = command.to_ascii_lowercase();
    if [
        "rm -rf", "sudo ", " su ", "shutdown", "reboot", "mkfs", "iptables", "nft ", "curl ",
        "wget ", "| sh", "| bash", "userdel", "chmod -r", "chown -r",
    ]
    .iter()
    .any(|pattern| lower.contains(pattern))
    {
        "high"
    } else if can_save_command_rule(command) {
        "low"
    } else {
        "medium"
    }
}

fn can_save_command_rule(command: &str) -> bool {
    if command != command.trim() {
        return false;
    }
    if matches!(
        command,
        "pwd" | "whoami" | "id" | "uptime" | "date" | "uname -a" | "uname -r" | "uname -s"
    ) {
        return true;
    }
    command.strip_prefix("printf ").is_some_and(|value| {
        !value.is_empty()
            && value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/' | b'=' | b'+')
            })
    })
}

fn command_summary(command: &str) -> String {
    format!("command:sha256:{:x}", Sha256::digest(command.as_bytes()))
}

fn path_summary(path: &str) -> String {
    let name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("/");
    format!("path:{name}:sha256:{:x}", Sha256::digest(path.as_bytes()))
}

fn bounded_preview(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        value.into()
    } else {
        let mut boundary = limit;
        while boundary > 0 && !value.is_char_boundary(boundary) {
            boundary -= 1;
        }
        format!(
            "{}\n…内容已截断，仅展示前 {boundary} 字节",
            &value[..boundary]
        )
    }
}

async fn file_write_preview(
    context: &BrokerContext,
    args: &FileWriteArgs,
    root: &str,
    cancelled: Arc<AtomicBool>,
) -> AppResult<String> {
    let Some(expected_sha256) = args.expected_sha256.as_deref() else {
        return Ok(format!(
            "新建文件\n+++ 新内容\n{}",
            prefixed_preview_lines(&bounded_preview(&args.content, 16 * 1024), "+ ")
        ));
    };
    let existing = crate::sftp::read_text_range(
        context.db.clone(),
        context.sessions.clone(),
        args.session_id.clone(),
        args.path.clone(),
        root.into(),
        0,
        16 * 1024,
        cancelled,
    )
    .await?;
    if existing.sha256 != expected_sha256 {
        return Err(AppError::Remote(
            "远端文件内容已变化，expectedSha256 不匹配".into(),
        ));
    }
    let old = prefixed_preview_lines(&bounded_preview(&existing.content, 16 * 1024), "- ");
    let new = prefixed_preview_lines(&bounded_preview(&args.content, 16 * 1024), "+ ");
    Ok(format!(
        "覆盖文件（审批后仍会再次校验 expectedSha256）\n--- 当前内容\n{old}\n+++ 新内容\n{new}"
    ))
}

fn prefixed_preview_lines(value: &str, prefix: &str) -> String {
    value
        .lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn encode_cursor(offset: usize) -> String {
    format!("offset:{offset}")
}

fn decode_cursor(cursor: Option<&str>) -> AppResult<usize> {
    match cursor {
        None => Ok(0),
        Some(value) => value
            .strip_prefix("offset:")
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value <= 1_000_000)
            .ok_or_else(|| AppError::Validation("MCP 分页游标无效".into())),
    }
}

fn constant_time_equal(left: &str, right: &str) -> bool {
    left.len() == right.len() && bool::from(left.as_bytes().ct_eq(right.as_bytes()))
}

fn ensure_not_cancelled(cancelled: &AtomicBool) -> AppResult<()> {
    if cancelled.load(std::sync::atomic::Ordering::Acquire) {
        Err(AppError::Unavailable("MCP 请求已取消".into()))
    } else {
        Ok(())
    }
}

fn app_error_code(error: &AppError) -> &'static str {
    match error {
        AppError::NotFound(_) => "not_found",
        AppError::Validation(_) => "invalid_params",
        AppError::HostKeyUnknown { .. } => "host_key_unknown",
        AppError::HostKeyChanged { .. } => "host_key_changed",
        AppError::Authentication(_) => "authentication",
        AppError::PermissionDenied(_) => "permission_denied",
        AppError::Remote(_) => "remote",
        AppError::Storage(_) => "storage",
        AppError::Unavailable(_) => "unavailable",
        AppError::Internal(_) => "internal",
    }
}

fn safe_error(error: &AppError) -> String {
    match error {
        AppError::Storage(_) | AppError::Internal(_) => {
            "CNshell 内部处理失败，请在应用内查看脱敏诊断".into()
        }
        _ => error.to_string(),
    }
}

fn random_secret() -> String {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use rand::RngCore;
    let mut bytes = [0_u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn client_account(id: &str) -> String {
    format!("client-v2:{id}")
}

fn save_secret(account: &str, secret: &str) -> AppResult<()> {
    keyring::Entry::new(KEYCHAIN_SERVICE, account)
        .map_err(|error| AppError::Storage(error.to_string()))?
        .set_password(secret)
        .map_err(|error| AppError::Storage(format!("MCP 系统凭据保存失败：{error}")))
}

fn load_secret(account: &str) -> AppResult<Option<String>> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, account)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(AppError::Storage(format!("MCP 系统凭据读取失败：{error}"))),
    }
}

fn valid_sha256_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn cleanup_sidecar_digest_is_trusted(
    registered_sha256: &str,
    current_sha256: &str,
    bundled_sha256: &str,
) -> bool {
    valid_sha256_digest(registered_sha256)
        && valid_sha256_digest(current_sha256)
        && valid_sha256_digest(bundled_sha256)
        && constant_time_equal(current_sha256, bundled_sha256)
}

pub fn provision_client_secret(id: &str) -> AppResult<String> {
    Uuid::parse_str(id).map_err(|_| AppError::Validation("MCP 客户端 ID 无效".into()))?;
    let secret = match load_secret(&client_account(id))? {
        Some(secret) => secret,
        None => {
            let secret = random_secret();
            save_secret(&client_account(id), &secret)?;
            secret
        }
    };
    Ok(format!("sha256:{:x}", Sha256::digest(secret.as_bytes())))
}

pub fn revoke_client_secret(id: &str, expected_sha256: &str) -> AppResult<()> {
    Uuid::parse_str(id).map_err(|_| AppError::Validation("MCP 客户端 ID 无效".into()))?;
    if !valid_sha256_digest(expected_sha256) {
        return Err(AppError::Validation("MCP 客户端凭据摘要无效".into()));
    }
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &client_account(id))
        .map_err(|error| AppError::Storage(error.to_string()))?;
    let secret = match entry.get_password() {
        Ok(secret) => secret,
        Err(keyring::Error::NoEntry) => return Ok(()),
        Err(error) => {
            return Err(AppError::Storage(format!("MCP 系统凭据读取失败：{error}")));
        }
    };
    let actual_sha256 = format!("sha256:{:x}", Sha256::digest(secret.as_bytes()));
    if !constant_time_equal(expected_sha256, &actual_sha256) {
        return Err(AppError::PermissionDenied(
            "MCP 客户端凭据摘要不匹配".into(),
        ));
    }
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(AppError::Storage(format!("MCP 系统凭据删除失败：{error}"))),
    }
}

fn write_discovery(path: &Path, document: &DiscoveryDocument) -> AppResult<()> {
    if let Ok(metadata) = std::fs::symlink_metadata(path)
        && metadata.file_type().is_symlink()
    {
        return Err(AppError::PermissionDenied(
            "MCP discovery 文件不能是符号链接".into(),
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Storage("MCP discovery 目录无效".into()))?;
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".mcp-broker-{}.tmp", Uuid::new_v4()));
    let encoded =
        serde_json::to_vec(document).map_err(|error| AppError::Storage(error.to_string()))?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    #[cfg(target_os = "windows")]
    if let Err(error) = apply_windows_owner_only_acl(&temporary) {
        drop(file);
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    file.write_all(&encoded)?;
    file.sync_all()?;
    std::fs::rename(&temporary, path)?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn apply_windows_owner_only_acl(path: &Path) -> AppResult<()> {
    use std::{os::windows::ffi::OsStrExt as _, ptr};
    use windows_sys::Win32::{
        Foundation::{GetLastError, LocalFree},
        Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
            },
            DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
            SetFileSecurityW,
        },
    };

    // Protected DACL; full control is granted only to the file owner.
    let sddl: Vec<u16> = "D:P(A;;FA;;;OW)\0".encode_utf16().collect();
    let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            ptr::null_mut(),
        )
    };
    if converted == 0 {
        return Err(AppError::Storage(format!(
            "MCP discovery Windows ACL 创建失败：{}",
            unsafe { GetLastError() }
        )));
    }

    let path_wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let applied = unsafe {
        SetFileSecurityW(
            path_wide.as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor,
        )
    };
    let apply_error = if applied == 0 {
        Some(unsafe { GetLastError() })
    } else {
        None
    };
    unsafe {
        LocalFree(descriptor.cast());
    }
    if let Some(error) = apply_error {
        return Err(AppError::Storage(format!(
            "MCP discovery Windows ACL 应用失败：{error}"
        )));
    }
    Ok(())
}

fn remove_discovery(path: &Path) -> AppResult<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(AppError::PermissionDenied(
            "拒绝清理符号链接 MCP discovery 文件".into(),
        )),
        Ok(_) => {
            std::fs::remove_file(path)?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub async fn create_client(db: &Database, name: &str) -> AppResult<McpClient> {
    let name = name.trim();
    if name.is_empty() || name.len() > 128 || name.contains('\0') {
        return Err(AppError::Validation(
            "MCP 客户端名称必须是 128 字符以内的非空文本".into(),
        ));
    }
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO mcp_clients(id,name,status,created_at,updated_at) VALUES(?,?,'active',?,?)",
    )
    .bind(&id)
    .bind(name)
    .bind(&now)
    .bind(&now)
    .execute(&db.pool)
    .await?;
    append_audit(
        db,
        AuditInput::event("client", "info", "registered").client(&id),
    )
    .await?;
    get_client(db, &id).await
}

pub async fn list_clients(db: &Database) -> AppResult<Vec<McpClient>> {
    let rows = sqlx::query("SELECT id,name,status,executable_path,executable_sha256,created_at,updated_at,last_used_at,revoked_at,show_hostnames FROM mcp_clients ORDER BY created_at DESC")
        .fetch_all(&db.pool)
        .await?;
    let mut clients = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row.get("id");
        let connection_ids = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT connection_id FROM mcp_grants WHERE client_id=? ORDER BY connection_id",
        )
        .bind(&id)
        .fetch_all(&db.pool)
        .await?;
        let tools = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT tool FROM mcp_grants WHERE client_id=? ORDER BY tool",
        )
        .bind(&id)
        .fetch_all(&db.pool)
        .await?;
        let remote_roots = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT remote_root FROM mcp_grants WHERE client_id=? ORDER BY remote_root",
        )
        .bind(&id)
        .fetch_all(&db.pool)
        .await?;
        if remote_roots.len() > 1 {
            return Err(AppError::Storage(
                "MCP 客户端存在不一致的远端授权根，请重新保存授权".into(),
            ));
        }
        clients.push(McpClient {
            id,
            name: row.get("name"),
            status: row.get("status"),
            executable_path: row.get("executable_path"),
            executable_sha256: row.get("executable_sha256"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            last_used_at: row.get("last_used_at"),
            revoked_at: row.get("revoked_at"),
            show_hostnames: row.get::<i64, _>("show_hostnames") != 0,
            connection_ids,
            tools,
            remote_root: remote_roots
                .into_iter()
                .next()
                .unwrap_or_else(|| "/".into()),
        });
    }
    Ok(clients)
}

async fn get_client(db: &Database, id: &str) -> AppResult<McpClient> {
    list_clients(db)
        .await?
        .into_iter()
        .find(|client| client.id == id)
        .ok_or_else(|| AppError::NotFound(format!("MCP 客户端 {id}")))
}

pub async fn save_grants(db: &Database, input: &McpClientGrantInput) -> AppResult<McpClient> {
    let client = get_client(db, &input.client_id).await?;
    if client.status != "active" {
        return Err(AppError::PermissionDenied("MCP 客户端已经撤销".into()));
    }
    if input.connection_ids.len() > 256 || input.tools.len() > WRITE_TOOLS.len() + READ_TOOLS.len()
    {
        return Err(AppError::Validation("MCP 授权数量超过限制".into()));
    }
    let root = normalize_remote_path(&input.remote_root)?;
    for tool in &input.tools {
        if !known_tool(tool) {
            return Err(AppError::Validation(format!("未知 MCP 工具：{tool}")));
        }
    }
    for connection_id in &input.connection_ids {
        if context_connection_protocol(db, connection_id).await? != "ssh" {
            return Err(AppError::Validation("MCP 只允许授权 SSH 连接".into()));
        }
    }
    let mut transaction = db.pool.begin().await?;
    sqlx::query("DELETE FROM mcp_grants WHERE client_id=?")
        .bind(&input.client_id)
        .execute(&mut *transaction)
        .await?;
    let now = Utc::now().to_rfc3339();
    sqlx::query("DELETE FROM mcp_approval_rules WHERE client_id=?")
        .bind(&input.client_id)
        .execute(&mut *transaction)
        .await?;
    for connection_id in &input.connection_ids {
        for tool in &input.tools {
            sqlx::query("INSERT INTO mcp_grants(id,client_id,connection_id,tool,remote_root,created_at) VALUES(?,?,?,?,?,?)")
                .bind(Uuid::new_v4().to_string())
                .bind(&input.client_id)
                .bind(connection_id)
                .bind(tool)
                .bind(&root)
                .bind(&now)
                .execute(&mut *transaction)
                .await?;
        }
    }
    sqlx::query("UPDATE mcp_clients SET updated_at=?,show_hostnames=? WHERE id=?")
        .bind(&now)
        .bind(if input.show_hostnames { 1_i64 } else { 0_i64 })
        .bind(&input.client_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    append_audit(
        db,
        AuditInput::event("grant", "medium", "updated")
            .client(&input.client_id)
            .target(&format!(
                "connections:{};tools:{};root:{}",
                input.connection_ids.len(),
                input.tools.len(),
                path_summary(&root)
            )),
    )
    .await?;
    get_client(db, &input.client_id).await
}

pub async fn revoke_client(
    app: &AppHandle,
    db: &Database,
    manager: &McpManager,
    sessions: &SessionManager,
    id: &str,
) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    let revoked = sqlx::query("UPDATE mcp_clients SET status='revoked',updated_at=?,revoked_at=? WHERE id=? AND status='active' RETURNING executable_path,executable_sha256,client_secret_sha256")
        .bind(&now)
        .bind(&now)
        .bind(id)
        .fetch_optional(&db.pool)
        .await?;
    let Some(revoked) = revoked else {
        return Err(AppError::NotFound(format!("活动 MCP 客户端 {id}")));
    };
    let executable_path = revoked.get::<Option<String>, _>("executable_path");
    let executable_sha256 = revoked.get::<Option<String>, _>("executable_sha256");
    let client_secret_sha256 = revoked.get::<Option<String>, _>("client_secret_sha256");
    revoke_runtime_access(manager, sessions, id);

    let mut cleanup_failed = false;
    let local_grant_ids = match sqlx::query_scalar::<_, String>(
        "SELECT id FROM mcp_local_grants WHERE client_id=? AND revoked_at IS NULL",
    )
    .bind(id)
    .fetch_all(&db.pool)
    .await
    {
        Ok(ids) => ids,
        Err(_) => {
            cleanup_failed = true;
            Vec::new()
        }
    };
    if sqlx::query(
        "UPDATE mcp_local_grants SET revoked_at=? WHERE client_id=? AND revoked_at IS NULL",
    )
    .bind(&now)
    .bind(id)
    .execute(&db.pool)
    .await
    .is_err()
    {
        cleanup_failed = true;
    }
    for grant_id in local_grant_ids {
        let cleaned = crate::bookmark::delete_mcp_local_grant(&grant_id).is_ok()
            && sqlx::query("DELETE FROM mcp_local_grants WHERE id=? AND client_id=?")
                .bind(&grant_id)
                .bind(id)
                .execute(&db.pool)
                .await
                .is_ok();
        if !cleaned {
            cleanup_failed = true;
        }
    }
    if let (Some(path), Some(executable_digest), Some(secret_digest)) = (
        executable_path.as_deref(),
        executable_sha256.as_deref(),
        client_secret_sha256.as_deref(),
    ) {
        match cleanup_sidecar_client_secret(app, id, path, executable_digest, secret_digest).await {
            Ok(()) => {
                if sqlx::query(
                    "UPDATE mcp_clients SET client_secret_sha256=NULL WHERE id=? AND status='revoked'",
                )
                .bind(id)
                .execute(&db.pool)
                .await
                .is_err()
                {
                    cleanup_failed = true;
                }
            }
            Err(_) => cleanup_failed = true,
        }
    }
    let outcome = if cleanup_failed {
        "revoked-cleanup-pending"
    } else {
        "revoked"
    };
    if append_audit(
        db,
        AuditInput::event("client", "medium", outcome).client(id),
    )
    .await
    .is_err()
    {
        tracing::error!("MCP 客户端撤销审计写入失败");
    }
    if cleanup_failed {
        tracing::warn!("MCP 客户端已撤销，但部分凭据或授权材料仍待清理");
    }
    Ok(())
}

async fn cleanup_sidecar_client_secret(
    app: &AppHandle,
    client_id: &str,
    registered_path: &str,
    registered_executable_sha256: &str,
    expected_secret_sha256: &str,
) -> AppResult<()> {
    if !valid_sha256_digest(expected_secret_sha256) {
        return Err(AppError::PermissionDenied(
            "MCP sidecar 登记摘要无效".into(),
        ));
    }
    let managed_sidecar = sidecar_path(app)?.canonicalize()?;
    let registered_sidecar = PathBuf::from(registered_path).canonicalize()?;
    if managed_sidecar != registered_sidecar {
        return Err(AppError::PermissionDenied(
            "MCP sidecar 登记路径已经变化".into(),
        ));
    }
    let actual_executable_sha256 = format!(
        "sha256:{:x}",
        Sha256::digest(std::fs::read(&managed_sidecar)?)
    );
    if !cleanup_sidecar_digest_is_trusted(
        registered_executable_sha256,
        &actual_executable_sha256,
        BUNDLED_SIDECAR_SHA256,
    ) {
        return Err(AppError::PermissionDenied(
            "MCP sidecar 不是当前主程序构建时绑定的受管资源".into(),
        ));
    }
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::process::Command::new(&managed_sidecar)
            .args([
                "--revoke-client-secret",
                client_id,
                "--expected-sha256",
                expected_secret_sha256,
            ])
            .output(),
    )
    .await
    .map_err(|_| AppError::Unavailable("MCP sidecar 凭据清理超时".into()))?
    .map_err(|error| AppError::Unavailable(format!("MCP sidecar 凭据清理失败：{error}")))?;
    if !output.status.success() || !output.stdout.is_empty() {
        return Err(AppError::Unavailable(
            "MCP sidecar 无法清理客户端凭据".into(),
        ));
    }
    Ok(())
}

async fn retry_revoked_client_cleanup(app: &AppHandle, db: &Database) {
    let rows = match sqlx::query(
        "SELECT id,executable_path,executable_sha256,client_secret_sha256 FROM mcp_clients WHERE status='revoked' ORDER BY revoked_at DESC LIMIT 256",
    )
    .fetch_all(&db.pool)
    .await
    {
        Ok(rows) => rows,
        Err(_) => {
            tracing::warn!("MCP 已撤销客户端清理扫描失败");
            return;
        }
    };
    for row in rows {
        let client_id = row.get::<String, _>("id");
        let mut cleanup_failed = false;
        let mut had_cleanup_work = false;
        match sqlx::query_scalar::<_, String>("SELECT id FROM mcp_local_grants WHERE client_id=?")
            .bind(&client_id)
            .fetch_all(&db.pool)
            .await
        {
            Ok(grant_ids) => {
                had_cleanup_work |= !grant_ids.is_empty();
                for grant_id in grant_ids {
                    let cleaned = crate::bookmark::delete_mcp_local_grant(&grant_id).is_ok()
                        && sqlx::query("DELETE FROM mcp_local_grants WHERE id=? AND client_id=?")
                            .bind(&grant_id)
                            .bind(&client_id)
                            .execute(&db.pool)
                            .await
                            .is_ok();
                    if !cleaned {
                        cleanup_failed = true;
                    }
                }
            }
            Err(_) => cleanup_failed = true,
        }
        let executable_path = row.get::<Option<String>, _>("executable_path");
        let executable_sha256 = row.get::<Option<String>, _>("executable_sha256");
        let client_secret_sha256 = row.get::<Option<String>, _>("client_secret_sha256");
        if let (Some(path), Some(executable_digest), Some(secret_digest)) = (
            executable_path.as_deref(),
            executable_sha256.as_deref(),
            client_secret_sha256.as_deref(),
        ) {
            had_cleanup_work = true;
            if cleanup_sidecar_client_secret(
                app,
                &client_id,
                path,
                executable_digest,
                secret_digest,
            )
            .await
            .is_ok()
            {
                if sqlx::query(
                    "UPDATE mcp_clients SET client_secret_sha256=NULL WHERE id=? AND status='revoked'",
                )
                .bind(&client_id)
                .execute(&db.pool)
                .await
                .is_err()
                {
                    cleanup_failed = true;
                }
            } else {
                cleanup_failed = true;
            }
        }
        if had_cleanup_work
            && !cleanup_failed
            && append_audit(
                db,
                AuditInput::event("client", "info", "revoked-cleanup-completed").client(&client_id),
            )
            .await
            .is_err()
        {
            tracing::error!("MCP 已撤销客户端清理审计写入失败");
        }
    }
}

fn revoke_runtime_access(manager: &McpManager, sessions: &SessionManager, id: &str) {
    let session_ids = {
        let runtime = manager.inner.lock();
        runtime
            .sessions
            .iter()
            .filter(|(_, session)| session.client_id == id)
            .map(|(session_id, _)| session_id.clone())
            .collect::<Vec<_>>()
    };
    {
        let mut runtime = manager.inner.lock();
        for session_id in session_ids {
            runtime.sessions.remove(&session_id);
            sessions.remove_external(&session_id);
        }
        let approvals = runtime
            .approvals
            .iter()
            .filter(|(_, pending)| pending.view.client_id == id)
            .map(|(approval_id, _)| approval_id.clone())
            .collect::<Vec<_>>();
        for approval_id in approvals {
            if let Some(mut pending) = runtime.approvals.remove(&approval_id)
                && let Some(decision) = pending.decision.take()
            {
                let _ = decision.send(ApprovalDecision::Reject);
            }
        }
        runtime.session_rules.retain(|key| key.client_id != id);
        runtime
            .confirmed_connections
            .retain(|(client_id, _)| client_id != id);
    }
    manager.cancel_client_requests(id);
}

pub async fn list_local_grants(db: &Database, client_id: &str) -> AppResult<Vec<McpLocalGrant>> {
    get_client(db, client_id).await?;
    let rows = sqlx::query("SELECT id,client_id,direction,display_name,path_hint,persistent,created_at,expires_at,revoked_at FROM mcp_local_grants WHERE client_id=? ORDER BY created_at DESC")
        .bind(client_id)
        .fetch_all(&db.pool)
        .await?;
    Ok(rows.into_iter().map(local_grant_from_row).collect())
}

fn local_grant_from_row(row: sqlx::sqlite::SqliteRow) -> McpLocalGrant {
    McpLocalGrant {
        id: row.get("id"),
        client_id: row.get("client_id"),
        direction: row.get("direction"),
        display_name: row.get("display_name"),
        path_hint: row.get("path_hint"),
        persistent: row.get::<i64, _>("persistent") != 0,
        created_at: row.get("created_at"),
        expires_at: row.get("expires_at"),
        revoked_at: row.get("revoked_at"),
    }
}

pub async fn create_local_grant(
    app: &AppHandle,
    db: &Database,
    client_id: &str,
    direction: &str,
    selection: &str,
    persistent: bool,
) -> AppResult<Option<McpLocalGrant>> {
    let client = get_client(db, client_id).await?;
    if client.status != "active" {
        return Err(AppError::PermissionDenied("MCP 客户端已经撤销".into()));
    }
    if !["upload", "download"].contains(&direction) {
        return Err(AppError::Validation("MCP 本地授权方向无效".into()));
    }
    if !["file", "directory"].contains(&selection)
        || direction == "download" && selection != "directory"
    {
        return Err(AppError::Validation(
            "上传可授权文件或文件夹，下载只能授权目标文件夹".into(),
        ));
    }
    let (sender, receiver) = oneshot::channel();
    let dialog = app.dialog().file().set_title(if direction == "upload" {
        "选择允许 MCP 上传的本地项目"
    } else {
        "选择允许 MCP 写入的下载文件夹"
    });
    if selection == "file" {
        dialog.pick_file(move |path| {
            let _ = sender.send(path);
        });
    } else {
        dialog.pick_folder(move |path| {
            let _ = sender.send(path);
        });
    }
    let Some(selected) = receiver
        .await
        .map_err(|_| AppError::Unavailable("本地路径选择器已关闭".into()))?
    else {
        return Ok(None);
    };
    let path = selected
        .into_path()
        .map_err(|_| AppError::Validation("选择的本地路径无效".into()))?;
    let metadata = std::fs::symlink_metadata(&path)?;
    if selection == "file" && !metadata.is_file() || selection == "directory" && !metadata.is_dir()
    {
        return Err(AppError::Validation("选择的本地路径类型不匹配".into()));
    }
    let id = Uuid::new_v4().to_string();
    crate::bookmark::save_mcp_local_grant(&id, &path, direction == "upload")?;
    let now = Utc::now();
    let display_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "已授权文件夹".into());
    let expires_at = (!persistent).then(|| (now + ChronoDuration::hours(24)).to_rfc3339());
    if let Err(error) = sqlx::query("INSERT INTO mcp_local_grants(id,client_id,direction,display_name,path_hint,persistent,created_at,expires_at) VALUES(?,?,?,?,?,?,?,?)")
        .bind(&id)
        .bind(client_id)
        .bind(direction)
        .bind(&display_name)
        .bind(&display_name)
        .bind(if persistent { 1_i64 } else { 0_i64 })
        .bind(now.to_rfc3339())
        .bind(expires_at)
        .execute(&db.pool)
        .await
    {
        let _ = crate::bookmark::delete_mcp_local_grant(&id);
        return Err(error.into());
    }
    append_audit(
        db,
        AuditInput::event("local-grant", "medium", "created")
            .client(client_id)
            .target(&format!("{direction}:{selection}")),
    )
    .await?;
    Ok(list_local_grants(db, client_id)
        .await?
        .into_iter()
        .find(|grant| grant.id == id))
}

pub async fn revoke_local_grant(db: &Database, id: &str) -> AppResult<()> {
    let row =
        sqlx::query("SELECT client_id FROM mcp_local_grants WHERE id=? AND revoked_at IS NULL")
            .bind(id)
            .fetch_optional(&db.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("活动 MCP 本地授权 {id}")))?;
    let client_id: String = row.get("client_id");
    sqlx::query("UPDATE mcp_local_grants SET revoked_at=? WHERE id=? AND revoked_at IS NULL")
        .bind(Utc::now().to_rfc3339())
        .bind(id)
        .execute(&db.pool)
        .await?;
    crate::bookmark::delete_mcp_local_grant(id)?;
    append_audit(
        db,
        AuditInput::event("local-grant", "medium", "revoked")
            .client(&client_id)
            .target("local-capability"),
    )
    .await
}

async fn context_connection_protocol(db: &Database, id: &str) -> AppResult<String> {
    Ok(db.get_connection(id).await?.protocol)
}

async fn granted_connection_ids(
    db: &Database,
    client_id: &str,
    tool: &str,
) -> AppResult<std::collections::HashSet<String>> {
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT connection_id FROM mcp_grants WHERE client_id=? AND tool=? AND (expires_at IS NULL OR expires_at>?)",
    )
    .bind(client_id)
    .bind(tool)
    .bind(Utc::now().to_rfc3339())
    .fetch_all(&db.pool)
    .await?
    .into_iter()
    .collect())
}

pub async fn list_audit(db: &Database) -> AppResult<Vec<McpAuditEvent>> {
    let rows = sqlx::query("SELECT id,request_id,client_id,connection_id,tool,target_summary,risk,outcome,duration_ms,transferred_bytes,truncated,created_at FROM mcp_audit_events ORDER BY created_at DESC,rowid DESC LIMIT ?")
        .bind(MAX_AUDIT_EVENTS)
        .fetch_all(&db.pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| McpAuditEvent {
            id: row.get("id"),
            request_id: row.get("request_id"),
            client_id: row.get("client_id"),
            connection_id: row.get("connection_id"),
            tool: row.get("tool"),
            target_summary: row.get("target_summary"),
            risk: row.get("risk"),
            outcome: row.get("outcome"),
            duration_ms: row.get("duration_ms"),
            transferred_bytes: row.get("transferred_bytes"),
            truncated: row.get::<i64, _>("truncated") != 0,
            created_at: row.get("created_at"),
        })
        .collect())
}

pub async fn export_audit(db: &Database, path: &str) -> AppResult<usize> {
    if path.len() > 16 * 1024 {
        return Err(AppError::Validation("MCP 审计导出路径超过 16 KB".into()));
    }
    let target = Path::new(path);
    if !target.is_absolute() || target.extension().and_then(|value| value.to_str()) != Some("json")
    {
        return Err(AppError::Validation(
            "MCP 审计必须导出为绝对路径 JSON 文件".into(),
        ));
    }
    if let Ok(metadata) = std::fs::symlink_metadata(target)
        && metadata.file_type().is_symlink()
    {
        return Err(AppError::PermissionDenied(
            "拒绝覆盖符号链接 MCP 审计目标".into(),
        ));
    }
    let parent = target
        .parent()
        .filter(|value| value.is_dir())
        .ok_or_else(|| AppError::Validation("MCP 审计导出目录不存在".into()))?;
    let events = list_audit(db).await?;
    let count = events.len();
    let payload = serde_json::to_vec_pretty(&json!({
        "schemaVersion": 1,
        "exportedAt": Utc::now().to_rfc3339(),
        "events": events,
    }))
    .map_err(|error| AppError::Internal(error.to_string()))?;
    let target = target.to_path_buf();
    let temporary = parent.join(format!(".cnshell-mcp-audit-{}.tmp", Uuid::new_v4()));
    tokio::task::spawn_blocking(move || -> AppResult<()> {
        let result = (|| {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temporary)?;
            file.write_all(&payload)?;
            file.sync_all()?;
            std::fs::rename(&temporary, &target)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    })
    .await
    .map_err(|error| AppError::Internal(format!("MCP 审计导出任务失败：{error}")))??;
    append_audit(
        db,
        AuditInput::event("audit", "info", "exported").target("mcp-audit"),
    )
    .await?;
    Ok(count)
}

struct AuditInput {
    request_id: Option<String>,
    client_id: Option<String>,
    connection_id: Option<String>,
    tool: String,
    target: String,
    risk: String,
    outcome: String,
    duration_ms: Option<i64>,
    transferred_bytes: Option<i64>,
    truncated: bool,
}

impl AuditInput {
    fn event(tool: &str, risk: &str, outcome: &str) -> Self {
        Self {
            request_id: None,
            client_id: None,
            connection_id: None,
            tool: tool.into(),
            target: String::new(),
            risk: risk.into(),
            outcome: outcome.into(),
            duration_ms: None,
            transferred_bytes: None,
            truncated: false,
        }
    }

    fn request(request_id: &str, client_id: &str, tool: &str, outcome: &str) -> Self {
        Self::event(
            tool,
            if WRITE_TOOLS.contains(&tool) {
                "medium"
            } else {
                "low"
            },
            outcome,
        )
        .client(client_id)
        .request_id(request_id)
    }

    fn request_id(mut self, value: &str) -> Self {
        self.request_id = Some(value.into());
        self
    }

    fn client(mut self, value: &str) -> Self {
        self.client_id = Some(value.into());
        self
    }

    fn connection(mut self, value: &str) -> Self {
        self.connection_id = Some(value.into());
        self
    }

    fn target(mut self, value: &str) -> Self {
        self.target = value.into();
        self
    }

    fn duration(mut self, value: i64) -> Self {
        self.duration_ms = Some(value);
        self
    }

    fn transferred_bytes(mut self, value: i64) -> Self {
        self.transferred_bytes = Some(value);
        self
    }

    fn truncated(mut self, value: bool) -> Self {
        self.truncated = value;
        self
    }
}

async fn append_audit(db: &Database, input: AuditInput) -> AppResult<()> {
    let mut transaction = db.pool.begin().await?;
    sqlx::query("INSERT INTO mcp_audit_events(id,request_id,client_id,connection_id,tool,target_summary,risk,outcome,duration_ms,transferred_bytes,truncated,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(Uuid::new_v4().to_string())
        .bind(input.request_id)
        .bind(input.client_id)
        .bind(input.connection_id)
        .bind(input.tool)
        .bind(input.target)
        .bind(input.risk)
        .bind(input.outcome)
        .bind(input.duration_ms)
        .bind(input.transferred_bytes)
        .bind(if input.truncated { 1 } else { 0 })
        .bind(Utc::now().to_rfc3339())
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DELETE FROM mcp_audit_events WHERE id NOT IN (SELECT id FROM mcp_audit_events ORDER BY created_at DESC,rowid DESC LIMIT ?)")
        .bind(MAX_AUDIT_EVENTS)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}

async fn audit_approval(
    db: &Database,
    request: &BrokerRequest,
    client: &McpClient,
    connection_id: &str,
    risk: &str,
    target: &str,
    decision: ApprovalDecision,
) -> AppResult<()> {
    append_audit(
        db,
        AuditInput::event(&request.tool, risk, decision.audit_outcome())
            .request_id(&request.request_id)
            .client(&client.id)
            .connection(connection_id)
            .target(target),
    )
    .await
}

async fn persistent_rule_exists(db: &Database, key: &ApprovalRuleKey) -> AppResult<bool> {
    let exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM mcp_approval_rules WHERE client_id=? AND connection_id=? AND tool=? AND target_key=?",
    )
    .bind(&key.client_id)
    .bind(&key.connection_id)
    .bind(&key.tool)
    .bind(&key.target_key)
    .fetch_one(&db.pool)
    .await?;
    if exists > 0 {
        sqlx::query("UPDATE mcp_approval_rules SET last_used_at=? WHERE client_id=? AND connection_id=? AND tool=? AND target_key=?")
            .bind(Utc::now().to_rfc3339())
            .bind(&key.client_id)
            .bind(&key.connection_id)
            .bind(&key.tool)
            .bind(&key.target_key)
            .execute(&db.pool)
            .await?;
    }
    Ok(exists > 0)
}

async fn save_persistent_rule(
    db: &Database,
    key: &ApprovalRuleKey,
    command: &str,
) -> AppResult<()> {
    if key.tool != "cnshell_run_command"
        || !can_save_command_rule(command)
        || key.target_key != command_summary(command)
    {
        return Err(AppError::PermissionDenied(
            "只有低风险精确命令可以保存 MCP 规则".into(),
        ));
    }
    let exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM mcp_approval_rules WHERE client_id=? AND connection_id=? AND tool=? AND target_key=?",
    )
    .bind(&key.client_id)
    .bind(&key.connection_id)
    .bind(&key.tool)
    .bind(&key.target_key)
    .fetch_one(&db.pool)
    .await?;
    if exists == 0 {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM mcp_approval_rules WHERE client_id=?")
                .bind(&key.client_id)
                .fetch_one(&db.pool)
                .await?;
        if count >= MAX_MCP_APPROVAL_RULES_PER_CLIENT {
            return Err(AppError::Validation(
                "MCP 客户端精确规则已达到 256 条上限，请先撤销不再使用的规则".into(),
            ));
        }
    }
    let now = Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO mcp_approval_rules(id,client_id,connection_id,tool,target_key,created_at,last_used_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(client_id,connection_id,tool,target_key) DO UPDATE SET last_used_at=excluded.last_used_at")
        .bind(Uuid::new_v4().to_string())
        .bind(&key.client_id)
        .bind(&key.connection_id)
        .bind(&key.tool)
        .bind(&key.target_key)
        .bind(&now)
        .bind(&now)
        .execute(&db.pool)
        .await?;
    append_audit(
        db,
        AuditInput::event("approval-rule", "low", "created")
            .client(&key.client_id)
            .connection(&key.connection_id)
            .target(&key.target_key),
    )
    .await
}

pub async fn list_approval_rules(
    db: &Database,
    client_id: &str,
) -> AppResult<Vec<McpApprovalRule>> {
    let client = get_client(db, client_id).await?;
    if client.status != "active" {
        return Err(AppError::PermissionDenied("MCP 客户端已经撤销".into()));
    }
    let rows = sqlx::query(
        "SELECT r.id,r.client_id,r.connection_id,c.name AS connection_name,r.tool,r.target_key,r.created_at,r.last_used_at \
         FROM mcp_approval_rules r JOIN connections c ON c.id=r.connection_id \
         WHERE r.client_id=? ORDER BY r.created_at DESC,r.id DESC LIMIT ?",
    )
    .bind(client_id)
    .bind(MAX_MCP_APPROVAL_RULES_PER_CLIENT)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| McpApprovalRule {
            id: row.get("id"),
            client_id: row.get("client_id"),
            connection_id: row.get("connection_id"),
            connection_name: row.get("connection_name"),
            tool: row.get("tool"),
            target_summary: row.get("target_key"),
            created_at: row.get("created_at"),
            last_used_at: row.get("last_used_at"),
        })
        .collect())
}

pub async fn revoke_approval_rule(db: &Database, id: &str) -> AppResult<()> {
    Uuid::parse_str(id).map_err(|_| AppError::Validation("MCP 精确规则 ID 无效".into()))?;
    let row =
        sqlx::query("SELECT client_id,connection_id,target_key FROM mcp_approval_rules WHERE id=?")
            .bind(id)
            .fetch_optional(&db.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("MCP 精确规则 {id}")))?;
    let client_id: String = row.get("client_id");
    let connection_id: String = row.get("connection_id");
    let target_key: String = row.get("target_key");
    sqlx::query("DELETE FROM mcp_approval_rules WHERE id=?")
        .bind(id)
        .execute(&db.pool)
        .await?;
    append_audit(
        db,
        AuditInput::event("approval-rule", "medium", "revoked")
            .client(&client_id)
            .connection(&connection_id)
            .target(&target_key),
    )
    .await
}

pub async fn client_config(
    app: &AppHandle,
    manager: &McpManager,
    db: &Database,
    client: &McpClient,
) -> AppResult<McpClientConfig> {
    if client.status != "active" {
        return Err(AppError::PermissionDenied("MCP 客户端已经撤销".into()));
    }
    let command = sidecar_path(app)?;
    let args = vec![
        "--client-id".into(),
        client.id.clone(),
        "--client-name".into(),
        client.name.clone(),
        "--discovery".into(),
        manager.discovery_path().to_string_lossy().into_owned(),
    ];
    let command = command.canonicalize()?;
    let executable_bytes = std::fs::read(&command)?;
    let executable_sha256 = format!("sha256:{:x}", Sha256::digest(&executable_bytes));
    let command_string = command.to_string_lossy().into_owned();
    let provisioned = tokio::process::Command::new(&command)
        .args(["--provision-client-secret", &client.id])
        .output()
        .await
        .map_err(|error| AppError::Unavailable(format!("MCP sidecar 凭据初始化失败：{error}")))?;
    if !provisioned.status.success() {
        return Err(AppError::Unavailable(
            "MCP sidecar 无法初始化客户端凭据".into(),
        ));
    }
    let client_secret_sha256 = String::from_utf8(provisioned.stdout)
        .map_err(|_| AppError::Storage("MCP sidecar 凭据摘要格式无效".into()))?;
    let client_secret_sha256 = client_secret_sha256.trim();
    if client_secret_sha256.len() != 71
        || !client_secret_sha256.starts_with("sha256:")
        || !client_secret_sha256[7..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(AppError::Storage("MCP sidecar 凭据摘要格式无效".into()));
    }
    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE mcp_clients SET executable_path=?,executable_sha256=?,client_secret_sha256=?,updated_at=? WHERE id=? AND status='active'")
        .bind(&command_string)
        .bind(&executable_sha256)
        .bind(client_secret_sha256)
        .bind(&now)
        .bind(&client.id)
        .execute(&db.pool)
        .await?;
    append_audit(
        db,
        AuditInput::event("client", "info", "executable-bound")
            .client(&client.id)
            .target("cnshell-mcp-sidecar"),
    )
    .await?;
    let args_json =
        serde_json::to_string(&args).map_err(|error| AppError::Internal(error.to_string()))?;
    let command_json = serde_json::to_string(&command_string)
        .map_err(|error| AppError::Internal(error.to_string()))?;
    Ok(McpClientConfig {
        client_id: client.id.clone(),
        client_name: client.name.clone(),
        command: command_string,
        args: args.clone(),
        codex_toml: format!(
            "[mcp_servers.cnshell]\ncommand = {command_json}\nargs = {args_json}\n"
        ),
        json: serde_json::to_string_pretty(&json!({
            "mcpServers": { "cnshell": { "command": command, "args": args } }
        }))
        .map_err(|error| AppError::Internal(error.to_string()))?,
    })
}

fn sidecar_path(app: &AppHandle) -> AppResult<PathBuf> {
    let executable = if cfg!(target_os = "windows") {
        "cnshell-mcp.exe"
    } else {
        "cnshell-mcp"
    };
    let resource = app
        .path()
        .resource_dir()
        .map_err(|error| AppError::Unavailable(error.to_string()))?
        .join("mcp")
        .join(executable);
    if resource.is_file() {
        return Ok(resource);
    }
    let current = std::env::current_exe()?;
    let development = current
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(executable);
    if development.is_file() {
        Ok(development)
    } else {
        Err(AppError::Unavailable(
            "未找到 cnshell-mcp sidecar，请先构建桌面资源".into(),
        ))
    }
}

pub async fn broker_call(
    client_id: &str,
    client_name: &str,
    discovery_path: &Path,
    tool: &str,
    arguments: serde_json::Map<String, Value>,
    cancellation: CancellationToken,
) -> AppResult<Value> {
    if !known_tool(tool) {
        return Err(AppError::Validation("未知 MCP 工具".into()));
    }
    broker_operation_call(
        client_id,
        client_name,
        discovery_path,
        tool,
        arguments,
        cancellation,
    )
    .await
}

pub async fn broker_resource_call(
    client_id: &str,
    client_name: &str,
    discovery_path: &Path,
    uri: &str,
    cancellation: CancellationToken,
) -> AppResult<Value> {
    let operation = match uri {
        "cnshell://connections" => RESOURCE_CONNECTIONS_OPERATION,
        "cnshell://audit/recent" => RESOURCE_AUDIT_OPERATION,
        _ => return Err(AppError::NotFound("未知 CNshell MCP Resource".into())),
    };
    broker_operation_call(
        client_id,
        client_name,
        discovery_path,
        operation,
        serde_json::Map::new(),
        cancellation,
    )
    .await
}

async fn broker_operation_call(
    client_id: &str,
    client_name: &str,
    discovery_path: &Path,
    operation: &str,
    arguments: serde_json::Map<String, Value>,
    cancellation: CancellationToken,
) -> AppResult<Value> {
    if !known_broker_operation(operation) {
        return Err(AppError::Validation("未知 MCP Broker 操作".into()));
    }
    let metadata = std::fs::symlink_metadata(discovery_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            AppError::Unavailable(
                "CNshell MCP Broker 未运行，请打开 CNshell 并在设置中启用 MCP".into(),
            )
        } else {
            error.into()
        }
    })?;
    if metadata.file_type().is_symlink() || metadata.len() > 16 * 1024 {
        return Err(AppError::PermissionDenied(
            "MCP discovery 文件类型或大小无效".into(),
        ));
    }
    let discovery: DiscoveryDocument = serde_json::from_slice(&std::fs::read(discovery_path)?)
        .map_err(|_| AppError::Storage("MCP discovery 文件损坏".into()))?;
    if discovery.schema_version != BROKER_PROTOCOL_VERSION {
        return Err(AppError::Unavailable("MCP Broker 协议版本不兼容".into()));
    }
    if discovery.broker_token.len() < 32 {
        return Err(AppError::Storage("MCP Broker discovery 凭据无效".into()));
    }
    let client_secret = load_secret(&client_account(client_id))?
        .ok_or_else(|| AppError::PermissionDenied("MCP 客户端未登记或已撤销".into()))?;
    let executable = std::env::current_exe()?.canonicalize()?;
    let executable_bytes = std::fs::read(&executable)?;
    let request = BrokerRequest {
        protocol_version: BROKER_PROTOCOL_VERSION,
        generation: discovery.generation.clone(),
        broker_token: discovery.broker_token.clone(),
        client_id: client_id.into(),
        client_secret,
        client_name: client_name.into(),
        executable_path: Some(executable.to_string_lossy().into_owned()),
        executable_sha256: Some(format!("sha256:{:x}", Sha256::digest(&executable_bytes))),
        request_id: Uuid::new_v4().to_string(),
        tool: operation.into(),
        arguments,
    };
    let encoded =
        serde_json::to_vec(&request).map_err(|error| AppError::Internal(error.to_string()))?;
    if encoded.len() > MAX_BROKER_MESSAGE_BYTES {
        return Err(AppError::Validation("MCP Broker 请求超过 1 MiB".into()));
    }
    let address: std::net::SocketAddr = discovery
        .address
        .parse()
        .map_err(|_| AppError::Storage("MCP Broker 地址无效".into()))?;
    if !address.ip().is_loopback() {
        return Err(AppError::PermissionDenied(
            "MCP Broker 地址不是本机回环地址".into(),
        ));
    }
    let mut stream = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(address))
        .await
        .map_err(|_| AppError::Unavailable("连接 CNshell MCP Broker 超时".into()))??;
    stream.write_u32(encoded.len() as u32).await?;
    stream.write_all(&encoded).await?;
    let length = tokio::select! {
        result = stream.read_u32() => result? as usize,
        _ = cancellation.cancelled() => {
            let _ = stream.shutdown().await;
            return Err(AppError::Unavailable("MCP 请求已取消".into()));
        }
    };
    if length == 0 || length > MAX_BROKER_MESSAGE_BYTES {
        return Err(AppError::Storage("MCP Broker 响应大小无效".into()));
    }
    let mut response = vec![0_u8; length];
    tokio::select! {
        result = stream.read_exact(&mut response) => { result?; }
        _ = cancellation.cancelled() => {
            let _ = stream.shutdown().await;
            return Err(AppError::Unavailable("MCP 请求已取消".into()));
        }
    }
    let response: BrokerResponse = serde_json::from_slice(&response)
        .map_err(|_| AppError::Storage("MCP Broker 响应格式无效".into()))?;
    if response.ok {
        response
            .result
            .ok_or_else(|| AppError::Internal("MCP Broker 成功响应缺少结果".into()))
    } else {
        let error = response.error.unwrap_or(crate::mcp_protocol::BrokerError {
            code: "internal".into(),
            message: "MCP Broker 请求失败".into(),
        });
        Err(match error.code.as_str() {
            "not_found" => AppError::NotFound(error.message),
            "invalid_params" => AppError::Validation(error.message),
            "authentication" => AppError::Authentication(error.message),
            "permission_denied" => AppError::PermissionDenied(error.message),
            "remote" => AppError::Remote(error.message),
            "unavailable" => AppError::Unavailable(error.message),
            _ => AppError::Internal(error.message),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn remote_paths_are_normalized_and_bounded() {
        assert!(matches!(
            normalize_remote_path("/var/log/../secret"),
            Err(AppError::Validation(_))
        ));
        assert_eq!(normalize_remote_path("//var///log").unwrap(), "/var/log");
        assert!(remote_path_is_within("/var/log/app.log", "/var/log"));
        assert!(!remote_path_is_within("/var/logger", "/var/log"));
    }

    #[test]
    fn token_comparison_and_cursors_are_strict() {
        assert!(constant_time_equal("same", "same"));
        assert!(!constant_time_equal("same", "different"));
        assert_eq!(decode_cursor(Some("offset:12")).unwrap(), 12);
        assert!(decode_cursor(Some("12")).is_err());
    }

    #[test]
    fn upgraded_sidecar_can_clean_old_credentials_but_tampering_is_rejected() {
        let old = format!("sha256:{}", "1".repeat(64));
        let current = format!("sha256:{}", "2".repeat(64));
        let tampered = format!("sha256:{}", "3".repeat(64));

        assert!(cleanup_sidecar_digest_is_trusted(&old, &current, &current));
        assert!(!cleanup_sidecar_digest_is_trusted(
            &old, &tampered, &current
        ));
        assert!(!cleanup_sidecar_digest_is_trusted(
            "unavailable",
            &current,
            &current
        ));
    }

    #[test]
    fn command_audit_uses_hash_instead_of_command_text() {
        let summary = command_summary("echo very-secret-value");
        assert!(summary.starts_with("command:sha256:"));
        assert!(!summary.contains("very-secret-value"));
    }

    #[test]
    fn only_conservative_commands_can_save_persistent_rules() {
        assert_eq!(command_risk("pwd"), "low");
        assert_eq!(command_risk("printf cnshell-mcp-command-ok"), "low");
        assert!(can_save_command_rule("uname -a"));

        assert_eq!(command_risk("cat /etc/hostname"), "medium");
        assert_eq!(command_risk("printf value;touch-file"), "medium");
        assert_eq!(command_risk(" printf cnshell-mcp-command-ok"), "medium");
        assert!(!can_save_command_rule("printf %s secret"));
        assert_eq!(command_risk("sudo -i"), "high");
    }

    #[tokio::test]
    async fn persistent_rules_revalidate_the_full_command_before_saving() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("cnshell.sqlite"))
            .await
            .unwrap();
        let medium = "cat /etc/hostname";
        let key = ApprovalRuleKey {
            client_id: "client".into(),
            connection_id: "connection".into(),
            tool: "cnshell_run_command".into(),
            target_key: command_summary(medium),
        };

        assert!(save_persistent_rule(&db, &key, medium).await.is_err());
        assert!(save_persistent_rule(&db, &key, "pwd").await.is_err());
    }

    #[test]
    fn write_tools_are_not_classified_as_read_only() {
        assert!(READ_TOOLS.contains(&"cnshell_file_read"));
        assert!(WRITE_TOOLS.contains(&"cnshell_file_delete"));
        assert!(!READ_TOOLS.contains(&"cnshell_run_command"));
        assert!(known_broker_operation(RESOURCE_CONNECTIONS_OPERATION));
        assert!(known_broker_operation(RESOURCE_AUDIT_OPERATION));
        assert!(!known_tool(RESOURCE_CONNECTIONS_OPERATION));
        assert!(!known_tool(RESOURCE_AUDIT_OPERATION));
    }

    #[test]
    fn local_grant_paths_reject_absolute_and_parent_paths() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("source.txt"), b"content").unwrap();

        assert!(resolve_local_grant_path(directory.path(), "../outside", false).is_err());
        assert!(resolve_local_grant_path(directory.path(), "/etc/passwd", false).is_err());
        assert_eq!(
            resolve_local_grant_path(directory.path(), "source.txt", false).unwrap(),
            directory.path().join("source.txt")
        );
        assert_eq!(
            resolve_local_grant_path(directory.path(), "new.txt", true).unwrap(),
            directory.path().join("new.txt")
        );
    }

    #[cfg(unix)]
    #[test]
    fn local_grant_paths_reject_symlink_components() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), b"secret").unwrap();
        symlink(outside.path(), directory.path().join("escape")).unwrap();

        let error = resolve_local_grant_path(directory.path(), "escape/secret.txt", false)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("越过授权目录") || error.contains("符号链接"),
            "unexpected error: {error}"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn local_grant_paths_reject_junction_components() {
        use std::process::Command;

        let directory = tempfile::tempdir().unwrap();
        let inside = directory.path().join("inside");
        let junction = directory.path().join("junction");
        std::fs::create_dir(&inside).unwrap();
        std::fs::write(inside.join("source.txt"), b"content").unwrap();

        let output = Command::new("cmd.exe")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(&junction)
            .arg(&inside)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "unable to create junction fixture: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let error = resolve_local_grant_path(directory.path(), "junction/source.txt", false)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("重解析点"),
            "unexpected junction error: {error}"
        );
        std::fs::remove_dir(&junction).unwrap();
    }

    #[test]
    fn execution_limit_is_released_by_raii() {
        let directory = tempfile::tempdir().unwrap();
        let manager = McpManager::new(directory.path().to_path_buf());
        let first = manager.acquire_execution("client").unwrap();
        let second = manager.acquire_execution("client").unwrap();
        assert!(manager.acquire_execution("client").is_err());

        drop(first);
        let replacement = manager.acquire_execution("client").unwrap();
        drop(second);
        drop(replacement);
        assert!(!manager.inner.lock().in_flight.contains_key("client"));
    }

    #[test]
    fn transfer_targets_are_exclusive_until_permit_drop() {
        let directory = tempfile::tempdir().unwrap();
        let manager = McpManager::new(directory.path().to_path_buf());
        let target = directory.path().join("download.txt");
        let permit = manager.acquire_transfer_target(&target).unwrap();
        assert!(manager.acquire_transfer_target(&target).is_err());
        drop(permit);
        assert!(manager.acquire_transfer_target(&target).is_ok());
    }

    #[test]
    fn duplicate_request_ids_are_rejected_and_cancel_propagates() {
        let directory = tempfile::tempdir().unwrap();
        let manager = McpManager::new(directory.path().to_path_buf());
        let cancelled = Arc::new(AtomicBool::new(false));
        let _permit = manager
            .register_request("request", "client", cancelled.clone())
            .unwrap();
        assert!(
            manager
                .register_request("request", "client", Arc::new(AtomicBool::new(false)))
                .is_err()
        );
        manager.cancel_request("request");
        assert!(cancelled.load(Ordering::Acquire));
    }

    #[test]
    fn bounded_directory_response_shortens_pages_without_breaking_cursor() {
        let entry = crate::models::RemoteFile {
            name: "x".repeat(8 * 1024),
            path: "/tmp/entry".into(),
            kind: "file".into(),
            size: 1,
            modified_at: None,
            permissions: "-rw-------".into(),
            owner: None,
            group: None,
        };
        let response =
            bounded_file_list_response("request", "/tmp", vec![entry; 500], 0, 500).unwrap();
        let encoded = serde_json::to_vec(&response).unwrap();
        assert!(encoded.len() <= MAX_MCP_DIRECTORY_RESPONSE_BYTES);
        let next = response["nextCursor"].as_str().unwrap();
        let offset = decode_cursor(Some(next)).unwrap();
        assert!((1..500).contains(&offset));
    }

    #[tokio::test]
    async fn cancelling_request_rejects_and_removes_matching_approval() {
        let directory = tempfile::tempdir().unwrap();
        let manager = McpManager::new(directory.path().to_path_buf());
        let (sender, receiver) = oneshot::channel();
        manager.inner.lock().approvals.insert(
            "approval".into(),
            PendingApproval {
                view: McpApproval {
                    id: "approval".into(),
                    request_id: "request".into(),
                    client_id: "client".into(),
                    client_name: "test".into(),
                    connection_id: "connection".into(),
                    connection_name: "server".into(),
                    tool: "cnshell_run_command".into(),
                    risk: "medium".into(),
                    target: "command".into(),
                    preview: "uptime".into(),
                    can_allow_session: false,
                    can_save_rule: false,
                    created_at: Utc::now().to_rfc3339(),
                    expires_at: (Utc::now() + ChronoDuration::seconds(120)).to_rfc3339(),
                },
                rule_key: None,
                decision: Some(sender),
            },
        );

        manager.cancel_request("request");
        assert!(!manager.inner.lock().approvals.contains_key("approval"));
        assert_eq!(receiver.await.unwrap(), ApprovalDecision::Reject);
    }

    #[test]
    fn auth_failures_back_off_and_success_clears_state() {
        let directory = tempfile::tempdir().unwrap();
        let manager = McpManager::new(directory.path().to_path_buf());
        manager.record_auth_failure("client");
        assert!(manager.check_auth_backoff("client").is_err());
        manager.clear_auth_failure("client");
        assert!(manager.check_auth_backoff("client").is_ok());
    }

    #[test]
    fn approval_decisions_cannot_expand_ineligible_requests() {
        let directory = tempfile::tempdir().unwrap();
        let manager = McpManager::new(directory.path().to_path_buf());
        let (sender, _receiver) = oneshot::channel();
        manager.inner.lock().approvals.insert(
            "approval".into(),
            PendingApproval {
                view: McpApproval {
                    id: "approval".into(),
                    request_id: "request".into(),
                    client_id: "client".into(),
                    client_name: "test".into(),
                    connection_id: "connection".into(),
                    connection_name: "server".into(),
                    tool: "cnshell_file_delete".into(),
                    risk: "high".into(),
                    target: "/tmp/data".into(),
                    preview: "recursive delete".into(),
                    can_allow_session: false,
                    can_save_rule: false,
                    created_at: Utc::now().to_rfc3339(),
                    expires_at: (Utc::now() + ChronoDuration::seconds(120)).to_rfc3339(),
                },
                rule_key: None,
                decision: Some(sender),
            },
        );
        assert!(manager.decide("approval", "session").is_err());
        assert!(manager.decide("approval", "persistent").is_err());
        assert!(manager.decide("approval", "once").is_ok());
    }

    #[test]
    fn file_previews_are_bounded_and_line_prefixed() {
        let preview = prefixed_preview_lines("first\nsecond", "+ ");
        assert_eq!(preview, "+ first\n+ second");
        let bounded = bounded_preview(&"a".repeat(40_000), 16 * 1024);
        assert!(bounded.len() < 17 * 1024);
        assert!(bounded.contains("内容已截断"));
    }

    #[tokio::test]
    async fn audit_export_is_private_atomic_and_rejects_symlink_targets() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("cnshell.sqlite"))
            .await
            .unwrap();
        append_audit(
            &db,
            AuditInput::event("test", "info", "completed").target("metadata-only"),
        )
        .await
        .unwrap();

        let output = directory.path().join("audit.json");
        assert_eq!(
            export_audit(&db, output.to_str().unwrap()).await.unwrap(),
            1
        );
        let payload = std::fs::read_to_string(&output).unwrap();
        assert!(payload.contains("metadata-only"));
        assert!(!payload.contains("stdout"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&output).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let link = directory.path().join("audit-link.json");
            symlink(&output, &link).unwrap();
            assert!(matches!(
                export_audit(&db, link.to_str().unwrap()).await,
                Err(AppError::PermissionDenied(_))
            ));
        }
    }

    #[tokio::test]
    async fn broker_stop_is_idempotent_and_removes_discovery() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("cnshell.sqlite"))
            .await
            .unwrap();
        let manager = McpManager::new(directory.path().to_path_buf());
        write_discovery(
            &manager.discovery_path(),
            &DiscoveryDocument {
                schema_version: BROKER_PROTOCOL_VERSION,
                address: "127.0.0.1:12345".into(),
                generation: "generation".into(),
                broker_token: "broker-token-with-at-least-32-characters".into(),
                process_id: 42,
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .unwrap();
        manager.inner.lock().running = true;

        let sessions = SessionManager::default();
        manager.stop(&sessions, &db).await.unwrap();
        manager.stop(&sessions, &db).await.unwrap();

        assert!(!manager.discovery_path().exists());
        let stopped: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM mcp_audit_events WHERE tool='broker' AND outcome='stopped'",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(stopped, 1);
    }

    #[test]
    fn final_manager_drop_removes_discovery() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mcp-broker.json");
        let manager = McpManager::new(directory.path().to_path_buf());
        write_discovery(
            &path,
            &DiscoveryDocument {
                schema_version: BROKER_PROTOCOL_VERSION,
                address: "127.0.0.1:12345".into(),
                generation: "generation".into(),
                broker_token: "broker-token-with-at-least-32-characters".into(),
                process_id: 42,
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .unwrap();
        let clone = manager.clone();
        drop(clone);
        assert!(path.exists());
        drop(manager);
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn client_hostname_visibility_and_exact_rules_are_scoped_and_revoked() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("cnshell.sqlite"))
            .await
            .unwrap();
        let connection = crate::models::SaveConnectionInput {
            id: "connection".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "server".into(),
            host: "example.test".into(),
            port: 22,
            username: "ubuntu".into(),
            auth_type: "sshAgent".into(),
            private_key_path: None,
            certificate_path: None,
            host_key_policy: "strict".into(),
            note: String::new(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            credential: None,
        };
        db.save_connection(&connection, None).await.unwrap();
        let client = create_client(&db, "test").await.unwrap();
        let input = McpClientGrantInput {
            client_id: client.id.clone(),
            connection_ids: vec![connection.id.clone()],
            tools: vec!["cnshell_run_command".into()],
            remote_root: "/tmp".into(),
            show_hostnames: true,
        };
        let client = save_grants(&db, &input).await.unwrap();
        assert!(client.show_hostnames);
        let key = ApprovalRuleKey {
            client_id: client.id.clone(),
            connection_id: connection.id.clone(),
            tool: "cnshell_run_command".into(),
            target_key: command_summary("uptime"),
        };
        save_persistent_rule(&db, &key, "uptime").await.unwrap();
        assert!(persistent_rule_exists(&db, &key).await.unwrap());

        let updated = McpClientGrantInput {
            show_hostnames: false,
            ..input
        };
        let client = save_grants(&db, &updated).await.unwrap();
        assert!(!client.show_hostnames);
        assert!(!persistent_rule_exists(&db, &key).await.unwrap());
    }

    #[tokio::test]
    async fn exact_command_rules_can_be_listed_and_revoked_without_plaintext() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("cnshell.sqlite"))
            .await
            .unwrap();
        let connection = crate::models::SaveConnectionInput {
            id: "rule-connection".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "Rule Server".into(),
            host: "rule.example".into(),
            port: 22,
            username: "ubuntu".into(),
            auth_type: "sshAgent".into(),
            private_key_path: None,
            certificate_path: None,
            host_key_policy: "strict".into(),
            note: String::new(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            credential: None,
        };
        db.save_connection(&connection, None).await.unwrap();
        let client = create_client(&db, "rule-client").await.unwrap();
        let plaintext = "printf never-store-this-command";
        let key = ApprovalRuleKey {
            client_id: client.id.clone(),
            connection_id: connection.id.clone(),
            tool: "cnshell_run_command".into(),
            target_key: command_summary(plaintext),
        };
        save_persistent_rule(&db, &key, plaintext).await.unwrap();

        let rules = list_approval_rules(&db, &client.id).await.unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].connection_name, "Rule Server");
        assert_eq!(rules[0].target_summary, key.target_key);
        assert!(!rules[0].target_summary.contains(plaintext));

        revoke_approval_rule(&db, &rules[0].id).await.unwrap();
        assert!(
            list_approval_rules(&db, &client.id)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(!persistent_rule_exists(&db, &key).await.unwrap());
        let revoked: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM mcp_audit_events WHERE client_id=? AND tool='approval-rule' AND outcome='revoked'",
        )
        .bind(&client.id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(revoked, 1);
    }

    #[tokio::test]
    async fn dynamic_resources_follow_connection_grants_and_redact_audit_details() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("cnshell.sqlite"))
            .await
            .unwrap();
        let connection = crate::models::SaveConnectionInput {
            id: "resource-connection".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "Resource Server".into(),
            host: "resource-secret.example".into(),
            port: 22,
            username: "resource-user".into(),
            auth_type: "sshAgent".into(),
            private_key_path: None,
            certificate_path: None,
            host_key_policy: "strict".into(),
            note: "must-not-leak".into(),
            tags: vec!["resource".into()],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            credential: None,
        };
        db.save_connection(&connection, None).await.unwrap();
        let client = create_client(&db, "resource-client").await.unwrap();

        let command_only = McpClientGrantInput {
            client_id: client.id.clone(),
            connection_ids: vec![connection.id.clone()],
            tools: vec!["cnshell_run_command".into()],
            remote_root: "/tmp".into(),
            show_hostnames: false,
        };
        let client = save_grants(&db, &command_only).await.unwrap();
        let hidden = resource_connections(&db, &client).await.unwrap();
        assert_eq!(hidden["connections"].as_array().unwrap().len(), 0);

        let discoverable = McpClientGrantInput {
            tools: vec![
                "cnshell_list_connections".into(),
                "cnshell_run_command".into(),
            ],
            ..command_only
        };
        let client = save_grants(&db, &discoverable).await.unwrap();
        let visible = resource_connections(&db, &client).await.unwrap();
        let serialized_connections = serde_json::to_string(&visible).unwrap();
        assert!(serialized_connections.contains("Resource Server"));
        assert!(!serialized_connections.contains("resource-secret.example"));
        assert!(!serialized_connections.contains("resource-user"));
        assert!(!serialized_connections.contains("must-not-leak"));

        append_audit(
            &db,
            AuditInput::request(
                "request-secret-id",
                &client.id,
                "cnshell_run_command",
                "completed",
            )
            .connection(&connection.id)
            .target("command:sha256:target-secret"),
        )
        .await
        .unwrap();
        let audit = resource_recent_audit(&db, &client).await.unwrap();
        let serialized_audit = serde_json::to_string(&audit).unwrap();
        assert!(serialized_audit.contains("cnshell_run_command"));
        assert!(serialized_audit.contains("resource-connection"));
        assert!(!serialized_audit.contains("request-secret-id"));
        assert!(!serialized_audit.contains("target-secret"));
        assert!(!serialized_audit.contains("targetSummary"));
    }

    #[cfg(unix)]
    #[test]
    fn discovery_is_private_and_symlinks_are_rejected() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mcp-broker.json");
        let document = DiscoveryDocument {
            schema_version: BROKER_PROTOCOL_VERSION,
            address: "127.0.0.1:12345".into(),
            generation: "generation".into(),
            broker_token: "broker-token-with-at-least-32-characters".into(),
            process_id: 42,
            created_at: Utc::now().to_rfc3339(),
        };
        write_discovery(&path, &document).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        std::fs::remove_file(&path).unwrap();
        let outside = directory.path().join("outside.json");
        std::fs::write(&outside, b"unchanged").unwrap();
        symlink(&outside, &path).unwrap();
        assert!(write_discovery(&path, &document).is_err());
        assert_eq!(std::fs::read(&outside).unwrap(), b"unchanged");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn discovery_uses_a_protected_owner_rights_dacl() {
        use std::{os::windows::ffi::OsStrExt as _, ptr};
        use windows_sys::Win32::{
            Foundation::{ERROR_INSUFFICIENT_BUFFER, GetLastError, LocalFree},
            Security::{
                Authorization::{
                    ConvertSecurityDescriptorToStringSecurityDescriptorW, SDDL_REVISION_1,
                },
                DACL_SECURITY_INFORMATION, GetFileSecurityW, PSECURITY_DESCRIPTOR,
            },
        };

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mcp-broker.json");
        let document = DiscoveryDocument {
            schema_version: BROKER_PROTOCOL_VERSION,
            address: "127.0.0.1:12345".into(),
            generation: "generation".into(),
            broker_token: "broker-token-with-at-least-32-characters".into(),
            process_id: 42,
            created_at: Utc::now().to_rfc3339(),
        };
        write_discovery(&path, &document).unwrap();

        let path_wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut descriptor_len = 0_u32;
        let first_read = unsafe {
            GetFileSecurityW(
                path_wide.as_ptr(),
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                0,
                &mut descriptor_len,
            )
        };
        assert!(
            first_read == 0 && unsafe { GetLastError() } == ERROR_INSUFFICIENT_BUFFER,
            "unable to determine discovery ACL descriptor length"
        );

        let mut descriptor = vec![0_u8; descriptor_len as usize];
        let descriptor_ptr: PSECURITY_DESCRIPTOR = descriptor.as_mut_ptr().cast();
        let read = unsafe {
            GetFileSecurityW(
                path_wide.as_ptr(),
                DACL_SECURITY_INFORMATION,
                descriptor_ptr,
                descriptor_len,
                &mut descriptor_len,
            )
        };
        assert!(read != 0, "unable to read discovery ACL");

        let mut sddl_ptr = ptr::null_mut();
        let mut sddl_len = 0_u32;
        let converted = unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor_ptr,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &mut sddl_ptr,
                &mut sddl_len,
            )
        };
        assert!(converted != 0, "unable to serialize discovery ACL");
        let sddl = unsafe {
            let value =
                String::from_utf16_lossy(std::slice::from_raw_parts(sddl_ptr, sddl_len as usize));
            LocalFree(sddl_ptr.cast());
            value
        };
        assert!(
            sddl.contains("D:P(A;;FA;;;OW)"),
            "unexpected discovery ACL: {sddl}"
        );
    }

    #[test]
    fn cancellation_flag_is_lock_free_for_transfer_callbacks() {
        let flag = AtomicBool::new(false);
        flag.store(true, Ordering::Release);
        assert!(flag.load(Ordering::Acquire));
    }
}
