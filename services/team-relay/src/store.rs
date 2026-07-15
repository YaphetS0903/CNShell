use crate::{
    error::{RelayError, RelayResult},
    models::{
        AcceptWorkspaceInvitationInput, AccountSessionOutput, BootstrapWorkspaceInput,
        CreateDeviceSessionInput, CreateWorkspaceInvitationInput, DeviceChallengeOutput,
        DeviceRegistration, DeviceSessionOutput, LoginInput, RegisterAccountInput, RelayAuditEvent,
        UpdateMemberInput, WorkspaceDeviceView, WorkspaceInvitationOutput, WorkspaceMemberView,
        WorkspaceSnapshot,
    },
};
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng as PasswordOsRng},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration as ChronoDuration, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use rand::{RngCore, rngs::OsRng};
use sha2::{Digest, Sha256};
use sqlx::{
    Row, Sqlite, SqlitePool, Transaction,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use std::{str::FromStr, sync::OnceLock, time::Duration};

const ACCOUNT_SESSION_MINUTES: i64 = 10;
const DEVICE_SESSION_MINUTES: i64 = 15;
const DEVICE_CHALLENGE_MINUTES: i64 = 2;
const WORKSPACE_INVITATION_HOURS: i64 = 24;
const MAX_AUDIT_EVENTS: i64 = 4096;
const MAX_ACCOUNT_SESSIONS: i64 = 16;
const MAX_ACCOUNT_WORKSPACES: i64 = 32;
const MAX_DEVICE_SESSIONS: i64 = 16;
const MAX_DEVICE_CHALLENGES: i64 = 32;
const MAX_WORKSPACE_INVITATIONS: i64 = 256;
const MAX_WORKSPACE_MEMBERS: i64 = 256;

#[derive(Debug, Clone)]
pub struct AccountAuth {
    pub account_id: String,
    pub email: String,
    pub display_name: String,
}

#[derive(Debug, Clone)]
pub struct DeviceAuth {
    pub account_id: String,
    pub workspace_id: String,
    pub member_id: String,
    pub device_id: String,
    pub role: String,
    pub key_epoch: i64,
    pub token_hash: String,
}

#[derive(Clone)]
pub struct RelayStore {
    pub(crate) pool: SqlitePool,
}

impl RelayStore {
    pub async fn open(database_url: &str) -> RelayResult<Self> {
        let options = SqliteConnectOptions::from_str(database_url)
            .map_err(|_| RelayError::Validation("relay 数据库 URL 无效".into()))?
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|_| RelayError::Internal)?;
        Ok(Self { pool })
    }

    pub async fn register_account(
        &self,
        input: RegisterAccountInput,
    ) -> RelayResult<AccountSessionOutput> {
        let email = clean_email(&input.email)?;
        let display_name = clean_name(&input.display_name, "显示名称")?;
        validate_password(&input.password)?;
        let password = input.password;
        let password_hash = tokio::task::spawn_blocking(move || {
            let salt = SaltString::generate(&mut PasswordOsRng);
            Argon2::default()
                .hash_password(password.as_bytes(), &salt)
                .map(|value| value.to_string())
        })
        .await
        .map_err(|_| RelayError::Internal)?
        .map_err(|_| RelayError::Internal)?;
        let account_id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query("INSERT INTO accounts(id,email,display_name,password_hash,status,created_at,updated_at) VALUES(?,?,?,?,'active',?,?)")
            .bind(&account_id)
            .bind(&email)
            .bind(display_name)
            .bind(password_hash)
            .bind(&now)
            .bind(&now)
            .execute(&self.pool)
            .await;
        match result {
            Ok(_) => self.issue_account_session(&account_id).await,
            Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
                Err(RelayError::Conflict("该邮箱已经注册".into()))
            }
            Err(error) => Err(error.into()),
        }
    }

    pub async fn login(&self, input: LoginInput) -> RelayResult<AccountSessionOutput> {
        let email = clean_email(&input.email)?;
        let row =
            sqlx::query("SELECT id,password_hash FROM accounts WHERE email=? AND status='active'")
                .bind(email)
                .fetch_optional(&self.pool)
                .await?;
        let (account_id, password_hash) = row.map_or_else(
            || (None, dummy_password_hash().clone()),
            |row| (Some(row.get::<String, _>(0)), row.get(1)),
        );
        let password = input.password;
        let valid = tokio::task::spawn_blocking(move || {
            PasswordHash::new(&password_hash).ok().is_some_and(|hash| {
                Argon2::default()
                    .verify_password(password.as_bytes(), &hash)
                    .is_ok()
            })
        })
        .await
        .map_err(|_| RelayError::Internal)?;
        let Some(account_id) = account_id.filter(|_| valid) else {
            return Err(RelayError::Authentication("邮箱或密码错误".into()));
        };
        self.issue_account_session(&account_id).await
    }

    async fn issue_account_session(&self, account_id: &str) -> RelayResult<AccountSessionOutput> {
        let (token, token_hash) = new_token("account-session");
        let now = Utc::now();
        let expires_at = now + ChronoDuration::minutes(ACCOUNT_SESSION_MINUTES);
        sqlx::query("INSERT INTO account_sessions(id,account_id,token_hash,expires_at,revoked_at,created_at) VALUES(?,?,?,?,NULL,?)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(account_id)
            .bind(token_hash)
            .bind(expires_at.to_rfc3339())
            .bind(now.to_rfc3339())
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM account_sessions WHERE expires_at<=? OR (account_id=? AND id NOT IN (SELECT id FROM account_sessions WHERE account_id=? ORDER BY created_at DESC LIMIT ?))")
            .bind(now.to_rfc3339())
            .bind(account_id)
            .bind(account_id)
            .bind(MAX_ACCOUNT_SESSIONS)
            .execute(&self.pool)
            .await?;
        Ok(AccountSessionOutput {
            account_id: account_id.into(),
            token,
            expires_at: expires_at.to_rfc3339(),
        })
    }

    pub async fn authenticate_account(&self, token: &str) -> RelayResult<AccountAuth> {
        validate_token_shape(token)?;
        let token_hash = token_hash("account-session", token);
        let row = sqlx::query("SELECT a.id,a.email,a.display_name FROM account_sessions s JOIN accounts a ON a.id=s.account_id WHERE s.token_hash=? AND s.revoked_at IS NULL AND s.expires_at>? AND a.status='active'")
            .bind(token_hash)
            .bind(Utc::now().to_rfc3339())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| RelayError::Authentication("账号会话无效或已过期".into()))?;
        Ok(AccountAuth {
            account_id: row.get(0),
            email: row.get(1),
            display_name: row.get(2),
        })
    }

    pub async fn bootstrap_workspace(
        &self,
        account: &AccountAuth,
        input: BootstrapWorkspaceInput,
    ) -> RelayResult<DeviceSessionOutput> {
        validate_uuid(&input.workspace_id, "工作区 ID")?;
        validate_uuid(&input.member_id, "成员 ID")?;
        validate_device(&input.device)?;
        let workspace_name = clean_name(&input.workspace_name, "工作区名称")?;
        let now = Utc::now().to_rfc3339();
        let mut transaction = self.pool.begin().await?;
        let owned_workspaces: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM workspaces w JOIN members m ON m.workspace_id=w.id WHERE m.account_id=? AND m.role='owner' AND m.status='active' AND w.status='active'",
        )
        .bind(&account.account_id)
        .fetch_one(&mut *transaction)
        .await?;
        if owned_workspaces >= MAX_ACCOUNT_WORKSPACES {
            return Err(RelayError::Conflict(
                "账号拥有的活动工作区已经达到 32 个上限".into(),
            ));
        }
        sqlx::query("INSERT INTO workspaces(id,name,key_epoch,status,created_at,updated_at) VALUES(?,?,1,'active',?,?)")
            .bind(&input.workspace_id)
            .bind(workspace_name)
            .bind(&now)
            .bind(&now)
            .execute(&mut *transaction)
            .await
            .map_err(map_conflict("工作区 ID 已存在"))?;
        sqlx::query("INSERT INTO members(id,workspace_id,account_id,display_name,role,status,joined_at,updated_at,removed_at) VALUES(?,?,?,?,'owner','active',?,?,NULL)")
            .bind(&input.member_id)
            .bind(&input.workspace_id)
            .bind(&account.account_id)
            .bind(&account.display_name)
            .bind(&now)
            .bind(&now)
            .execute(&mut *transaction)
            .await?;
        insert_device(
            &mut transaction,
            &input.workspace_id,
            &input.member_id,
            &input.device,
            &now,
        )
        .await?;
        audit(
            &mut transaction,
            &input.workspace_id,
            &input.member_id,
            "workspace-created",
            "workspace",
            &input.workspace_id,
        )
        .await?;
        let output = issue_device_session(
            &mut transaction,
            &input.workspace_id,
            &input.member_id,
            &input.device.id,
            "owner",
            1,
        )
        .await?;
        transaction.commit().await?;
        Ok(output)
    }

    pub async fn authenticate_device(&self, token: &str) -> RelayResult<DeviceAuth> {
        validate_token_shape(token)?;
        let hash = token_hash("device-session", token);
        let row = sqlx::query("SELECT m.account_id,d.workspace_id,d.member_id,d.id,m.role,w.key_epoch,s.token_hash FROM device_sessions s JOIN devices d ON d.id=s.device_id JOIN members m ON m.id=d.member_id AND m.workspace_id=d.workspace_id JOIN workspaces w ON w.id=d.workspace_id WHERE s.token_hash=? AND s.revoked_at IS NULL AND s.expires_at>? AND d.status='active' AND m.status='active' AND w.status='active'")
            .bind(&hash)
            .bind(Utc::now().to_rfc3339())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| RelayError::Authentication("设备会话无效、已过期或已撤销".into()))?;
        Ok(DeviceAuth {
            account_id: row.get(0),
            workspace_id: row.get(1),
            member_id: row.get(2),
            device_id: row.get(3),
            role: row.get(4),
            key_epoch: row.get(5),
            token_hash: row.get(6),
        })
    }

    pub async fn create_workspace_invitation(
        &self,
        auth: &DeviceAuth,
        input: CreateWorkspaceInvitationInput,
    ) -> RelayResult<WorkspaceInvitationOutput> {
        require_permission(&auth.role, "memberManage")?;
        validate_uuid(&input.member_id, "邀请成员 ID")?;
        let email = clean_email(&input.email)?;
        if !matches!(input.role.as_str(), "admin" | "operator" | "viewer") {
            return Err(RelayError::Validation("邀请角色无效".into()));
        }
        if auth.role == "admin" && input.role == "admin" {
            return Err(RelayError::PermissionDenied(
                "Admin 不能邀请另一个 Admin".into(),
            ));
        }
        let (token, hash) = new_token("workspace-invitation");
        let invitation_id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let expires_at = now + ChronoDuration::hours(WORKSPACE_INVITATION_HOURS);
        sqlx::query(
            "DELETE FROM workspace_invitations WHERE expires_at<=? OR revoked_at IS NOT NULL",
        )
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await?;
        let invitation_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workspace_invitations WHERE workspace_id=? AND accepted_at IS NULL AND revoked_at IS NULL")
            .bind(&auth.workspace_id)
            .fetch_one(&self.pool)
            .await?;
        if invitation_count >= MAX_WORKSPACE_INVITATIONS {
            return Err(RelayError::Conflict(
                "工作区待接受邀请已经达到 256 条上限".into(),
            ));
        }
        sqlx::query("INSERT INTO workspace_invitations(id,workspace_id,invited_by_member_id,member_id,email,role,token_hash,expires_at,accepted_at,revoked_at,created_at) VALUES(?,?,?,?,?,?,?,?,NULL,NULL,?)")
            .bind(&invitation_id)
            .bind(&auth.workspace_id)
            .bind(&auth.member_id)
            .bind(input.member_id)
            .bind(email)
            .bind(input.role)
            .bind(hash)
            .bind(expires_at.to_rfc3339())
            .bind(now.to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(map_conflict("邀请成员 ID 已存在"))?;
        Ok(WorkspaceInvitationOutput {
            invitation_id,
            token,
            expires_at: expires_at.to_rfc3339(),
        })
    }

    pub async fn accept_workspace_invitation(
        &self,
        account: &AccountAuth,
        input: AcceptWorkspaceInvitationInput,
    ) -> RelayResult<DeviceSessionOutput> {
        validate_device(&input.device)?;
        validate_token_shape(&input.token)?;
        let invitation_hash = token_hash("workspace-invitation", &input.token);
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query("SELECT id,workspace_id,member_id,email,role,accepted_at FROM workspace_invitations WHERE token_hash=? AND revoked_at IS NULL AND expires_at>?")
            .bind(invitation_hash)
            .bind(Utc::now().to_rfc3339())
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or_else(|| RelayError::Authentication("工作区邀请无效或已过期".into()))?;
        let invitation_id: String = row.get(0);
        let workspace_id: String = row.get(1);
        let member_id: String = row.get(2);
        let invited_email: String = row.get(3);
        let role: String = row.get(4);
        if invited_email != account.email {
            return Err(RelayError::PermissionDenied(
                "工作区邀请不属于当前账号".into(),
            ));
        }
        if row.get::<Option<String>, _>(5).is_some() {
            let matching_device: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM members m JOIN devices d ON d.member_id=m.id AND d.workspace_id=m.workspace_id WHERE m.id=? AND m.workspace_id=? AND m.account_id=? AND m.status='active' AND d.id=? AND d.name=? AND d.encryption_public_key=? AND d.signing_public_key=? AND d.fingerprint=? AND d.status='active'")
                .bind(&member_id)
                .bind(&workspace_id)
                .bind(&account.account_id)
                .bind(&input.device.id)
                .bind(&input.device.name)
                .bind(&input.device.encryption_public_key)
                .bind(&input.device.signing_public_key)
                .bind(&input.device.fingerprint)
                .fetch_one(&mut *transaction)
                .await?;
            if matching_device != 1 {
                return Err(RelayError::Authentication(
                    "工作区邀请已经由另一设备接受".into(),
                ));
            }
            let epoch: i64 = sqlx::query_scalar(
                "SELECT key_epoch FROM workspaces WHERE id=? AND status='active'",
            )
            .bind(&workspace_id)
            .fetch_one(&mut *transaction)
            .await?;
            let output = issue_device_session(
                &mut transaction,
                &workspace_id,
                &member_id,
                &input.device.id,
                &role,
                epoch,
            )
            .await?;
            transaction.commit().await?;
            return Ok(output);
        }
        let member_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM members WHERE workspace_id=? AND status='active'",
        )
        .bind(&workspace_id)
        .fetch_one(&mut *transaction)
        .await?;
        if member_count >= MAX_WORKSPACE_MEMBERS {
            return Err(RelayError::Conflict(
                "工作区活动成员已经达到 256 人上限".into(),
            ));
        }
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO members(id,workspace_id,account_id,display_name,role,status,joined_at,updated_at,removed_at) VALUES(?,?,?,?,?,'active',?,?,NULL)")
            .bind(&member_id)
            .bind(&workspace_id)
            .bind(&account.account_id)
            .bind(&account.display_name)
            .bind(&role)
            .bind(&now)
            .bind(&now)
            .execute(&mut *transaction)
            .await
            .map_err(map_conflict("账号已经属于该工作区"))?;
        insert_device(
            &mut transaction,
            &workspace_id,
            &member_id,
            &input.device,
            &now,
        )
        .await?;
        sqlx::query(
            "UPDATE workspace_invitations SET accepted_at=? WHERE id=? AND accepted_at IS NULL",
        )
        .bind(&now)
        .bind(&invitation_id)
        .execute(&mut *transaction)
        .await?;
        let epoch: i64 = sqlx::query_scalar("UPDATE workspaces SET key_epoch=key_epoch+1,updated_at=? WHERE id=? AND status='active' RETURNING key_epoch")
            .bind(&now)
            .bind(&workspace_id)
            .fetch_one(&mut *transaction)
            .await?;
        close_workspace_rooms(&mut transaction, &workspace_id, &now).await?;
        audit(
            &mut transaction,
            &workspace_id,
            &member_id,
            "invitation-accepted",
            "member",
            &member_id,
        )
        .await?;
        let output = issue_device_session(
            &mut transaction,
            &workspace_id,
            &member_id,
            &input.device.id,
            &role,
            epoch,
        )
        .await?;
        transaction.commit().await?;
        Ok(output)
    }

    pub async fn workspace_snapshot(
        &self,
        auth: &DeviceAuth,
        workspace_id: &str,
    ) -> RelayResult<WorkspaceSnapshot> {
        if auth.workspace_id != workspace_id {
            return Err(RelayError::PermissionDenied("设备不属于该工作区".into()));
        }
        require_permission(&auth.role, "memberRead")?;
        let workspace =
            sqlx::query("SELECT name,key_epoch FROM workspaces WHERE id=? AND status='active'")
                .bind(workspace_id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or_else(|| RelayError::NotFound("工作区不存在".into()))?;
        let member_rows = sqlx::query("SELECT id,display_name,role,status FROM members WHERE workspace_id=? ORDER BY joined_at,id")
            .bind(workspace_id)
            .fetch_all(&self.pool)
            .await?;
        let device_rows = sqlx::query("SELECT id,member_id,name,encryption_public_key,signing_public_key,fingerprint,status FROM devices WHERE workspace_id=? ORDER BY created_at,id")
            .bind(workspace_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(WorkspaceSnapshot {
            id: workspace_id.into(),
            name: workspace.get(0),
            key_epoch: workspace.get(1),
            members: member_rows
                .into_iter()
                .map(|row| WorkspaceMemberView {
                    id: row.get(0),
                    display_name: row.get(1),
                    role: row.get(2),
                    status: row.get(3),
                })
                .collect(),
            devices: device_rows
                .into_iter()
                .map(|row| WorkspaceDeviceView {
                    id: row.get(0),
                    member_id: row.get(1),
                    name: row.get(2),
                    encryption_public_key: row.get(3),
                    signing_public_key: row.get(4),
                    fingerprint: row.get(5),
                    status: row.get(6),
                })
                .collect(),
        })
    }

    pub async fn list_audit(
        &self,
        auth: &DeviceAuth,
        workspace_id: &str,
    ) -> RelayResult<Vec<RelayAuditEvent>> {
        if auth.workspace_id != workspace_id {
            return Err(RelayError::PermissionDenied("设备不属于该工作区".into()));
        }
        if !matches!(auth.role.as_str(), "owner" | "admin") {
            return Err(RelayError::PermissionDenied(
                "当前角色不能读取团队审计".into(),
            ));
        }
        let rows = sqlx::query("SELECT id,workspace_id,actor_member_id,action,target_type,target_id,created_at FROM relay_audit_events WHERE workspace_id=? ORDER BY created_at DESC,rowid DESC LIMIT ?")
            .bind(workspace_id)
            .bind(MAX_AUDIT_EVENTS)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| RelayAuditEvent {
                id: row.get(0),
                workspace_id: row.get(1),
                actor_member_id: row.get(2),
                action: row.get(3),
                target_type: row.get(4),
                target_id: row.get(5),
                created_at: row.get(6),
            })
            .collect())
    }

    pub async fn delete_workspace(&self, auth: &DeviceAuth, workspace_id: &str) -> RelayResult<()> {
        if auth.workspace_id != workspace_id || auth.role != "owner" {
            return Err(RelayError::PermissionDenied(
                "只有工作区 Owner 可以永久删除团队数据".into(),
            ));
        }
        let affected = sqlx::query("DELETE FROM workspaces WHERE id=?")
            .bind(workspace_id)
            .execute(&self.pool)
            .await?
            .rows_affected();
        if affected != 1 {
            return Err(RelayError::NotFound("工作区不存在".into()));
        }
        Ok(())
    }

    pub async fn update_member(
        &self,
        auth: &DeviceAuth,
        workspace_id: &str,
        member_id: &str,
        input: UpdateMemberInput,
    ) -> RelayResult<i64> {
        if auth.workspace_id != workspace_id {
            return Err(RelayError::PermissionDenied("设备不属于该工作区".into()));
        }
        require_permission(&auth.role, "memberManage")?;
        if !matches!(
            input.role.as_str(),
            "owner" | "admin" | "operator" | "viewer"
        ) || !matches!(input.status.as_str(), "active" | "removed")
        {
            return Err(RelayError::Validation("成员角色或状态无效".into()));
        }
        let mut transaction = self.pool.begin().await?;
        let current = sqlx::query("SELECT role,status FROM members WHERE id=? AND workspace_id=?")
            .bind(member_id)
            .bind(workspace_id)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or_else(|| RelayError::NotFound("成员不存在".into()))?;
        let current_role: String = current.get(0);
        let current_status: String = current.get(1);
        if auth.role != "owner" && (current_role == "owner" || input.role == "owner") {
            return Err(RelayError::PermissionDenied(
                "只有 Owner 可以管理 Owner".into(),
            ));
        }
        if member_id == auth.member_id && input.status == "removed" {
            return Err(RelayError::Validation("不能移除当前会话成员".into()));
        }
        if current_role == "owner"
            && current_status == "active"
            && (input.role != "owner" || input.status != "active")
        {
            let owners: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM members WHERE workspace_id=? AND role='owner' AND status='active'")
                .bind(workspace_id)
                .fetch_one(&mut *transaction)
                .await?;
            if owners <= 1 {
                return Err(RelayError::Validation(
                    "工作区必须保留至少一名 Owner".into(),
                ));
            }
        }
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE members SET role=?,status=?,updated_at=?,removed_at=CASE WHEN ?='removed' THEN ? ELSE NULL END WHERE id=? AND workspace_id=?")
            .bind(&input.role)
            .bind(&input.status)
            .bind(&now)
            .bind(&input.status)
            .bind(&now)
            .bind(member_id)
            .bind(workspace_id)
            .execute(&mut *transaction)
            .await?;
        if input.status == "removed" {
            sqlx::query("UPDATE device_sessions SET revoked_at=? WHERE revoked_at IS NULL AND device_id IN (SELECT id FROM devices WHERE member_id=? AND workspace_id=?)")
                .bind(&now)
                .bind(member_id)
                .bind(workspace_id)
                .execute(&mut *transaction)
                .await?;
            sqlx::query("UPDATE devices SET status='revoked',revoked_at=?,updated_at=? WHERE member_id=? AND workspace_id=? AND status='active'")
                .bind(&now)
                .bind(&now)
                .bind(member_id)
                .bind(workspace_id)
                .execute(&mut *transaction)
                .await?;
        }
        let epoch = advance_epoch(&mut transaction, workspace_id, &now).await?;
        close_workspace_rooms(&mut transaction, workspace_id, &now).await?;
        audit(
            &mut transaction,
            workspace_id,
            &auth.member_id,
            "member-updated",
            "member",
            member_id,
        )
        .await?;
        transaction.commit().await?;
        Ok(epoch)
    }

    pub async fn revoke_device(
        &self,
        auth: &DeviceAuth,
        workspace_id: &str,
        device_id: &str,
    ) -> RelayResult<i64> {
        if auth.workspace_id != workspace_id {
            return Err(RelayError::PermissionDenied("设备不属于该工作区".into()));
        }
        require_permission(&auth.role, "memberManage")?;
        validate_uuid(device_id, "设备 ID")?;
        let now = Utc::now().to_rfc3339();
        let mut transaction = self.pool.begin().await?;
        let affected = sqlx::query("UPDATE devices SET status='revoked',revoked_at=?,updated_at=? WHERE id=? AND workspace_id=? AND status='active'")
            .bind(&now)
            .bind(&now)
            .bind(device_id)
            .bind(workspace_id)
            .execute(&mut *transaction)
            .await?
            .rows_affected();
        if affected != 1 {
            return Err(RelayError::NotFound("活动设备不存在".into()));
        }
        sqlx::query(
            "UPDATE device_sessions SET revoked_at=? WHERE device_id=? AND revoked_at IS NULL",
        )
        .bind(&now)
        .bind(device_id)
        .execute(&mut *transaction)
        .await?;
        let epoch = advance_epoch(&mut transaction, workspace_id, &now).await?;
        close_workspace_rooms(&mut transaction, workspace_id, &now).await?;
        audit(
            &mut transaction,
            workspace_id,
            &auth.member_id,
            "device-revoked",
            "device",
            device_id,
        )
        .await?;
        transaction.commit().await?;
        Ok(epoch)
    }

    pub async fn create_device_challenge(
        &self,
        workspace_id: &str,
        device_id: &str,
    ) -> RelayResult<DeviceChallengeOutput> {
        validate_uuid(workspace_id, "工作区 ID")?;
        validate_uuid(device_id, "设备 ID")?;
        let active: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices d JOIN members m ON m.id=d.member_id AND m.workspace_id=d.workspace_id JOIN workspaces w ON w.id=d.workspace_id WHERE d.id=? AND d.workspace_id=? AND d.status='active' AND m.status='active' AND w.status='active'")
            .bind(device_id)
            .bind(workspace_id)
            .fetch_one(&self.pool)
            .await?;
        if active != 1 {
            return Err(RelayError::NotFound("活动设备不存在".into()));
        }
        let challenge_id = uuid::Uuid::new_v4().to_string();
        let (challenge, challenge_hash) = new_token("device-challenge");
        let now = Utc::now();
        let expires_at = now + ChronoDuration::minutes(DEVICE_CHALLENGE_MINUTES);
        sqlx::query("DELETE FROM device_challenges WHERE expires_at<=? OR used_at IS NOT NULL")
            .bind(now.to_rfc3339())
            .execute(&self.pool)
            .await?;
        let challenge_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM device_challenges WHERE device_id=? AND used_at IS NULL",
        )
        .bind(device_id)
        .fetch_one(&self.pool)
        .await?;
        if challenge_count >= MAX_DEVICE_CHALLENGES {
            return Err(RelayError::Conflict(
                "设备待处理挑战已经达到 32 条上限".into(),
            ));
        }
        sqlx::query("INSERT INTO device_challenges(id,device_id,challenge_hash,expires_at,used_at,created_at) VALUES(?,?,?,?,NULL,?)")
            .bind(&challenge_id)
            .bind(device_id)
            .bind(challenge_hash)
            .bind(expires_at.to_rfc3339())
            .bind(now.to_rfc3339())
            .execute(&self.pool)
            .await?;
        Ok(DeviceChallengeOutput {
            challenge_id,
            challenge,
            expires_at: expires_at.to_rfc3339(),
        })
    }

    pub async fn create_device_session(
        &self,
        input: CreateDeviceSessionInput,
    ) -> RelayResult<DeviceSessionOutput> {
        validate_uuid(&input.challenge_id, "设备挑战 ID")?;
        validate_token_shape(&input.challenge)?;
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query("SELECT c.device_id,c.challenge_hash,d.workspace_id,d.member_id,d.signing_public_key,m.role,w.key_epoch FROM device_challenges c JOIN devices d ON d.id=c.device_id JOIN members m ON m.id=d.member_id AND m.workspace_id=d.workspace_id JOIN workspaces w ON w.id=d.workspace_id WHERE c.id=? AND c.used_at IS NULL AND c.expires_at>? AND d.status='active' AND m.status='active' AND w.status='active'")
            .bind(&input.challenge_id)
            .bind(Utc::now().to_rfc3339())
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or_else(|| RelayError::Authentication("设备挑战无效或已过期".into()))?;
        let device_id: String = row.get(0);
        let expected_hash: String = row.get(1);
        if token_hash("device-challenge", &input.challenge) != expected_hash {
            return Err(RelayError::Authentication("设备挑战内容不匹配".into()));
        }
        let signing_public: String = row.get(4);
        verify_device_session_signature(
            &signing_public,
            &input.challenge_id,
            &input.challenge,
            &input.signature,
        )?;
        let now = Utc::now().to_rfc3339();
        let affected =
            sqlx::query("UPDATE device_challenges SET used_at=? WHERE id=? AND used_at IS NULL")
                .bind(&now)
                .bind(&input.challenge_id)
                .execute(&mut *transaction)
                .await?
                .rows_affected();
        if affected != 1 {
            return Err(RelayError::Authentication("设备挑战已经使用".into()));
        }
        let output = issue_device_session(
            &mut transaction,
            row.get::<String, _>(2).as_str(),
            row.get::<String, _>(3).as_str(),
            &device_id,
            row.get::<String, _>(5).as_str(),
            row.get(6),
        )
        .await?;
        transaction.commit().await?;
        Ok(output)
    }
}

fn clean_email(value: &str) -> RelayResult<String> {
    let value = value.trim().to_ascii_lowercase();
    let mut parts = value.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    if value.len() > 254
        || local.is_empty()
        || domain.is_empty()
        || !domain.contains('.')
        || parts.next().is_some()
        || value
            .chars()
            .any(|value| value.is_control() || value.is_whitespace())
    {
        return Err(RelayError::Validation("邮箱格式无效".into()));
    }
    Ok(value)
}

fn clean_name(value: &str, field: &str) -> RelayResult<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(RelayError::Validation(format!("{field}无效")));
    }
    Ok(value.into())
}

fn validate_password(value: &str) -> RelayResult<()> {
    if value.len() < 12 || value.len() > 1024 || value.chars().any(char::is_control) {
        return Err(RelayError::Validation(
            "密码必须为 12 至 1024 字节且不能包含控制字符".into(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_uuid(value: &str, field: &str) -> RelayResult<()> {
    if value.len() > 64 || uuid::Uuid::parse_str(value).is_err() {
        return Err(RelayError::Validation(format!("{field}无效")));
    }
    Ok(())
}

fn validate_token_shape(value: &str) -> RelayResult<()> {
    if value.len() != 43
        || URL_SAFE_NO_PAD
            .decode(value)
            .ok()
            .is_none_or(|bytes| bytes.len() != 32)
    {
        return Err(RelayError::Authentication("会话令牌格式无效".into()));
    }
    Ok(())
}

fn new_token(domain: &str) -> (String, String) {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let token = URL_SAFE_NO_PAD.encode(bytes);
    let hash = token_hash(domain, &token);
    (token, hash)
}

fn token_hash(domain: &str, token: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"cnshell-team-relay-v1\0");
    digest.update(domain.as_bytes());
    digest.update(b"\0");
    digest.update(token.as_bytes());
    format!("sha256:{:x}", digest.finalize())
}

fn decode_public_key(value: &str, prefix: &str) -> RelayResult<[u8; 32]> {
    let encoded = value
        .strip_prefix(&format!("{prefix}:"))
        .ok_or_else(|| RelayError::Validation(format!("设备密钥必须使用 {prefix} 格式")))?;
    URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| RelayError::Validation("设备公钥编码无效".into()))?
        .try_into()
        .map_err(|_| RelayError::Validation("设备公钥长度无效".into()))
}

fn validate_device(device: &DeviceRegistration) -> RelayResult<()> {
    validate_uuid(&device.id, "设备 ID")?;
    clean_name(&device.name, "设备名称")?;
    let encryption = decode_public_key(&device.encryption_public_key, "x25519")?;
    let signing = decode_public_key(&device.signing_public_key, "ed25519")?;
    VerifyingKey::from_bytes(&signing)
        .map_err(|_| RelayError::Validation("Ed25519 公钥无效".into()))?;
    let mut digest = Sha256::new();
    digest.update(b"cnshell-team-device-v1\0");
    digest.update(encryption);
    digest.update(signing);
    let expected = format!("sha256:{:x}", digest.finalize());
    if device.fingerprint != expected {
        return Err(RelayError::Validation("设备组合指纹不匹配".into()));
    }
    Ok(())
}

async fn insert_device(
    transaction: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
    member_id: &str,
    device: &DeviceRegistration,
    now: &str,
) -> RelayResult<()> {
    sqlx::query("INSERT INTO devices(id,workspace_id,member_id,name,encryption_public_key,signing_public_key,fingerprint,status,created_at,updated_at,revoked_at) VALUES(?,?,?,?,?,?,?,'active',?,?,NULL)")
        .bind(&device.id)
        .bind(workspace_id)
        .bind(member_id)
        .bind(clean_name(&device.name, "设备名称")?)
        .bind(&device.encryption_public_key)
        .bind(&device.signing_public_key)
        .bind(&device.fingerprint)
        .bind(now)
        .bind(now)
        .execute(&mut **transaction)
        .await
        .map_err(map_conflict("设备 ID 已存在"))?;
    Ok(())
}

async fn issue_device_session(
    transaction: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
    member_id: &str,
    device_id: &str,
    role: &str,
    key_epoch: i64,
) -> RelayResult<DeviceSessionOutput> {
    let (token, hash) = new_token("device-session");
    let now = Utc::now();
    let expires_at = now + ChronoDuration::minutes(DEVICE_SESSION_MINUTES);
    sqlx::query("INSERT INTO device_sessions(id,device_id,token_hash,expires_at,revoked_at,created_at) VALUES(?,?,?,?,NULL,?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(device_id)
        .bind(hash)
        .bind(expires_at.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&mut **transaction)
        .await?;
    sqlx::query("DELETE FROM device_sessions WHERE expires_at<=? OR (device_id=? AND id NOT IN (SELECT id FROM device_sessions WHERE device_id=? ORDER BY created_at DESC LIMIT ?))")
        .bind(now.to_rfc3339())
        .bind(device_id)
        .bind(device_id)
        .bind(MAX_DEVICE_SESSIONS)
        .execute(&mut **transaction)
        .await?;
    Ok(DeviceSessionOutput {
        workspace_id: workspace_id.into(),
        member_id: member_id.into(),
        device_id: device_id.into(),
        role: role.into(),
        key_epoch,
        token,
        expires_at: expires_at.to_rfc3339(),
    })
}

fn require_permission(role: &str, permission: &str) -> RelayResult<()> {
    let allowed = match permission {
        "memberRead" => matches!(role, "owner" | "admin" | "operator" | "viewer"),
        "memberManage" => matches!(role, "owner" | "admin"),
        "terminalView" => matches!(role, "owner" | "admin" | "operator" | "viewer"),
        "terminalControl" => matches!(role, "owner" | "admin" | "operator"),
        "shareManage" => matches!(role, "owner" | "admin"),
        _ => false,
    };
    if allowed {
        Ok(())
    } else {
        Err(RelayError::PermissionDenied(format!(
            "角色 {role} 没有 {permission} 权限"
        )))
    }
}

pub(crate) fn require_device_permission(auth: &DeviceAuth, permission: &str) -> RelayResult<()> {
    require_permission(&auth.role, permission)
}

async fn advance_epoch(
    transaction: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
    now: &str,
) -> RelayResult<i64> {
    Ok(sqlx::query_scalar("UPDATE workspaces SET key_epoch=key_epoch+1,updated_at=? WHERE id=? AND status='active' RETURNING key_epoch")
        .bind(now)
        .bind(workspace_id)
        .fetch_one(&mut **transaction)
        .await?)
}

async fn close_workspace_rooms(
    transaction: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
    now: &str,
) -> RelayResult<()> {
    sqlx::query("UPDATE terminal_rooms SET status='closed',closed_at=? WHERE workspace_id=? AND status='active'")
        .bind(now)
        .bind(workspace_id)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

pub(crate) async fn audit(
    transaction: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
    member_id: &str,
    action: &str,
    target_type: &str,
    target_id: &str,
) -> RelayResult<()> {
    sqlx::query("INSERT INTO relay_audit_events(id,workspace_id,actor_member_id,action,target_type,target_id,created_at) VALUES(?,?,?,?,?,?,?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(workspace_id)
        .bind(member_id)
        .bind(action)
        .bind(target_type)
        .bind(target_id)
        .bind(Utc::now().to_rfc3339())
        .execute(&mut **transaction)
        .await?;
    sqlx::query("DELETE FROM relay_audit_events WHERE workspace_id=? AND id NOT IN (SELECT id FROM relay_audit_events WHERE workspace_id=? ORDER BY created_at DESC,rowid DESC LIMIT ?)")
        .bind(workspace_id)
        .bind(workspace_id)
        .bind(MAX_AUDIT_EVENTS)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

fn verify_device_session_signature(
    signing_public_key: &str,
    challenge_id: &str,
    challenge: &str,
    signature: &str,
) -> RelayResult<()> {
    let public = decode_public_key(signing_public_key, "ed25519")?;
    let verifying = VerifyingKey::from_bytes(&public)
        .map_err(|_| RelayError::Validation("设备签名公钥无效".into()))?;
    let encoded = signature
        .strip_prefix("ed25519:")
        .ok_or_else(|| RelayError::Validation("设备挑战签名格式无效".into()))?;
    let signature = Signature::from_slice(
        &URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| RelayError::Validation("设备挑战签名编码无效".into()))?,
    )
    .map_err(|_| RelayError::Validation("设备挑战签名长度无效".into()))?;
    let payload = format!("cnshell-relay-device-session-v1\0{challenge_id}\0{challenge}");
    verifying
        .verify(payload.as_bytes(), &signature)
        .map_err(|_| RelayError::Authentication("设备挑战签名验证失败".into()))
}

fn map_conflict(message: &'static str) -> impl FnOnce(sqlx::Error) -> RelayError {
    move |error| match error {
        sqlx::Error::Database(database) if database.is_unique_violation() => {
            RelayError::Conflict(message.into())
        }
        other => RelayError::Storage(other),
    }
}

fn dummy_password_hash() -> &'static String {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| {
        let salt = SaltString::generate(&mut PasswordOsRng);
        Argon2::default()
            .hash_password(b"cnshell-relay-dummy-password", &salt)
            .expect("fixed dummy password hashes")
            .to_string()
    })
}
