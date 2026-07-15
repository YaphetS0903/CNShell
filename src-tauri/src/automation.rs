use crate::{
    db::Database,
    error::{AppError, AppResult},
    models::{AutomationPlan, AutomationRun, AutomationStep, AutomationStepResult},
    ssh,
};
use chrono::{Datelike, LocalResult, NaiveTime, TimeZone, Utc, Weekday};
use chrono_tz::Tz;
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
    if schedule.id.trim().is_empty()
        || schedule.id.len() > 128
        || !schedule
            .id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'-' || value == b'_')
    {
        return Err(AppError::Validation("定时任务 ID 无效".into()));
    }
    validate(&schedule.plan)?;
    if !["once", "interval", "daily", "weekly", "cron"].contains(&schedule.schedule_type.as_str()) {
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
    parse_time_zone(schedule)?;
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
        "daily" => {
            parse_wall_time(expression)?;
        }
        "weekly" => {
            parse_weekly_expression(expression)?;
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
    let time_zone = parse_time_zone(schedule)?;
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
            let last_run = schedule
                .last_run_at
                .as_deref()
                .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                .map(|value| value.with_timezone(&Utc));
            let base = last_run.filter(|value| *value > after).unwrap_or(after);
            Ok(Some(
                (base + chrono::Duration::seconds(seconds)).to_rfc3339(),
            ))
        }
        "daily" => next_calendar_run(schedule, after, time_zone, None),
        "weekly" => {
            let (weekday, _) = parse_weekly_expression(schedule.expression.trim())?;
            next_calendar_run(schedule, after, time_zone, Some(weekday))
        }
        "cron" => {
            let parsed = Schedule::from_str(schedule.expression.trim())
                .map_err(|error| AppError::Validation(format!("Cron 表达式无效：{error}")))?;
            let local_after = after.with_timezone(&time_zone);
            for candidate in parsed.after(&local_after).take(4096) {
                let candidate = candidate.with_timezone(&Utc);
                if schedule.last_occurrence_key.as_deref()
                    != Some(occurrence_key(schedule, candidate)?.as_str())
                {
                    return Ok(Some(candidate.to_rfc3339()));
                }
            }
            Err(AppError::Unavailable(
                "Cron 在可检查范围内没有下一次执行时间".into(),
            ))
        }
        _ => Err(AppError::Validation("定时任务类型无效".into())),
    }
}

pub fn prepare_schedule_for_save(
    mut schedule: crate::models::AutomationSchedule,
    existing: Option<&crate::models::AutomationSchedule>,
    now: chrono::DateTime<Utc>,
) -> AppResult<crate::models::AutomationSchedule> {
    validate_schedule(&schedule)?;
    if let Some(existing) = existing {
        if existing.id != schedule.id {
            return Err(AppError::Validation("定时任务 ID 不匹配".into()));
        }
        schedule.last_run_at.clone_from(&existing.last_run_at);
        schedule
            .last_occurrence_key
            .clone_from(&existing.last_occurrence_key);
    } else {
        schedule.last_run_at = None;
        schedule.last_occurrence_key = None;
    }
    schedule.next_run_at = next_run_at(&schedule, now)?;
    if schedule.enabled && schedule.next_run_at.is_none() {
        return Err(AppError::Validation(
            "定时任务没有未来执行时间；请检查一次执行时间".into(),
        ));
    }
    Ok(schedule)
}

fn parse_time_zone(schedule: &crate::models::AutomationSchedule) -> AppResult<Tz> {
    schedule.time_zone.trim().parse::<Tz>().map_err(|_| {
        AppError::Validation("定时任务时区必须是有效 IANA 名称，例如 Asia/Shanghai".into())
    })
}

fn parse_wall_time(value: &str) -> AppResult<NaiveTime> {
    NaiveTime::parse_from_str(value, "%H:%M")
        .map_err(|_| AppError::Validation("每日时间必须使用 HH:MM 24 小时格式".into()))
}

fn parse_weekly_expression(value: &str) -> AppResult<(Weekday, NaiveTime)> {
    let (weekday, time) = value
        .split_once('@')
        .ok_or_else(|| AppError::Validation("每周时间必须使用 mon@HH:MM 格式".into()))?;
    let weekday = match weekday.to_ascii_lowercase().as_str() {
        "mon" => Weekday::Mon,
        "tue" => Weekday::Tue,
        "wed" => Weekday::Wed,
        "thu" => Weekday::Thu,
        "fri" => Weekday::Fri,
        "sat" => Weekday::Sat,
        "sun" => Weekday::Sun,
        _ => return Err(AppError::Validation("每周任务星期值无效".into())),
    };
    Ok((weekday, parse_wall_time(time)?))
}

fn next_calendar_run(
    schedule: &crate::models::AutomationSchedule,
    after: chrono::DateTime<Utc>,
    time_zone: Tz,
    weekday: Option<Weekday>,
) -> AppResult<Option<String>> {
    let time = if schedule.schedule_type == "weekly" {
        parse_weekly_expression(schedule.expression.trim())?.1
    } else {
        parse_wall_time(schedule.expression.trim())?
    };
    let start_date = after.with_timezone(&time_zone).date_naive();
    for offset in 0..=14 {
        let Some(date) = start_date.checked_add_days(chrono::Days::new(offset)) else {
            break;
        };
        if weekday.is_some_and(|expected| date.weekday() != expected) {
            continue;
        }
        let local = date.and_time(time);
        let candidate = match time_zone.from_local_datetime(&local) {
            LocalResult::Single(value) => value.with_timezone(&Utc),
            LocalResult::Ambiguous(first, second) => {
                std::cmp::min(first.with_timezone(&Utc), second.with_timezone(&Utc))
            }
            LocalResult::None => continue,
        };
        if candidate <= after {
            continue;
        }
        if schedule.last_occurrence_key.as_deref()
            == Some(occurrence_key(schedule, candidate)?.as_str())
        {
            continue;
        }
        return Ok(Some(candidate.to_rfc3339()));
    }
    Err(AppError::Unavailable(
        "日历任务在可检查范围内没有下一次执行时间".into(),
    ))
}

fn occurrence_key(
    schedule: &crate::models::AutomationSchedule,
    scheduled_at: chrono::DateTime<Utc>,
) -> AppResult<String> {
    if matches!(schedule.schedule_type.as_str(), "daily" | "weekly" | "cron") {
        let time_zone = parse_time_zone(schedule)?;
        return Ok(format!(
            "{}:{}",
            time_zone,
            scheduled_at
                .with_timezone(&time_zone)
                .format("%Y-%m-%dT%H:%M:%S")
        ));
    }
    Ok(scheduled_at.to_rfc3339())
}

fn advance_schedule(
    schedule: &mut crate::models::AutomationSchedule,
    now: chrono::DateTime<Utc>,
    scheduled_at: chrono::DateTime<Utc>,
) {
    schedule.last_run_at = Some(now.to_rfc3339());
    schedule.last_occurrence_key = occurrence_key(schedule, scheduled_at).ok();
    if schedule.schedule_type == "once" {
        schedule.enabled = false;
        schedule.next_run_at = None;
    } else {
        schedule.next_run_at = next_run_at(schedule, now).ok().flatten();
    }
}

fn collect_due_plans(
    schedules: &mut [crate::models::AutomationSchedule],
    now: chrono::DateTime<Utc>,
) -> (bool, Vec<AutomationPlan>) {
    let mut changed = false;
    let mut due_plans = Vec::new();
    for schedule in schedules {
        if !schedule.enabled {
            continue;
        }
        let scheduled_at = schedule
            .next_run_at
            .as_deref()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&Utc));
        let Some(scheduled_at) = scheduled_at else {
            schedule.next_run_at = next_run_at(schedule, now).ok().flatten();
            if schedule.schedule_type == "once" && schedule.next_run_at.is_none() {
                schedule.enabled = false;
            }
            changed = true;
            continue;
        };
        if scheduled_at > now {
            continue;
        }
        let due = if schedule.misfire_policy != "skip" {
            true
        } else {
            let grace = match schedule.schedule_type.as_str() {
                "interval" => schedule
                    .expression
                    .parse::<u64>()
                    .map(Duration::from_secs)
                    .unwrap_or(Duration::from_secs(15)),
                "daily" | "weekly" | "cron" => Duration::from_secs(120),
                _ => Duration::from_secs(30),
            };
            now.signed_duration_since(scheduled_at)
                .to_std()
                .map(|delay| delay <= grace)
                .unwrap_or(false)
        };
        if due {
            due_plans.push(schedule.plan.clone());
        }
        advance_schedule(schedule, now, scheduled_at);
        changed = true;
    }
    (changed, due_plans)
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
            let (changed, due_plans) = collect_due_plans(&mut schedules, now);
            if changed {
                let Ok(value) = serde_json::to_value(&schedules) else {
                    continue;
                };
                if db.save_named_state(SCHEDULES_KEY, &value).await.is_err() {
                    continue;
                }
            }
            for plan in due_plans {
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
            time_zone: "UTC".into(),
            next_run_at: None,
            last_run_at: None,
            last_occurrence_key: None,
        };
        assert!(validate_schedule(&schedule).is_ok());
        assert!(next_run_at(&schedule, Utc::now()).unwrap().is_some());
        schedule.schedule_type = "daily".into();
        schedule.expression = "09:30".into();
        schedule.time_zone = "Asia/Shanghai".into();
        let after = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            next_run_at(&schedule, after).unwrap().as_deref(),
            Some("2026-01-01T01:30:00+00:00")
        );
        schedule.schedule_type = "weekly".into();
        schedule.expression = "fri@09:30".into();
        assert_eq!(
            next_run_at(&schedule, after).unwrap().as_deref(),
            Some("2026-01-02T01:30:00+00:00")
        );
        schedule.schedule_type = "cron".into();
        schedule.expression = "0 30 9 * * *".into();
        assert!(validate_schedule(&schedule).is_ok());
        assert_eq!(
            next_run_at(&schedule, after).unwrap().as_deref(),
            Some("2026-01-01T01:30:00+00:00")
        );
        schedule.expression = "not cron".into();
        assert!(validate_schedule(&schedule).is_err());
        schedule.expression = "0 30 9 * * *".into();
        schedule.time_zone = "Mars/Olympus".into();
        assert!(validate_schedule(&schedule).is_err());
    }

    #[test]
    fn calendar_schedules_do_not_repeat_a_dst_fallback_wall_time() {
        let mut schedule = crate::models::AutomationSchedule {
            id: "dst".into(),
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
            schedule_type: "daily".into(),
            expression: "01:30".into(),
            enabled: true,
            misfire_policy: "runOnce".into(),
            time_zone: "America/New_York".into(),
            next_run_at: None,
            last_run_at: None,
            last_occurrence_key: None,
        };
        let before_fallback = chrono::DateTime::parse_from_rfc3339("2026-11-01T05:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let first = next_run_at(&schedule, before_fallback).unwrap().unwrap();
        assert_eq!(first, "2026-11-01T05:30:00+00:00");
        schedule.last_occurrence_key = Some(
            occurrence_key(
                &schedule,
                chrono::DateTime::parse_from_rfc3339(&first)
                    .unwrap()
                    .with_timezone(&Utc),
            )
            .unwrap(),
        );
        let after_first = chrono::DateTime::parse_from_rfc3339("2026-11-01T05:31:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            next_run_at(&schedule, after_first).unwrap().as_deref(),
            Some("2026-11-02T06:30:00+00:00")
        );

        schedule.schedule_type = "cron".into();
        schedule.expression = "0 30 1 * * *".into();
        assert_eq!(
            next_run_at(&schedule, after_first).unwrap().as_deref(),
            Some("2026-11-02T06:30:00+00:00")
        );
    }

    #[test]
    fn schedule_save_keeps_runtime_cursors_server_authoritative() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-07-16T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let schedule = crate::models::AutomationSchedule {
            id: "owned".into(),
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
            time_zone: "UTC".into(),
            next_run_at: Some("2099-01-01T00:00:00Z".into()),
            last_run_at: Some("2099-01-01T00:00:00Z".into()),
            last_occurrence_key: Some("client-forged".into()),
        };
        let created = prepare_schedule_for_save(schedule.clone(), None, now).unwrap();
        assert_eq!(created.last_run_at, None);
        assert_eq!(created.last_occurrence_key, None);
        assert_eq!(
            created.next_run_at.as_deref(),
            Some("2026-07-16T00:01:00+00:00")
        );
        let mut due = created.clone();
        due.next_run_at = Some("2026-07-15T23:59:59+00:00".into());
        let mut due_schedules = vec![due];
        let (changed, plans) = collect_due_plans(&mut due_schedules, now);
        assert!(changed);
        assert_eq!(plans.len(), 1);
        assert!(
            chrono::DateTime::parse_from_rfc3339(due_schedules[0].next_run_at.as_deref().unwrap())
                .unwrap()
                > now
        );
        let (_, duplicate_plans) = collect_due_plans(&mut due_schedules, now);
        assert!(duplicate_plans.is_empty());

        let mut skipped = created.clone();
        skipped.next_run_at = Some("2026-07-15T23:00:00+00:00".into());
        let mut skipped_schedules = vec![skipped];
        let (changed, skipped_plans) = collect_due_plans(&mut skipped_schedules, now);
        assert!(changed);
        assert!(skipped_plans.is_empty());
        assert!(skipped_schedules[0].last_occurrence_key.is_some());

        let mut existing = schedule.clone();
        existing.last_run_at = Some("2026-07-15T23:59:30+00:00".into());
        existing.last_occurrence_key = Some("server-owned".into());
        let updated = prepare_schedule_for_save(schedule, Some(&existing), now).unwrap();
        assert_eq!(updated.last_run_at, existing.last_run_at);
        assert_eq!(updated.last_occurrence_key, existing.last_occurrence_key);
        assert_eq!(
            updated.next_run_at.as_deref(),
            Some("2026-07-16T00:01:00+00:00")
        );

        let mut past_once = updated;
        past_once.schedule_type = "once".into();
        past_once.expression = "2026-07-15T00:00:00Z".into();
        assert!(prepare_schedule_for_save(past_once.clone(), None, now).is_err());
        past_once.next_run_at = None;
        let mut legacy_past_once = vec![past_once];
        let (changed, plans) = collect_due_plans(&mut legacy_past_once, now);
        assert!(changed);
        assert!(plans.is_empty());
        assert!(!legacy_past_once[0].enabled);
    }
}
