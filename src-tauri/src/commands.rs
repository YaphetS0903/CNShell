use crate::{
    AppState,
    db::Database,
    error::{AppError, AppResult},
    models::*,
    monitor::MonitorState,
    sftp::TransferManager,
    ssh::SessionManager,
};
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn connection_list(state: State<'_, AppState>) -> AppResult<Vec<ConnectionProfile>> {
    state.db.list_connections().await
}
#[tauri::command]
pub async fn connection_deleted_list(
    state: State<'_, AppState>,
) -> AppResult<Vec<ConnectionProfile>> {
    state.db.deleted_connections().await
}

#[tauri::command]
pub async fn folder_list(state: State<'_, AppState>) -> AppResult<Vec<Folder>> {
    state.db.folders().await
}
#[tauri::command]
pub async fn folder_save(
    state: State<'_, AppState>,
    id: String,
    name: String,
    parent_id: Option<String>,
) -> AppResult<Folder> {
    state.db.save_folder(&id, &name, parent_id.as_deref()).await
}
#[tauri::command]
pub async fn folder_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.delete_folder(&id).await
}
#[tauri::command]
pub async fn connection_move(
    state: State<'_, AppState>,
    id: String,
    folder_id: Option<String>,
) -> AppResult<()> {
    state.db.move_connection(&id, folder_id.as_deref()).await
}

#[tauri::command]
pub async fn ssh_certificate_inspect(path: String) -> AppResult<SshCertificateInfo> {
    crate::certificate::inspect(std::path::Path::new(&path))
}

#[tauri::command]
pub async fn connection_save(
    state: State<'_, AppState>,
    input: SaveConnectionInput,
) -> AppResult<ConnectionProfile> {
    crate::db::validate_connection(&input)?;
    state.sessions.invalidate_transport(&input.id);
    let previous_bookmark = crate::bookmark::load(&input.id)?;
    let previous_certificate_bookmark = crate::bookmark::load_certificate(&input.id)?;
    let existing = state.db.get_connection(&input.id).await.ok();
    if input.protocol == "rdp"
        && input.credential.as_deref().unwrap_or("").is_empty()
        && !existing
            .as_ref()
            .is_some_and(|profile| profile.has_credential)
    {
        return Err(AppError::Validation("RDP 连接必须输入密码".into()));
    }
    if input.protocol == "ssh" && input.auth_type == "sshAgent" {
        let previous = crate::ssh::load_credential(&input.id)?;
        crate::ssh::delete_credential(&input.id)?;
        if let Err(error) = crate::bookmark::delete(&input.id) {
            restore_credential(&input.id, previous.as_deref());
            return Err(error);
        }
        if let Err(error) = crate::bookmark::delete_certificate(&input.id) {
            restore_credential(&input.id, previous.as_deref());
            crate::bookmark::restore(&input.id, previous_bookmark.as_deref());
            return Err(error);
        }
        return match state.db.save_connection(&input, None).await {
            Ok(saved) => Ok(saved),
            Err(error) => {
                restore_credential(&input.id, previous.as_deref());
                crate::bookmark::restore(&input.id, previous_bookmark.as_deref());
                crate::bookmark::restore_certificate(
                    &input.id,
                    previous_certificate_bookmark.as_deref(),
                );
                Err(error)
            }
        };
    }
    let secret = input
        .credential
        .as_deref()
        .filter(|value| !value.is_empty());
    let auth_changed = existing
        .as_ref()
        .is_some_and(|profile| profile.auth_type != input.auth_type);
    let previous = if secret.is_some() || auth_changed {
        crate::ssh::load_credential(&input.id)?
    } else {
        None
    };
    if auth_changed && secret.is_none() {
        crate::ssh::delete_credential(&input.id)?;
    }
    let credential_ref = if let Some(secret) = secret {
        match crate::ssh::save_credential(&input.id, secret) {
            Ok(reference) => Some(reference),
            Err(error) => return Err(error),
        }
    } else {
        None
    };
    let bookmark_unchanged = previous_bookmark.is_some()
        && existing.as_ref().is_some_and(|profile| {
            matches!(profile.auth_type.as_str(), "privateKey" | "sshCertificate")
                && profile.private_key_path == input.private_key_path
        });
    let bookmark_result = if bookmark_unchanged {
        Ok(())
    } else if matches!(input.auth_type.as_str(), "privateKey" | "sshCertificate") {
        crate::bookmark::save(
            &input.id,
            std::path::Path::new(input.private_key_path.as_deref().unwrap_or_default()),
        )
    } else {
        crate::bookmark::delete(&input.id)
    };
    if let Err(error) = bookmark_result {
        if secret.is_some() || auth_changed {
            restore_credential(&input.id, previous.as_deref());
        }
        crate::bookmark::restore(&input.id, previous_bookmark.as_deref());
        return Err(error);
    }
    let certificate_unchanged = previous_certificate_bookmark.is_some()
        && existing.as_ref().is_some_and(|profile| {
            profile.auth_type == "sshCertificate"
                && profile.certificate_path == input.certificate_path
        });
    let certificate_result = if certificate_unchanged {
        Ok(())
    } else if input.auth_type == "sshCertificate" {
        let path = std::path::Path::new(input.certificate_path.as_deref().unwrap_or_default());
        crate::certificate::inspect(path)
            .and_then(|_| crate::bookmark::save_certificate(&input.id, path))
    } else {
        crate::bookmark::delete_certificate(&input.id)
    };
    if let Err(error) = certificate_result {
        if secret.is_some() || auth_changed {
            restore_credential(&input.id, previous.as_deref());
        }
        crate::bookmark::restore(&input.id, previous_bookmark.as_deref());
        crate::bookmark::restore_certificate(&input.id, previous_certificate_bookmark.as_deref());
        return Err(error);
    }
    let saved = match state
        .db
        .save_connection(&input, credential_ref.as_deref())
        .await
    {
        Ok(saved) => saved,
        Err(error) => {
            if secret.is_some() || auth_changed {
                restore_credential(&input.id, previous.as_deref());
            }
            crate::bookmark::restore(&input.id, previous_bookmark.as_deref());
            crate::bookmark::restore_certificate(
                &input.id,
                previous_certificate_bookmark.as_deref(),
            );
            return Err(error);
        }
    };
    Ok(saved)
}

fn restore_credential(id: &str, previous: Option<&str>) {
    if let Some(previous) = previous {
        let _ = crate::ssh::save_credential(id, previous);
    } else {
        let _ = crate::ssh::delete_credential(id);
    }
}

fn duplicate_input(source: ConnectionProfile, new_id: String) -> AppResult<SaveConnectionInput> {
    if new_id.trim().is_empty() || new_id == source.id {
        return Err(AppError::Validation("复制连接的新 ID 无效".into()));
    }
    Ok(SaveConnectionInput {
        id: new_id,
        folder_id: source.folder_id,
        protocol: source.protocol,
        name: format!("{} 副本", source.name),
        host: source.host,
        port: source.port,
        username: source.username,
        auth_type: source.auth_type,
        private_key_path: source.private_key_path,
        certificate_path: source.certificate_path,
        host_key_policy: source.host_key_policy,
        note: source.note,
        tags: source.tags,
        encoding: source.encoding,
        startup_command: source.startup_command,
        proxy_id: source.proxy_id,
        environment: source.environment,
        credential: None,
    })
}
#[tauri::command]
pub async fn connection_duplicate(
    state: State<'_, AppState>,
    id: String,
    new_id: String,
) -> AppResult<ConnectionProfile> {
    let source = state.db.get_connection(&id).await?;
    let input = duplicate_input(source, new_id.clone())?;
    crate::db::validate_connection(&input)?;
    if state.db.connection_id_exists(&new_id).await? {
        return Err(AppError::Validation("复制连接的新 ID 已存在".into()));
    }
    let saved = state
        .db
        .insert_connection(&input, None)
        .await
        .map_err(|error| match error {
            AppError::Storage(message) if message.contains("UNIQUE constraint failed") => {
                AppError::Validation("复制连接的新 ID 已存在".into())
            }
            other => other,
        })?;
    if let Err(error) = crate::bookmark::copy(&id, &new_id) {
        let _ = state.db.remove_inserted_connection(&new_id).await;
        return Err(error);
    }
    if let Err(error) = crate::bookmark::copy_certificate(&id, &new_id) {
        let _ = crate::bookmark::delete(&new_id);
        let _ = state.db.remove_inserted_connection(&new_id).await;
        return Err(error);
    }
    let secret = match crate::ssh::load_credential(&id) {
        Ok(secret) => secret,
        Err(error) => {
            let _ = crate::bookmark::delete(&new_id);
            let _ = crate::bookmark::delete_certificate(&new_id);
            let _ = state.db.remove_inserted_connection(&new_id).await;
            return Err(error);
        }
    };
    let Some(secret) = secret else {
        return Ok(saved);
    };
    let reference = match crate::ssh::save_credential(&new_id, &secret) {
        Ok(reference) => reference,
        Err(error) => {
            let _ = crate::bookmark::delete(&new_id);
            let _ = crate::bookmark::delete_certificate(&new_id);
            let _ = state.db.remove_inserted_connection(&new_id).await;
            return Err(error);
        }
    };
    match state
        .db
        .set_connection_credential_ref(&new_id, &reference)
        .await
    {
        Ok(saved) => Ok(saved),
        Err(error) => {
            let _ = crate::ssh::delete_credential(&new_id);
            let _ = crate::bookmark::delete(&new_id);
            let _ = crate::bookmark::delete_certificate(&new_id);
            let _ = state.db.remove_inserted_connection(&new_id).await;
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn connection_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.sessions.invalidate_transport(&id);
    state.db.delete_connection(&id).await
}
#[tauri::command]
pub async fn connection_restore(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.restore_connection(&id).await
}
#[tauri::command]
pub async fn connection_purge(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.sessions.invalidate_transport(&id);
    let previous = crate::ssh::load_credential(&id)?;
    let previous_bookmark = crate::bookmark::load(&id)?;
    let previous_certificate_bookmark = crate::bookmark::load_certificate(&id)?;
    crate::ssh::delete_credential(&id)?;
    if let Err(error) = crate::bookmark::delete(&id) {
        restore_credential(&id, previous.as_deref());
        return Err(error);
    }
    if let Err(error) = crate::bookmark::delete_certificate(&id) {
        restore_credential(&id, previous.as_deref());
        crate::bookmark::restore(&id, previous_bookmark.as_deref());
        return Err(error);
    }
    match state.db.purge_connection(&id).await {
        Ok(()) => Ok(()),
        Err(error) => {
            restore_credential(&id, previous.as_deref());
            crate::bookmark::restore(&id, previous_bookmark.as_deref());
            crate::bookmark::restore_certificate(&id, previous_certificate_bookmark.as_deref());
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn connection_test_start(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<BackgroundTask> {
    let profile = state.db.get_connection(&id).await?;
    let db = state.db.clone();
    Ok(state
        .tasks
        .spawn(app, "connectionDiagnostic", move |_| async move {
            serde_json::to_value(crate::ssh::diagnose(&db, &profile).await)
                .map_err(|error| AppError::Internal(error.to_string()))
        }))
}

#[tauri::command]
pub async fn connection_trust_host(
    state: State<'_, AppState>,
    id: String,
    fingerprint: String,
    algorithm: String,
) -> AppResult<()> {
    let profile = state.db.get_connection(&id).await?;
    state.sessions.invalidate_transport(&id);
    state
        .db
        .trust_host(&profile.host, profile.port, &algorithm, &fingerprint)
        .await
}

#[tauri::command]
pub async fn openssh_import(path: String) -> AppResult<Vec<OpenSshHost>> {
    crate::openssh::import_config(std::path::Path::new(&path))
}

#[tauri::command]
pub async fn openssh_generate_key(path: String, comment: String) -> AppResult<GeneratedSshKey> {
    crate::openssh::generate_key(std::path::Path::new(&path), &comment)
}

#[tauri::command]
pub async fn openssh_deploy_key(
    state: State<'_, AppState>,
    connection_id: String,
    public_key: String,
) -> AppResult<()> {
    let profile = state.db.get_connection(&connection_id).await?;
    if profile.protocol != "ssh" {
        return Err(AppError::Validation("只能向 SSH 连接部署公钥".into()));
    }
    crate::openssh::deploy_public_key(&state.db, &profile, &public_key).await
}

#[tauri::command]
pub fn protocol_capabilities() -> Vec<ProtocolCapability> {
    crate::protocols::capabilities()
}

#[tauri::command]
pub async fn protocol_options_get(
    state: State<'_, AppState>,
    connection_id: String,
) -> AppResult<ConnectionProtocolOptions> {
    state.db.get_connection(&connection_id).await?;
    Ok(state
        .db
        .load_named_state(&format!("cnshell.protocol.{connection_id}"))
        .await?
        .unwrap_or(ConnectionProtocolOptions {
            connection_id,
            agent_forwarding: false,
            x11_enabled: false,
            mosh_enabled: false,
            mosh_port_start: crate::mosh::default_ports().0,
            mosh_port_end: crate::mosh::default_ports().1,
        }))
}

#[tauri::command]
pub async fn protocol_options_save(
    state: State<'_, AppState>,
    options: ConnectionProtocolOptions,
) -> AppResult<ConnectionProtocolOptions> {
    let profile = state.db.get_connection(&options.connection_id).await?;
    if profile.protocol != "ssh" {
        return Err(AppError::Validation("协议选项仅支持 SSH 连接".into()));
    }
    crate::protocols::validate_options(
        options.agent_forwarding,
        options.x11_enabled,
        options.mosh_enabled,
        options.mosh_port_start,
        options.mosh_port_end,
    )?;
    state
        .db
        .save_named_state(
            &format!("cnshell.protocol.{}", options.connection_id),
            &serde_json::to_value(&options)
                .map_err(|error| AppError::Internal(error.to_string()))?,
        )
        .await?;
    Ok(options)
}

#[tauri::command]
pub fn automation_validate(plan: AutomationPlan) -> AppResult<AutomationPlan> {
    crate::automation::validate(&plan)?;
    Ok(plan)
}

#[tauri::command]
pub async fn automation_start(
    app: AppHandle,
    state: State<'_, AppState>,
    plan: AutomationPlan,
) -> AppResult<BackgroundTask> {
    crate::automation::validate(&plan)?;
    let db = state.db.clone();
    Ok(state
        .tasks
        .spawn(app, "automation", move |cancelled| async move {
            serde_json::to_value(crate::automation::run(db, plan, cancelled).await?)
                .map_err(|error| AppError::Internal(error.to_string()))
        }))
}

#[tauri::command]
pub async fn sync_write(
    state: State<'_, AppState>,
    folder: String,
    passphrase: String,
    options: SyncOptions,
) -> AppResult<SyncResult> {
    crate::backup::sync_write(&state.db, &folder, &passphrase, &options).await
}

#[tauri::command]
pub async fn sync_read(
    state: State<'_, AppState>,
    folder: String,
    passphrase: String,
) -> AppResult<SyncResult> {
    crate::backup::sync_read(&state.db, &folder, &passphrase).await
}

#[tauri::command]
pub async fn terminal_open(
    app: AppHandle,
    state: State<'_, AppState>,
    connection_id: String,
    cols: u32,
    rows: u32,
) -> AppResult<TerminalSession> {
    let profile = state.db.get_connection(&connection_id).await?;
    if profile.protocol != "ssh" {
        return Err(AppError::Validation("RDP 连接请使用远程桌面入口".into()));
    }
    let options = state
        .db
        .load_named_state::<ConnectionProtocolOptions>(&format!("cnshell.protocol.{connection_id}"))
        .await?
        .unwrap_or(ConnectionProtocolOptions {
            connection_id: connection_id.clone(),
            agent_forwarding: false,
            x11_enabled: false,
            mosh_enabled: false,
            mosh_port_start: crate::mosh::default_ports().0,
            mosh_port_end: crate::mosh::default_ports().1,
        });
    let session = if options.mosh_enabled {
        state
            .mosh
            .open(
                app,
                state.db.clone(),
                state.sessions.clone(),
                profile,
                state.logs.clone(),
                cols,
                rows,
                options.mosh_port_start,
                options.mosh_port_end,
            )
            .await?
    } else {
        crate::ssh::open_terminal(
            app,
            state.db.clone(),
            state.sessions.clone(),
            profile,
            cols,
            rows,
            state.logs.clone(),
            options.agent_forwarding,
            options.x11_enabled,
        )
        .await?
    };
    let _ = state.db.mark_connected(&connection_id).await;
    Ok(session)
}

#[tauri::command]
pub async fn terminal_input(
    state: State<'_, AppState>,
    session_id: String,
    data: String,
) -> AppResult<()> {
    if state.mosh.contains(&session_id) {
        return state.mosh.input(&session_id, &data);
    }
    crate::ssh::terminal_input(state.sessions.clone(), session_id, data).await
}

#[tauri::command]
pub async fn terminal_resize(
    state: State<'_, AppState>,
    session_id: String,
    cols: u32,
    rows: u32,
) -> AppResult<()> {
    if state.mosh.contains(&session_id) {
        return state.mosh.resize(&session_id, cols, rows);
    }
    crate::ssh::terminal_resize(state.sessions.clone(), session_id, cols, rows).await
}

#[tauri::command]
pub async fn terminal_close(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    let result = if state.mosh.contains(&session_id) {
        state.mosh.close(&session_id)
    } else {
        crate::ssh::terminal_close(state.sessions.clone(), session_id.clone()).await
    };
    let _ = state.logs.stop(&session_id);
    state.monitor.remove(&session_id);
    result
}

#[tauri::command]
pub async fn zmodem_start(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    transfer_id: String,
    paths: Vec<String>,
) -> AppResult<ZmodemEvent> {
    crate::ssh::zmodem_start(app, state.sessions.clone(), session_id, transfer_id, paths).await
}

#[tauri::command]
pub async fn zmodem_cancel(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    transfer_id: String,
) -> AppResult<ZmodemEvent> {
    crate::ssh::zmodem_cancel(app, state.sessions.clone(), session_id, transfer_id).await
}

#[tauri::command]
pub async fn terminal_log_start(
    state: State<'_, AppState>,
    session_id: String,
    format: String,
    line_timestamps: bool,
    retention_days: u64,
    max_total_bytes: u64,
) -> AppResult<SessionLogStatus> {
    let profile = state.sessions.profile(&session_id)?;
    state.logs.start(
        &session_id,
        &profile.name,
        &format,
        line_timestamps,
        retention_days,
        max_total_bytes,
    )
}

#[tauri::command]
pub async fn terminal_log_stop(
    state: State<'_, AppState>,
    session_id: String,
) -> AppResult<SessionLogStatus> {
    state.logs.stop(&session_id)
}

#[tauri::command]
pub async fn terminal_log_status(
    state: State<'_, AppState>,
    session_id: String,
) -> AppResult<SessionLogStatus> {
    Ok(state.logs.status(&session_id))
}

#[tauri::command]
pub async fn terminal_log_export(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> AppResult<()> {
    state.logs.export(&session_id, std::path::Path::new(&path))
}

#[tauri::command]
pub async fn batch_start(
    app: AppHandle,
    state: State<'_, AppState>,
    connection_ids: Vec<String>,
    command: String,
    concurrency: usize,
) -> AppResult<BatchExecution> {
    let mut unique = std::collections::HashSet::new();
    let mut profiles = Vec::new();
    for id in connection_ids {
        if unique.insert(id.clone()) {
            let profile = state.db.get_connection(&id).await?;
            if profile.protocol != "ssh" {
                return Err(AppError::Validation(format!(
                    "{} 不是 SSH 连接",
                    profile.name
                )));
            }
            profiles.push(profile);
        }
    }
    state
        .batches
        .start(app, state.db.clone(), profiles, command, concurrency)
}

#[tauri::command]
pub async fn batch_get(state: State<'_, AppState>, id: String) -> AppResult<BatchExecution> {
    state.batches.get(&id)
}

#[tauri::command]
pub async fn batch_cancel(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<BatchExecution> {
    state.batches.cancel(&app, &id)
}

#[tauri::command]
pub async fn external_edit_start(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    application: Option<String>,
) -> AppResult<ExternalEditSession> {
    state
        .external_edits
        .start(
            state.db.clone(),
            state.sessions.clone(),
            session_id,
            path,
            application,
        )
        .await
}

#[tauri::command]
pub async fn external_edit_read(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<ExternalEditSnapshot> {
    state.external_edits.read(&id)
}

#[tauri::command]
pub async fn external_edit_discard(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.external_edits.discard(&id)
}

#[tauri::command]
pub async fn sftp_list(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    show_hidden: bool,
) -> AppResult<Vec<RemoteFile>> {
    crate::sftp::list(
        state.db.clone(),
        state.sessions.clone(),
        session_id,
        path,
        show_hidden,
    )
    .await
}

#[tauri::command]
pub fn sftp_join_path(parent: String, name: String) -> AppResult<String> {
    crate::sftp::join_path(&parent, &name)
}

#[tauri::command]
pub async fn sftp_mkdir(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> AppResult<()> {
    crate::sftp::mkdir(state.db.clone(), state.sessions.clone(), session_id, path).await
}

#[tauri::command]
pub async fn sftp_rename(
    state: State<'_, AppState>,
    session_id: String,
    source: String,
    destination: String,
) -> AppResult<()> {
    crate::sftp::rename(
        state.db.clone(),
        state.sessions.clone(),
        session_id,
        source,
        destination,
    )
    .await
}

#[tauri::command]
pub async fn sftp_delete(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    recursive: bool,
) -> AppResult<()> {
    crate::sftp::delete(
        state.db.clone(),
        state.sessions.clone(),
        session_id,
        path,
        recursive,
    )
    .await
}

#[tauri::command]
pub async fn sftp_chmod(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    mode: u32,
) -> AppResult<()> {
    crate::sftp::chmod(
        state.db.clone(),
        state.sessions.clone(),
        session_id,
        path,
        mode,
    )
    .await
}

#[tauri::command]
pub async fn sftp_open_text(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> AppResult<crate::sftp::TextFile> {
    crate::sftp::open_text(state.db.clone(), state.sessions.clone(), session_id, path).await
}

#[tauri::command]
pub async fn sftp_save_text(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    content: String,
    expected_modified_at: Option<u64>,
) -> AppResult<()> {
    crate::sftp::save_text(
        state.db.clone(),
        state.sessions.clone(),
        session_id,
        path,
        content,
        expected_modified_at,
    )
    .await
}

#[tauri::command]
pub async fn sftp_create_text(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> AppResult<()> {
    crate::sftp::create_text(state.db.clone(), state.sessions.clone(), session_id, path).await
}
#[tauri::command]
pub async fn sftp_archive_start(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    extract: bool,
) -> AppResult<BackgroundTask> {
    let db = state.db.clone();
    let sessions = state.sessions.clone();
    Ok(state.tasks.spawn(app, "sftpArchive", move |_| async move {
        let result = crate::sftp::archive(db, sessions, session_id, path, extract).await?;
        Ok(serde_json::Value::String(result))
    }))
}
#[tauri::command]
pub async fn sftp_open_local_start(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    application: Option<String>,
) -> AppResult<BackgroundTask> {
    let db = state.db.clone();
    let sessions = state.sessions.clone();
    Ok(state.tasks.spawn(app, "sftpPreview", move |_| async move {
        let result = crate::sftp::open_local(db, sessions, session_id, path, application).await?;
        Ok(serde_json::Value::String(result))
    }))
}

#[tauri::command]
pub async fn sftp_directory_transfer_start(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    direction: String,
    source: String,
    destination: String,
    conflict_policy: String,
) -> AppResult<BackgroundTask> {
    let db = state.db.clone();
    let sessions = state.sessions.clone();
    Ok(state
        .tasks
        .spawn(app, "sftpDirectoryTransfer", move |cancelled| async move {
            let result = crate::sftp::transfer_directory(
                db,
                sessions,
                session_id,
                direction,
                source,
                destination,
                conflict_policy,
                cancelled,
            )
            .await?;
            Ok(serde_json::Value::String(result))
        }))
}

#[tauri::command]
pub fn task_get(state: State<'_, AppState>, id: String) -> AppResult<BackgroundTask> {
    state.tasks.get(&id)
}

#[tauri::command]
pub fn task_cancel(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.tasks.cancel(&app, &id)
}

#[tauri::command]
pub async fn transfer_enqueue(
    app: AppHandle,
    state: State<'_, AppState>,
    input: TransferInput,
) -> AppResult<TransferTask> {
    crate::sftp::enqueue(
        app,
        state.db.clone(),
        state.sessions.clone(),
        state.transfers.clone(),
        input,
    )
    .await
}

#[tauri::command]
pub async fn transfer_list(state: State<'_, AppState>) -> AppResult<Vec<TransferTask>> {
    state.db.transfers().await
}

#[tauri::command]
pub async fn transfer_cancel(state: State<'_, AppState>, id: String) -> AppResult<()> {
    if state.transfers.cancel(&id) {
        Ok(())
    } else {
        Err(AppError::NotFound(format!("传输 {id}")))
    }
}

#[tauri::command]
pub async fn transfer_pause(state: State<'_, AppState>, id: String) -> AppResult<()> {
    if state.transfers.pause(&id) {
        Ok(())
    } else {
        Err(AppError::NotFound(format!("传输 {id}")))
    }
}

#[tauri::command]
pub async fn transfer_resume(state: State<'_, AppState>, id: String) -> AppResult<()> {
    if state.transfers.resume(&id) {
        Ok(())
    } else {
        Err(AppError::NotFound(format!("传输 {id}")))
    }
}

#[tauri::command]
pub async fn transfer_retry(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<TransferTask> {
    let old = state
        .db
        .transfers()
        .await?
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| AppError::NotFound(format!("传输 {id}")))?;
    if !["failed", "cancelled"].contains(&old.status.as_str()) {
        return Err(AppError::Validation("只有失败或已取消任务可以重试".into()));
    }
    let input = TransferInput {
        session_id: old.session_id,
        direction: old.direction,
        source: old.source,
        destination: old.destination,
        conflict_policy: old.conflict_policy,
    };
    crate::sftp::enqueue(
        app,
        state.db.clone(),
        state.sessions.clone(),
        state.transfers.clone(),
        input,
    )
    .await
}

#[tauri::command]
pub async fn proxy_list(state: State<'_, AppState>) -> AppResult<Vec<ProxyProfile>> {
    state.db.proxies().await
}
#[tauri::command]
pub async fn proxy_save(
    state: State<'_, AppState>,
    input: SaveProxyInput,
) -> AppResult<ProxyProfile> {
    crate::db::validate_proxy(&input)?;
    state.sessions.clear_transports();
    let key_id = format!("proxy:{}", input.id);
    if input.proxy_type == "sshJump" || input.username.as_deref().unwrap_or("").is_empty() {
        let previous = crate::ssh::load_credential(&key_id)?;
        crate::ssh::delete_credential(&key_id)?;
        return match state.db.save_proxy(&input, None).await {
            Ok(saved) => Ok(saved),
            Err(error) => {
                restore_credential(&key_id, previous.as_deref());
                Err(error)
            }
        };
    }
    let secret = input.credential.as_deref().filter(|v| !v.is_empty());
    let previous = if secret.is_some() {
        crate::ssh::load_credential(&key_id)?
    } else {
        None
    };
    let reference = if let Some(secret) = secret {
        Some(crate::ssh::save_credential(&key_id, secret)?)
    } else {
        None
    };
    match state.db.save_proxy(&input, reference.as_deref()).await {
        Ok(saved) => Ok(saved),
        Err(error) => {
            if secret.is_some() {
                restore_credential(&key_id, previous.as_deref());
            }
            Err(error)
        }
    }
}
#[tauri::command]
pub async fn proxy_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.sessions.clear_transports();
    let key_id = format!("proxy:{id}");
    let previous = crate::ssh::load_credential(&key_id)?;
    crate::ssh::delete_credential(&key_id)?;
    match state.db.delete_proxy(&id).await {
        Ok(()) => Ok(()),
        Err(error) => {
            restore_credential(&key_id, previous.as_deref());
            Err(error)
        }
    }
}
#[tauri::command]
pub async fn tunnel_list(
    state: State<'_, AppState>,
    connection_id: String,
) -> AppResult<Vec<PortForward>> {
    let mut items = state.db.forwards(&connection_id).await?;
    for item in &mut items {
        let (status, error) = state.tunnels.status(&item.id);
        item.status = Some(status);
        item.error = error;
    }
    Ok(items)
}
#[tauri::command]
pub async fn tunnel_save(state: State<'_, AppState>, input: PortForward) -> AppResult<PortForward> {
    state.db.save_forward(&input).await
}
#[tauri::command]
pub async fn tunnel_start(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let forward = state.db.get_forward(&id).await?;
    crate::tunnel::start(
        state.db.clone(),
        state.sessions.clone(),
        state.tunnels.clone(),
        forward,
    )
    .await
}
#[tauri::command]
pub async fn tunnel_stop(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.tunnels.stop(&id)
}
#[tauri::command]
pub async fn tunnel_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let _ = state.tunnels.stop(&id);
    state.db.delete_forward(&id).await
}

#[tauri::command]
pub async fn snippet_list(state: State<'_, AppState>) -> AppResult<Vec<CommandSnippet>> {
    state.db.snippets().await
}
#[tauri::command]
pub async fn snippet_save(
    state: State<'_, AppState>,
    input: CommandSnippet,
) -> AppResult<CommandSnippet> {
    state.db.save_snippet(&input).await
}
#[tauri::command]
pub async fn snippet_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.delete_snippet(&id).await
}
#[tauri::command]
pub async fn history_add(
    state: State<'_, AppState>,
    connection_id: String,
    command: String,
) -> AppResult<()> {
    let settings = state.db.get_settings().await?;
    if settings.remember_command_history {
        state.db.add_history(&connection_id, &command).await
    } else {
        Ok(())
    }
}
#[tauri::command]
pub async fn history_list(
    state: State<'_, AppState>,
    connection_id: String,
) -> AppResult<Vec<String>> {
    state.db.history(&connection_id).await
}
#[tauri::command]
pub async fn history_clear(state: State<'_, AppState>) -> AppResult<u64> {
    state.db.clear_history().await
}
#[tauri::command]
pub async fn workspace_save(state: State<'_, AppState>, value: serde_json::Value) -> AppResult<()> {
    state.db.save_workspace(&value).await
}
#[tauri::command]
pub async fn workspace_load(state: State<'_, AppState>) -> AppResult<Option<serde_json::Value>> {
    state.db.load_workspace().await
}
#[tauri::command]
pub async fn connection_export(
    state: State<'_, AppState>,
    path: String,
    include_secrets: bool,
    passphrase: Option<String>,
) -> AppResult<()> {
    crate::backup::export(&state.db, &path, include_secrets, passphrase.as_deref()).await
}
#[tauri::command]
pub async fn connection_export_one(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> AppResult<()> {
    crate::backup::export_one(&state.db, &id, &path).await
}
#[tauri::command]
pub async fn connection_import(
    state: State<'_, AppState>,
    path: String,
    passphrase: Option<String>,
) -> AppResult<usize> {
    crate::backup::import(&state.db, &path, passphrase.as_deref()).await
}

#[tauri::command]
pub async fn monitor_snapshot(
    state: State<'_, AppState>,
    session_id: String,
) -> AppResult<MonitorSnapshot> {
    crate::monitor::snapshot(
        state.db.clone(),
        state.sessions.clone(),
        state.monitor.clone(),
        session_id,
    )
    .await
}

#[tauri::command]
pub async fn monitor_process_signal(
    state: State<'_, AppState>,
    session_id: String,
    pid: u32,
    started_at: String,
    expected_command: String,
    signal: String,
) -> AppResult<()> {
    crate::monitor::signal_process(
        &state.db,
        &state.sessions,
        &session_id,
        pid,
        &started_at,
        &expected_command,
        &signal,
    )
    .await
}

#[tauri::command]
pub async fn monitor_network_sockets(
    state: State<'_, AppState>,
    session_id: String,
) -> AppResult<NetworkSocketReport> {
    crate::monitor::network_sockets(&state.db, &state.sessions, &session_id).await
}

#[tauri::command]
pub async fn monitor_network_diagnostic_start(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    kind: String,
    target: String,
) -> AppResult<BackgroundTask> {
    let profile = state.sessions.profile(&session_id)?;
    let db = state.db.clone();
    Ok(state
        .tasks
        .spawn(app, "networkDiagnostic", move |cancelled| async move {
            serde_json::to_value(
                crate::monitor::network_diagnostic(profile, db, kind, target, cancelled).await?,
            )
            .map_err(|error| AppError::Internal(error.to_string()))
        }))
}

#[tauri::command]
pub async fn monitor_system_info(
    state: State<'_, AppState>,
    session_id: String,
) -> AppResult<SystemInfo> {
    crate::monitor::system_info(state.db.clone(), state.sessions.clone(), session_id).await
}
#[tauri::command]
pub async fn monitor_export_system_info(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> AppResult<()> {
    let info =
        crate::monitor::system_info(state.db.clone(), state.sessions.clone(), session_id).await?;
    crate::monitor::export_system_info(std::path::Path::new(&path), &info)
}

#[tauri::command]
pub fn rdp_preflight() -> RdpPreflight {
    crate::rdp::preflight()
}

#[tauri::command]
pub async fn rdp_open(
    app: AppHandle,
    state: State<'_, AppState>,
    connection_id: String,
) -> AppResult<TerminalSession> {
    let profile = state.db.get_connection(&connection_id).await?;
    if profile.protocol != "rdp" {
        return Err(AppError::Validation("该连接不是 RDP 类型".into()));
    }
    let session = state.rdp.open(app, profile)?;
    let _ = state.db.mark_connected(&connection_id).await;
    Ok(session)
}

#[tauri::command]
pub fn rdp_close(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    state.rdp.close(&session_id)
}

#[tauri::command]
pub async fn settings_get(state: State<'_, AppState>) -> AppResult<AppSettings> {
    state.db.get_settings().await
}

#[tauri::command]
pub async fn settings_save(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> AppResult<AppSettings> {
    crate::db::validate_settings(&settings)?;
    state.db.save_settings(&settings).await?;
    Ok(settings)
}

#[tauri::command]
pub async fn diagnostics_export(state: State<'_, AppState>, path: String) -> AppResult<()> {
    crate::diagnostics::export(&state.db, &path).await
}

#[tauri::command]
pub fn diagnostics_environment() -> crate::diagnostics::FeedbackEnvironment {
    crate::diagnostics::feedback_environment()
}

#[tauri::command]
pub fn diagnostics_reveal(path: String) -> AppResult<()> {
    crate::diagnostics::reveal(&path)
}

#[allow(dead_code)]
fn _assert_state_types(_: Database, _: SessionManager, _: TransferManager, _: MonitorState) {}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile() -> ConnectionProfile {
        ConnectionProfile {
            id: "source".into(),
            folder_id: Some("folder".into()),
            protocol: "ssh".into(),
            name: "Server".into(),
            host: "example.test".into(),
            port: 22,
            username: "root".into(),
            auth_type: "sshAgent".into(),
            private_key_path: None,
            certificate_path: None,
            host_key_policy: "strict".into(),
            note: "note".into(),
            tags: vec!["tag".into()],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            has_credential: false,
            created_at: "old".into(),
            updated_at: "old".into(),
            last_connected_at: Some("old".into()),
        }
    }
    #[test]
    fn duplicate_requires_a_distinct_id_and_resets_runtime_metadata() {
        assert!(duplicate_input(profile(), "".into()).is_err());
        assert!(duplicate_input(profile(), "source".into()).is_err());
        let duplicate = duplicate_input(profile(), "copy".into()).unwrap();
        assert_eq!(duplicate.id, "copy");
        assert_eq!(duplicate.name, "Server 副本");
        assert_eq!(duplicate.folder_id.as_deref(), Some("folder"));
    }
}
