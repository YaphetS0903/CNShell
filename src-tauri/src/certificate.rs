use crate::{
    error::{AppError, AppResult},
    models::SshCertificateInfo,
};
use chrono::{DateTime, NaiveDateTime, Utc};
use std::{path::Path, process::Command};

const MAX_CERTIFICATE_BYTES: u64 = 1024 * 1024;
const MAX_INSPECT_OUTPUT: usize = 64 * 1024;

pub fn inspect(path: &Path) -> AppResult<SshCertificateInfo> {
    if !path.is_absolute() || !path.is_file() {
        return Err(AppError::Validation(
            "SSH Certificate 必须是存在的本地绝对文件路径".into(),
        ));
    }
    if path.metadata()?.len() > MAX_CERTIFICATE_BYTES {
        return Err(AppError::Validation(
            "SSH Certificate 文件不能超过 1 MB".into(),
        ));
    }
    let output = Command::new("/usr/bin/ssh-keygen")
        .args(["-L", "-f"])
        .arg(path)
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("LC_ALL", "C")
        .env("TZ", "UTC")
        .output()
        .map_err(|error| AppError::Unavailable(format!("无法运行 ssh-keygen：{error}")))?;
    if output.stdout.len().saturating_add(output.stderr.len()) > MAX_INSPECT_OUTPUT {
        return Err(AppError::Validation("SSH Certificate 检查输出过大".into()));
    }
    if !output.status.success() {
        return Err(AppError::Validation(
            "所选文件不是有效的 OpenSSH 用户证书".into(),
        ));
    }
    let text = String::from_utf8(output.stdout)
        .map_err(|_| AppError::Validation("SSH Certificate 检查输出不是 UTF-8".into()))?;
    parse(path, &text, Utc::now())
}

fn parse(path: &Path, text: &str, now: DateTime<Utc>) -> AppResult<SshCertificateInfo> {
    let mut certificate_type = String::new();
    let mut key_id = String::new();
    let mut serial = String::new();
    let mut signing_ca = String::new();
    let mut valid_from = String::new();
    let mut valid_to = String::new();
    let mut principals = Vec::new();
    let mut reading_principals = false;
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(value) = line.strip_prefix("Type:") {
            certificate_type = value.trim().to_owned();
        } else if let Some(value) = line.strip_prefix("Key ID:") {
            key_id = value.trim().trim_matches('"').to_owned();
        } else if let Some(value) = line.strip_prefix("Serial:") {
            serial = value.trim().to_owned();
        } else if let Some(value) = line.strip_prefix("Signing CA:") {
            signing_ca = value.trim().to_owned();
        } else if let Some(value) = line.strip_prefix("Valid:") {
            let value = value.trim();
            if value == "forever" {
                valid_from = "always".into();
                valid_to = "forever".into();
            } else if let Some((from, to)) = value
                .strip_prefix("from ")
                .and_then(|value| value.split_once(" to "))
            {
                valid_from = from.to_owned();
                valid_to = to.to_owned();
            }
        } else if line == "Principals:" {
            reading_principals = true;
        } else if reading_principals {
            if line.ends_with(':')
                || line == "(none)"
                || line.starts_with("Critical Options:")
                || line.starts_with("Extensions:")
            {
                reading_principals = false;
            } else if !line.is_empty() {
                principals.push(line.to_owned());
            }
        }
    }
    if !certificate_type.contains("user certificate")
        || serial.is_empty()
        || signing_ca.is_empty()
        || valid_from.is_empty()
        || valid_to.is_empty()
    {
        return Err(AppError::Validation(
            "OpenSSH 证书缺少用户类型、签发者或有效期信息".into(),
        ));
    }
    let after = parse_time(&valid_from)?;
    let before = parse_time(&valid_to)?;
    let valid_now =
        after.is_none_or(|value| now >= value) && before.is_none_or(|value| now < value);
    let status = if after.is_some_and(|value| now < value) {
        "notYetValid"
    } else if before.is_some_and(|value| now >= value) {
        "expired"
    } else {
        "valid"
    };
    Ok(SshCertificateInfo {
        path: path.to_string_lossy().into_owned(),
        certificate_type,
        key_id,
        serial,
        signing_ca,
        valid_from,
        valid_to,
        principals,
        valid_now,
        status: status.into(),
    })
}

fn parse_time(value: &str) -> AppResult<Option<DateTime<Utc>>> {
    if value == "forever" || value == "always" {
        return Ok(None);
    }
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S")
        .map(|value| Some(value.and_utc()))
        .map_err(|_| AppError::Validation("OpenSSH 证书有效期格式无效".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_user_certificate_and_enforces_validity() {
        let text = r#"fixture-cert.pub:
        Type: ssh-ed25519-cert-v01@openssh.com user certificate
        Public key: ED25519-CERT SHA256:test
        Signing CA: ED25519 SHA256:ca (using ssh-ed25519)
        Key ID: "deploy-2026"
        Serial: 42
        Valid: from 2026-07-01T00:00:00 to 2026-08-01T00:00:00
        Principals:
                ubuntu
                deploy
        Critical Options: (none)
"#;
        let now = DateTime::parse_from_rfc3339("2026-07-15T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let info = parse(Path::new("/tmp/fixture-cert.pub"), text, now).unwrap();
        assert!(info.valid_now);
        assert_eq!(info.status, "valid");
        assert_eq!(info.principals, ["ubuntu", "deploy"]);
        let expired = parse(
            Path::new("/tmp/fixture-cert.pub"),
            text,
            DateTime::parse_from_rfc3339("2026-09-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        )
        .unwrap();
        assert_eq!(expired.status, "expired");
    }

    #[test]
    fn rejects_host_certificates_and_malformed_validity() {
        assert!(
            parse(
                Path::new("/tmp/host-cert.pub"),
                "Type: ssh-ed25519-cert-v01@openssh.com host certificate\n",
                Utc::now()
            )
            .is_err()
        );
        assert!(parse_time("tomorrow").is_err());
    }
}
