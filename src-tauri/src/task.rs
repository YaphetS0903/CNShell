use crate::{
    error::{AppError, AppResult},
    models::BackgroundTask,
};
use chrono::Utc;
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

#[derive(Clone)]
struct TaskEntry {
    task: BackgroundTask,
    cancelled: Arc<AtomicBool>,
}

#[derive(Clone, Default)]
pub struct TaskManager {
    entries: Arc<Mutex<HashMap<String, TaskEntry>>>,
}

impl TaskManager {
    pub fn spawn<F, Fut>(&self, app: AppHandle, kind: &str, operation: F) -> BackgroundTask
    where
        F: FnOnce(Arc<AtomicBool>) -> Fut + Send + 'static,
        Fut: Future<Output = AppResult<serde_json::Value>> + Send + 'static,
    {
        let task = BackgroundTask {
            id: Uuid::new_v4().to_string(),
            kind: kind.into(),
            status: "queued".into(),
            result: None,
            error: None,
            created_at: Utc::now().to_rfc3339(),
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        self.entries.lock().insert(
            task.id.clone(),
            TaskEntry {
                task: task.clone(),
                cancelled: cancelled.clone(),
            },
        );
        let manager = self.clone();
        let task_id = task.id.clone();
        tauri::async_runtime::spawn(async move {
            manager.update(&app, &task_id, "running", None, None);
            let result = operation(cancelled.clone()).await;
            if cancelled.load(Ordering::Acquire) {
                manager.update(&app, &task_id, "cancelled", None, None);
                return;
            }
            match result {
                Ok(value) => manager.update(&app, &task_id, "completed", Some(value), None),
                Err(error) => {
                    manager.update(&app, &task_id, "failed", None, Some(error.to_string()))
                }
            }
        });
        task
    }

    pub fn get(&self, id: &str) -> AppResult<BackgroundTask> {
        self.entries
            .lock()
            .get(id)
            .map(|entry| entry.task.clone())
            .ok_or_else(|| AppError::NotFound(format!("后台任务 {id}")))
    }

    pub fn cancel(&self, app: &AppHandle, id: &str) -> AppResult<()> {
        let mut entries = self.entries.lock();
        let entry = entries
            .get_mut(id)
            .ok_or_else(|| AppError::NotFound(format!("后台任务 {id}")))?;
        if ["completed", "failed", "cancelled"].contains(&entry.task.status.as_str()) {
            return Ok(());
        }
        entry.cancelled.store(true, Ordering::Release);
        entry.task.status = "cancelled".into();
        let task = entry.task.clone();
        drop(entries);
        let _ = app.emit("background-task", task);
        Ok(())
    }

    fn update(
        &self,
        app: &AppHandle,
        id: &str,
        status: &str,
        result: Option<serde_json::Value>,
        error: Option<String>,
    ) {
        let mut entries = self.entries.lock();
        let Some(entry) = entries.get_mut(id) else {
            return;
        };
        if entry.cancelled.load(Ordering::Acquire) && status != "cancelled" {
            return;
        }
        entry.task.status = status.into();
        entry.task.result = result;
        entry.task.error = error;
        let task = entry.task.clone();
        drop(entries);
        let _ = app.emit("background-task", task);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_tasks_are_rejected() {
        assert!(matches!(
            TaskManager::default().get("missing"),
            Err(AppError::NotFound(_))
        ));
    }
}
