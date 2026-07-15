use crate::{
    db::Database,
    error::{AppError, AppResult},
    models::{AutomationPlan, AutomationRun, AutomationStep, AutomationStepResult},
    ssh,
};
use chrono::Utc;
use cron::Schedule;
use regex::Regex;
use std::str::FromStr;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use uuid::Uuid;

const MAX_STEPS: usize = 50;
const MAX_TIMEOUT_SECONDS: u64 = 3600;
const SCHEDULES_KEY: &str = "cnshell.automation.schedules";

pub fn schedules_key() -> &'static str {
    SCHEDULES_KEY
}

pub fn validate_schedule(schedule: &crate::models::AutomationSchedule) -> AppResult<()> {
    if schedule.id.trim().is_empty() {
        return Err(AppError::Validation("定时任务 ID 不能为空".into()));
    }
    validate(&schedule.plan)?;
    if !["once", "interval", "cron"].contains(&schedule.schedule_type.as_str()) {
        return Err(AppError::Validation("定时任务类型无效".into()));
    }
    if !["skip", "runOnce"].contains(&schedule.misfire_policy.as_str()) {
        return Err(AppError::Validation("错过执行策略无效".into()));
    }
    let expression = schedule.expression.trim();
    if expression.is_empty() || expression.len() > 128 {
        return Err(AppError::Validation(
            "定时任务表达式不能为空且不能超过 128 字符".into(),
        ));
    }
    match schedule.schedule_type.as_str() {
        "once" => {
            chrono::DateTime::parse_from_rfc3339(expression)
                .map_err(|_| AppError::Validation("一次执行时间必须是 RFC3339 时间".into()))?;
        }
        "interval" => {
            let seconds = expression
                .parse::<u64>()
                .map_err(|_| AppError::Validation("间隔必须是秒数".into()))?;
            if !(10..=2_592_000).contains(&seconds) {
                return Err(AppError::Validation("间隔必须为 10 秒至 30 天".into()));
            }
        }
        "cron" => {
            Schedule::from_str(expression)
                .map_err(|error| AppError::Validation(format!("Cron 表达式无效：{error}")))?;
        }
        _ => unreachable!(),
    }
    Ok(())
}

pub fn next_run_at(
    schedule: &crate::models::AutomationSchedule,
    after: chrono::DateTime<Utc>,
) -> AppResult<Option<String>> {
    if !schedule.enabled {
        return Ok(None);
    }
    match schedule.schedule_type.as_str() {
        "once" => {
            let at = chrono::DateTime::parse_from_rfc3339(schedule.expression.trim())
                .map_err(|_| AppError::Validation("一次执行时间必须是 RFC3339 时间".into()))?
                .with_timezone(&Utc);
            Ok((at > after).then(|| at.to_rfc3339()))
        }
        "interval" => {
            let seconds: i64 = schedule
                .expression
                .trim()
                .parse()
                .map_err(|_| AppError::Validation("间隔必须是秒数".into()))?;
            let base = schedule
                .last_run_at
                .as_deref()
                .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                .map(|value| value.with_timezone(&Utc))
                .unwrap_or(after);
            Ok(Some(
                (base + chrono::Duration::seconds(seconds)).to_rfc3339(),
            ))
        }
        "cron" => {
            let parsed = Schedule::from_str(schedule.expression.trim())
                .map_err(|error| AppError::Validation(format!("Cron 表达式无效：{error}")))?;
            Ok(parsed.after(&after).next().map(|value| value.to_rfc3339()))
        }
        _ => Err(AppError::Validation("定时任务类型无效".into())),
    }
}

pub fn start_scheduler(app: tauri::AppHandle, db: Database, tasks: crate::task::TaskManager) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(15));
        loop {
            ticker.tick().await;
            let now = Utc::now();
            let mut schedules = match db
                .load_named_state::<Vec<crate::models::AutomationSchedule>>(SCHEDULES_KEY)
                .await
            {
                Ok(Some(value)) => value,
                Ok(None) => continue,
                Err(_) => continue,
            };
            let mut changed = false;
            for schedule in &mut schedules {
                if !schedule.enabled {
                    continue;
                }
                let due = schedule
                    .next_run_at
                    .as_deref()
                    .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                    .map(|value| {
                        let scheduled_at = value.with_timezone(&Utc);
                        if scheduled_at > now {
                            return false;
                        }
                        if schedule.misfire_policy != "skip" {
                            return true;
                        }
                        let grace = match schedule.schedule_type.as_str() {
                            "interval" => schedule
                                .expression
                                .parse::<u64>()
                                .map(Duration::from_secs)
                                .unwrap_or(Duration::from_secs(15)),
                            "cron" => Duration::from_secs(120),
                            _ => Duration::from_secs(30),
                        };
                        now.signed_duration_since(scheduled_at)
                            .to_std()
                            .map(|delay| delay <= grace)
                            .unwrap_or(false)
                    })
                    .unwrap_or(false);
                if !due {
                    if schedule
                        .next_run_at
                        .as_deref()
                        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                        .map(|value| value.with_timezone(&Utc) <= now)
                        .unwrap_or(false)
                    {
                        schedule.last_run_at = Some(now.to_rfc3339());
                        if schedule.schedule_type == "once" {
                            schedule.enabled = false;
                            schedule.next_run_at = None;
                        } else {
                            schedule.next_run_at = next_run_at(schedule, now).ok().flatten();
                        }
                        changed = true;
                    }
                    continue;
                }
                let plan = schedule.plan.clone();
                schedule.last_run_at = Some(now.to_rfc3339());
                if schedule.schedule_type == "once" {
                    schedule.enabled = false;
                    schedule.next_run_at = None;
                } else {
                    schedule.next_run_at = next_run_at(schedule, now).ok().flatten();
                }
                changed = true;
                let db_for_run = db.clone();
                tasks.spawn(
                    app.clone(),
                    "automation-scheduled",
                    move |cancelled| async move {
                        serde_json::to_value(run(db_for_run, plan, cancelled).await?)
                            .map_err(|error| AppError::Internal(error.to_string()))
                    },
                );
            }
            if changed {
                if let Ok(value) = serde_json::to_value(&schedules) {
                    let _ = db.save_named_state(SCHEDULES_KEY, &value).await;
                }
            }
        }
    });
}

pub fn validate(plan: &AutomationPlan) -> AppResult<()> {
    if plan.id.trim().is_empty()
        || plan.name.trim().is_empty()
        || plan.connection_id.trim().is_empty()
    {
        return Err(AppError::Validation("自动化名称和目标连接不能为空".into()));
    }
    if plan.steps.is_empty() || plan.steps.len() > MAX_STEPS {
        return Err(AppError::Validation(format!(
            "自动化步骤必须为 1～{MAX_STEPS} 项"
        )));
    }
    let mut ids = std::collections::HashSet::new();
    for step in &plan.steps {
        if step.id.is_empty() || !ids.insert(step.id.as_str()) {
            return Err(AppError::Validation("步骤 ID 不能为空或重复".into()));
        }
        let timeout = step.timeout_seconds.unwrap_or(30);
        if timeout == 0 || timeout > MAX_TIMEOUT_SECONDS {
            return Err(AppError::Validation("步骤超时必须为 1～3600 秒".into()));
        }
        match step.kind.as_str() {
            "command" => {
                if step.command.as_deref().unwrap_or("").trim().is_empty() {
                    return Err(AppError::Validation("命令步骤不能为空".into()));
                }
            }
            "waitForMatch" => {
                let pattern = step.pattern.as_deref().unwrap_or("");
                if pattern.is_empty() || pattern.len() > 512 {
                    return Err(AppError::Validation("匹配表达式必须为 1～512 字符".into()));
                }
                Regex::new(pattern)
                    .map_err(|error| AppError::Validation(format!("匹配表达式无效：{error}")))?;
            }
            "condition" => {
                let pattern = step.pattern.as_deref().unwrap_or("");
                if pattern.is_empty() || pattern.len() > 512 {
                    return Err(AppError::Validation("条件表达式必须为 1～512 字符".into()));
                }
                Regex::new(pattern)
                    .map_err(|error| AppError::Validation(format!("条件表达式无效：{error}")))?;
                if !["continueIfMatch", "stopIfMatch", "stopIfMissing"]
                    .contains(&step.action.as_deref().unwrap_or(""))
                {
                    return Err(AppError::Validation("条件动作无效".into()));
                }
            }
            "transfer" => {
                if !["upload", "download"].contains(&step.direction.as_deref().unwrap_or(""))
                    || step.local_path.as_deref().unwrap_or("").is_empty()
                    || step.remote_path.as_deref().unwrap_or("").is_empty()
                {
                    return Err(AppError::Validation("文件传输步骤缺少方向或路径".into()));
                }
            }
            _ => {
                return Err(AppError::Validation(format!(
                    "不支持的自动化步骤：{}",
                    step.kind
                )));
            }
        }
    }
    Ok(())
}

pub async fn run(
    db: Database,
    plan: AutomationPlan,
    cancelled: Arc<AtomicBool>,
) -> AppResult<AutomationRun> {
    validate(&plan)?;
    let profile = db.get_connection(&plan.connection_id).await?;
    if profile.protocol != "ssh" {
        return Err(AppError::Validation("自动化仅支持 SSH 连接".into()));
    }
    let run_id = Uuid::new_v4().to_string();
    let mut results = Vec::new();
    let mut accumulated = String::new();
    for step in &plan.steps {
        if cancelled.load(Ordering::Acquire) {
            return Ok(AutomationRun {
                run_id,
                plan_id: plan.id,
                status: "cancelled".into(),
                current_step: Some(step.id.clone()),
                results,
            });
        }
        let started_at = Utc::now().to_rfc3339();
        let started = Instant::now();
        let result = execute_step(&db, &profile, step, &mut accumulated, cancelled.clone()).await;
        let duration_ms = started.elapsed().as_millis() as u64;
        match result {
            Ok(StepOutcome::Continue(output)) => results.push(AutomationStepResult {
                step_id: step.id.clone(),
                kind: step.kind.clone(),
                status: "completed".into(),
                started_at,
                duration_ms,
                output,
                error: None,
            }),
            Ok(StepOutcome::Stop(output)) => {
                results.push(AutomationStepResult {
                    step_id: step.id.clone(),
                    kind: step.kind.clone(),
                    status: "completed".into(),
                    started_at,
                    duration_ms,
                    output,
                    error: None,
                });
                return Ok(AutomationRun {
                    run_id,
                    plan_id: plan.id,
                    status: "completed".into(),
                    current_step: None,
                    results,
                });
            }
            Err(error) => {
                results.push(AutomationStepResult {
                    step_id: step.id.clone(),
                    kind: step.kind.clone(),
                    status: "failed".into(),
                    started_at,
                    duration_ms,
                    output: String::new(),
                    error: Some(error.to_string()),
                });
                return Ok(AutomationRun {
                    run_id,
                    plan_id: plan.id,
                    status: "failed".into(),
                    current_step: Some(step.id.clone()),
                    results,
                });
            }
        }
    }
    Ok(AutomationRun {
        run_id,
        plan_id: plan.id,
        status: "completed".into(),
        current_step: None,
        results,
    })
}

enum StepOutcome {
    Continue(String),
    Stop(String),
}

async fn execute_step(
    db: &Database,
    profile: &crate::models::ConnectionProfile,
    step: &AutomationStep,
    accumulated: &mut String,
    cancelled: Arc<AtomicBool>,
) -> AppResult<StepOutcome> {
    let timeout = Duration::from_secs(step.timeout_seconds.unwrap_or(30));
    match step.kind.as_str() {
        "command" => {
            let result = ssh::execute_profile_command(
                db,
                profile,
                step.command.as_deref().unwrap_or(""),
                cancelled,
                timeout,
            )
            .await?;
            let combined = format!("{}{}", result.stdout, result.stderr);
            accumulated.push_str(&combined);
            if result.exit_code != 0 {
                return Err(AppError::Remote(format!(
                    "命令退出码 {}：{}",
                    result.exit_code,
                    result.stderr.trim()
                )));
            }
            Ok(StepOutcome::Continue(if result.truncated {
                format!("{combined}\n[输出已截断]")
            } else {
                combined
            }))
        }
        "waitForMatch" => {
            let regex = Regex::new(step.pattern.as_deref().unwrap_or(""))?;
            let started = Instant::now();
            loop {
                if regex.is_match(accumulated) {
                    return Ok(StepOutcome::Continue("已在此前输出中匹配".into()));
                }
                if cancelled.load(Ordering::Acquire) {
                    return Err(AppError::Unavailable("自动化已取消".into()));
                }
                if started.elapsed() >= timeout {
                    return Err(AppError::Unavailable(
                        "等待匹配超时；该步骤只观察本工作流此前命令的输出".into(),
                    ));
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        "condition" => {
            let matched = Regex::new(step.pattern.as_deref().unwrap_or(""))?.is_match(accumulated);
            match step.action.as_deref() {
                Some("continueIfMatch") if !matched => {
                    Err(AppError::Unavailable("条件不匹配，工作流停止".into()))
                }
                Some("stopIfMatch") if matched => {
                    Ok(StepOutcome::Stop("条件匹配，按计划结束".into()))
                }
                Some("stopIfMissing") if !matched => {
                    Ok(StepOutcome::Stop("条件缺失，按计划结束".into()))
                }
                _ => Ok(StepOutcome::Continue(format!(
                    "条件检查：{}",
                    if matched { "匹配" } else { "未匹配" }
                ))),
            }
        }
        "transfer" => {
            transfer(db, profile, step, cancelled, timeout).await?;
            Ok(StepOutcome::Continue(format!(
                "{} 完成",
                if step.direction.as_deref() == Some("upload") {
                    "上传"
                } else {
                    "下载"
                }
            )))
        }
        _ => Err(AppError::Validation("不支持的步骤".into())),
    }
}

async fn transfer(
    db: &Database,
    profile: &crate::models::ConnectionProfile,
    step: &AutomationStep,
    cancelled: Arc<AtomicBool>,
    timeout: Duration,
) -> AppResult<()> {
    let connected = ssh::verified_connection(db, profile, false).await?;
    let direction = step.direction.clone().unwrap_or_default();
    let local = step.local_path.clone().unwrap_or_default();
    let remote = step.remote_path.clone().unwrap_or_default();
    tokio::time::timeout(
        timeout,
        tokio::task::spawn_blocking(move || -> AppResult<()> {
            use std::io::{Read, Write};
            let sftp = connected.session.sftp()?;
            let mut buffer = [0_u8; 128 * 1024];
            if direction == "upload" {
                let mut source = std::fs::File::open(&local)?;
                let temporary = format!("{remote}.cnshell-automation-part");
                let mut target = sftp.create(std::path::Path::new(&temporary))?;
                loop {
                    if cancelled.load(Ordering::Acquire) {
                        let _ = sftp.unlink(std::path::Path::new(&temporary));
                        return Err(AppError::Unavailable("自动化已取消".into()));
                    }
                    let count = source.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    target.write_all(&buffer[..count])?;
                }
                target.fsync()?;
                drop(target);
                sftp.rename(
                    std::path::Path::new(&temporary),
                    std::path::Path::new(&remote),
                    None,
                )?;
            } else {
                let mut source = sftp.open(std::path::Path::new(&remote))?;
                let temporary = format!("{local}.cnshell-automation-part");
                let mut target = std::fs::File::create(&temporary)?;
                loop {
                    if cancelled.load(Ordering::Acquire) {
                        let _ = std::fs::remove_file(&temporary);
                        return Err(AppError::Unavailable("自动化已取消".into()));
                    }
                    let count = source.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    target.write_all(&buffer[..count])?;
                }
                target.sync_all()?;
                drop(target);
                std::fs::rename(temporary, local)?;
            }
            Ok(())
        }),
    )
    .await
    .map_err(|_| AppError::Unavailable("文件传输步骤超时".into()))?
    .map_err(|error| AppError::Internal(error.to_string()))?
}

impl From<regex::Error> for AppError {
    fn from(value: regex::Error) -> Self {
        AppError::Validation(format!("正则表达式无效：{value}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plan(step: AutomationStep) -> AutomationPlan {
        AutomationPlan {
            id: "p".into(),
            name: "计划".into(),
            connection_id: "c".into(),
            steps: vec![step],
        }
    }
    #[test]
    fn rejects_unknown_and_unsafe_shapes() {
        assert!(
            validate(&plan(AutomationStep {
                id: "s".into(),
                kind: "python".into(),
                command: None,
                pattern: None,
                timeout_seconds: Some(30),
                action: None,
                direction: None,
                local_path: None,
                remote_path: None
            }))
            .is_err()
        );
        assert!(
            validate(&plan(AutomationStep {
                id: "s".into(),
                kind: "waitForMatch".into(),
                command: None,
                pattern: Some("(".into()),
                timeout_seconds: Some(30),
                action: None,
                direction: None,
                local_path: None,
                remote_path: None
            }))
            .is_err()
        );
    }

    #[test]
    fn validates_schedule_types_and_calculates_next_runs() {
        let mut schedule = crate::models::AutomationSchedule {
            id: "schedule".into(),
            plan: plan(AutomationStep {
                id: "step".into(),
                kind: "command".into(),
                command: Some("true".into()),
                pattern: None,
                timeout_seconds: Some(30),
                action: None,
                direction: None,
                local_path: None,
                remote_path: None,
            }),
            schedule_type: "interval".into(),
            expression: "60".into(),
            enabled: true,
            misfire_policy: "skip".into(),
            next_run_at: None,
            last_run_at: None,
        };
        assert!(validate_schedule(&schedule).is_ok());
        assert!(next_run_at(&schedule, Utc::now()).unwrap().is_some());
        schedule.schedule_type = "cron".into();
        schedule.expression = "0 0 * * * *".into();
        assert!(validate_schedule(&schedule).is_ok());
        schedule.expression = "not cron".into();
        assert!(validate_schedule(&schedule).is_err());
    }
}
