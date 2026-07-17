use crate::{
    db::Database,
    error::{AppError, AppResult},
    models::{ExternalEditSession, ExternalEditSnapshot},
    sftp,
    ssh::SessionManager,
};
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use uuid::Uuid;

const MAX_TEXT_BYTES: usize = 10 * 1024 * 1024;

#[derive(Clone, Default)]
pub struct ExternalEditManager {
    entries: Arc<Mutex<HashMap<String, ExternalEditSession>>>,
}

impl ExternalEditManager {
    pub async fn start(
        &self,
        db: Database,
        sessions: SessionManager,
        session_id: String,
        remote_path: String,
        application: Option<String>,
    ) -> AppResult<ExternalEditSession> {
        let file = sftp::open_text(db, sessions, session_id, remote_path.clone()).await?;
        let id = Uuid::new_v4().to_string();
        let directory = cache_root().join(&id);
        std::fs::create_dir_all(&directory)?;
        let name = Path::new(&remote_path)
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .unwrap_or("remote.txt");
        let local_path = directory.join(name);
        let mut local = std::fs::File::create(&local_path)?;
        use std::io::Write;
        local.write_all(file.content.as_bytes())?;
        local.sync_all()?;
        open_application(&local_path, application.as_deref())?;
        let entry = ExternalEditSession {
            id: id.clone(),
            remote_path,
            local_path: local_path.to_string_lossy().into_owned(),
            expected_modified_at: file.modified_at,
            started_at: chrono::Utc::now().to_rfc3339(),
        };
        self.entries.lock().insert(id, entry.clone());
        Ok(entry)
    }
    pub fn read(&self, id: &str) -> AppResult<ExternalEditSnapshot> {
        let entry = self
            .entries
            .lock()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("外部编辑会话 {id}")))?;
        let bytes = std::fs::read(&entry.local_path)?;
        if bytes.len() > MAX_TEXT_BYTES {
            return Err(AppError::Validation(
                "外部编辑后的文件超过 10 MB，不能回传内置文本编辑器".into(),
            ));
        }
        let content = String::from_utf8(bytes)
            .map_err(|_| AppError::Validation("外部编辑后的文件不是有效 UTF-8 文本".into()))?;
        Ok(ExternalEditSnapshot {
            id: entry.id,
            content,
            expected_modified_at: entry.expected_modified_at,
        })
    }
    pub fn discard(&self, id: &str) -> AppResult<()> {
        let entry = self
            .entries
            .lock()
            .remove(id)
            .ok_or_else(|| AppError::NotFound(format!("外部编辑会话 {id}")))?;
        if let Some(parent) = Path::new(&entry.local_path).parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
        Ok(())
    }
}

pub fn cleanup_cache() -> AppResult<()> {
    let root = cache_root();
    if root.exists() {
        std::fs::remove_dir_all(&root)?;
    }
    std::fs::create_dir_all(root)?;
    Ok(())
}
fn cache_root() -> PathBuf {
    std::env::temp_dir().join("CNshellExternalEdit")
}
fn open_application(path: &Path, application: Option<&str>) -> AppResult<()> {
    crate::platform::open_local_path(path, application)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cleanup_removes_stale_external_edit_files() {
        let root = cache_root();
        let stale = root.join("stale");
        std::fs::create_dir_all(&stale).unwrap();
        std::fs::write(stale.join("secret.txt"), b"secret").unwrap();
        cleanup_cache().unwrap();
        assert!(!stale.exists());
    }
}
