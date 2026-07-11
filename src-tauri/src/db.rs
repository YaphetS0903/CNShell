use crate::{
    error::{AppError, AppResult},
    models::*,
};
use chrono::Utc;
use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow},
};
use std::{path::Path, str::FromStr};

#[derive(Clone)]
pub struct Database {
    pub pool: SqlitePool,
}

impl Database {
    pub async fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        backup_database_files(path)?;
        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))?
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|error| AppError::Storage(error.to_string()))?;
        sqlx::query("UPDATE transfer_tasks SET status='failed', error='应用上次退出时传输未完成，请确认后重试' WHERE status IN ('queued','running','paused')")
            .execute(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn list_connections(&self) -> AppResult<Vec<ConnectionProfile>> {
        let rows = sqlx::query("SELECT id, folder_id, protocol, name, host, port, username, auth_type, private_key_path, host_key_policy, note, tags, encoding, startup_command, proxy_id, environment, credential_ref, created_at, updated_at, last_connected_at FROM connections WHERE deleted_at IS NULL ORDER BY name COLLATE NOCASE")
            .fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(connection_from_row).collect())
    }

    pub async fn deleted_connections(&self) -> AppResult<Vec<ConnectionProfile>> {
        let rows=sqlx::query("SELECT id, folder_id, protocol, name, host, port, username, auth_type, private_key_path, host_key_policy, note, tags, encoding, startup_command, proxy_id, environment, credential_ref, created_at, updated_at, last_connected_at FROM connections WHERE deleted_at IS NOT NULL ORDER BY deleted_at DESC").fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(connection_from_row).collect())
    }

    pub async fn get_connection(&self, id: &str) -> AppResult<ConnectionProfile> {
        self.list_connections()
            .await?
            .into_iter()
            .find(|item| item.id == id)
            .ok_or_else(|| AppError::NotFound(id.into()))
    }

    pub async fn connection_id_exists(&self, id: &str) -> AppResult<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM connections WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        Ok(count > 0)
    }

    pub async fn sanitize_import_references(
        &self,
        input: &mut SaveConnectionInput,
    ) -> AppResult<()> {
        if let Some(folder_id) = input.folder_id.as_deref() {
            let exists: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM folders WHERE id=? AND deleted_at IS NULL",
            )
            .bind(folder_id)
            .fetch_one(&self.pool)
            .await?;
            if exists == 0 {
                input.folder_id = None;
            }
        }
        if let Some(proxy_id) = input.proxy_id.as_deref() {
            let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM proxies WHERE id=?")
                .bind(proxy_id)
                .fetch_one(&self.pool)
                .await?;
            if exists == 0 {
                input.proxy_id = None;
            }
        }
        Ok(())
    }

    pub async fn import_connections(
        &self,
        inputs: &[(SaveConnectionInput, Option<String>)],
    ) -> AppResult<()> {
        let mut transaction = self.pool.begin().await?;
        for (input, credential_ref) in inputs {
            let now = Utc::now().to_rfc3339();
            let existing =
                sqlx::query_scalar::<_, String>("SELECT created_at FROM connections WHERE id=?")
                    .bind(&input.id)
                    .fetch_optional(&mut *transaction)
                    .await?;
            let created = existing.unwrap_or_else(|| now.clone());
            sqlx::query("INSERT INTO connections (id,folder_id,protocol,name,host,port,username,auth_type,credential_ref,private_key_path,host_key_policy,note,tags,encoding,startup_command,proxy_id,environment,created_at,updated_at,deleted_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,NULL) ON CONFLICT(id) DO UPDATE SET folder_id=excluded.folder_id,protocol=excluded.protocol,name=excluded.name,host=excluded.host,port=excluded.port,username=excluded.username,auth_type=excluded.auth_type,credential_ref=CASE WHEN excluded.protocol='ssh' AND excluded.auth_type='sshAgent' THEN NULL WHEN connections.auth_type!=excluded.auth_type THEN excluded.credential_ref ELSE COALESCE(excluded.credential_ref,connections.credential_ref) END,private_key_path=excluded.private_key_path,host_key_policy=excluded.host_key_policy,note=excluded.note,tags=excluded.tags,encoding=excluded.encoding,startup_command=excluded.startup_command,proxy_id=excluded.proxy_id,environment=excluded.environment,updated_at=excluded.updated_at,deleted_at=NULL")
                .bind(&input.id).bind(&input.folder_id).bind(&input.protocol).bind(&input.name).bind(&input.host).bind(input.port).bind(&input.username).bind(&input.auth_type).bind(credential_ref).bind(&input.private_key_path).bind(&input.host_key_policy).bind(&input.note).bind(serde_json::to_string(&input.tags).unwrap_or_else(|_|"[]".into())).bind(&input.encoding).bind(&input.startup_command).bind(&input.proxy_id).bind(serde_json::to_string(&input.environment).unwrap_or_else(|_|"{}".into())).bind(&created).bind(&now).execute(&mut *transaction).await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn set_connection_credential_ref(
        &self,
        id: &str,
        credential_ref: &str,
    ) -> AppResult<ConnectionProfile> {
        let affected = sqlx::query("UPDATE connections SET credential_ref=?,updated_at=? WHERE id=? AND deleted_at IS NULL")
            .bind(credential_ref)
            .bind(Utc::now().to_rfc3339())
            .bind(id)
            .execute(&self.pool)
            .await?
            .rows_affected();
        if affected == 0 {
            return Err(AppError::NotFound(id.into()));
        }
        self.get_connection(id).await
    }

    pub async fn remove_inserted_connection(&self, id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM connections WHERE id=?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn save_connection(
        &self,
        input: &SaveConnectionInput,
        credential_ref: Option<&str>,
    ) -> AppResult<ConnectionProfile> {
        validate_connection(input)?;
        let now = Utc::now().to_rfc3339();
        let existing =
            sqlx::query_scalar::<_, String>("SELECT created_at FROM connections WHERE id = ?")
                .bind(&input.id)
                .fetch_optional(&self.pool)
                .await?;
        let created = existing.unwrap_or_else(|| now.clone());
        sqlx::query("INSERT INTO connections (id,folder_id,protocol,name,host,port,username,auth_type,credential_ref,private_key_path,host_key_policy,note,tags,encoding,startup_command,proxy_id,environment,created_at,updated_at,deleted_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,NULL) ON CONFLICT(id) DO UPDATE SET folder_id=excluded.folder_id,protocol=excluded.protocol,name=excluded.name,host=excluded.host,port=excluded.port,username=excluded.username,auth_type=excluded.auth_type,credential_ref=CASE WHEN excluded.protocol='ssh' AND excluded.auth_type='sshAgent' THEN NULL WHEN connections.auth_type!=excluded.auth_type THEN excluded.credential_ref ELSE COALESCE(excluded.credential_ref,connections.credential_ref) END,private_key_path=excluded.private_key_path,host_key_policy=excluded.host_key_policy,note=excluded.note,tags=excluded.tags,encoding=excluded.encoding,startup_command=excluded.startup_command,proxy_id=excluded.proxy_id,environment=excluded.environment,updated_at=excluded.updated_at,deleted_at=NULL")
            .bind(&input.id).bind(&input.folder_id).bind(&input.protocol).bind(&input.name).bind(&input.host).bind(input.port).bind(&input.username).bind(&input.auth_type).bind(credential_ref).bind(&input.private_key_path).bind(&input.host_key_policy).bind(&input.note).bind(serde_json::to_string(&input.tags).unwrap_or_else(|_| "[]".into())).bind(&input.encoding).bind(&input.startup_command).bind(&input.proxy_id).bind(serde_json::to_string(&input.environment).unwrap_or_else(|_|"{}".into())).bind(&created).bind(&now).execute(&self.pool).await?;
        self.get_connection(&input.id).await
    }

    pub async fn insert_connection(
        &self,
        input: &SaveConnectionInput,
        credential_ref: Option<&str>,
    ) -> AppResult<ConnectionProfile> {
        validate_connection(input)?;
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO connections (id,folder_id,protocol,name,host,port,username,auth_type,credential_ref,private_key_path,host_key_policy,note,tags,encoding,startup_command,proxy_id,environment,created_at,updated_at,deleted_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,NULL)")
            .bind(&input.id).bind(&input.folder_id).bind(&input.protocol).bind(&input.name).bind(&input.host).bind(input.port).bind(&input.username).bind(&input.auth_type).bind(credential_ref).bind(&input.private_key_path).bind(&input.host_key_policy).bind(&input.note).bind(serde_json::to_string(&input.tags).unwrap_or_else(|_| "[]".into())).bind(&input.encoding).bind(&input.startup_command).bind(&input.proxy_id).bind(serde_json::to_string(&input.environment).unwrap_or_else(|_|"{}".into())).bind(&now).bind(&now).execute(&self.pool).await?;
        self.get_connection(&input.id).await
    }

    pub async fn delete_connection(&self, id: &str) -> AppResult<()> {
        let result = sqlx::query("UPDATE connections SET deleted_at=? WHERE id=?")
            .bind(Utc::now().to_rfc3339())
            .bind(id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(id.into()));
        }
        Ok(())
    }

    pub async fn restore_connection(&self, id: &str) -> AppResult<()> {
        let affected=sqlx::query("UPDATE connections SET deleted_at=NULL,updated_at=? WHERE id=? AND deleted_at IS NOT NULL").bind(Utc::now().to_rfc3339()).bind(id).execute(&self.pool).await?.rows_affected();
        if affected == 0 {
            return Err(AppError::NotFound(id.into()));
        }
        Ok(())
    }
    pub async fn purge_connection(&self, id: &str) -> AppResult<()> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query("UPDATE proxies SET jump_connection_id=NULL WHERE jump_connection_id=?")
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM command_history WHERE connection_id=?")
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        let affected = sqlx::query("DELETE FROM connections WHERE id=? AND deleted_at IS NOT NULL")
            .bind(id)
            .execute(&mut *transaction)
            .await?
            .rows_affected();
        if affected == 0 {
            return Err(AppError::NotFound(id.into()));
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn folders(&self) -> AppResult<Vec<Folder>> {
        Ok(sqlx::query_as("SELECT id,name,parent_id,sort_order FROM folders WHERE deleted_at IS NULL ORDER BY sort_order,name").fetch_all(&self.pool).await?)
    }
    pub async fn save_folder(
        &self,
        id: &str,
        name: &str,
        parent_id: Option<&str>,
    ) -> AppResult<Folder> {
        if id.is_empty() || id.len() > 128 || name.trim().is_empty() || name.len() > 256 {
            return Err(AppError::Validation("文件夹 ID 或名称无效".into()));
        }
        if let Some(parent) = parent_id {
            if parent == id {
                return Err(AppError::Validation("文件夹不能作为自己的上级".into()));
            }
            let exists: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM folders WHERE id=? AND deleted_at IS NULL",
            )
            .bind(parent)
            .fetch_one(&self.pool)
            .await?;
            if exists == 0 {
                return Err(AppError::NotFound(format!("上级文件夹 {parent}")));
            }
            let creates_cycle: i64 = sqlx::query_scalar("WITH RECURSIVE descendants(id) AS (SELECT id FROM folders WHERE parent_id=? AND deleted_at IS NULL UNION ALL SELECT folders.id FROM folders JOIN descendants ON folders.parent_id=descendants.id WHERE folders.deleted_at IS NULL) SELECT COUNT(*) FROM descendants WHERE id=?")
                .bind(id).bind(parent).fetch_one(&self.pool).await?;
            if creates_cycle > 0 {
                return Err(AppError::Validation("文件夹层级不能形成循环".into()));
            }
        }
        let sort_order: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sort_order),-1)+1 FROM folders WHERE deleted_at IS NULL",
        )
        .fetch_one(&self.pool)
        .await?;
        sqlx::query("INSERT INTO folders(id,name,parent_id,sort_order,deleted_at)VALUES(?,?,?,?,NULL)ON CONFLICT(id)DO UPDATE SET name=excluded.name,parent_id=excluded.parent_id,deleted_at=NULL").bind(id).bind(name.trim()).bind(parent_id).bind(sort_order).execute(&self.pool).await?;
        sqlx::query_as("SELECT id,name,parent_id,sort_order FROM folders WHERE id=?")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(AppError::from)
    }
    pub async fn delete_folder(&self, id: &str) -> AppResult<()> {
        let mut transaction = self.pool.begin().await?;
        let exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM folders WHERE id=? AND deleted_at IS NULL")
                .bind(id)
                .fetch_one(&mut *transaction)
                .await?;
        if exists == 0 {
            return Err(AppError::NotFound(format!("文件夹 {id}")));
        }
        sqlx::query("WITH RECURSIVE subtree(id) AS (SELECT id FROM folders WHERE id=? AND deleted_at IS NULL UNION ALL SELECT folders.id FROM folders JOIN subtree ON folders.parent_id=subtree.id WHERE folders.deleted_at IS NULL) UPDATE connections SET folder_id=NULL WHERE folder_id IN (SELECT id FROM subtree)")
            .bind(id).execute(&mut *transaction).await?;
        sqlx::query("WITH RECURSIVE subtree(id) AS (SELECT id FROM folders WHERE id=? AND deleted_at IS NULL UNION ALL SELECT folders.id FROM folders JOIN subtree ON folders.parent_id=subtree.id WHERE folders.deleted_at IS NULL) UPDATE folders SET deleted_at=? WHERE id IN (SELECT id FROM subtree)")
            .bind(id).bind(Utc::now().to_rfc3339()).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(())
    }
    pub async fn move_connection(&self, id: &str, folder_id: Option<&str>) -> AppResult<()> {
        if let Some(folder) = folder_id {
            let exists: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM folders WHERE id=? AND deleted_at IS NULL",
            )
            .bind(folder)
            .fetch_one(&self.pool)
            .await?;
            if exists == 0 {
                return Err(AppError::NotFound(format!("文件夹 {folder}")));
            }
        }
        let affected = sqlx::query(
            "UPDATE connections SET folder_id=?,updated_at=? WHERE id=? AND deleted_at IS NULL",
        )
        .bind(folder_id)
        .bind(Utc::now().to_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if affected == 0 {
            return Err(AppError::NotFound(id.into()));
        }
        Ok(())
    }
    pub async fn mark_connected(&self, id: &str) -> AppResult<()> {
        sqlx::query("UPDATE connections SET last_connected_at=? WHERE id=?")
            .bind(Utc::now().to_rfc3339())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn known_host(&self, host: &str, port: i64) -> AppResult<Option<(String, String)>> {
        Ok(
            sqlx::query("SELECT algorithm,fingerprint FROM known_hosts WHERE host=? AND port=?")
                .bind(host)
                .bind(port)
                .fetch_optional(&self.pool)
                .await?
                .map(|row| (row.get("algorithm"), row.get("fingerprint"))),
        )
    }

    pub async fn trust_host(
        &self,
        host: &str,
        port: i64,
        algorithm: &str,
        fingerprint: &str,
    ) -> AppResult<()> {
        sqlx::query("INSERT INTO known_hosts(host,port,algorithm,fingerprint,updated_at) VALUES(?,?,?,?,?) ON CONFLICT(host,port) DO UPDATE SET algorithm=excluded.algorithm,fingerprint=excluded.fingerprint,updated_at=excluded.updated_at")
            .bind(host).bind(port).bind(algorithm).bind(fingerprint).bind(Utc::now().to_rfc3339()).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn transfers(&self) -> AppResult<Vec<TransferTask>> {
        Ok(sqlx::query_as("SELECT id,session_id,direction,source,destination,total_bytes,transferred_bytes,status,conflict_policy,error,created_at FROM transfer_tasks ORDER BY created_at DESC").fetch_all(&self.pool).await?)
    }

    pub async fn upsert_transfer(&self, task: &TransferTask) -> AppResult<()> {
        sqlx::query("INSERT INTO transfer_tasks(id,session_id,direction,source,destination,total_bytes,transferred_bytes,status,conflict_policy,error,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET total_bytes=excluded.total_bytes,transferred_bytes=excluded.transferred_bytes,status=excluded.status,error=excluded.error")
            .bind(&task.id).bind(&task.session_id).bind(&task.direction).bind(&task.source).bind(&task.destination).bind(task.total_bytes).bind(task.transferred_bytes).bind(&task.status).bind(&task.conflict_policy).bind(&task.error).bind(&task.created_at).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn get_settings(&self) -> AppResult<AppSettings> {
        let value = sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key='app'")
            .fetch_optional(&self.pool)
            .await?;
        Ok(value
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default())
    }

    pub async fn save_settings(&self, settings: &AppSettings) -> AppResult<()> {
        let value = serde_json::to_string(settings)
            .map_err(|error| AppError::Storage(error.to_string()))?;
        sqlx::query("INSERT INTO settings(key,value) VALUES('app',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value").bind(value).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn proxies(&self) -> AppResult<Vec<ProxyProfile>> {
        let rows=sqlx::query("SELECT id,name,type,host,port,username,jump_connection_id,credential_ref FROM proxies ORDER BY name").fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|row| ProxyProfile {
                id: row.get("id"),
                name: row.get("name"),
                proxy_type: row.get("type"),
                host: row.get("host"),
                port: row.get("port"),
                username: row.get("username"),
                jump_connection_id: row.get("jump_connection_id"),
                has_credential: row.get::<Option<String>, _>("credential_ref").is_some(),
            })
            .collect())
    }
    pub async fn get_proxy(&self, id: &str) -> AppResult<ProxyProfile> {
        self.proxies()
            .await?
            .into_iter()
            .find(|item| item.id == id)
            .ok_or_else(|| AppError::NotFound(format!("代理 {id}")))
    }
    pub async fn save_proxy(
        &self,
        input: &SaveProxyInput,
        credential_ref: Option<&str>,
    ) -> AppResult<ProxyProfile> {
        validate_proxy(input)?;
        if input.proxy_type == "sshJump" {
            let jump_id = input.jump_connection_id.as_deref().unwrap_or("");
            let protocol: Option<String> = sqlx::query_scalar(
                "SELECT protocol FROM connections WHERE id=? AND deleted_at IS NULL",
            )
            .bind(jump_id)
            .fetch_optional(&self.pool)
            .await?;
            if protocol.as_deref() != Some("ssh") {
                return Err(AppError::Validation("跳板连接不存在或不是 SSH 连接".into()));
            }
        }
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO proxies(id,name,type,host,port,username,credential_ref,jump_connection_id,created_at,updated_at)VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id)DO UPDATE SET name=excluded.name,type=excluded.type,host=excluded.host,port=excluded.port,username=excluded.username,credential_ref=CASE WHEN excluded.type='sshJump' OR excluded.username IS NULL OR excluded.username='' THEN NULL ELSE COALESCE(excluded.credential_ref,proxies.credential_ref) END,jump_connection_id=excluded.jump_connection_id,updated_at=excluded.updated_at").bind(&input.id).bind(&input.name).bind(&input.proxy_type).bind(&input.host).bind(input.port).bind(&input.username).bind(credential_ref).bind(&input.jump_connection_id).bind(&now).bind(&now).execute(&self.pool).await?;
        self.get_proxy(&input.id).await
    }
    pub async fn delete_proxy(&self, id: &str) -> AppResult<()> {
        let used: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM connections WHERE proxy_id=?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        if used > 0 {
            return Err(AppError::Validation(
                "该代理仍被连接或已删除项目使用".into(),
            ));
        }
        let affected = sqlx::query("DELETE FROM proxies WHERE id=?")
            .bind(id)
            .execute(&self.pool)
            .await?
            .rows_affected();
        if affected == 0 {
            return Err(AppError::NotFound(format!("代理 {id}")));
        }
        Ok(())
    }
    pub async fn forwards(&self, connection_id: &str) -> AppResult<Vec<PortForward>> {
        let mut items:Vec<PortForward>=sqlx::query_as("SELECT id,connection_id,type,bind_host,bind_port,destination_host,destination_port,auto_start FROM port_forwards WHERE connection_id=? ORDER BY bind_port").bind(connection_id).fetch_all(&self.pool).await?;
        for item in &mut items {
            item.status = Some("stopped".into());
        }
        Ok(items)
    }
    pub async fn get_forward(&self, id: &str) -> AppResult<PortForward> {
        let mut item:PortForward=sqlx::query_as("SELECT id,connection_id,type,bind_host,bind_port,destination_host,destination_port,auto_start FROM port_forwards WHERE id=?").bind(id).fetch_optional(&self.pool).await?.ok_or_else(||AppError::NotFound(format!("隧道 {id}")))?;
        item.status = Some("stopped".into());
        Ok(item)
    }
    pub async fn save_forward(&self, input: &PortForward) -> AppResult<PortForward> {
        validate_forward(input)?;
        sqlx::query("INSERT INTO port_forwards(id,connection_id,type,bind_host,bind_port,destination_host,destination_port,auto_start)VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(id)DO UPDATE SET type=excluded.type,bind_host=excluded.bind_host,bind_port=excluded.bind_port,destination_host=excluded.destination_host,destination_port=excluded.destination_port,auto_start=excluded.auto_start").bind(&input.id).bind(&input.connection_id).bind(&input.forward_type).bind(&input.bind_host).bind(input.bind_port).bind(&input.destination_host).bind(input.destination_port).bind(input.auto_start).execute(&self.pool).await?;
        self.get_forward(&input.id).await
    }
    pub async fn delete_forward(&self, id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM port_forwards WHERE id=?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    pub async fn snippets(&self) -> AppResult<Vec<CommandSnippet>> {
        let rows=sqlx::query("SELECT id,name,command,description,tags,sort_order FROM command_snippets ORDER BY sort_order,name").fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|row| CommandSnippet {
                id: row.get("id"),
                name: row.get("name"),
                command: row.get("command"),
                description: row.get("description"),
                tags: serde_json::from_str(row.get::<String, _>("tags").as_str())
                    .unwrap_or_default(),
                sort_order: row.get("sort_order"),
            })
            .collect())
    }
    pub async fn save_snippet(&self, input: &CommandSnippet) -> AppResult<CommandSnippet> {
        if input.id.is_empty()
            || input.id.len() > 128
            || input.name.trim().is_empty()
            || input.name.len() > 256
            || input.command.trim().is_empty()
            || input.command.len() > 64 * 1024
            || input.description.len() > 4096
            || input.tags.len() > 100
            || input.tags.iter().any(|tag| tag.len() > 256)
        {
            return Err(AppError::Validation(
                "快捷命令字段无效或超过长度限制".into(),
            ));
        }
        sqlx::query("INSERT INTO command_snippets(id,name,command,description,tags,sort_order)VALUES(?,?,?,?,?,?)ON CONFLICT(id)DO UPDATE SET name=excluded.name,command=excluded.command,description=excluded.description,tags=excluded.tags,sort_order=excluded.sort_order").bind(&input.id).bind(&input.name).bind(&input.command).bind(&input.description).bind(serde_json::to_string(&input.tags).unwrap_or_else(|_|"[]".into())).bind(input.sort_order).execute(&self.pool).await?;
        Ok(input.clone())
    }
    pub async fn delete_snippet(&self, id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM command_snippets WHERE id=?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    pub async fn add_history(&self, connection_id: &str, command: &str) -> AppResult<()> {
        if command.len() > 64 * 1024 {
            return Err(AppError::Validation("单条命令历史不能超过 64 KB".into()));
        }
        if command.trim().is_empty() || looks_sensitive(command) {
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO command_history(id,connection_id,command,created_at)VALUES(?,?,?,?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(connection_id)
        .bind(command)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        sqlx::query("DELETE FROM command_history WHERE id IN (SELECT id FROM command_history WHERE connection_id=? ORDER BY created_at DESC LIMIT -1 OFFSET 500)").bind(connection_id).execute(&self.pool).await?;
        Ok(())
    }
    pub async fn history(&self, connection_id: &str) -> AppResult<Vec<String>> {
        Ok(sqlx::query_scalar("SELECT command FROM command_history WHERE connection_id=? ORDER BY created_at DESC LIMIT 500").bind(connection_id).fetch_all(&self.pool).await?)
    }
    pub async fn clear_history(&self) -> AppResult<u64> {
        Ok(sqlx::query("DELETE FROM command_history")
            .execute(&self.pool)
            .await?
            .rows_affected())
    }
    pub async fn save_workspace(&self, value: &serde_json::Value) -> AppResult<()> {
        let serialized = value.to_string();
        if serialized.len() > 1024 * 1024 {
            return Err(AppError::Validation("工作区状态不能超过 1 MB".into()));
        }
        sqlx::query("INSERT INTO workspace_state(key,value,updated_at)VALUES('main',?,?)ON CONFLICT(key)DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at").bind(serialized).bind(Utc::now().to_rfc3339()).execute(&self.pool).await?;
        Ok(())
    }
    pub async fn load_workspace(&self) -> AppResult<Option<serde_json::Value>> {
        let value =
            sqlx::query_scalar::<_, String>("SELECT value FROM workspace_state WHERE key='main'")
                .fetch_optional(&self.pool)
                .await?;
        Ok(value.and_then(|v| serde_json::from_str(&v).ok()))
    }
}

fn backup_database_files(path: &Path) -> AppResult<()> {
    if !path.exists() {
        return Ok(());
    }
    std::fs::copy(
        path,
        path.with_extension(format!(
            "{}.backup",
            path.extension()
                .and_then(|value| value.to_str())
                .unwrap_or("sqlite")
        )),
    )?;
    for suffix in ["-wal", "-shm"] {
        let sidecar = std::path::PathBuf::from(format!("{}{suffix}", path.display()));
        if sidecar.exists() {
            std::fs::copy(
                &sidecar,
                std::path::PathBuf::from(format!("{}.backup", sidecar.display())),
            )?;
        }
    }
    Ok(())
}

fn connection_from_row(row: SqliteRow) -> ConnectionProfile {
    ConnectionProfile {
        id: row.get("id"),
        folder_id: row.get("folder_id"),
        protocol: row.get("protocol"),
        name: row.get("name"),
        host: row.get("host"),
        port: row.get("port"),
        username: row.get("username"),
        auth_type: row.get("auth_type"),
        private_key_path: row.get("private_key_path"),
        host_key_policy: row.get("host_key_policy"),
        note: row.get("note"),
        tags: serde_json::from_str(row.get::<String, _>("tags").as_str()).unwrap_or_default(),
        encoding: row.get("encoding"),
        startup_command: row.get("startup_command"),
        proxy_id: row.get("proxy_id"),
        environment: serde_json::from_str(row.get::<String, _>("environment").as_str())
            .unwrap_or_default(),
        has_credential: row.get::<Option<String>, _>("credential_ref").is_some(),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        last_connected_at: row.get("last_connected_at"),
    }
}

fn looks_sensitive(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    [
        "password=",
        "passwd ",
        "token=",
        "secret=",
        "api_key=",
        "api-key=",
        "authorization:",
        "private_key=",
        "access_key=",
        "secret_access_key",
        "sshpass ",
        "curl -u ",
        "curl --user ",
        "mysql -p",
        "psql postgresql://",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

pub fn validate_connection(input: &SaveConnectionInput) -> AppResult<()> {
    if input.id.is_empty()
        || input.id.len() > 128
        || input.name.len() > 256
        || input.host.len() > 1024
        || input.username.len() > 256
        || input.note.len() > 64 * 1024
        || input.tags.len() > 100
        || input.tags.iter().any(|tag| tag.len() > 256)
        || input
            .startup_command
            .as_ref()
            .is_some_and(|value| value.len() > 64 * 1024)
        || input
            .private_key_path
            .as_ref()
            .is_some_and(|value| value.len() > 16 * 1024)
        || input
            .credential
            .as_ref()
            .is_some_and(|value| value.len() > 16 * 1024)
    {
        return Err(AppError::Validation("连接字段超过长度限制".into()));
    }
    if input.name.trim().is_empty() {
        return Err(AppError::Validation("名称不能为空".into()));
    }
    if input.host.trim().is_empty()
        || input
            .host
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(AppError::Validation("主机地址无效".into()));
    }
    if !(1..=65535).contains(&input.port) {
        return Err(AppError::Validation("端口必须在 1 到 65535 之间".into()));
    }
    if input.username.trim().is_empty() || input.username.chars().any(char::is_control) {
        return Err(AppError::Validation("用户名不能为空或包含控制字符".into()));
    }
    if !["ssh", "rdp"].contains(&input.protocol.as_str()) {
        return Err(AppError::Validation("不支持的协议".into()));
    }
    if input.protocol == "ssh"
        && !["password", "privateKey", "sshAgent"].contains(&input.auth_type.as_str())
    {
        return Err(AppError::Validation("SSH 认证方式无效".into()));
    }
    if input.auth_type == "privateKey" && input.private_key_path.as_deref().unwrap_or("").is_empty()
    {
        return Err(AppError::Validation("私钥认证必须选择私钥文件".into()));
    }
    if !["strict", "acceptNew"].contains(&input.host_key_policy.as_str()) {
        return Err(AppError::Validation("主机密钥策略无效".into()));
    }
    if input.encoding != "UTF-8" {
        return Err(AppError::Validation("当前版本仅支持 UTF-8 终端编码".into()));
    }
    if input.environment.iter().any(|(key, value)| {
        key.is_empty()
            || key.len() > 128
            || value.len() > 4096
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    }) {
        return Err(AppError::Validation("终端环境变量名称或值无效".into()));
    }
    Ok(())
}

pub fn validate_proxy(input: &SaveProxyInput) -> AppResult<()> {
    if input.id.is_empty()
        || input.id.len() > 128
        || input.name.trim().is_empty()
        || input.name.len() > 256
        || input.host.len() > 1024
        || input
            .username
            .as_ref()
            .is_some_and(|value| value.len() > 256)
        || input
            .credential
            .as_ref()
            .is_some_and(|value| value.len() > 16 * 1024)
    {
        return Err(AppError::Validation("代理字段无效或超过长度限制".into()));
    }
    match input.proxy_type.as_str() {
        "socks5" | "http" => {
            if input.host.trim().is_empty() {
                return Err(AppError::Validation("代理主机不能为空".into()));
            }
            if !(1..=65535).contains(&input.port) {
                return Err(AppError::Validation("代理端口无效".into()));
            }
        }
        "sshJump" => {
            if input.jump_connection_id.as_deref().unwrap_or("").is_empty() {
                return Err(AppError::Validation("请选择 SSH 跳板连接".into()));
            }
        }
        _ => return Err(AppError::Validation("代理类型无效".into())),
    }
    Ok(())
}

fn validate_forward(input: &PortForward) -> AppResult<()> {
    if !["local", "remote", "dynamic"].contains(&input.forward_type.as_str()) {
        return Err(AppError::Validation("端口转发类型无效".into()));
    }
    if input.bind_host.trim().is_empty() || !(1..=65535).contains(&input.bind_port) {
        return Err(AppError::Validation("监听地址或端口无效".into()));
    }
    if input.forward_type != "dynamic"
        && (input
            .destination_host
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
            || !matches!(input.destination_port, Some(1..=65535)))
    {
        return Err(AppError::Validation("目标地址或端口无效".into()));
    }
    Ok(())
}

pub fn validate_settings(settings: &AppSettings) -> AppResult<()> {
    if !["system", "dark", "light", "highContrast"].contains(&settings.theme.as_str()) {
        return Err(AppError::Validation("主题设置无效".into()));
    }
    if ![1000, 2000, 5000].contains(&settings.monitor_interval_ms) {
        return Err(AppError::Validation("监控刷新间隔无效".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_port() {
        let input = SaveConnectionInput {
            id: "1".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "x".into(),
            host: "host".into(),
            port: 0,
            username: "root".into(),
            auth_type: "sshAgent".into(),
            private_key_path: None,
            host_key_policy: "strict".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            credential: None,
        };
        assert!(validate_connection(&input).is_err());
    }
    #[test]
    fn rejects_invalid_settings_and_environment() {
        let mut settings = AppSettings::default();
        settings.theme = "invented".into();
        assert!(validate_settings(&settings).is_err());
        let mut input = SaveConnectionInput {
            id: "1".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "x".into(),
            host: "host".into(),
            port: 22,
            username: "root".into(),
            auth_type: "sshAgent".into(),
            private_key_path: None,
            host_key_policy: "strict".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            credential: None,
        };
        input.environment.insert("BAD-NAME".into(), "x".into());
        assert!(validate_connection(&input).is_err());
        let proxy = SaveProxyInput {
            id: "proxy".into(),
            name: "bad".into(),
            proxy_type: "socks5".into(),
            host: "host".into(),
            port: 0,
            username: None,
            jump_connection_id: None,
            credential: Some("must-not-be-stored".into()),
        };
        assert!(validate_proxy(&proxy).is_err());
        let mut jump = proxy;
        jump.proxy_type = "sshJump".into();
        jump.host = "".into();
        jump.port = 1080;
        jump.credential = None;
        assert!(validate_proxy(&jump).is_err());
        jump.jump_connection_id = Some("connection".into());
        assert!(validate_proxy(&jump).is_ok());
    }
    #[test]
    fn sensitive_history_is_detected() {
        for command in [
            "export TOKEN=abc",
            "export AWS_SECRET_ACCESS_KEY=abc",
            "sshpass -p secret ssh host",
            "curl -u user:password https://example.test",
            "mysql -proot",
        ] {
            assert!(
                looks_sensitive(command),
                "missed sensitive command: {command}"
            );
        }
        assert!(!looks_sensitive("uname -a"));
    }
    #[tokio::test]
    async fn rejects_oversized_ipc_payloads() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("limits.sqlite"))
            .await
            .unwrap();
        let mut input = SaveConnectionInput {
            id: "limit".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "Server".into(),
            host: "example.test".into(),
            port: 22,
            username: "root".into(),
            auth_type: "sshAgent".into(),
            private_key_path: None,
            host_key_policy: "strict".into(),
            note: "x".repeat(64 * 1024 + 1),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            credential: None,
        };
        assert!(validate_connection(&input).is_err());
        input.note.clear();
        db.save_connection(&input, None).await.unwrap();
        assert!(
            db.add_history("limit", &"x".repeat(64 * 1024 + 1))
                .await
                .is_err()
        );
        assert!(
            db.save_workspace(&serde_json::json!({"data":"x".repeat(1024*1024+1)}))
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn all_command_history_can_be_cleared() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("history.sqlite"))
            .await
            .unwrap();
        let input = SaveConnectionInput {
            id: "history".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "Server".into(),
            host: "example.test".into(),
            port: 22,
            username: "root".into(),
            auth_type: "sshAgent".into(),
            private_key_path: None,
            host_key_policy: "strict".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            credential: None,
        };
        db.save_connection(&input, None).await.unwrap();
        db.add_history(&input.id, "uname -a").await.unwrap();
        assert_eq!(db.clear_history().await.unwrap(), 1);
        assert!(db.history(&input.id).await.unwrap().is_empty());
    }
    #[tokio::test]
    async fn migrations_and_connection_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("test.sqlite"))
            .await
            .unwrap();
        let input = SaveConnectionInput {
            id: "roundtrip".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "Server".into(),
            host: "example.test".into(),
            port: 22,
            username: "root".into(),
            auth_type: "sshAgent".into(),
            private_key_path: None,
            host_key_policy: "strict".into(),
            note: "".into(),
            tags: vec!["test".into()],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            credential: None,
        };
        db.save_connection(&input, None).await.unwrap();
        let saved = db.get_connection("roundtrip").await.unwrap();
        assert_eq!(saved.host, "example.test");
        assert_eq!(saved.tags, vec!["test"]);
    }
    #[tokio::test]
    async fn switching_to_ssh_agent_clears_credential_reference() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("auth.sqlite"))
            .await
            .unwrap();
        let mut input = SaveConnectionInput {
            id: "auth".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "Server".into(),
            host: "example.test".into(),
            port: 22,
            username: "root".into(),
            auth_type: "password".into(),
            private_key_path: None,
            host_key_policy: "strict".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            credential: None,
        };
        assert!(
            db.save_connection(&input, Some("connection:auth"))
                .await
                .unwrap()
                .has_credential
        );
        input.auth_type = "sshAgent".into();
        assert!(
            !db.save_connection(&input, None)
                .await
                .unwrap()
                .has_credential
        );
    }
    #[tokio::test]
    async fn switching_password_and_private_key_does_not_reuse_old_secret() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("auth-type.sqlite"))
            .await
            .unwrap();
        let mut input = SaveConnectionInput {
            id: "auth".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "Server".into(),
            host: "example.test".into(),
            port: 22,
            username: "root".into(),
            auth_type: "password".into(),
            private_key_path: None,
            host_key_policy: "strict".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            credential: None,
        };
        assert!(
            db.save_connection(&input, Some("connection:auth"))
                .await
                .unwrap()
                .has_credential
        );
        input.auth_type = "privateKey".into();
        input.private_key_path = Some("/tmp/key".into());
        assert!(
            !db.save_connection(&input, None)
                .await
                .unwrap()
                .has_credential
        );
    }
    #[tokio::test]
    async fn switching_to_rdp_preserves_password_credential_reference() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("rdp-auth.sqlite"))
            .await
            .unwrap();
        let mut input = SaveConnectionInput {
            id: "remote".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "Remote".into(),
            host: "example.test".into(),
            port: 22,
            username: "root".into(),
            auth_type: "password".into(),
            private_key_path: None,
            host_key_policy: "strict".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            credential: None,
        };
        assert!(
            db.save_connection(&input, Some("connection:remote"))
                .await
                .unwrap()
                .has_credential
        );
        input.protocol = "rdp".into();
        input.port = 3389;
        assert!(
            db.save_connection(&input, None)
                .await
                .unwrap()
                .has_credential
        );
    }
    #[tokio::test]
    async fn connection_id_collision_includes_soft_deleted_rows() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("ids.sqlite"))
            .await
            .unwrap();
        let input = SaveConnectionInput {
            id: "reserved".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "Server".into(),
            host: "example.test".into(),
            port: 22,
            username: "root".into(),
            auth_type: "sshAgent".into(),
            private_key_path: None,
            host_key_policy: "strict".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            credential: None,
        };
        assert!(!db.connection_id_exists("reserved").await.unwrap());
        db.insert_connection(&input, None).await.unwrap();
        assert!(db.connection_id_exists("reserved").await.unwrap());
        assert!(db.insert_connection(&input, None).await.is_err());
        db.delete_connection("reserved").await.unwrap();
        assert!(db.connection_id_exists("reserved").await.unwrap());
        assert!(db.insert_connection(&input, None).await.is_err());
    }
    #[tokio::test]
    async fn ssh_jump_proxy_requires_and_round_trips_an_ssh_connection() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("jump.sqlite"))
            .await
            .unwrap();
        let input = SaveConnectionInput {
            id: "jump-host".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "Jump".into(),
            host: "jump.example".into(),
            port: 22,
            username: "root".into(),
            auth_type: "sshAgent".into(),
            private_key_path: None,
            host_key_policy: "strict".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            credential: None,
        };
        db.save_connection(&input, None).await.unwrap();
        let mut proxy = SaveProxyInput {
            id: "jump".into(),
            name: "Jump proxy".into(),
            proxy_type: "sshJump".into(),
            host: "".into(),
            port: 1080,
            username: None,
            jump_connection_id: Some("missing".into()),
            credential: None,
        };
        assert!(db.save_proxy(&proxy, None).await.is_err());
        proxy.jump_connection_id = Some(input.id);
        let saved = db.save_proxy(&proxy, None).await.unwrap();
        assert_eq!(saved.jump_connection_id.as_deref(), Some("jump-host"));
        assert!(saved.host.is_empty());
    }
    #[tokio::test]
    async fn switching_proxy_to_ssh_jump_clears_credential_reference() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("proxy-auth.sqlite"))
            .await
            .unwrap();
        let connection = SaveConnectionInput {
            id: "jump-host".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "Jump".into(),
            host: "jump.example".into(),
            port: 22,
            username: "root".into(),
            auth_type: "sshAgent".into(),
            private_key_path: None,
            host_key_policy: "strict".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            credential: None,
        };
        db.save_connection(&connection, None).await.unwrap();
        let mut proxy = SaveProxyInput {
            id: "proxy".into(),
            name: "Proxy".into(),
            proxy_type: "socks5".into(),
            host: "proxy.example".into(),
            port: 1080,
            username: Some("user".into()),
            jump_connection_id: None,
            credential: None,
        };
        assert!(
            db.save_proxy(&proxy, Some("connection:proxy:proxy"))
                .await
                .unwrap()
                .has_credential
        );
        proxy.proxy_type = "sshJump".into();
        proxy.host.clear();
        proxy.jump_connection_id = Some(connection.id);
        let saved = db.save_proxy(&proxy, None).await.unwrap();
        assert!(!saved.has_credential);
    }
    #[tokio::test]
    async fn clearing_proxy_username_clears_credential_reference() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("proxy-user.sqlite"))
            .await
            .unwrap();
        let mut proxy = SaveProxyInput {
            id: "proxy".into(),
            name: "Proxy".into(),
            proxy_type: "socks5".into(),
            host: "proxy.example".into(),
            port: 1080,
            username: Some("user".into()),
            jump_connection_id: None,
            credential: None,
        };
        assert!(
            db.save_proxy(&proxy, Some("connection:proxy:proxy"))
                .await
                .unwrap()
                .has_credential
        );
        proxy.username = None;
        assert!(!db.save_proxy(&proxy, None).await.unwrap().has_credential);
    }
    #[tokio::test]
    async fn proxy_used_by_soft_deleted_connection_is_preserved_for_restore() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("proxy-trash.sqlite"))
            .await
            .unwrap();
        let proxy = SaveProxyInput {
            id: "proxy".into(),
            name: "Proxy".into(),
            proxy_type: "socks5".into(),
            host: "proxy.example".into(),
            port: 1080,
            username: None,
            jump_connection_id: None,
            credential: None,
        };
        db.save_proxy(&proxy, None).await.unwrap();
        let input = SaveConnectionInput {
            id: "connection".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "Server".into(),
            host: "example.test".into(),
            port: 22,
            username: "root".into(),
            auth_type: "sshAgent".into(),
            private_key_path: None,
            host_key_policy: "strict".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: Some(proxy.id.clone()),
            environment: Default::default(),
            credential: None,
        };
        db.save_connection(&input, None).await.unwrap();
        db.delete_connection(&input.id).await.unwrap();
        assert!(matches!(
            db.delete_proxy(&proxy.id).await,
            Err(AppError::Validation(_))
        ));
        db.restore_connection(&input.id).await.unwrap();
        assert_eq!(
            db.get_connection(&input.id)
                .await
                .unwrap()
                .proxy_id
                .as_deref(),
            Some("proxy")
        );
    }
    #[test]
    fn validates_forward_types_and_targets() {
        let mut forward = PortForward {
            id: "forward".into(),
            connection_id: "connection".into(),
            forward_type: "dynamic".into(),
            bind_host: "127.0.0.1".into(),
            bind_port: 1080,
            destination_host: None,
            destination_port: None,
            auto_start: false,
            status: None,
            error: None,
        };
        assert!(validate_forward(&forward).is_ok());
        forward.forward_type = "unknown".into();
        assert!(validate_forward(&forward).is_err());
        forward.forward_type = "local".into();
        assert!(validate_forward(&forward).is_err());
        forward.destination_host = Some("example.test".into());
        forward.destination_port = Some(22);
        assert!(validate_forward(&forward).is_ok());
        forward.bind_host.clear();
        assert!(validate_forward(&forward).is_err());
    }
    #[tokio::test]
    async fn unfinished_transfers_require_explicit_retry_after_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("restart.sqlite");
        let db = Database::open(&path).await.unwrap();
        let task = TransferTask {
            id: "unfinished".into(),
            session_id: "session".into(),
            direction: "download".into(),
            source: "/remote/file".into(),
            destination: "/local/file".into(),
            total_bytes: 100,
            transferred_bytes: 40,
            status: "running".into(),
            conflict_policy: "overwrite".into(),
            error: None,
            created_at: Utc::now().to_rfc3339(),
        };
        db.upsert_transfer(&task).await.unwrap();
        db.pool.close().await;
        let reopened = Database::open(&path).await.unwrap();
        let restored = reopened.transfers().await.unwrap().remove(0);
        assert_eq!(restored.status, "failed");
        assert_eq!(restored.transferred_bytes, 40);
        assert!(restored.error.unwrap().contains("确认后重试"));
    }
    #[tokio::test]
    async fn soft_deleted_connection_can_be_restored_or_permanently_purged() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("trash.sqlite"))
            .await
            .unwrap();
        let input = SaveConnectionInput {
            id: "trash".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "Recoverable".into(),
            host: "example.test".into(),
            port: 22,
            username: "root".into(),
            auth_type: "sshAgent".into(),
            private_key_path: None,
            host_key_policy: "strict".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            credential: None,
        };
        db.save_connection(&input, None).await.unwrap();
        db.add_history("trash", "uname -a").await.unwrap();
        db.delete_connection("trash").await.unwrap();
        assert!(db.list_connections().await.unwrap().is_empty());
        assert_eq!(db.deleted_connections().await.unwrap().len(), 1);
        db.restore_connection("trash").await.unwrap();
        assert_eq!(db.list_connections().await.unwrap().len(), 1);
        db.delete_connection("trash").await.unwrap();
        db.purge_connection("trash").await.unwrap();
        assert!(db.deleted_connections().await.unwrap().is_empty());
    }
    #[tokio::test]
    async fn folder_hierarchy_rejects_cycles_and_deletes_subtrees_safely() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("folder-tree.sqlite"))
            .await
            .unwrap();
        assert!(
            db.save_folder("orphan", "Orphan", Some("missing"))
                .await
                .is_err()
        );
        db.save_folder("root", "Root", None).await.unwrap();
        db.save_folder("child", "Child", Some("root"))
            .await
            .unwrap();
        db.save_folder("grandchild", "Grandchild", Some("child"))
            .await
            .unwrap();
        assert!(db.save_folder("root", "Root", Some("root")).await.is_err());
        assert!(
            db.save_folder("root", "Root", Some("grandchild"))
                .await
                .is_err()
        );
        let input = SaveConnectionInput {
            id: "nested-connection".into(),
            folder_id: Some("grandchild".into()),
            protocol: "ssh".into(),
            name: "Nested".into(),
            host: "example.test".into(),
            port: 22,
            username: "root".into(),
            auth_type: "sshAgent".into(),
            private_key_path: None,
            host_key_policy: "strict".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            credential: None,
        };
        db.save_connection(&input, None).await.unwrap();
        db.delete_folder("root").await.unwrap();
        assert!(db.folders().await.unwrap().is_empty());
        assert_eq!(
            db.get_connection("nested-connection")
                .await
                .unwrap()
                .folder_id,
            None
        );
    }
    #[tokio::test]
    async fn every_historical_schema_upgrades_without_data_loss() {
        use std::borrow::Cow;
        let full = sqlx::migrate!("./migrations");
        for count in 1..=full.iter().count() {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join(format!("v{count}.sqlite"));
            let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))
                .unwrap()
                .create_if_missing(true)
                .foreign_keys(true)
                .journal_mode(SqliteJournalMode::Wal);
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(options)
                .await
                .unwrap();
            let partial = sqlx::migrate::Migrator {
                migrations: Cow::Owned(full.iter().take(count).cloned().collect()),
                ..sqlx::migrate::Migrator::DEFAULT
            };
            partial.run(&pool).await.unwrap();
            sqlx::query("INSERT INTO connections(id,protocol,name,host,port,username,auth_type,host_key_policy,note,tags,encoding,created_at,updated_at)VALUES('historical','ssh','Historical','old.example',22,'root','sshAgent','strict','','[]','UTF-8','now','now')").execute(&pool).await.unwrap();
            pool.close().await;
            let upgraded = Database::open(&path).await.unwrap();
            assert_eq!(
                upgraded.get_connection("historical").await.unwrap().host,
                "old.example"
            );
            assert!(path.with_extension("sqlite.backup").exists());
            upgraded.pool.close().await;
        }
    }
}
