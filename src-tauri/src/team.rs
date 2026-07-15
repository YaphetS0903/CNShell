use crate::{
    db::Database,
    error::{AppError, AppResult},
    models::{
        CreateTeamWorkspaceInput, SaveTeamMemberInput, TeamAuditEvent, TeamMember,
        TeamPermissionReport, TeamWorkspace,
    },
};
use chrono::Utc;
use serde_json::json;
use sqlx::{Row, Sqlite, Transaction};
use std::{io::Write, path::Path};

const MAX_WORKSPACES: i64 = 32;
const MAX_MEMBERS: i64 = 256;
const MAX_AUDIT_EXPORT: i64 = 4096;
const ROLES: &[&str] = &["owner", "admin", "operator", "viewer"];

const OWNER_PERMISSIONS: &[&str] = &[
    "workspaceRead",
    "workspaceDelete",
    "memberRead",
    "memberManage",
    "ownerManage",
    "connectionRead",
    "connectionManage",
    "connectionUse",
    "terminalView",
    "terminalControl",
    "shareCreate",
    "shareManage",
    "auditRead",
    "auditExport",
];
const ADMIN_PERMISSIONS: &[&str] = &[
    "workspaceRead",
    "memberRead",
    "memberManage",
    "connectionRead",
    "connectionManage",
    "connectionUse",
    "terminalView",
    "terminalControl",
    "shareCreate",
    "shareManage",
    "auditRead",
    "auditExport",
];
const OPERATOR_PERMISSIONS: &[&str] = &[
    "workspaceRead",
    "memberRead",
    "connectionRead",
    "connectionUse",
    "terminalView",
    "terminalControl",
    "shareCreate",
];
const VIEWER_PERMISSIONS: &[&str] = &[
    "workspaceRead",
    "memberRead",
    "connectionRead",
    "terminalView",
];

fn permissions(role: &str) -> AppResult<&'static [&'static str]> {
    match role {
        "owner" => Ok(OWNER_PERMISSIONS),
        "admin" => Ok(ADMIN_PERMISSIONS),
        "operator" => Ok(OPERATOR_PERMISSIONS),
        "viewer" => Ok(VIEWER_PERMISSIONS),
        _ => Err(AppError::Validation("团队角色无效".into())),
    }
}

fn valid_uuid(value: &str) -> bool {
    value.len() <= 64 && uuid::Uuid::parse_str(value).is_ok()
}

fn clean_name(value: &str, field: &str) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 256
        || value.chars().any(|character| character.is_control())
    {
        return Err(AppError::Validation(format!(
            "{field}不能为空、含控制字符或超过 256 字节"
        )));
    }
    Ok(value.into())
}

async fn local_authorization(
    transaction: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
) -> AppResult<(String, String)> {
    if !valid_uuid(workspace_id) {
        return Err(AppError::Validation("团队工作区 ID 无效".into()));
    }
    let row = sqlx::query("SELECT w.local_member_id,m.role FROM team_workspaces w JOIN team_members m ON m.id=w.local_member_id AND m.workspace_id=w.id WHERE w.id=? AND m.status='active'")
        .bind(workspace_id)
        .fetch_optional(&mut **transaction)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("团队工作区 {workspace_id}")))?;
    Ok((row.get(0), row.get(1)))
}

fn require_permission(role: &str, permission: &str) -> AppResult<()> {
    if permissions(role)?.contains(&permission) {
        Ok(())
    } else {
        Err(AppError::PermissionDenied(format!(
            "角色 {role} 没有 {permission} 权限"
        )))
    }
}

async fn audit(
    transaction: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
    actor_member_id: &str,
    action: &str,
    target_type: &str,
    target_id: &str,
) -> AppResult<()> {
    sqlx::query("INSERT INTO team_audit_events(id,workspace_id,actor_member_id,action,target_type,target_id,created_at) VALUES(?,?,?,?,?,?,?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(workspace_id)
        .bind(actor_member_id)
        .bind(action)
        .bind(target_type)
        .bind(target_id)
        .bind(Utc::now().to_rfc3339())
        .execute(&mut **transaction)
        .await?;
    sqlx::query("DELETE FROM team_audit_events WHERE workspace_id=? AND id NOT IN (SELECT id FROM team_audit_events WHERE workspace_id=? ORDER BY created_at DESC,rowid DESC LIMIT ?)")
        .bind(workspace_id)
        .bind(workspace_id)
        .bind(MAX_AUDIT_EXPORT)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

pub async fn list_workspaces(db: &Database) -> AppResult<Vec<TeamWorkspace>> {
    Ok(sqlx::query_as::<_, TeamWorkspace>("SELECT w.id,w.name,w.local_member_id,m.role AS local_role,w.key_epoch,w.created_at,w.updated_at FROM team_workspaces w JOIN team_members m ON m.id=w.local_member_id AND m.workspace_id=w.id WHERE m.status='active' ORDER BY w.name COLLATE NOCASE")
        .fetch_all(&db.pool)
        .await?)
}

pub async fn create_workspace(
    db: &Database,
    input: CreateTeamWorkspaceInput,
) -> AppResult<TeamWorkspace> {
    let name = clean_name(&input.name, "团队名称")?;
    let owner_name = clean_name(&input.owner_name, "Owner 名称")?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM team_workspaces")
        .fetch_one(&db.pool)
        .await?;
    if count >= MAX_WORKSPACES {
        return Err(AppError::Validation("最多创建 32 个本地团队工作区".into()));
    }
    let workspace_id = uuid::Uuid::new_v4().to_string();
    let owner_id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let mut transaction = db.pool.begin().await?;
    sqlx::query("INSERT INTO team_workspaces(id,name,local_member_id,key_epoch,created_at,updated_at) VALUES(?,?,?,1,?,?)")
        .bind(&workspace_id)
        .bind(&name)
        .bind(&owner_id)
        .bind(&now)
        .bind(&now)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("INSERT INTO team_members(id,workspace_id,display_name,role,status,joined_at,updated_at,removed_at) VALUES(?,?,?,'owner','active',?,?,NULL)")
        .bind(&owner_id)
        .bind(&workspace_id)
        .bind(owner_name)
        .bind(&now)
        .bind(&now)
        .execute(&mut *transaction)
        .await?;
    audit(
        &mut transaction,
        &workspace_id,
        &owner_id,
        "workspace-created",
        "workspace",
        &workspace_id,
    )
    .await?;
    transaction.commit().await?;
    Ok(TeamWorkspace {
        id: workspace_id,
        name,
        local_member_id: owner_id,
        local_role: "owner".into(),
        key_epoch: 1,
        created_at: now.clone(),
        updated_at: now,
    })
}

pub async fn list_members(db: &Database, workspace_id: &str) -> AppResult<Vec<TeamMember>> {
    let mut transaction = db.pool.begin().await?;
    let (_, role) = local_authorization(&mut transaction, workspace_id).await?;
    require_permission(&role, "memberRead")?;
    let members = sqlx::query_as::<_, TeamMember>("SELECT id,workspace_id,display_name,role,status,joined_at,updated_at,removed_at FROM team_members WHERE workspace_id=? ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END,display_name COLLATE NOCASE")
        .bind(workspace_id)
        .fetch_all(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(members)
}

pub async fn permission_report(
    db: &Database,
    workspace_id: &str,
) -> AppResult<TeamPermissionReport> {
    let mut transaction = db.pool.begin().await?;
    let (member_id, role) = local_authorization(&mut transaction, workspace_id).await?;
    let report = TeamPermissionReport {
        workspace_id: workspace_id.into(),
        member_id,
        role: role.clone(),
        permissions: permissions(&role)?
            .iter()
            .map(|value| (*value).into())
            .collect(),
    };
    transaction.commit().await?;
    Ok(report)
}

pub async fn save_member(db: &Database, input: SaveTeamMemberInput) -> AppResult<TeamMember> {
    if !ROLES.contains(&input.role.as_str()) {
        return Err(AppError::Validation("团队角色无效".into()));
    }
    let display_name = clean_name(&input.display_name, "成员名称")?;
    let mut transaction = db.pool.begin().await?;
    let (actor_id, actor_role) = local_authorization(&mut transaction, &input.workspace_id).await?;
    require_permission(&actor_role, "memberManage")?;
    let member_id = input
        .member_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    if !valid_uuid(&member_id) {
        return Err(AppError::Validation("团队成员 ID 无效".into()));
    }
    let member_workspace: Option<String> =
        sqlx::query_scalar("SELECT workspace_id FROM team_members WHERE id=?")
            .bind(&member_id)
            .fetch_optional(&mut *transaction)
            .await?;
    if member_workspace
        .as_deref()
        .is_some_and(|value| value != input.workspace_id)
    {
        return Err(AppError::Validation(
            "团队成员 ID 已属于另一个工作区".into(),
        ));
    }
    let existing = sqlx::query_as::<_, TeamMember>("SELECT id,workspace_id,display_name,role,status,joined_at,updated_at,removed_at FROM team_members WHERE id=? AND workspace_id=?")
        .bind(&member_id)
        .bind(&input.workspace_id)
        .fetch_optional(&mut *transaction)
        .await?;
    if input.role == "owner"
        || existing
            .as_ref()
            .is_some_and(|member| member.role == "owner")
    {
        require_permission(&actor_role, "ownerManage")?;
    }
    if existing.is_none() {
        let active_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM team_members WHERE workspace_id=? AND status='active'",
        )
        .bind(&input.workspace_id)
        .fetch_one(&mut *transaction)
        .await?;
        if active_count >= MAX_MEMBERS {
            return Err(AppError::Validation("每个团队最多 256 名活动成员".into()));
        }
    }
    if existing.as_ref().is_some_and(|member| {
        member.role == "owner" && input.role != "owner" && member.status == "active"
    }) {
        let owner_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM team_members WHERE workspace_id=? AND role='owner' AND status='active'")
            .bind(&input.workspace_id)
            .fetch_one(&mut *transaction)
            .await?;
        if owner_count <= 1 {
            return Err(AppError::Validation(
                "团队必须至少保留一名活动 Owner".into(),
            ));
        }
    }
    let now = Utc::now().to_rfc3339();
    let joined_at = existing
        .as_ref()
        .map(|member| member.joined_at.clone())
        .unwrap_or_else(|| now.clone());
    let role_changed = existing
        .as_ref()
        .is_some_and(|member| member.role != input.role || member.status != "active");
    sqlx::query("INSERT INTO team_members(id,workspace_id,display_name,role,status,joined_at,updated_at,removed_at) VALUES(?,?,?,?,'active',?,?,NULL) ON CONFLICT(id) DO UPDATE SET display_name=excluded.display_name,role=excluded.role,status='active',updated_at=excluded.updated_at,removed_at=NULL")
        .bind(&member_id)
        .bind(&input.workspace_id)
        .bind(&display_name)
        .bind(&input.role)
        .bind(&joined_at)
        .bind(&now)
        .execute(&mut *transaction)
        .await?;
    if role_changed {
        sqlx::query("UPDATE team_workspaces SET key_epoch=key_epoch+1,updated_at=? WHERE id=?")
            .bind(&now)
            .bind(&input.workspace_id)
            .execute(&mut *transaction)
            .await?;
    }
    let action = match (&existing, role_changed) {
        (None, _) => "member-added",
        (Some(member), true) if member.status == "removed" => "member-restored",
        (Some(_), true) => "member-role-changed",
        _ => "member-updated",
    };
    audit(
        &mut transaction,
        &input.workspace_id,
        &actor_id,
        action,
        "member",
        &member_id,
    )
    .await?;
    transaction.commit().await?;
    Ok(TeamMember {
        id: member_id,
        workspace_id: input.workspace_id,
        display_name,
        role: input.role,
        status: "active".into(),
        joined_at,
        updated_at: now,
        removed_at: None,
    })
}

pub async fn remove_member(db: &Database, workspace_id: &str, member_id: &str) -> AppResult<()> {
    if !valid_uuid(member_id) {
        return Err(AppError::Validation("团队成员 ID 无效".into()));
    }
    let mut transaction = db.pool.begin().await?;
    let (actor_id, actor_role) = local_authorization(&mut transaction, workspace_id).await?;
    require_permission(&actor_role, "memberManage")?;
    if actor_id == member_id {
        return Err(AppError::Validation(
            "本机成员不能在本地移除自己；请先在服务端完成 Owner/设备交接".into(),
        ));
    }
    let target = sqlx::query_as::<_, TeamMember>("SELECT id,workspace_id,display_name,role,status,joined_at,updated_at,removed_at FROM team_members WHERE id=? AND workspace_id=?")
        .bind(member_id)
        .bind(workspace_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("团队成员 {member_id}")))?;
    if target.status != "active" {
        return Ok(());
    }
    if target.role == "owner" {
        require_permission(&actor_role, "ownerManage")?;
        let owner_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM team_members WHERE workspace_id=? AND role='owner' AND status='active'")
            .bind(workspace_id)
            .fetch_one(&mut *transaction)
            .await?;
        if owner_count <= 1 {
            return Err(AppError::Validation(
                "团队必须至少保留一名活动 Owner".into(),
            ));
        }
    }
    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE team_members SET status='removed',removed_at=?,updated_at=? WHERE id=? AND workspace_id=?")
        .bind(&now)
        .bind(&now)
        .bind(member_id)
        .bind(workspace_id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("UPDATE team_workspaces SET key_epoch=key_epoch+1,updated_at=? WHERE id=?")
        .bind(&now)
        .bind(workspace_id)
        .execute(&mut *transaction)
        .await?;
    audit(
        &mut transaction,
        workspace_id,
        &actor_id,
        "member-removed",
        "member",
        member_id,
    )
    .await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn list_audit(db: &Database, workspace_id: &str) -> AppResult<Vec<TeamAuditEvent>> {
    let mut transaction = db.pool.begin().await?;
    let (_, role) = local_authorization(&mut transaction, workspace_id).await?;
    require_permission(&role, "auditRead")?;
    let events = sqlx::query_as::<_, TeamAuditEvent>("SELECT id,workspace_id,actor_member_id,action,target_type,target_id,created_at FROM team_audit_events WHERE workspace_id=? ORDER BY created_at DESC,rowid DESC LIMIT ?")
        .bind(workspace_id)
        .bind(MAX_AUDIT_EXPORT)
        .fetch_all(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(events)
}

pub async fn export_audit(db: &Database, workspace_id: &str, path: &str) -> AppResult<usize> {
    if path.len() > 16 * 1024 {
        return Err(AppError::Validation("团队审计导出路径超过 16 KB".into()));
    }
    let target = Path::new(path);
    if !target.is_absolute() || target.extension().and_then(|value| value.to_str()) != Some("json")
    {
        return Err(AppError::Validation(
            "团队审计必须导出为绝对路径 JSON 文件".into(),
        ));
    }
    let parent = target
        .parent()
        .filter(|value| value.is_dir())
        .ok_or_else(|| AppError::Validation("团队审计导出目录不存在".into()))?;
    let mut transaction = db.pool.begin().await?;
    let (actor_id, role) = local_authorization(&mut transaction, workspace_id).await?;
    require_permission(&role, "auditExport")?;
    let events = sqlx::query_as::<_, TeamAuditEvent>("SELECT id,workspace_id,actor_member_id,action,target_type,target_id,created_at FROM team_audit_events WHERE workspace_id=? ORDER BY created_at DESC,rowid DESC LIMIT ?")
        .bind(workspace_id)
        .bind(MAX_AUDIT_EXPORT)
        .fetch_all(&mut *transaction)
        .await?;
    transaction.commit().await?;
    let count = events.len();
    let payload = serde_json::to_vec_pretty(&json!({
        "schemaVersion": 1,
        "workspaceId": workspace_id,
        "exportedAt": Utc::now().to_rfc3339(),
        "events": events,
    }))
    .map_err(|error| AppError::Internal(error.to_string()))?;
    let target = target.to_path_buf();
    let written_target = target.clone();
    let temporary = parent.join(format!(".cnshell-team-audit-{}.tmp", uuid::Uuid::new_v4()));
    tokio::task::spawn_blocking(move || -> AppResult<()> {
        let result = (|| {
            let mut file = std::fs::File::options()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
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
    .map_err(|error| AppError::Internal(format!("团队审计导出任务失败：{error}")))??;
    let mut transaction = match db.pool.begin().await {
        Ok(transaction) => transaction,
        Err(error) => {
            let _ = std::fs::remove_file(&written_target);
            return Err(error.into());
        }
    };
    if let Err(error) = audit(
        &mut transaction,
        workspace_id,
        &actor_id,
        "audit-exported",
        "workspace",
        workspace_id,
    )
    .await
    {
        let _ = std::fs::remove_file(&written_target);
        return Err(error);
    }
    if let Err(error) = transaction.commit().await {
        let _ = std::fs::remove_file(&written_target);
        return Err(error.into());
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn role_matrix_is_least_privilege() {
        assert!(permissions("owner").unwrap().contains(&"workspaceDelete"));
        assert!(!permissions("admin").unwrap().contains(&"ownerManage"));
        assert!(
            permissions("operator")
                .unwrap()
                .contains(&"terminalControl")
        );
        assert!(
            !permissions("operator")
                .unwrap()
                .contains(&"connectionManage")
        );
        assert_eq!(permissions("viewer").unwrap(), VIEWER_PERMISSIONS);
    }

    #[tokio::test]
    async fn workspace_members_roles_and_key_epochs_are_transactional() {
        let directory = tempdir().unwrap();
        let db = Database::open(&directory.path().join("cnshell.sqlite"))
            .await
            .unwrap();
        let workspace = create_workspace(
            &db,
            CreateTeamWorkspaceInput {
                name: "Ops".into(),
                owner_name: "Alice".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(workspace.local_role, "owner");
        let report = permission_report(&db, &workspace.id).await.unwrap();
        assert!(report.permissions.contains(&"ownerManage".into()));

        let bob = save_member(
            &db,
            SaveTeamMemberInput {
                workspace_id: workspace.id.clone(),
                member_id: None,
                display_name: "Bob".into(),
                role: "admin".into(),
            },
        )
        .await
        .unwrap();
        let other_workspace = create_workspace(
            &db,
            CreateTeamWorkspaceInput {
                name: "Zeta".into(),
                owner_name: "Zoe".into(),
            },
        )
        .await
        .unwrap();
        let collision = save_member(
            &db,
            SaveTeamMemberInput {
                workspace_id: other_workspace.id,
                member_id: Some(bob.id.clone()),
                display_name: "Collision".into(),
                role: "viewer".into(),
            },
        )
        .await
        .unwrap_err();
        assert!(collision.to_string().contains("另一个工作区"));
        let second_owner = save_member(
            &db,
            SaveTeamMemberInput {
                workspace_id: workspace.id.clone(),
                member_id: None,
                display_name: "Carol".into(),
                role: "owner".into(),
            },
        )
        .await
        .unwrap();
        let before_remove = list_workspaces(&db).await.unwrap().remove(0).key_epoch;
        remove_member(&db, &workspace.id, &bob.id).await.unwrap();
        let after_remove = list_workspaces(&db).await.unwrap().remove(0).key_epoch;
        assert_eq!(after_remove, before_remove + 1);

        sqlx::query("UPDATE team_workspaces SET local_member_id=? WHERE id=?")
            .bind(&bob.id)
            .bind(&workspace.id)
            .execute(&db.pool)
            .await
            .unwrap();
        sqlx::query("UPDATE team_members SET status='active',removed_at=NULL WHERE id=?")
            .bind(&bob.id)
            .execute(&db.pool)
            .await
            .unwrap();
        let denied = save_member(
            &db,
            SaveTeamMemberInput {
                workspace_id: workspace.id.clone(),
                member_id: Some(second_owner.id),
                display_name: "Carol".into(),
                role: "viewer".into(),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(denied, AppError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn audit_export_contains_metadata_but_no_output_or_credentials() {
        let directory = tempdir().unwrap();
        let db = Database::open(&directory.path().join("cnshell.sqlite"))
            .await
            .unwrap();
        let workspace = create_workspace(
            &db,
            CreateTeamWorkspaceInput {
                name: "Security".into(),
                owner_name: "Owner".into(),
            },
        )
        .await
        .unwrap();
        let export_path = directory.path().join("audit.json");
        assert_eq!(
            export_audit(&db, &workspace.id, export_path.to_str().unwrap())
                .await
                .unwrap(),
            1
        );
        let exported = std::fs::read_to_string(export_path).unwrap();
        assert!(exported.contains("workspace-created"));
        assert!(!exported.contains("terminalOutput"));
        assert!(!exported.contains("credential"));
        assert_eq!(list_audit(&db, &workspace.id).await.unwrap().len(), 2);
    }
}
