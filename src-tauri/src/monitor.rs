use crate::{
    db::Database,
    error::{AppError, AppResult},
    models::*,
    ssh::SessionManager,
};
use parking_lot::Mutex;
use std::{collections::HashMap, io::Read, path::Path, sync::Arc, time::Instant};

#[derive(Clone, Default)]
pub struct MonitorState {
    cpu: Arc<Mutex<HashMap<String, (u64, u64)>>>,
    network: Arc<Mutex<HashMap<String, (Instant, HashMap<String, (u64, u64)>)>>>,
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
echo __PROC__; ps -eo pid=,user=,pcpu=,pmem=,comm= --sort=-pcpu 2>/dev/null | head -n 500; \
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
    let profile = manager.profile(session_id)?;
    let mut transport = manager.acquire_transport(db, &profile, true).await?;
    tokio::task::spawn_blocking(move || {
        let result = (|| {
            let mut channel = transport.connected().session.channel_session()?;
            channel.exec(command)?;
            let mut output = String::new();
            channel.read_to_string(&mut output)?;
            channel.wait_close()?;
            Ok(output)
        })();
        if result.is_err() {
            transport.discard();
        }
        result
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()))?
}

fn section<'a>(text: &'a str, name: &str, next: &str) -> &'a str {
    text.split_once(name)
        .and_then(|(_, rest)| rest.split_once(next).map(|(body, _)| body))
        .unwrap_or("")
        .trim()
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
        if let Some((key, value)) = line.split_once(':') {
            if let Some(number) = value
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<u64>().ok())
            {
                map.insert(key, bytes_from_kb(number));
            }
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
            if c.len() < 5 {
                return None;
            }
            Some(ProcessInfo {
                pid: c[0].parse().ok()?,
                user: c[1].into(),
                cpu_percent: c[2].parse().ok()?,
                memory_percent: c[3].parse().ok()?,
                command: c[4..].join(" "),
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

pub async fn system_info(
    db: Database,
    manager: SessionManager,
    session_id: String,
) -> AppResult<SystemInfo> {
    let output = exec(&db, &manager, &session_id, SYSTEM_COMMAND).await?;
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
