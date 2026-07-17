use crate::{
    db::Database,
    error::{AppError, AppResult},
    models::{GeneratedSshKey, OpenSshHost},
    ssh,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

pub fn import_config(path: &Path) -> AppResult<Vec<OpenSshHost>> {
    let root = ssh_root()?;
    let canonical = path.canonicalize()?;
    if !canonical.starts_with(&root) {
        return Err(AppError::Validation(
            "OpenSSH 配置及 Include 必须位于 ~/.ssh 内".into(),
        ));
    }
    let mut visited = HashSet::new();
    let mut blocks = Vec::new();
    parse_file(&canonical, &root, &mut visited, &mut blocks)?;
    Ok(resolve_blocks(blocks))
}

#[derive(Default, Clone)]
struct Block {
    aliases: Vec<String>,
    values: HashMap<String, String>,
    source: String,
    warnings: Vec<String>,
}
fn parse_file(
    path: &Path,
    root: &Path,
    visited: &mut HashSet<PathBuf>,
    blocks: &mut Vec<Block>,
) -> AppResult<()> {
    let path = path.canonicalize()?;
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if !path.starts_with(&canonical_root) {
        return Err(AppError::Validation("Include 路径越出 ~/.ssh".into()));
    }
    if !visited.insert(path.clone()) {
        return Err(AppError::Validation(format!(
            "OpenSSH Include 形成循环：{}",
            path.display()
        )));
    }
    let text = std::fs::read_to_string(&path)?;
    if text.len() > 2 * 1024 * 1024 {
        return Err(AppError::Validation(
            "单个 SSH 配置文件不能超过 2 MB".into(),
        ));
    }
    let mut current: Option<Block> = None;
    for raw in text.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = split_directive(line) else {
            continue;
        };
        let lower = key.to_ascii_lowercase();
        if lower == "include" {
            for pattern in shell_words(value) {
                for include in expand_include(&path, &canonical_root, &pattern)? {
                    parse_file(&include, &canonical_root, visited, blocks)?;
                }
            }
            continue;
        }
        if lower == "host" {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            let aliases = shell_words(value);
            current = Some(Block {
                aliases,
                source: path.to_string_lossy().into_owned(),
                ..Default::default()
            });
            continue;
        }
        if let Some(block) = current.as_mut() {
            if ["hostname", "user", "port", "identityfile", "proxyjump"].contains(&lower.as_str()) {
                block
                    .values
                    .entry(lower)
                    .or_insert_with(|| expand_home(value));
            }
        }
    }
    if let Some(block) = current {
        blocks.push(block);
    }
    visited.remove(&path);
    Ok(())
}
fn resolve_blocks(blocks: Vec<Block>) -> Vec<OpenSshHost> {
    let mut result = Vec::new();
    for block in blocks {
        for alias in &block.aliases {
            if alias.contains(['*', '?', '!']) {
                continue;
            }
            let port = block
                .values
                .get("port")
                .and_then(|value| value.parse::<u16>().ok())
                .unwrap_or(22);
            let mut warnings = block.warnings.clone();
            if block.values.get("port").is_some() && port == 22 && block.values["port"] != "22" {
                warnings.push("端口无效，已使用 22".into());
            }
            result.push(OpenSshHost {
                alias: alias.clone(),
                hostname: block
                    .values
                    .get("hostname")
                    .cloned()
                    .unwrap_or_else(|| alias.clone()),
                user: block.values.get("user").cloned(),
                port,
                identity_file: block.values.get("identityfile").cloned(),
                proxy_jump: block.values.get("proxyjump").cloned(),
                source: block.source.clone(),
                warnings,
            });
        }
    }
    result
}

pub fn generate_key(path: &Path, comment: &str) -> AppResult<GeneratedSshKey> {
    validate_key_path(path)?;
    if comment.len() > 256 || comment.contains(['\n', '\r', '\0']) {
        return Err(AppError::Validation("密钥注释无效".into()));
    }
    if path.exists() || path.with_extension("pub").exists() {
        return Err(AppError::Validation("目标密钥文件已存在".into()));
    }
    let executable = ssh_keygen_path()
        .ok_or_else(|| AppError::Unavailable("未找到 OpenSSH ssh-keygen".into()))?;
    let output = Command::new(&executable)
        .args(["-t", "ed25519", "-a", "64", "-N", "", "-C", comment, "-f"])
        .arg(path)
        .output()
        .map_err(|error| AppError::Unavailable(format!("无法运行 ssh-keygen：{error}")))?;
    if !output.status.success() {
        return Err(AppError::Unavailable(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    let public_path = PathBuf::from(format!("{}.pub", path.display()));
    let public_key = std::fs::read_to_string(&public_path)?.trim().to_string();
    let fingerprint_output = Command::new(&executable)
        .args(["-lf"])
        .arg(&public_path)
        .output()?;
    if !fingerprint_output.status.success() {
        return Err(AppError::Unavailable("无法读取新密钥指纹".into()));
    }
    let fingerprint = String::from_utf8_lossy(&fingerprint_output.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .to_string();
    Ok(GeneratedSshKey {
        private_key_path: path.to_string_lossy().into_owned(),
        public_key_path: public_path.to_string_lossy().into_owned(),
        public_key,
        fingerprint,
    })
}

pub async fn deploy_public_key(
    db: &Database,
    profile: &crate::models::ConnectionProfile,
    public_key: &str,
) -> AppResult<()> {
    if public_key.len() > 16 * 1024
        || !public_key.starts_with("ssh-ed25519 ")
        || public_key.contains(['\n', '\r', '\0'])
    {
        return Err(AppError::Validation("只允许部署单行 Ed25519 公钥".into()));
    }
    let encoded = STANDARD.encode(public_key.as_bytes());
    let command = format!(
        "umask 077; mkdir -p -- \"$HOME/.ssh\" && touch -- \"$HOME/.ssh/authorized_keys\" && chmod 700 -- \"$HOME/.ssh\" && chmod 600 -- \"$HOME/.ssh/authorized_keys\" && key=$(printf %s {encoded} | base64 -d 2>/dev/null || printf %s {encoded} | base64 -D); grep -qxF -- \"$key\" \"$HOME/.ssh/authorized_keys\" || printf '%s\\n' \"$key\" >> \"$HOME/.ssh/authorized_keys\""
    );
    let result = ssh::execute_profile_command(
        db,
        profile,
        &command,
        Arc::new(AtomicBool::new(false)),
        Duration::from_secs(30),
    )
    .await?;
    if result.exit_code == 0 {
        Ok(())
    } else {
        Err(AppError::Remote(if result.stderr.is_empty() {
            result.stdout
        } else {
            result.stderr
        }))
    }
}

fn ssh_root() -> AppResult<PathBuf> {
    let home =
        home_directory().ok_or_else(|| AppError::Unavailable("无法确定用户主目录".into()))?;
    let root = PathBuf::from(home).join(".ssh");
    Ok(root.canonicalize().unwrap_or(root))
}
pub fn ssh_keygen_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    if let Some(windows) = std::env::var_os("WINDIR") {
        let path = PathBuf::from(windows)
            .join("System32")
            .join("OpenSSH")
            .join("ssh-keygen.exe");
        if path.is_file() {
            return Some(path);
        }
    }
    which::which(if cfg!(target_os = "windows") {
        "ssh-keygen.exe"
    } else {
        "ssh-keygen"
    })
    .ok()
}

fn home_directory() -> Option<std::ffi::OsString> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
}
fn validate_key_path(path: &Path) -> AppResult<()> {
    if !path.is_absolute() || path.file_name().is_none() {
        return Err(AppError::Validation("密钥路径必须是绝对文件路径".into()));
    }
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Validation("密钥目录无效".into()))?;
    if !parent.is_dir() {
        return Err(AppError::Validation("密钥目录不存在".into()));
    }
    Ok(())
}
fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (index, character) in line.char_indices() {
        if character == '"' {
            quoted = !quoted;
        }
        if character == '#' && !quoted {
            return &line[..index];
        }
    }
    line
}
fn split_directive(line: &str) -> Option<(&str, &str)> {
    if let Some((key, value)) = line.split_once('=') {
        return Some((key.trim(), value.trim()));
    }
    let index = line.find(char::is_whitespace)?;
    Some((&line[..index], line[index..].trim()))
}
fn shell_words(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .map(|item| item.trim_matches(['"', '\'']).to_string())
        .filter(|item| !item.is_empty())
        .collect()
}
fn expand_home(value: &str) -> String {
    let value = value.trim().trim_matches(['"', '\'']);
    if let Some(rest) = value.strip_prefix("~/") {
        if let Some(home) = home_directory() {
            return PathBuf::from(home)
                .join(rest)
                .to_string_lossy()
                .into_owned();
        }
    }
    value.into()
}
fn expand_include(config: &Path, root: &Path, pattern: &str) -> AppResult<Vec<PathBuf>> {
    let pattern = expand_home(pattern);
    let path = PathBuf::from(&pattern);
    let path = if path.is_absolute() {
        path
    } else {
        config.parent().unwrap_or(root).join(path)
    };
    if path.to_string_lossy().contains(['*', '?']) {
        let parent = path.parent().unwrap_or(root);
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let mut matches = Vec::new();
        for entry in std::fs::read_dir(parent)? {
            let entry = entry?;
            let file = entry.file_name();
            let file = file.to_string_lossy();
            if wildcard_match(name, &file) {
                matches.push(entry.path());
            }
        }
        matches.sort();
        Ok(matches)
    } else {
        Ok(path.exists().then_some(path).into_iter().collect())
    }
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    let pattern = pattern.as_bytes();
    let value = value.as_bytes();
    let (mut pattern_index, mut value_index, mut star, mut retry) = (0, 0, None, 0);
    while value_index < value.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?' || pattern[pattern_index] == value[value_index])
        {
            pattern_index += 1;
            value_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star = Some(pattern_index);
            pattern_index += 1;
            retry = value_index;
        } else if let Some(star_index) = star {
            pattern_index = star_index + 1;
            retry += 1;
            value_index = retry;
        } else {
            return false;
        }
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imports_hosts_and_include_without_wildcard_profiles() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join(".ssh");
        std::fs::create_dir_all(root.join("conf.d")).unwrap();
        std::fs::write(root.join("config"),"Include conf.d/*\nHost prod\n HostName 10.0.0.1\n User ubuntu\n Port 2222\n IdentityFile ~/.ssh/id_ed25519\n ProxyJump jump\nHost *.example\n User ignored\n").unwrap();
        std::fs::write(
            root.join("conf.d/jump"),
            "Host jump\n HostName jump.example.com\n",
        )
        .unwrap();
        let mut visited = HashSet::new();
        let mut blocks = Vec::new();
        parse_file(&root.join("config"), &root, &mut visited, &mut blocks).unwrap();
        let hosts = resolve_blocks(blocks);
        assert_eq!(hosts.len(), 2);
        let prod = hosts.iter().find(|host| host.alias == "prod").unwrap();
        assert_eq!(prod.port, 2222);
        assert_eq!(prod.proxy_jump.as_deref(), Some("jump"));
    }
    #[test]
    fn include_cycles_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::write(root.join("a"), "Include b").unwrap();
        std::fs::write(root.join("b"), "Include a").unwrap();
        let error =
            parse_file(&root.join("a"), root, &mut HashSet::new(), &mut vec![]).unwrap_err();
        assert!(error.to_string().contains("循环"));
    }
    #[test]
    fn include_question_mark_and_star_wildcards_match_files() {
        assert!(wildcard_match("host?.conf", "host1.conf"));
        assert!(!wildcard_match("host?.conf", "host12.conf"));
        assert!(wildcard_match("*.conf", "prod.conf"));
    }
}
