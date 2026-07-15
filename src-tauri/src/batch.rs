use crate::{
    db::Database,
    error::{AppError, AppResult},
    models::{BatchExecution, BatchTargetResult, ConnectionProfile},
    ssh,
};
use futures_util::{StreamExt, stream::FuturesUnordered};
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use tokio::sync::Semaphore;
use uuid::Uuid;

#[derive(Clone)]
struct BatchEntry {
    execution: BatchExecution,
    cancelled: Arc<AtomicBool>,
}

#[derive(Clone, Default)]
pub struct BatchManager {
    entries: Arc<Mutex<HashMap<String, BatchEntry>>>,
}

impl BatchManager {
    pub fn start(
        &self,
        app: AppHandle,
        db: Database,
        profiles: Vec<ConnectionProfile>,
        command: String,
        concurrency: usize,
    ) -> AppResult<BatchExecution> {
        validate_batch(&profiles, &command, concurrency)?;
        let execution = BatchExecution {
            id: Uuid::new_v4().to_string(),
            command: command.clone(),
            status: "queued".into(),
            created_at: chrono::Utc::now().to_rfc3339(),
            targets: profiles
                .iter()
                .map(|profile| BatchTargetResult {
                    connection_id: profile.id.clone(),
                    name: profile.name.clone(),
                    status: "queued".into(),
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: None,
                    duration_ms: None,
                    error: None,
                })
                .collect(),
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        self.entries.lock().insert(
            execution.id.clone(),
            BatchEntry {
                execution: execution.clone(),
                cancelled: cancelled.clone(),
            },
        );
        let manager = self.clone();
        let batch_id = execution.id.clone();
        tauri::async_runtime::spawn(async move {
            manager.set_batch_status(&app, &batch_id, "running");
            let semaphore = Arc::new(Semaphore::new(concurrency));
            let mut work = FuturesUnordered::new();
            for profile in profiles {
                let permit = semaphore.clone();
                let manager = manager.clone();
                let app = app.clone();
                let db = db.clone();
                let command = command.clone();
                let batch_id = batch_id.clone();
                let cancelled = cancelled.clone();
                work.push(async move {
                    let Ok(_permit) = permit.acquire_owned().await else {
                        return;
                    };
                    if cancelled.load(Ordering::Acquire) {
                        manager.finish_cancelled_target(&app, &batch_id, &profile.id);
                        return;
                    }
                    manager.set_target_running(&app, &batch_id, &profile.id);
                    let started = Instant::now();
                    let result = ssh::execute_profile_command(
                        &db,
                        &profile,
                        &command,
                        cancelled.clone(),
                        Duration::from_secs(600),
                    )
                    .await;
                    manager.finish_target(&app, &batch_id, &profile.id, started, result);
                });
            }
            while work.next().await.is_some() {}
            manager.finish_batch(&app, &batch_id);
        });
        Ok(execution)
    }

    pub fn get(&self, id: &str) -> AppResult<BatchExecution> {
        self.entries
            .lock()
            .get(id)
            .map(|entry| entry.execution.clone())
            .ok_or_else(|| AppError::NotFound(format!("批量任务 {id}")))
    }
    pub fn cancel(&self, app: &AppHandle, id: &str) -> AppResult<BatchExecution> {
        let mut entries = self.entries.lock();
        let entry = entries
            .get_mut(id)
            .ok_or_else(|| AppError::NotFound(format!("批量任务 {id}")))?;
        entry.cancelled.store(true, Ordering::Release);
        entry.execution.status = "cancelled".into();
        for target in &mut entry.execution.targets {
            if target.status == "queued" {
                target.status = "cancelled".into();
                target.error = Some("用户已取消".into());
            }
        }
        let execution = entry.execution.clone();
        drop(entries);
        let _ = app.emit("batch-execution", execution.clone());
        Ok(execution)
    }

    fn mutate(&self, app: &AppHandle, id: &str, operation: impl FnOnce(&mut BatchExecution)) {
        let mut entries = self.entries.lock();
        let Some(entry) = entries.get_mut(id) else {
            return;
        };
        operation(&mut entry.execution);
        let execution = entry.execution.clone();
        drop(entries);
        let _ = app.emit("batch-execution", execution);
    }
    fn set_batch_status(&self, app: &AppHandle, id: &str, status: &str) {
        self.mutate(app, id, |execution| execution.status = status.into());
    }
    fn set_target_running(&self, app: &AppHandle, id: &str, connection_id: &str) {
        self.mutate(app, id, |execution| {
            if let Some(target) = execution
                .targets
                .iter_mut()
                .find(|target| target.connection_id == connection_id)
            {
                target.status = "running".into();
            }
        });
    }
    fn finish_cancelled_target(&self, app: &AppHandle, id: &str, connection_id: &str) {
        self.mutate(app, id, |execution| {
            if let Some(target) = execution
                .targets
                .iter_mut()
                .find(|target| target.connection_id == connection_id)
            {
                target.status = "cancelled".into();
                target.error = Some("用户已取消".into());
            }
        });
    }
    fn finish_target(
        &self,
        app: &AppHandle,
        id: &str,
        connection_id: &str,
        started: Instant,
        result: AppResult<ssh::RemoteCommandResult>,
    ) {
        self.mutate(app, id, |execution| {
            let Some(target) = execution
                .targets
                .iter_mut()
                .find(|target| target.connection_id == connection_id)
            else {
                return;
            };
            target.duration_ms = Some(started.elapsed().as_millis().min(u64::MAX as u128) as u64);
            match result {
                Ok(output) => {
                    target.status = if output.exit_code == 0 {
                        "completed".into()
                    } else {
                        "failed".into()
                    };
                    target.stdout = output.stdout;
                    target.stderr = output.stderr;
                    target.exit_code = Some(output.exit_code);
                    if output.truncated {
                        target.error = Some("输出超过 5 MB，已截断显示".into());
                    }
                }
                Err(error) => {
                    target.status = if error.to_string().contains("已取消") {
                        "cancelled".into()
                    } else {
                        "failed".into()
                    };
                    target.error = Some(error.to_string());
                }
            }
        });
    }
    fn finish_batch(&self, app: &AppHandle, id: &str) {
        self.mutate(app, id, |execution| {
            if execution.status == "cancelled" {
                return;
            }
            execution.status = if execution
                .targets
                .iter()
                .all(|target| target.status == "completed")
            {
                "completed".into()
            } else {
                "failed".into()
            };
        });
    }
}

fn validate_batch(
    profiles: &[ConnectionProfile],
    command: &str,
    concurrency: usize,
) -> AppResult<()> {
    if profiles.is_empty() || profiles.len() > 50 {
        return Err(AppError::Validation(
            "批量执行目标数量必须在 1 到 50 之间".into(),
        ));
    }
    if !(1..=10).contains(&concurrency) {
        return Err(AppError::Validation("批量并发数必须在 1 到 10 之间".into()));
    }
    if command.trim().is_empty() || command.len() > 64 * 1024 || command.contains('\0') {
        return Err(AppError::Validation("批量命令为空或超过 64 KB".into()));
    }
    if is_destructive(command) {
        return Err(AppError::Validation(
            "批量模式默认禁止明显破坏性命令，请改为逐台执行".into(),
        ));
    }
    Ok(())
}

fn is_destructive(command: &str) -> bool {
    let value = command.to_ascii_lowercase();
    for segment in value.split([';', '\n', '&', '|']) {
        let tokens = segment
            .split_whitespace()
            .map(|token| token.trim_matches(['\'', '"']))
            .collect::<Vec<_>>();
        if tokens.iter().any(|token| {
            matches!(*token, "shutdown" | "reboot" | "poweroff") || token.starts_with("mkfs.")
        }) || tokens.iter().any(|token| token.starts_with("of=/dev/"))
            && tokens.iter().any(|token| *token == "dd")
        {
            return true;
        }
        for (index, token) in tokens.iter().enumerate() {
            if *token != "rm" {
                continue;
            }
            let mut recursive = false;
            let mut force = false;
            for argument in tokens.iter().skip(index + 1) {
                if argument.starts_with('-') {
                    recursive |= argument.contains('r') || argument.contains('R');
                    force |= argument.contains('f');
                } else if recursive && force && (*argument == "/" || argument.starts_with("/*")) {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile(id: &str) -> ConnectionProfile {
        ConnectionProfile {
            id: id.into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: id.into(),
            host: "localhost".into(),
            port: 22,
            username: "test".into(),
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
            has_credential: false,
            created_at: String::new(),
            updated_at: String::new(),
            last_connected_at: None,
        }
    }
    #[test]
    fn rejects_destructive_commands_and_invalid_concurrency() {
        assert!(validate_batch(&[profile("one")], "rm -rf /", 2).is_err());
        assert!(validate_batch(&[profile("one")], "sudo rm  -r -f  /", 2).is_err());
        assert!(validate_batch(&[profile("one")], "dd if=/dev/zero of=/dev/disk2", 2).is_err());
        assert!(validate_batch(&[profile("one")], "sudo reboot now", 2).is_err());
        assert!(validate_batch(&[profile("one")], "uname -a", 0).is_err());
        assert!(validate_batch(&[profile("one")], "uname -a", 1).is_ok());
    }
}
