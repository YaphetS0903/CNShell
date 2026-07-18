use crate::{
    db::Database,
    error::{AppError, AppResult},
    rdp,
};
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackEnvironment {
    app_version: String,
    operating_system: String,
    os_version: String,
    architecture: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticReport {
    product: String,
    version: String,
    generated_at: String,
    platform: String,
    architecture: String,
    connection_count: usize,
    protocol_counts: std::collections::BTreeMap<String, usize>,
    transfer_status_counts: std::collections::BTreeMap<String, usize>,
    rdp_available: bool,
    security: SecuritySummary,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SecuritySummary {
    credentials_in_system_store: bool,
    strict_host_key_default: bool,
    telemetry_enabled: bool,
    contains_hosts_or_commands: bool,
}

pub async fn export(db: &Database, path: &str) -> AppResult<()> {
    let connections = db.list_connections().await?;
    let transfers = db.transfers().await?;
    let mut protocols = std::collections::BTreeMap::new();
    let mut statuses = std::collections::BTreeMap::new();
    for connection in &connections {
        *protocols.entry(connection.protocol.clone()).or_insert(0) += 1;
    }
    for transfer in transfers {
        *statuses.entry(transfer.status).or_insert(0) += 1;
    }
    let report = DiagnosticReport {
        product: "CNshell".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        platform: std::env::consts::OS.into(),
        architecture: std::env::consts::ARCH.into(),
        connection_count: connections.len(),
        protocol_counts: protocols,
        transfer_status_counts: statuses,
        rdp_available: rdp::preflight().available,
        security: SecuritySummary {
            credentials_in_system_store: true,
            strict_host_key_default: true,
            telemetry_enabled: false,
            contains_hosts_or_commands: false,
        },
    };
    write_report(Path::new(path), &report)
}

pub fn feedback_environment() -> FeedbackEnvironment {
    FeedbackEnvironment {
        app_version: env!("CARGO_PKG_VERSION").into(),
        operating_system: std::env::consts::OS.into(),
        os_version: crate::platform::system_version(),
        architecture: std::env::consts::ARCH.into(),
    }
}

pub fn reveal(path: &str) -> AppResult<()> {
    let target = Path::new(path);
    if !target.is_file() || target.extension().and_then(|value| value.to_str()) != Some("json") {
        return Err(AppError::Validation(
            "诊断文件不存在或不是 JSON 文件".into(),
        ));
    }
    crate::platform::reveal_local_file(target)
}

fn write_report(target: &Path, report: &DiagnosticReport) -> AppResult<()> {
    let temp = target.with_extension(format!(
        "{}.tmp",
        target
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("json")
    ));
    let bytes =
        serde_json::to_vec_pretty(report).map_err(|error| AppError::Internal(error.to_string()))?;
    std::fs::write(&temp, bytes)?;
    if let Err(error) = std::fs::rename(&temp, target) {
        let _ = std::fs::remove_file(&temp);
        return Err(AppError::from(error));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn report() -> DiagnosticReport {
        DiagnosticReport {
            product: "CNshell".into(),
            version: "test".into(),
            generated_at: "now".into(),
            platform: "macos".into(),
            architecture: "arm64".into(),
            connection_count: 0,
            protocol_counts: Default::default(),
            transfer_status_counts: Default::default(),
            rdp_available: false,
            security: SecuritySummary {
                credentials_in_system_store: true,
                strict_host_key_default: true,
                telemetry_enabled: false,
                contains_hosts_or_commands: false,
            },
        }
    }
    #[test]
    fn report_schema_never_serializes_sensitive_fields() {
        let json = serde_json::to_value(report()).unwrap();
        let object = json.as_object().unwrap();
        assert!(!object.contains_key("host"));
        assert!(!object.contains_key("username"));
        assert!(!object.contains_key("command"));
        assert!(!object.contains_key("path"));
    }
    #[test]
    fn diagnostic_export_is_atomic_and_cleans_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("diagnostics.json");
        write_report(&path, &report()).unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["product"], "CNshell");
        assert!(!directory.path().join("diagnostics.json.tmp").exists());
    }
    #[test]
    fn feedback_environment_contains_only_public_runtime_metadata() {
        let json = serde_json::to_value(feedback_environment()).unwrap();
        let object = json.as_object().unwrap();
        assert_eq!(object.len(), 4);
        assert!(object.contains_key("appVersion"));
        assert!(object.contains_key("operatingSystem"));
        assert!(object.contains_key("osVersion"));
        assert!(object.contains_key("architecture"));
    }
}
