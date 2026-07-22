use crate::{
    db::Database,
    error::{AppError, AppResult},
    models::*,
    ssh::SessionManager,
};
use parking_lot::Mutex;
use std::{collections::HashMap, path::Path, sync::Arc, time::Instant};

type CpuSnapshot = (u64, u64);
type NetworkCounters = HashMap<String, (u64, u64)>;
type NetworkSnapshot = (Instant, NetworkCounters);

#[derive(Clone, Default)]
pub struct MonitorState {
    cpu: Arc<Mutex<HashMap<String, CpuSnapshot>>>,
    network: Arc<Mutex<HashMap<String, NetworkSnapshot>>>,
}

impl MonitorState {
    pub fn remove(&self, session_id: &str) {
        self.cpu.lock().remove(session_id);
        self.network.lock().remove(session_id);
    }
    #[cfg(test)]
    fn contains(&self, session_id: &str) -> bool {
        self.cpu.lock().contains_key(session_id) || self.network.lock().contains_key(session_id)
    }
}

const SNAPSHOT_COMMAND: &str = r#"LC_ALL=C; \
echo __HOST__; hostname 2>/dev/null; hostname -I 2>/dev/null | awk '{print $1}'; cat /proc/uptime 2>/dev/null; cat /proc/loadavg 2>/dev/null; \
echo __CPU__; head -n1 /proc/stat 2>/dev/null; \
echo __MEM__; cat /proc/meminfo 2>/dev/null; \
echo __PROC__; LC_ALL=C ps -eo pid=,lstart=,user=,pcpu=,pmem=,args= --sort=-pcpu 2>/dev/null | head -n 500; \
echo __NET__; cat /proc/net/dev 2>/dev/null; \
echo __DISK__; df -Pk 2>/dev/null; \
echo __END__"#;

const SYSTEM_COMMAND: &str = r#"LC_ALL=C; \
echo __BASIC__; hostname; . /etc/os-release 2>/dev/null; echo "${PRETTY_NAME:-Unknown Linux}"; uname -r; uname -m; \
echo __CPU__; grep -m1 'model name\|Hardware' /proc/cpuinfo 2>/dev/null | cut -d: -f2-; grep -c '^processor' /proc/cpuinfo 2>/dev/null; \
echo __MEM__; awk '/MemTotal/{print $2}' /proc/meminfo 2>/dev/null; \
echo __ADDR__; ip -o addr show 2>/dev/null; \
echo __DISK__; df -Pk 2>/dev/null; echo __END__"#;

async fn exec(
    db: &Database,
    manager: &SessionManager,
    session_id: &str,
    command: &'static str,
) -> AppResult<String> {
    exec_cancelable(
        db,
        manager,
        session_id,
        command,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
    .await
}

async fn exec_cancelable(
    db: &Database,
    manager: &SessionManager,
    session_id: &str,
    command: &'static str,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
) -> AppResult<String> {
    let profile = manager.profile(session_id)?;
    let result = crate::ssh::execute_pooled_command(
        db,
        manager,
        &profile,
        command,
        cancelled,
        std::time::Duration::from_secs(30),
        5 * 1024 * 1024,
    )
    .await?;
    Ok(result.stdout)
}

fn section<'a>(text: &'a str, name: &str, next: &str) -> &'a str {
    let Some((_, body_start)) = marker_line_bounds(text, name, 0) else {
        return "";
    };
    let Some((body_end, _)) = marker_line_bounds(text, next, body_start) else {
        return "";
    };
    text[body_start..body_end].trim()
}

fn marker_line_bounds(text: &str, marker: &str, from: usize) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut start = from;
    while start <= text.len() {
        let end = text[start..]
            .find('\n')
            .map(|offset| start + offset)
            .unwrap_or(text.len());
        let content_end = if end > start && bytes[end - 1] == b'\r' {
            end - 1
        } else {
            end
        };
        if &text[start..content_end] == marker {
            return Some((start, if end < text.len() { end + 1 } else { end }));
        }
        if end == text.len() {
            break;
        }
        start = end + 1;
    }
    None
}

fn parse_cpu(line: &str) -> Option<(u64, u64)> {
    let values = line
        .split_whitespace()
        .skip(1)
        .filter_map(|value| value.parse::<u64>().ok())
        .collect::<Vec<_>>();
    if values.len() < 4 {
        return None;
    }
    let idle = values[3] + values.get(4).copied().unwrap_or(0);
    Some((values.iter().sum(), idle))
}

fn bytes_from_kb(value: u64) -> u64 {
    value.saturating_mul(1024)
}

fn parse_memory(body: &str) -> (u64, u64, u64, u64) {
    let mut map = HashMap::new();
    for line in body.lines() {
        if let Some((key, value)) = line.split_once(':')
            && let Some(number) = value
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<u64>().ok())
        {
            map.insert(key, bytes_from_kb(number));
        }
    }
    let total = *map.get("MemTotal").unwrap_or(&0);
    let available = *map
        .get("MemAvailable")
        .or_else(|| map.get("MemFree"))
        .unwrap_or(&0);
    let swap_total = *map.get("SwapTotal").unwrap_or(&0);
    let swap_free = *map.get("SwapFree").unwrap_or(&0);
    (
        total.saturating_sub(available),
        total,
        swap_total.saturating_sub(swap_free),
        swap_total,
    )
}

fn parse_disks(body: &str) -> Vec<DiskInfo> {
    body.lines()
        .skip(1)
        .filter_map(|line| {
            let c = line.split_whitespace().collect::<Vec<_>>();
            if c.len() < 6 {
                return None;
            }
            let total = bytes_from_kb(c[1].parse().ok()?);
            let used = bytes_from_kb(c[2].parse().ok()?);
            let available = bytes_from_kb(c[3].parse().ok()?);
            Some(DiskInfo {
                filesystem: c[0].into(),
                mount_point: c[5..].join(" "),
                total_bytes: total,
                used_bytes: used,
                available_bytes: available,
                used_percent: c[4].trim_end_matches('%').parse().unwrap_or(0.0),
            })
        })
        .collect()
}

fn parse_network(body: &str) -> HashMap<String, (u64, u64)> {
    body.lines()
        .skip(2)
        .filter_map(|line| {
            let (name, values) = line.split_once(':')?;
            let v = values.split_whitespace().collect::<Vec<_>>();
            Some((
                name.trim().into(),
                (v.first()?.parse().ok()?, v.get(8)?.parse().ok()?),
            ))
        })
        .collect()
}

fn capability_warnings(
    hostname: &str,
    cpu: Option<(u64, u64)>,
    memory_total: u64,
    process_count: usize,
    network_count: usize,
    disk_count: usize,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if hostname.is_empty() {
        warnings.push("主机信息不可用".into());
    }
    if cpu.is_none() {
        warnings.push("CPU 数据不可用（缺少 /proc/stat 或权限不足）".into());
    }
    if memory_total == 0 {
        warnings.push("内存数据不可用（缺少 /proc/meminfo 或权限不足）".into());
    }
    if process_count == 0 {
        warnings.push("进程列表不可用（缺少 ps/procps 或权限不足）".into());
    }
    if network_count == 0 {
        warnings.push("网络数据不可用（缺少 /proc/net/dev 或权限不足）".into());
    }
    if disk_count == 0 {
        warnings.push("磁盘数据不可用（缺少 df 或权限不足）".into());
    }
    warnings
}

pub async fn snapshot(
    db: Database,
    manager: SessionManager,
    state: MonitorState,
    session_id: String,
) -> AppResult<MonitorSnapshot> {
    let started = Instant::now();
    let output = exec(&db, &manager, &session_id, SNAPSHOT_COMMAND).await?;
    let host = section(&output, "__HOST__", "__CPU__")
        .lines()
        .collect::<Vec<_>>();
    let cpu_body = section(&output, "__CPU__", "__MEM__");
    let mem = section(&output, "__MEM__", "__PROC__");
    let proc_body = section(&output, "__PROC__", "__NET__");
    let net_body = section(&output, "__NET__", "__DISK__");
    let disk_body = section(&output, "__DISK__", "__END__");
    let current_cpu = parse_cpu(cpu_body.lines().next().unwrap_or(""));
    let cpu_percent = current_cpu
        .map(|current| {
            let previous = state.cpu.lock().insert(session_id.clone(), current);
            previous
                .map(|p| {
                    let total = current.0.saturating_sub(p.0);
                    let idle = current.1.saturating_sub(p.1);
                    if total == 0 {
                        0.0
                    } else {
                        100.0 * (total - idle) as f64 / total as f64
                    }
                })
                .unwrap_or(0.0)
        })
        .unwrap_or(0.0);
    let (memory_used_bytes, memory_total_bytes, swap_used_bytes, swap_total_bytes) =
        parse_memory(mem);
    let load_line = host.get(3).copied().unwrap_or("");
    let load_values = load_line
        .split_whitespace()
        .take(3)
        .map(|v| v.parse().unwrap_or(0.0))
        .collect::<Vec<f64>>();
    let uptime_seconds = host
        .get(2)
        .and_then(|v| v.split_whitespace().next())
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0) as u64;
    let processes: Vec<ProcessInfo> = proc_body
        .lines()
        .filter_map(|line| {
            let c = line.split_whitespace().collect::<Vec<_>>();
            if c.len() < 10 {
                return None;
            }
            Some(ProcessInfo {
                pid: c[0].parse().ok()?,
                started_at: c[1..6].join(" "),
                user: c[6].into(),
                cpu_percent: c[7].parse().ok()?,
                memory_percent: c[8].parse().ok()?,
                command: c[9..].join(" "),
            })
        })
        .collect();
    let current_net = parse_network(net_body);
    let now = Instant::now();
    let previous = state
        .network
        .lock()
        .insert(session_id.clone(), (now, current_net.clone()));
    let elapsed = previous
        .as_ref()
        .map(|(when, _)| now.duration_since(*when).as_secs_f64())
        .unwrap_or(1.0)
        .max(0.001);
    let networks: Vec<NetworkInfo> = current_net
        .into_iter()
        .map(|(name, (rx, tx))| {
            let (old_rx, old_tx) = previous
                .as_ref()
                .and_then(|(_, map)| map.get(&name))
                .copied()
                .unwrap_or((rx, tx));
            NetworkInfo {
                interface_name: name,
                rx_bytes_per_second: ((rx.saturating_sub(old_rx)) as f64 / elapsed) as u64,
                tx_bytes_per_second: ((tx.saturating_sub(old_tx)) as f64 / elapsed) as u64,
                rx_total_bytes: rx,
                tx_total_bytes: tx,
            }
        })
        .collect();
    let disks = parse_disks(disk_body);
    let hostname = host.first().unwrap_or(&"").to_string();
    let warnings = capability_warnings(
        &hostname,
        current_cpu,
        memory_total_bytes,
        processes.len(),
        networks.len(),
        disks.len(),
    );
    Ok(MonitorSnapshot {
        session_id,
        timestamp: chrono::Utc::now().timestamp_millis(),
        hostname,
        ip: host.get(1).unwrap_or(&"").to_string(),
        uptime_seconds,
        load: [
            *load_values.first().unwrap_or(&0.0),
            *load_values.get(1).unwrap_or(&0.0),
            *load_values.get(2).unwrap_or(&0.0),
        ],
        cpu_percent,
        memory_used_bytes,
        memory_total_bytes,
        swap_used_bytes,
        swap_total_bytes,
        latency_ms: Some(started.elapsed().as_secs_f64() * 1000.0),
        processes,
        disks,
        networks,
        warnings,
    })
}

pub async fn signal_process(
    db: &Database,
    manager: &SessionManager,
    session_id: &str,
    pid: u32,
    started_at: &str,
    expected_command: &str,
    signal: &str,
) -> AppResult<()> {
    if pid < 2 {
        return Err(AppError::Validation("禁止向 PID 0 或 1 发送信号".into()));
    }
    if !matches!(signal, "TERM" | "HUP" | "KILL") {
        return Err(AppError::Validation(
            "进程信号仅支持 TERM、HUP 或 KILL".into(),
        ));
    }
    if started_at.len() > 64 || expected_command.is_empty() || expected_command.len() > 64 * 1024 {
        return Err(AppError::Validation("进程身份参数无效".into()));
    }
    let command = format!(
        "current=$(LC_ALL=C ps -p {pid} -o lstart= -o args= 2>/dev/null); [ -n \"$current\" ] || exit 74; start=$(printf '%s\\n' \"$current\" | awk '{{print $1\" \"$2\" \"$3\" \"$4\" \"$5}}'); args=$(printf '%s\\n' \"$current\" | awk '{{for(i=6;i<=NF;i++) printf \"%s%s\",$i,(i<NF?\" \":\"\")}}'); [ \"$start\" = {} ] && [ \"$args\" = {} ] || exit 75; kill -{signal} -- {pid}",
        shell_quote(started_at),
        shell_quote(expected_command)
    );
    let profile = manager.profile(session_id)?;
    let result = crate::ssh::execute_profile_command(
        db,
        &profile,
        &command,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
        std::time::Duration::from_secs(30),
    )
    .await?;
    match result.exit_code {
        0 => Ok(()),
        74 => Err(AppError::Remote("进程已退出，未发送信号".into())),
        75 => Err(AppError::Remote(
            "PID 对应的进程身份已变化，已拒绝操作".into(),
        )),
        _ => Err(AppError::Remote(format!(
            "发送信号失败：{}",
            if result.stderr.is_empty() {
                result.stdout
            } else {
                result.stderr
            }
        ))),
    }
}

pub async fn network_sockets(
    db: &Database,
    manager: &SessionManager,
    session_id: &str,
) -> AppResult<NetworkSocketReport> {
    let profile = manager.profile(session_id)?;
    let command = "if command -v ss >/dev/null 2>&1; then ss -H -tunlp 2>&1; elif command -v netstat >/dev/null 2>&1; then netstat -tunlp 2>&1 | tail -n +3; else printf __CNSHELL_MISSING__; fi";
    let result = crate::ssh::execute_profile_command(
        db,
        &profile,
        command,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
        std::time::Duration::from_secs(30),
    )
    .await?;
    if result.stdout.contains("__CNSHELL_MISSING__") {
        return Ok(NetworkSocketReport {
            items: vec![],
            warning: Some("远端缺少 ss 和 netstat，端口与连接列表不可用".into()),
        });
    }
    let items = result
        .stdout
        .lines()
        .filter_map(parse_socket_line)
        .take(5000)
        .collect();
    Ok(NetworkSocketReport {
        items,
        warning: (!result.stderr.trim().is_empty()).then(|| result.stderr.trim().to_string()),
    })
}

pub async fn network_diagnostic(
    profile: ConnectionProfile,
    db: Database,
    kind: String,
    target: String,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
) -> AppResult<NetworkDiagnosticResult> {
    validate_target(&target)?;
    let command = match kind.as_str() {
        "ping" => format!(
            "if command -v ping >/dev/null 2>&1; then ping -c 5 -W 3 -- {}; else printf __CNSHELL_MISSING__; exit 127; fi",
            shell_quote(&target)
        ),
        "traceroute" => format!(
            "if command -v traceroute >/dev/null 2>&1; then traceroute -m 20 -w 2 -- {}; else printf __CNSHELL_MISSING__; exit 127; fi",
            shell_quote(&target)
        ),
        _ => return Err(AppError::Validation("网络诊断类型无效".into())),
    };
    let started = Instant::now();
    let result = crate::ssh::execute_profile_command(
        &db,
        &profile,
        &command,
        cancelled,
        std::time::Duration::from_secs(45),
    )
    .await?;
    let output = format!("{}{}", result.stdout, result.stderr);
    if result.exit_code != 0 {
        return Err(if output.contains("__CNSHELL_MISSING__") {
            AppError::Unavailable(format!("远端缺少 {kind} 工具"))
        } else {
            AppError::Remote(format!("{kind} 执行失败：{output}"))
        });
    }
    Ok(NetworkDiagnosticResult {
        kind,
        target,
        output,
        duration_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
    })
}

fn parse_socket_line(line: &str) -> Option<NetworkSocket> {
    let columns = line.split_whitespace().collect::<Vec<_>>();
    if columns.len() < 5 {
        return None;
    }
    let modern = matches!(
        columns[0].to_ascii_lowercase().as_str(),
        "tcp" | "udp" | "tcp6" | "udp6"
    ) && columns
        .get(1)
        .is_some_and(|value| !["0", "1"].contains(value));
    if modern {
        Some(NetworkSocket {
            protocol: columns[0].into(),
            state: columns[1].into(),
            local_address: columns.get(4)?.to_string(),
            peer_address: columns.get(5).unwrap_or(&"").to_string(),
            process: columns.get(6..).unwrap_or(&[]).join(" "),
        })
    } else {
        Some(NetworkSocket {
            protocol: columns[0].into(),
            state: columns.get(5).unwrap_or(&"").to_string(),
            local_address: columns.get(3)?.to_string(),
            peer_address: columns.get(4).unwrap_or(&"").to_string(),
            process: columns.get(6..).unwrap_or(&[]).join(" "),
        })
    }
}
fn validate_target(target: &str) -> AppResult<()> {
    if target.is_empty()
        || target.len() > 253
        || target.starts_with('-')
        || !target.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | ':' | '_')
        })
    {
        return Err(AppError::Validation(
            "诊断目标必须是有效主机名或 IP 地址".into(),
        ));
    }
    Ok(())
}
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub async fn system_info(
    db: Database,
    manager: SessionManager,
    session_id: String,
) -> AppResult<SystemInfo> {
    system_info_cancelable(
        db,
        manager,
        session_id,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
    .await
}

pub async fn system_info_cancelable(
    db: Database,
    manager: SessionManager,
    session_id: String,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
) -> AppResult<SystemInfo> {
    let output = exec_cancelable(&db, &manager, &session_id, SYSTEM_COMMAND, cancelled).await?;
    let basic = section(&output, "__BASIC__", "__CPU__")
        .lines()
        .collect::<Vec<_>>();
    let cpu = section(&output, "__CPU__", "__MEM__")
        .lines()
        .collect::<Vec<_>>();
    let memory = section(&output, "__MEM__", "__ADDR__")
        .trim()
        .parse::<u64>()
        .unwrap_or(0);
    let addresses = section(&output, "__ADDR__", "__DISK__");
    let mut interface_map: HashMap<String, Vec<String>> = HashMap::new();
    for line in addresses.lines() {
        let c = line.split_whitespace().collect::<Vec<_>>();
        if c.len() >= 4 {
            interface_map
                .entry(c[1].into())
                .or_default()
                .push(c[3].into());
        }
    }
    Ok(SystemInfo {
        hostname: basic.first().unwrap_or(&"").to_string(),
        os: basic.get(1).unwrap_or(&"Unknown Linux").to_string(),
        kernel: basic.get(2).unwrap_or(&"").to_string(),
        architecture: basic.get(3).unwrap_or(&"").to_string(),
        cpu_model: cpu.first().unwrap_or(&"").trim().to_string(),
        cpu_cores: cpu.get(1).and_then(|v| v.parse().ok()).unwrap_or(0),
        memory_total_bytes: bytes_from_kb(memory),
        interfaces: interface_map
            .into_iter()
            .map(|(name, addresses)| NetworkInterface { name, addresses })
            .collect(),
        disks: parse_disks(section(&output, "__DISK__", "__END__")),
    })
}

pub fn export_system_info(path: &Path, info: &SystemInfo) -> AppResult<()> {
    let temp = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or("json")
    ));
    std::fs::write(
        &temp,
        serde_json::to_vec_pretty(info).map_err(|error| AppError::Internal(error.to_string()))?,
    )?;
    if let Err(error) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(AppError::from(error));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_cpu_and_memory() {
        assert_eq!(parse_cpu("cpu 10 2 3 85 0 0"), Some((100, 85)));
        let (m, t, s, st) = parse_memory(
            "MemTotal: 1000 kB\nMemAvailable: 250 kB\nSwapTotal: 500 kB\nSwapFree: 400 kB",
        );
        assert_eq!((m, t, s, st), (768000, 1024000, 102400, 512000));
    }
    #[test]
    fn parses_df() {
        let disks = parse_disks(
            "Filesystem 1024-blocks Used Available Capacity Mounted on\n/dev/vda 1000 400 600 40% /",
        );
        assert_eq!(disks[0].used_percent, 40.0);
    }
    #[test]
    fn section_ignores_monitor_markers_inside_process_commands() {
        let output = "__PROC__\n100 sh -c echo __NET__; echo __DISK__; echo __END__\n__NET__\nInter-| Receive\n eth0: 1 0 0 0 0 0 0 0 2\n__DISK__\nFilesystem 1024-blocks Used Available Capacity Mounted on\n/dev/vda 1000 400 600 40% /\n__END__\n";
        assert!(section(output, "__NET__", "__DISK__").contains("eth0:"));
        assert!(section(output, "__DISK__", "__END__").contains("/dev/vda"));
    }
    #[test]
    fn parses_ss_and_netstat_without_mixing_columns() {
        let ss = parse_socket_line(
            "tcp LISTEN 0 4096 0.0.0.0:22 0.0.0.0:* users:((\"sshd\",pid=12,fd=3))",
        )
        .unwrap();
        assert_eq!(ss.state, "LISTEN");
        assert_eq!(ss.local_address, "0.0.0.0:22");
        assert!(ss.process.contains("sshd"));
        let netstat = parse_socket_line("tcp 0 0 127.0.0.1:25 0.0.0.0:* LISTEN 99/master").unwrap();
        assert_eq!(netstat.state, "LISTEN");
        assert_eq!(netstat.process, "99/master");
    }
    #[test]
    fn diagnostic_targets_reject_shell_syntax() {
        assert!(validate_target("example.com").is_ok());
        assert!(validate_target("2001:db8::1").is_ok());
        for value in ["-Ievil", "example.com;id", "$(id)", "host name"] {
            assert!(
                validate_target(value).is_err(),
                "accepted unsafe target {value}"
            );
        }
    }
    #[test]
    fn reports_each_missing_monitor_capability() {
        let warnings = capability_warnings("", None, 0, 0, 0, 0);
        assert_eq!(warnings.len(), 6);
        assert!(warnings.iter().any(|item| item.contains("procps")));
        assert!(warnings.iter().any(|item| item.contains("df")));
    }
    #[test]
    fn monitor_state_is_removed_with_closed_session() {
        let state = MonitorState::default();
        state.cpu.lock().insert("session".into(), (1, 1));
        state
            .network
            .lock()
            .insert("session".into(), (Instant::now(), Default::default()));
        assert!(state.contains("session"));
        state.remove("session");
        assert!(!state.contains("session"));
    }
    #[test]
    fn exports_system_info_atomically() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("system.json");
        let info = SystemInfo {
            hostname: "host".into(),
            os: "Linux".into(),
            kernel: "6".into(),
            architecture: "arm64".into(),
            cpu_model: "cpu".into(),
            cpu_cores: 2,
            memory_total_bytes: 1024,
            interfaces: vec![],
            disks: vec![],
        };
        export_system_info(&path, &info).unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["hostname"], "host");
        assert!(!directory.path().join("system.json.tmp").exists());
    }
}
