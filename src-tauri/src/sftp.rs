use crate::{
    db::Database,
    error::{AppError, AppResult},
    models::{RemoteFile, TransferInput, TransferTask},
    ssh::SessionManager,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use sha2::{Digest, Sha256};
use ssh2::{ErrorCode, FileStat, OpenFlags, OpenType, RenameFlags, Sftp};
#[cfg(unix)]
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::{
    io::{Read, Write},
    net::Shutdown,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

const DIRECTORY_READ_TIMEOUT: Duration = Duration::from_secs(20);

fn atomic_replace(sftp: &Sftp, source: &Path, destination: &Path) -> AppResult<()> {
    match sftp.posix_rename(source, destination) {
        Ok(()) => Ok(()),
        Err(error) if error.code() == ErrorCode::SFTP(8) => match sftp.lstat(destination) {
            Err(not_found) if not_found.code() == ErrorCode::SFTP(2) => {
                sftp.rename(source, destination, Some(RenameFlags::empty()))?;
                Ok(())
            }
            Ok(_) => Err(AppError::Unavailable(
                "远端 SFTP 服务不支持 posix-rename@openssh.com，无法安全地原子覆盖已有文件".into(),
            )),
            Err(stat_error) => Err(stat_error.into()),
        },
        Err(error) => Err(error.into()),
    }
}

const RAW_PATH_PREFIX: &str = "cnshell-raw-path:";
pub const MCP_MAX_DIRECTORY_ENTRIES: usize = 100_000;
const MCP_MAX_DIRECTORY_BYTES: u64 = 20 * 1024 * 1024 * 1024;
pub(crate) fn remote_path(path: &str) -> AppResult<PathBuf> {
    if let Some(encoded) = path.strip_prefix(RAW_PATH_PREFIX) {
        let bytes = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| AppError::Validation("远端原始路径令牌无效".into()))?;
        if bytes.is_empty()
            || bytes.len() > 16 * 1024
            || bytes.contains(&0)
            || bytes.first() != Some(&b'/')
        {
            return Err(AppError::Validation("远端原始路径令牌无效".into()));
        }
        #[cfg(unix)]
        return Ok(PathBuf::from(std::ffi::OsString::from_vec(bytes)));
        #[cfg(not(unix))]
        return Err(AppError::Unavailable("原始字节路径仅支持 Unix 平台".into()));
    }
    if path.is_empty() || path.len() > 16 * 1024 || path.contains('\0') || !path.starts_with('/') {
        Err(AppError::Validation(
            "远端路径必须是 16 KB 以内的绝对路径".into(),
        ))
    } else {
        Ok(PathBuf::from(path))
    }
}
fn validate_remote_path(path: &str) -> AppResult<()> {
    remote_path(path).map(|_| ())
}

fn remote_parent_path(path: &str) -> AppResult<PathBuf> {
    if path.starts_with(RAW_PATH_PREFIX) {
        let decoded = remote_path(path)?;
        return Ok(decoded.parent().unwrap_or(Path::new("/")).to_path_buf());
    }
    validate_remote_path(path)?;
    let trimmed = if path.len() > 1 {
        path.trim_end_matches('/')
    } else {
        path
    };
    let separator = trimmed.rfind('/').unwrap_or(0);
    Ok(PathBuf::from(if separator == 0 {
        "/"
    } else {
        &trimmed[..separator]
    }))
}

fn remote_child_path(parent: &str, name: &str) -> AppResult<PathBuf> {
    if parent.starts_with(RAW_PATH_PREFIX) {
        let mut decoded = remote_path(parent)?;
        decoded.push(name);
        return Ok(decoded);
    }
    validate_remote_path(parent)?;
    Ok(PathBuf::from(if parent == "/" {
        format!("/{name}")
    } else {
        format!("{}/{name}", parent.trim_end_matches('/'))
    }))
}

fn remote_utf8_name(path: &str) -> Option<&str> {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
}

pub fn join_path(parent: &str, name: &str) -> AppResult<String> {
    if name.is_empty()
        || name.len() > 4096
        || name.contains(['\0', '/'])
        || name == "."
        || name == ".."
    {
        return Err(AppError::Validation("远端文件名无效".into()));
    }
    Ok(wire_path(&remote_child_path(parent, name)?))
}
#[cfg(unix)]
pub(crate) fn wire_path(path: &Path) -> String {
    path.to_str().map(ToOwned::to_owned).unwrap_or_else(|| {
        format!(
            "{RAW_PATH_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(path.as_os_str().as_bytes())
        )
    })
}
#[cfg(not(unix))]
fn wire_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
#[cfg(unix)]
pub(crate) fn display_name(path: &Path) -> String {
    let bytes = path
        .file_name()
        .map(|name| name.as_bytes())
        .unwrap_or_default();
    match std::str::from_utf8(bytes) {
        Ok(name) => name.into(),
        Err(_) => bytes
            .iter()
            .map(|byte| {
                if byte.is_ascii_graphic() && *byte != b'\\' {
                    (*byte as char).to_string()
                } else {
                    format!("\\x{byte:02X}")
                }
            })
            .collect(),
    }
}
#[cfg(not(unix))]
fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}
fn validate_local_path(path: &str) -> AppResult<()> {
    if path.is_empty()
        || path.len() > 32 * 1024
        || path.contains('\0')
        || !Path::new(path).is_absolute()
    {
        Err(AppError::Validation(
            "本地路径必须是 32 KB 以内的绝对路径".into(),
        ))
    } else {
        Ok(())
    }
}

fn validate_upload_source(path: &str) -> AppResult<()> {
    validate_local_path(path)?;
    if std::fs::metadata(path)?.is_file() {
        Ok(())
    } else {
        Err(AppError::Validation(
            "拖入文件夹请使用“上传文件夹”按钮".into(),
        ))
    }
}

fn validate_deletable_path(path: &str) -> AppResult<()> {
    let decoded = remote_path(path)?;
    if [Path::new("/"), Path::new("/."), Path::new("/..")].contains(&decoded.as_path()) {
        Err(AppError::Validation("禁止删除远端根目录".into()))
    } else {
        Ok(())
    }
}

pub fn permission_string(mode: Option<u32>, kind: &str) -> String {
    let mode = mode.unwrap_or(0);
    let mut result = String::with_capacity(10);
    result.push(match kind {
        "directory" => 'd',
        "symlink" => 'l',
        _ => '-',
    });
    for shift in [6, 3, 0] {
        result.push(if mode & (0o4 << shift) != 0 { 'r' } else { '-' });
        result.push(if mode & (0o2 << shift) != 0 { 'w' } else { '-' });
        result.push(if mode & (0o1 << shift) != 0 { 'x' } else { '-' });
    }
    result
}

fn remote_kind(stat: &FileStat) -> &'static str {
    match stat.file_type() {
        ssh2::FileType::Directory => "directory",
        ssh2::FileType::Symlink => "symlink",
        ssh2::FileType::RegularFile => "file",
        _ => "other",
    }
}

async fn with_sftp<T, F>(
    db: Database,
    manager: SessionManager,
    session_id: String,
    operation: F,
) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(Sftp) -> AppResult<T> + Send + 'static,
{
    with_sftp_timeout(db, manager, session_id, None, operation).await
}

async fn with_sftp_timeout<T, F>(
    db: Database,
    manager: SessionManager,
    session_id: String,
    timeout: Option<(&'static str, Duration)>,
    operation: F,
) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(Sftp) -> AppResult<T> + Send + 'static,
{
    let profile = manager.profile(&session_id)?;
    let mut transport = manager
        .acquire_auxiliary_transport(&db, &profile, "sftp")
        .await?;
    let interrupt = transport.try_clone_transport().ok();
    let session_timeout = timeout.map(|(_, duration)| {
        u32::try_from(duration.as_millis())
            .unwrap_or(u32::MAX)
            .max(1)
    });
    let timed_out = Arc::new(AtomicBool::new(false));
    let timed_out_in_task = Arc::clone(&timed_out);
    let mut task = tokio::task::spawn_blocking(move || {
        if let Some(duration) = session_timeout {
            transport.connected().session.set_timeout(duration);
        }
        let result = transport
            .connected()
            .session
            .sftp()
            .map_err(AppError::from)
            .and_then(operation);
        if session_timeout.is_some() {
            transport.connected().session.set_timeout(0);
        }
        if result.is_err() || timed_out_in_task.load(Ordering::Acquire) {
            transport.discard();
        }
        result
    });
    let joined = if let Some((operation_name, duration)) = timeout {
        match tokio::time::timeout(duration, &mut task).await {
            Ok(joined) => joined,
            Err(_) => {
                timed_out.store(true, Ordering::Release);
                if let Some(stream) = interrupt {
                    let _ = stream.shutdown(Shutdown::Both);
                }
                manager.invalidate_transport(&profile.id);
                let _ = tokio::time::timeout(Duration::from_secs(1), &mut task).await;
                return Err(AppError::Unavailable(format!(
                    "{operation_name}超时，已重置 SFTP 文件连接，请重试"
                )));
            }
        }
    } else {
        task.await
    };
    joined.map_err(|error| AppError::Internal(error.to_string()))?
}

pub async fn list(
    db: Database,
    manager: SessionManager,
    session_id: String,
    path: String,
    show_hidden: bool,
) -> AppResult<Vec<RemoteFile>> {
    validate_remote_path(&path)?;
    with_sftp_timeout(
        db,
        manager,
        session_id,
        Some(("目录读取", DIRECTORY_READ_TIMEOUT)),
        move |sftp| {
            let directory = remote_path(&path)?;
            let mut entries = sftp
                .readdir(&directory)?
                .into_iter()
                .filter_map(|(entry_path, stat)| {
                    let name = display_name(&entry_path);
                    if !show_hidden && name.starts_with('.') {
                        return None;
                    }
                    let kind = remote_kind(&stat).to_string();
                    Some(RemoteFile {
                        name,
                        path: wire_path(&entry_path),
                        kind: kind.clone(),
                        size: stat.size.unwrap_or(0),
                        modified_at: stat.mtime,
                        permissions: permission_string(stat.perm, &kind),
                        owner: stat.uid,
                        group: stat.gid,
                    })
                })
                .collect::<Vec<_>>();
            entries.sort_by(|a, b| match (a.kind.as_str(), b.kind.as_str()) {
                ("directory", "file") => std::cmp::Ordering::Less,
                ("file", "directory") => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            });
            Ok(entries)
        },
    )
    .await
}

/// List a directory for MCP with a hard entry-count boundary.
///
/// The regular file browser intentionally keeps its historical unbounded
/// listing behavior. MCP calls this variant so a hostile or accidentally
/// huge remote directory cannot create an unbounded allocation before the
/// broker applies pagination.
pub async fn list_bounded(
    db: Database,
    manager: SessionManager,
    session_id: String,
    path: String,
    root: String,
    show_hidden: bool,
    cancelled: Arc<AtomicBool>,
) -> AppResult<Vec<RemoteFile>> {
    validate_remote_path(&path)?;
    validate_remote_path(&root)?;
    with_sftp(db, manager, session_id, move |sftp| {
        check_directory_transfer_cancelled(&cancelled)?;
        validate_mcp_boundary_on_sftp(&sftp, &path, &root, false)?;
        let directory = remote_path(&path)?;
        let (raw_entries, more) = sftp.readdir_limited(
            &directory,
            MCP_MAX_DIRECTORY_ENTRIES.saturating_add(1),
            8 * 1024 * 1024,
        )?;
        if more || raw_entries.len() > MCP_MAX_DIRECTORY_ENTRIES {
            return Err(AppError::Validation(
                "MCP 远端目录超过 100,000 项的首版限制，请缩小目标目录".into(),
            ));
        }
        check_directory_transfer_cancelled(&cancelled)?;
        let mut entries = raw_entries
            .into_iter()
            .filter_map(|(entry_path, stat)| {
                let name = display_name(&entry_path);
                if !show_hidden && name.starts_with('.') {
                    return None;
                }
                let kind = remote_kind(&stat).to_string();
                Some(RemoteFile {
                    name,
                    path: wire_path(&entry_path),
                    kind: kind.clone(),
                    size: stat.size.unwrap_or(0),
                    modified_at: stat.mtime,
                    permissions: permission_string(stat.perm, &kind),
                    owner: stat.uid,
                    group: stat.gid,
                })
            })
            .collect::<Vec<_>>();
        entries.sort_by(|a, b| match (a.kind.as_str(), b.kind.as_str()) {
            ("directory", "file") => std::cmp::Ordering::Less,
            ("file", "directory") => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });
        Ok(entries)
    })
    .await
}

pub async fn path_kind(
    db: Database,
    manager: SessionManager,
    session_id: String,
    path: String,
    root: String,
) -> AppResult<String> {
    validate_remote_path(&path)?;
    validate_remote_path(&root)?;
    with_sftp(db, manager, session_id, move |sftp| {
        validate_mcp_boundary_on_sftp(&sftp, &path, &root, false)?;
        let stat = sftp.lstat(&remote_path(&path)?)?;
        let kind = remote_kind(&stat);
        if kind == "symlink" || kind == "other" {
            return Err(AppError::Validation(
                "MCP 传输不跟随远端符号链接或特殊文件".into(),
            ));
        }
        Ok(kind.into())
    })
    .await
}

/// Resolve both sides on the remote server before an MCP file operation.
/// Lexical prefix checks alone are insufficient because an intermediate
/// symlink can redirect an apparently authorized path outside its grant.
pub async fn validate_mcp_remote_path_boundary(
    db: Database,
    manager: SessionManager,
    session_id: String,
    path: String,
    root: String,
    allow_missing: bool,
) -> AppResult<()> {
    validate_remote_path(&path)?;
    validate_remote_path(&root)?;
    with_sftp(db, manager, session_id, move |sftp| {
        validate_mcp_boundary_on_sftp(&sftp, &path, &root, allow_missing)
    })
    .await
}

fn validate_mcp_boundary_on_sftp(
    sftp: &Sftp,
    path: &str,
    root: &str,
    allow_missing: bool,
) -> AppResult<()> {
    let requested_root = remote_path(root)?;
    let canonical_root = sftp.realpath(&requested_root)?;
    let requested = remote_path(path)?;
    let relative = requested
        .strip_prefix(&requested_root)
        .map_err(|_| AppError::PermissionDenied("MCP 远端路径不在授权根内".into()))?;
    let (resolved, expected) = match sftp.realpath(&requested) {
        Ok(path) => (path, canonical_root.join(relative)),
        Err(error) if allow_missing && error.code() == ErrorCode::SFTP(2) => {
            let parent = requested
                .parent()
                .ok_or_else(|| AppError::Validation("MCP 远端目标没有父目录".into()))?;
            let relative_parent = parent
                .strip_prefix(&requested_root)
                .map_err(|_| AppError::PermissionDenied("MCP 远端目标父目录不在授权根内".into()))?;
            (sftp.realpath(parent)?, canonical_root.join(relative_parent))
        }
        Err(error) => return Err(error.into()),
    };
    validate_mcp_resolved_path(&resolved, &expected)?;
    Ok(())
}

fn validate_mcp_resolved_path(resolved: &Path, expected: &Path) -> AppResult<()> {
    if resolved != expected {
        return Err(AppError::PermissionDenied(
            "MCP 远端路径不能经过符号链接或越过授权根".into(),
        ));
    }
    Ok(())
}

pub async fn mkdir_for_mcp(
    db: Database,
    manager: SessionManager,
    session_id: String,
    path: String,
    root: String,
    cancelled: Arc<AtomicBool>,
) -> AppResult<()> {
    validate_remote_path(&path)?;
    validate_remote_path(&root)?;
    with_sftp(db, manager, session_id, move |sftp| {
        check_directory_transfer_cancelled(&cancelled)?;
        validate_mcp_boundary_on_sftp(&sftp, &path, &root, true)?;
        sftp.mkdir(&remote_path(&path)?, 0o755)?;
        Ok(())
    })
    .await
}

pub fn validate_local_directory_tree_for_mcp(root: &Path, cancelled: &AtomicBool) -> AppResult<()> {
    let mut pending = vec![root.to_path_buf()];
    let mut entries = 0_usize;
    let mut bytes = 0_u64;
    while let Some(path) = pending.pop() {
        check_directory_transfer_cancelled(cancelled)?;
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || local_metadata_is_reparse_point(&metadata) {
            return Err(AppError::PermissionDenied(
                "MCP 文件夹传输不允许符号链接或重解析点".into(),
            ));
        }
        entries += 1;
        if entries > MCP_MAX_DIRECTORY_ENTRIES {
            return Err(AppError::Validation(
                "MCP 文件夹超过 100,000 个项目的首版限制".into(),
            ));
        }
        if metadata.is_dir() {
            for child in std::fs::read_dir(&path)? {
                pending.push(child?.path());
            }
        } else if metadata.is_file() {
            bytes = bytes.saturating_add(metadata.len());
            if bytes > MCP_MAX_DIRECTORY_BYTES {
                return Err(AppError::Validation(
                    "MCP 文件夹超过 20 GiB 的首版限制".into(),
                ));
            }
        } else {
            return Err(AppError::Validation(
                "MCP 文件夹传输不允许设备、socket 或其他特殊文件".into(),
            ));
        }
    }
    Ok(())
}

pub async fn validate_remote_directory_tree_for_mcp(
    db: Database,
    manager: SessionManager,
    session_id: String,
    root: String,
    cancelled: Arc<AtomicBool>,
) -> AppResult<()> {
    validate_remote_path(&root)?;
    with_sftp(db, manager, session_id, move |sftp| {
        let mut pending = vec![remote_path(&root)?];
        let mut entries = 0_usize;
        let mut bytes = 0_u64;
        while let Some(path) = pending.pop() {
            check_directory_transfer_cancelled(&cancelled)?;
            let stat = sftp.lstat(&path)?;
            entries += 1;
            if entries > MCP_MAX_DIRECTORY_ENTRIES {
                return Err(AppError::Validation(
                    "MCP 文件夹超过 100,000 个项目的首版限制".into(),
                ));
            }
            match stat.file_type() {
                ssh2::FileType::Directory => {
                    for (child, _) in sftp.readdir(&path)? {
                        pending.push(child);
                    }
                }
                ssh2::FileType::RegularFile => {
                    bytes = bytes.saturating_add(stat.size.unwrap_or(0));
                    if bytes > MCP_MAX_DIRECTORY_BYTES {
                        return Err(AppError::Validation(
                            "MCP 文件夹超过 20 GiB 的首版限制".into(),
                        ));
                    }
                }
                _ => {
                    return Err(AppError::PermissionDenied(
                        "MCP 文件夹传输不允许远端符号链接或特殊文件".into(),
                    ));
                }
            }
        }
        Ok(())
    })
    .await
}

#[cfg(target_os = "windows")]
fn local_metadata_is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(target_os = "windows"))]
fn local_metadata_is_reparse_point(_metadata: &std::fs::Metadata) -> bool {
    false
}

pub async fn mkdir(
    db: Database,
    manager: SessionManager,
    session_id: String,
    path: String,
) -> AppResult<()> {
    validate_remote_path(&path)?;
    with_sftp(db, manager, session_id, move |sftp| {
        sftp.mkdir(&remote_path(&path)?, 0o755)?;
        Ok(())
    })
    .await
}

pub async fn rename(
    db: Database,
    manager: SessionManager,
    session_id: String,
    source: String,
    destination: String,
) -> AppResult<()> {
    validate_remote_path(&source)?;
    validate_remote_path(&destination)?;
    with_sftp(db, manager, session_id, move |sftp| {
        atomic_replace(&sftp, &remote_path(&source)?, &remote_path(&destination)?)?;
        Ok(())
    })
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn rename_for_mcp(
    db: Database,
    manager: SessionManager,
    session_id: String,
    source: String,
    destination: String,
    source_root: String,
    destination_root: String,
    expected_sha256: Option<String>,
    cancelled: Arc<AtomicBool>,
) -> AppResult<()> {
    validate_remote_path(&source)?;
    validate_remote_path(&destination)?;
    validate_remote_path(&source_root)?;
    validate_remote_path(&destination_root)?;
    if expected_sha256
        .as_ref()
        .is_some_and(|value| value.len() != 71 || !value.starts_with("sha256:"))
    {
        return Err(AppError::Validation("MCP expectedSha256 格式无效".into()));
    }
    with_sftp(db, manager, session_id, move |sftp| {
        check_directory_transfer_cancelled(&cancelled)?;
        validate_mcp_boundary_on_sftp(&sftp, &source, &source_root, false)?;
        validate_mcp_boundary_on_sftp(&sftp, &destination, &destination_root, true)?;
        let source = remote_path(&source)?;
        let destination = remote_path(&destination)?;
        let source_stat = sftp.lstat(&source)?;
        if !matches!(
            source_stat.file_type(),
            ssh2::FileType::RegularFile | ssh2::FileType::Directory
        ) {
            return Err(AppError::Validation(
                "MCP 只能重命名普通文件或目录，不跟随符号链接".into(),
            ));
        }
        match sftp.lstat(&destination) {
            Ok(_) => {
                return Err(AppError::Remote("MCP 重命名目标已经存在，拒绝覆盖".into()));
            }
            Err(error) if error.code() == ErrorCode::SFTP(2) => {}
            Err(error) => return Err(error.into()),
        }
        if source_stat.file_type() == ssh2::FileType::RegularFile {
            let expected = expected_sha256.as_deref().ok_or_else(|| {
                AppError::Validation("重命名普通文件必须提供 expectedSha256".into())
            })?;
            if source_stat.size.unwrap_or(0) > 64 * 1024 * 1024 {
                return Err(AppError::Validation(
                    "MCP 重命名冲突检查只支持 64 MiB 以下普通文件".into(),
                ));
            }
            let mut file = sftp.open(&source)?;
            let mut digest = Sha256::new();
            let mut buffer = vec![0_u8; 64 * 1024];
            loop {
                check_directory_transfer_cancelled(&cancelled)?;
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                digest.update(&buffer[..read]);
            }
            let actual = format!("sha256:{:x}", digest.finalize());
            if actual != expected {
                return Err(AppError::Remote(
                    "远端文件内容已变化，expectedSha256 不匹配".into(),
                ));
            }
        } else if expected_sha256.is_some() {
            return Err(AppError::Validation(
                "目录重命名不接受 expectedSha256".into(),
            ));
        }
        // The base SFTP RENAME operation must fail when the destination exists.
        // Avoid the POSIX rename extension here because it permits replacement.
        check_directory_transfer_cancelled(&cancelled)?;
        sftp.rename(&source, &destination, Some(RenameFlags::empty()))?;
        Ok(())
    })
    .await
}

fn delete_recursive(sftp: &Sftp, path: &Path) -> AppResult<()> {
    let stat = sftp.lstat(path)?;
    if stat.is_dir() {
        for (child, _) in sftp.readdir(path)? {
            delete_recursive(sftp, &child)?;
        }
        sftp.rmdir(path)?;
    } else {
        sftp.unlink(path)?;
    }
    Ok(())
}

fn delete_recursive_for_mcp(
    sftp: &Sftp,
    path: &Path,
    cancelled: &AtomicBool,
    visited: &mut usize,
) -> AppResult<()> {
    check_directory_transfer_cancelled(cancelled)?;
    *visited = visited.saturating_add(1);
    if *visited > MCP_MAX_DIRECTORY_ENTRIES {
        return Err(AppError::Validation(
            "MCP 递归删除超过 100,000 个项目的首版限制".into(),
        ));
    }
    let stat = sftp.lstat(path)?;
    match stat.file_type() {
        ssh2::FileType::Directory => {
            for (child, _) in sftp.readdir(path)? {
                delete_recursive_for_mcp(sftp, &child, cancelled, visited)?;
            }
            check_directory_transfer_cancelled(cancelled)?;
            sftp.rmdir(path)?;
        }
        ssh2::FileType::RegularFile | ssh2::FileType::Symlink => {
            check_directory_transfer_cancelled(cancelled)?;
            sftp.unlink(path)?;
        }
        _ => {
            return Err(AppError::PermissionDenied(
                "MCP 递归删除不处理远端特殊文件".into(),
            ));
        }
    }
    Ok(())
}

pub async fn delete_for_mcp(
    db: Database,
    manager: SessionManager,
    session_id: String,
    path: String,
    root: String,
    recursive: bool,
    cancelled: Arc<AtomicBool>,
) -> AppResult<()> {
    validate_deletable_path(&path)?;
    validate_remote_path(&root)?;
    with_sftp(db, manager, session_id, move |sftp| {
        check_directory_transfer_cancelled(&cancelled)?;
        validate_mcp_boundary_on_sftp(&sftp, &path, &root, false)?;
        let decoded = remote_path(&path)?;
        let stat = sftp.lstat(&decoded)?;
        if stat.file_type() == ssh2::FileType::Directory {
            if recursive {
                delete_recursive_for_mcp(&sftp, &decoded, &cancelled, &mut 0)
            } else {
                sftp.rmdir(&decoded).map_err(AppError::from)
            }
        } else if matches!(
            stat.file_type(),
            ssh2::FileType::RegularFile | ssh2::FileType::Symlink
        ) {
            sftp.unlink(&decoded).map_err(AppError::from)
        } else {
            Err(AppError::PermissionDenied(
                "MCP 删除不处理远端特殊文件".into(),
            ))
        }
    })
    .await
}

pub async fn delete(
    db: Database,
    manager: SessionManager,
    session_id: String,
    path: String,
    recursive: bool,
) -> AppResult<()> {
    validate_deletable_path(&path)?;
    with_sftp(db, manager, session_id, move |sftp| {
        let decoded = remote_path(&path)?;
        let path = decoded.as_path();
        let stat = sftp.lstat(path)?;
        if stat.is_dir() {
            if recursive {
                delete_recursive(&sftp, path)
            } else {
                sftp.rmdir(path).map_err(AppError::from)
            }
        } else {
            sftp.unlink(path).map_err(AppError::from)
        }
    })
    .await
}

pub async fn chmod(
    db: Database,
    manager: SessionManager,
    session_id: String,
    path: String,
    mode: u32,
) -> AppResult<()> {
    validate_remote_path(&path)?;
    if mode > 0o7777 {
        return Err(AppError::Validation("文件权限超出八进制 7777".into()));
    }
    with_sftp(db, manager, session_id, move |sftp| {
        sftp.setstat(
            &remote_path(&path)?,
            FileStat {
                size: None,
                uid: None,
                gid: None,
                perm: Some(mode),
                atime: None,
                mtime: None,
            },
        )?;
        Ok(())
    })
    .await
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextFile {
    pub content: String,
    pub modified_at: Option<u64>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextFileRange {
    pub content: String,
    pub size: u64,
    pub next_offset: Option<u64>,
    pub sha256: String,
    pub modified_at: Option<u64>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AtomicTextWrite {
    pub sha256: String,
    pub created: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectTransferResult {
    pub final_path: String,
    pub transferred_bytes: u64,
}

#[expect(
    clippy::too_many_arguments,
    reason = "The MCP transfer boundary carries independently validated local and remote capabilities"
)]
pub async fn transfer_file_direct(
    db: Database,
    manager: SessionManager,
    session_id: String,
    direction: String,
    source: String,
    destination: String,
    conflict_policy: String,
    cancelled: Arc<AtomicBool>,
    remote_root: String,
) -> AppResult<DirectTransferResult> {
    if !["upload", "download"].contains(&direction.as_str())
        || !["overwrite", "skip", "rename"].contains(&conflict_policy.as_str())
    {
        return Err(AppError::Validation("MCP 文件传输参数无效".into()));
    }
    if direction == "upload" {
        validate_upload_source(&source)?;
        validate_remote_path(&destination)?;
    } else {
        validate_remote_path(&source)?;
        validate_local_path(&destination)?;
    }
    validate_remote_path(&remote_root)?;
    let profile = manager.profile(&session_id)?;
    let mut transport = manager
        .acquire_auxiliary_transport(&db, &profile, "sftp")
        .await?;
    tokio::task::spawn_blocking(move || {
        let result = (|| {
            let sftp = transport.connected().session.sftp()?;
            let mut buffer = vec![0_u8; 256 * 1024];
            if direction == "upload" {
                validate_mcp_boundary_on_sftp(&sftp, &destination, &remote_root, true)?;
                let metadata = std::fs::symlink_metadata(&source)?;
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(AppError::Validation(
                        "MCP 只能上传普通本地文件，不跟随符号链接".into(),
                    ));
                }
                let mut final_path = destination.clone();
                if let Ok(stat) = sftp.lstat(Path::new(&final_path)) {
                    if stat.file_type() != ssh2::FileType::RegularFile {
                        return Err(AppError::Validation(
                            "MCP 不能覆盖远端符号链接或特殊文件".into(),
                        ));
                    }
                    match conflict_policy.as_str() {
                        "skip" => {
                            return Ok(DirectTransferResult {
                                final_path,
                                transferred_bytes: 0,
                            });
                        }
                        "rename" => {
                            let original = final_path.clone();
                            let mut index = 1;
                            while sftp.lstat(Path::new(&final_path)).is_ok() {
                                final_path = renamed_remote_path(&original, index);
                                index += 1;
                            }
                        }
                        "overwrite" => {}
                        _ => unreachable!(),
                    }
                }
                let temporary =
                    PathBuf::from(format!("{final_path}.cnshell-part-{}", Uuid::new_v4()));
                let upload = (|| {
                    let mut local = std::fs::File::open(&source)?;
                    let total = local.metadata()?.len();
                    let mut remote = sftp.open_mode(
                        &temporary,
                        OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
                        0o600,
                        OpenType::File,
                    )?;
                    let mut transferred = 0_u64;
                    loop {
                        check_directory_transfer_cancelled(&cancelled)?;
                        let read = local.read(&mut buffer)?;
                        if read == 0 {
                            break;
                        }
                        remote.write_all(&buffer[..read])?;
                        transferred += read as u64;
                    }
                    remote.fsync()?;
                    drop(remote);
                    if transferred != total || sftp.stat(&temporary)?.size.unwrap_or(0) != total {
                        return Err(AppError::Remote("MCP 上传大小校验失败".into()));
                    }
                    atomic_replace(&sftp, &temporary, Path::new(&final_path))?;
                    Ok(DirectTransferResult {
                        final_path,
                        transferred_bytes: transferred,
                    })
                })();
                if upload.is_err() {
                    let _ = sftp.unlink(&temporary);
                }
                upload
            } else {
                validate_mcp_boundary_on_sftp(&sftp, &source, &remote_root, false)?;
                let remote_path = remote_path(&source)?;
                let stat = sftp.lstat(&remote_path)?;
                if stat.file_type() != ssh2::FileType::RegularFile {
                    return Err(AppError::Validation(
                        "MCP 只能下载普通远端文件，不跟随符号链接".into(),
                    ));
                }
                let mut final_path = PathBuf::from(&destination);
                if final_path.exists() {
                    let metadata = std::fs::symlink_metadata(&final_path)?;
                    if metadata.file_type().is_symlink() || !metadata.is_file() {
                        return Err(AppError::Validation(
                            "MCP 不能覆盖本地符号链接或特殊文件".into(),
                        ));
                    }
                    match conflict_policy.as_str() {
                        "skip" => {
                            return Ok(DirectTransferResult {
                                final_path: final_path.to_string_lossy().into_owned(),
                                transferred_bytes: 0,
                            });
                        }
                        "rename" => {
                            let original = destination.clone();
                            let mut index = 1;
                            while final_path.exists() {
                                final_path = PathBuf::from(renamed_path(&original, index));
                                index += 1;
                            }
                        }
                        "overwrite" => {}
                        _ => unreachable!(),
                    }
                }
                let parent = final_path
                    .parent()
                    .ok_or_else(|| AppError::Validation("MCP 本地目标无父目录".into()))?;
                let parent_metadata = std::fs::symlink_metadata(parent)?;
                if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
                    return Err(AppError::Validation("MCP 本地目标目录无效".into()));
                }
                let part = parent.join(format!(".cnshell-part-{}", Uuid::new_v4()));
                let backup = parent.join(format!(".cnshell-backup-{}", Uuid::new_v4()));
                let download = (|| {
                    let mut remote = sftp.open(&remote_path)?;
                    let mut local = std::fs::File::create(&part)?;
                    let mut transferred = 0_u64;
                    loop {
                        check_directory_transfer_cancelled(&cancelled)?;
                        let read = remote.read(&mut buffer)?;
                        if read == 0 {
                            break;
                        }
                        local.write_all(&buffer[..read])?;
                        transferred += read as u64;
                    }
                    local.sync_all()?;
                    if transferred != stat.size.unwrap_or(0) {
                        return Err(AppError::Remote("MCP 下载大小校验失败".into()));
                    }
                    let replacing = final_path.exists();
                    if replacing {
                        std::fs::rename(&final_path, &backup)?;
                    }
                    if let Err(error) = std::fs::rename(&part, &final_path) {
                        if replacing {
                            let _ = std::fs::rename(&backup, &final_path);
                        }
                        return Err(error.into());
                    }
                    if replacing {
                        let _ = std::fs::remove_file(&backup);
                    }
                    Ok(DirectTransferResult {
                        final_path: final_path.to_string_lossy().into_owned(),
                        transferred_bytes: transferred,
                    })
                })();
                if download.is_err() {
                    let _ = std::fs::remove_file(&part);
                    if backup.exists() && !final_path.exists() {
                        let _ = std::fs::rename(&backup, &final_path);
                    }
                }
                download
            }
        })();
        if result.is_err() {
            transport.discard();
        }
        result
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()))?
}

fn validate_text_size(size: usize) -> AppResult<()> {
    if size > 10 * 1024 * 1024 {
        Err(AppError::Validation(
            "内置编辑器仅处理 10 MB 以下文本文件".into(),
        ))
    } else {
        Ok(())
    }
}

pub async fn open_text(
    db: Database,
    manager: SessionManager,
    session_id: String,
    path: String,
) -> AppResult<TextFile> {
    validate_remote_path(&path)?;
    with_sftp(db, manager, session_id, move |sftp| {
        let decoded = remote_path(&path)?;
        let stat = sftp.stat(&decoded)?;
        validate_text_size(stat.size.unwrap_or(0) as usize)?;
        let mut file = sftp.open(&decoded)?;
        let mut bytes = Vec::with_capacity(stat.size.unwrap_or(0) as usize);
        file.read_to_end(&mut bytes)?;
        let content = String::from_utf8(bytes).map_err(|_| {
            AppError::Validation("文件不是有效 UTF-8 文本，请使用外部编辑器".into())
        })?;
        Ok(TextFile {
            content,
            modified_at: stat.mtime,
        })
    })
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn read_text_range(
    db: Database,
    manager: SessionManager,
    session_id: String,
    path: String,
    root: String,
    offset: u64,
    max_bytes: usize,
    cancelled: Arc<AtomicBool>,
) -> AppResult<TextFileRange> {
    validate_remote_path(&path)?;
    validate_remote_path(&root)?;
    if max_bytes == 0 || max_bytes > 256 * 1024 {
        return Err(AppError::Validation(
            "MCP 文本读取范围必须在 1 到 256 KiB 之间".into(),
        ));
    }
    with_sftp(db, manager, session_id, move |sftp| {
        check_directory_transfer_cancelled(&cancelled)?;
        validate_mcp_boundary_on_sftp(&sftp, &path, &root, false)?;
        let decoded = remote_path(&path)?;
        let stat = sftp.lstat(&decoded)?;
        if stat.file_type() != ssh2::FileType::RegularFile {
            return Err(AppError::Validation(
                "MCP 只读取普通文本文件，不跟随符号链接".into(),
            ));
        }
        let size = stat.size.unwrap_or(0);
        if size > 64 * 1024 * 1024 {
            return Err(AppError::Validation(
                "MCP 文本读取只支持 64 MiB 以下文件".into(),
            ));
        }
        if offset > size {
            return Err(AppError::Validation("MCP 文本读取偏移超过文件大小".into()));
        }
        let mut file = sftp.open(&decoded)?;
        let mut digest = Sha256::new();
        let mut captured = Vec::with_capacity(max_bytes);
        let mut position = 0_u64;
        let mut buffer = vec![0_u8; 64 * 1024];
        loop {
            check_directory_transfer_cancelled(&cancelled)?;
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
            let chunk_start = position;
            let chunk_end = position + read as u64;
            let requested_end = offset.saturating_add(max_bytes as u64);
            if chunk_end > offset && chunk_start < requested_end {
                let from = offset.saturating_sub(chunk_start) as usize;
                let to = (requested_end.min(chunk_end) - chunk_start) as usize;
                captured.extend_from_slice(&buffer[from..to]);
            }
            position = chunk_end;
        }
        let content = String::from_utf8(captured)
            .map_err(|_| AppError::Validation("MCP 文件范围不是有效 UTF-8 文本".into()))?;
        let next = offset.saturating_add(content.len() as u64);
        Ok(TextFileRange {
            content,
            size,
            next_offset: (next < size).then_some(next),
            sha256: format!("sha256:{:x}", digest.finalize()),
            modified_at: stat.mtime,
        })
    })
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn write_text_atomic(
    db: Database,
    manager: SessionManager,
    session_id: String,
    path: String,
    root: String,
    content: String,
    expected_sha256: Option<String>,
    cancelled: Arc<AtomicBool>,
) -> AppResult<AtomicTextWrite> {
    validate_remote_path(&path)?;
    validate_remote_path(&root)?;
    if content.len() > 256 * 1024 {
        return Err(AppError::Validation(
            "MCP 单次文本写入不能超过 256 KiB".into(),
        ));
    }
    if expected_sha256
        .as_ref()
        .is_some_and(|value| value.len() != 71 || !value.starts_with("sha256:"))
    {
        return Err(AppError::Validation("MCP expectedSha256 格式无效".into()));
    }
    with_sftp(db, manager, session_id, move |sftp| {
        check_directory_transfer_cancelled(&cancelled)?;
        validate_mcp_boundary_on_sftp(&sftp, &path, &root, true)?;
        let target = remote_path(&path)?;
        let existing = match sftp.lstat(&target) {
            Ok(stat) => {
                if stat.file_type() != ssh2::FileType::RegularFile {
                    return Err(AppError::Validation(
                        "MCP 只能覆盖普通文件，不跟随符号链接".into(),
                    ));
                }
                Some(stat)
            }
            Err(error) if error.code() == ssh2::ErrorCode::SFTP(2) => None,
            Err(error) => return Err(error.into()),
        };
        if let Some(expected) = expected_sha256.as_deref() {
            let stat = existing.as_ref().ok_or_else(|| {
                AppError::Remote("远端文件不存在，expectedSha256 无法匹配".into())
            })?;
            let mut file = sftp.open(&target)?;
            let mut digest = Sha256::new();
            let mut buffer = vec![0_u8; 64 * 1024];
            let mut read_total = 0_u64;
            loop {
                check_directory_transfer_cancelled(&cancelled)?;
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                read_total += read as u64;
                if read_total > 10 * 1024 * 1024 {
                    return Err(AppError::Validation(
                        "MCP 冲突检查不处理超过 10 MiB 的文本文件".into(),
                    ));
                }
                digest.update(&buffer[..read]);
            }
            let actual = format!("sha256:{:x}", digest.finalize());
            if actual != expected {
                return Err(AppError::Remote(
                    "远端文件内容已变化，expectedSha256 不匹配".into(),
                ));
            }
            let _ = stat;
        } else if existing.is_some() {
            return Err(AppError::Remote(
                "覆盖已有文件必须提供 expectedSha256".into(),
            ));
        }
        let mut temp = target.clone();
        let mut name = target.file_name().unwrap_or_default().to_os_string();
        name.push(format!(".cnshell-mcp-{}", Uuid::new_v4()));
        temp.set_file_name(name);
        let mode = existing
            .as_ref()
            .and_then(|stat| stat.perm)
            .unwrap_or(0o644) as i32;
        let write_result = (|| -> AppResult<()> {
            check_directory_transfer_cancelled(&cancelled)?;
            let mut file = sftp.open_mode(
                &temp,
                OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
                mode,
                OpenType::File,
            )?;
            file.write_all(content.as_bytes())?;
            file.fsync()?;
            drop(file);
            check_directory_transfer_cancelled(&cancelled)?;
            atomic_replace(&sftp, &temp, &target)?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ = sftp.unlink(&temp);
        }
        write_result?;
        Ok(AtomicTextWrite {
            sha256: format!("sha256:{:x}", Sha256::digest(content.as_bytes())),
            created: existing.is_none(),
        })
    })
    .await
}

pub async fn save_text(
    db: Database,
    manager: SessionManager,
    session_id: String,
    path: String,
    content: String,
    expected_modified_at: Option<u64>,
) -> AppResult<()> {
    validate_remote_path(&path)?;
    validate_text_size(content.len())?;
    with_sftp(db, manager, session_id, move |sftp| {
        let target = remote_path(&path)?;
        let stat = sftp.stat(&target)?;
        if expected_modified_at.is_some() && stat.mtime != expected_modified_at {
            return Err(AppError::Remote(
                "远端文件已被其他程序修改，请重新加载后合并".into(),
            ));
        }
        let mut temp = target.clone();
        let suffix = format!(".cnshell-{}", Uuid::new_v4());
        let mut name = target.file_name().unwrap_or_default().to_os_string();
        name.push(suffix);
        temp.set_file_name(name);
        let mode = stat.perm.unwrap_or(0o644) as i32;
        {
            let mut file = sftp.open_mode(
                &temp,
                OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
                mode,
                OpenType::File,
            )?;
            file.write_all(content.as_bytes())?;
            file.fsync()?;
        }
        if let Err(error) = atomic_replace(&sftp, &temp, &target) {
            let _ = sftp.unlink(&temp);
            return Err(error);
        }
        Ok(())
    })
    .await
}

pub async fn create_text(
    db: Database,
    manager: SessionManager,
    session_id: String,
    path: String,
) -> AppResult<()> {
    validate_remote_path(&path)?;
    with_sftp(db, manager, session_id, move |sftp| {
        let target = remote_path(&path)?;
        let mut file = sftp.open_mode(
            &target,
            OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUSIVE,
            0o644,
            OpenType::File,
        )?;
        file.fsync()?;
        Ok(())
    })
    .await
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub async fn archive(
    db: Database,
    manager: SessionManager,
    session_id: String,
    path: String,
    extract: bool,
) -> AppResult<String> {
    validate_remote_path(&path)?;
    if path == "/" {
        return Err(AppError::Validation("禁止归档远端根目录".into()));
    }
    if extract && !path.ends_with(".tar.gz") {
        return Err(AppError::Validation("仅支持解压 .tar.gz 归档".into()));
    }
    if path.starts_with(RAW_PATH_PREFIX) {
        return Err(AppError::Validation(
            "包含非 UTF-8 文件名的项目请先重命名后再归档".into(),
        ));
    }
    let profile = manager.profile(&session_id)?;
    let mut transport = manager
        .acquire_auxiliary_transport(&db, &profile, "sftp")
        .await?;
    tokio::task::spawn_blocking(move || {
        let result = (|| {
            let mut channel = transport.connected().session.channel_session()?;
            let command = if extract {
                let parent = remote_parent_path(&path)?;
                format!(
                    "tar -xzf {} -C {}",
                    shell_quote(&path),
                    shell_quote(&parent.to_string_lossy())
                )
            } else {
                let parent = remote_parent_path(&path)?;
                let name = remote_utf8_name(&path)
                    .ok_or_else(|| AppError::Validation("远端路径无法打包".into()))?;
                format!(
                    "tar -czf {} -C {} {}",
                    shell_quote(&format!("{path}.tar.gz")),
                    shell_quote(&parent.to_string_lossy()),
                    shell_quote(name)
                )
            };
            channel.exec(&command)?;
            let mut stderr = String::new();
            channel.stderr().read_to_string(&mut stderr)?;
            channel.wait_close()?;
            if channel.exit_status()? != 0 {
                return Err(AppError::Remote(format!("归档命令失败：{stderr}")));
            }
            Ok(if extract {
                path
            } else {
                format!("{path}.tar.gz")
            })
        })();
        if result.is_err() {
            transport.discard();
        }
        result
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()))?
}

pub async fn open_local(
    db: Database,
    manager: SessionManager,
    session_id: String,
    path: String,
    application: Option<String>,
) -> AppResult<String> {
    validate_remote_path(&path)?;
    let profile = manager.profile(&session_id)?;
    let mut transport = manager
        .acquire_auxiliary_transport(&db, &profile, "sftp")
        .await?;
    tokio::task::spawn_blocking(move || {
        let result = (|| {
            let decoded = remote_path(&path)?;
            let stat = transport.connected().session.sftp()?.stat(&decoded)?;
            if stat.size.unwrap_or(0) > 100 * 1024 * 1024 {
                return Err(AppError::Validation(
                    "默认应用预览仅支持 100 MB 以下文件，请使用下载".into(),
                ));
            }
            let directory = std::env::temp_dir()
                .join("CNshellPreview")
                .join(Uuid::new_v4().to_string());
            std::fs::create_dir_all(&directory)?;
            let name = display_name(&decoded);
            let local = directory.join(name);
            let sftp = transport.connected().session.sftp()?;
            let mut remote = sftp.open(&decoded)?;
            let mut output = std::fs::File::create(&local)?;
            std::io::copy(&mut remote, &mut output)?;
            crate::platform::open_local_path(&local, application.as_deref())?;
            Ok(local.to_string_lossy().into_owned())
        })();
        if result.is_err() {
            transport.discard();
        }
        result
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()))?
}

fn check_directory_transfer_cancelled(cancelled: &AtomicBool) -> AppResult<()> {
    if cancelled.load(Ordering::Acquire) {
        Err(AppError::Remote("文件夹传输已取消".into()))
    } else {
        Ok(())
    }
}

fn run_tar(arguments: &[&std::ffi::OsStr]) -> AppResult<()> {
    let status = Command::new("tar").args(arguments).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::Remote(format!("本地 tar 失败：{status}")))
    }
}

fn run_remote_command(session: &ssh2::Session, command: &str, failure: &str) -> AppResult<()> {
    let mut channel = session.channel_session()?;
    channel.exec(command)?;
    let mut stdout = Vec::new();
    channel.read_to_end(&mut stdout)?;
    let mut stderr = String::new();
    channel.stderr().read_to_string(&mut stderr)?;
    channel.wait_close()?;
    if channel.exit_status()? == 0 {
        Ok(())
    } else if stderr.trim().is_empty() {
        Err(AppError::Remote(failure.into()))
    } else {
        Err(AppError::Remote(format!("{failure}：{}", stderr.trim())))
    }
}

fn remove_local_path(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Directory transfer is an internal command boundary with explicit validated fields"
)]
pub async fn transfer_directory(
    db: Database,
    manager: SessionManager,
    session_id: String,
    direction: String,
    source: String,
    destination: String,
    conflict_policy: String,
    cancelled: Arc<AtomicBool>,
    remote_root: Option<String>,
) -> AppResult<String> {
    if !["upload", "download"].contains(&direction.as_str()) {
        return Err(AppError::Validation("文件夹传输方向无效".into()));
    }
    if !["overwrite", "skip", "rename"].contains(&conflict_policy.as_str()) {
        return Err(AppError::Validation("文件夹冲突策略无效".into()));
    }
    if direction == "upload" {
        validate_local_path(&source)?;
        validate_remote_path(&destination)?;
        if !Path::new(&source).is_dir() {
            return Err(AppError::Validation("上传源必须是本地文件夹".into()));
        }
    } else {
        validate_remote_path(&source)?;
        validate_local_path(&destination)?;
    }
    if let Some(root) = remote_root.as_deref() {
        validate_remote_path(root)?;
    }
    if source.starts_with(RAW_PATH_PREFIX) || destination.starts_with(RAW_PATH_PREFIX) {
        return Err(AppError::Validation(
            "文件夹打包传输暂不支持非 UTF-8 路径".into(),
        ));
    }
    let profile = manager.profile(&session_id)?;
    let mut transport = manager
        .acquire_auxiliary_transport(&db, &profile, "sftp")
        .await?;
    tokio::task::spawn_blocking(move || {
        let identifier = Uuid::new_v4().to_string();
        let local_archive =
            std::env::temp_dir().join(format!("cnshell-directory-{identifier}.tar.gz"));
        let result = (|| {
            check_directory_transfer_cancelled(&cancelled)?;
            let sftp = transport.connected().session.sftp()?;
            if direction == "upload" {
                if let Some(root) = remote_root.as_deref() {
                    validate_mcp_boundary_on_sftp(&sftp, &destination, root, true)?;
                }
                let source_path = Path::new(&source);
                run_tar(&[
                    std::ffi::OsStr::new("-czf"),
                    local_archive.as_os_str(),
                    std::ffi::OsStr::new("-C"),
                    source_path.as_os_str(),
                    std::ffi::OsStr::new("."),
                ])?;
                check_directory_transfer_cancelled(&cancelled)?;
                let mut final_destination = destination.clone();
                let mut overwrite_existing = false;
                if sftp.lstat(Path::new(&final_destination)).is_ok() {
                    match conflict_policy.as_str() {
                        "skip" => return Ok(final_destination),
                        "rename" => {
                            let original = final_destination.clone();
                            let mut index = 1;
                            while sftp.lstat(Path::new(&final_destination)).is_ok() {
                                final_destination = renamed_remote_path(&original, index);
                                index += 1;
                            }
                        }
                        "overwrite" => overwrite_existing = true,
                        _ => unreachable!(),
                    }
                }
                let parent = wire_path(&remote_parent_path(&final_destination)?);
                let remote_archive =
                    remote_child_path(&parent, &format!(".cnshell-directory-{identifier}.tar.gz"))?;
                let remote_staging =
                    remote_child_path(&parent, &format!(".cnshell-directory-{identifier}"))?;
                let remote_backup =
                    remote_child_path(&parent, &format!(".cnshell-directory-backup-{identifier}"))?;
                let transfer_result = (|| {
                    let mut local = std::fs::File::open(&local_archive)?;
                    let mut remote = sftp.open_mode(
                        &remote_archive,
                        OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
                        0o600,
                        OpenType::File,
                    )?;
                    let mut buffer = vec![0_u8; 256 * 1024];
                    loop {
                        check_directory_transfer_cancelled(&cancelled)?;
                        let read = local.read(&mut buffer)?;
                        if read == 0 {
                            break;
                        }
                        remote.write_all(&buffer[..read])?;
                    }
                    remote.fsync()?;
                    drop(remote);
                    sftp.mkdir(&remote_staging, 0o755)?;
                    drop(sftp);
                    run_remote_command(
                        &transport.connected().session,
                        &format!(
                            "tar -xzf {} -C {}",
                            shell_quote(&remote_archive.to_string_lossy()),
                            shell_quote(&remote_staging.to_string_lossy())
                        ),
                        "远端文件夹解包失败",
                    )?;
                    check_directory_transfer_cancelled(&cancelled)?;
                    let sftp = transport.connected().session.sftp()?;
                    if overwrite_existing {
                        sftp.rename(
                            Path::new(&final_destination),
                            &remote_backup,
                            Some(RenameFlags::ATOMIC),
                        )?;
                    }
                    if let Err(error) = sftp.rename(
                        &remote_staging,
                        Path::new(&final_destination),
                        Some(RenameFlags::ATOMIC),
                    ) {
                        if overwrite_existing {
                            let _ = sftp.rename(
                                &remote_backup,
                                Path::new(&final_destination),
                                Some(RenameFlags::ATOMIC),
                            );
                        }
                        return Err(AppError::from(error));
                    }
                    if overwrite_existing {
                        delete_recursive(&sftp, &remote_backup)?;
                    }
                    let _ = sftp.unlink(&remote_archive);
                    Ok(final_destination.clone())
                })();
                if transfer_result.is_err() {
                    let sftp = transport.connected().session.sftp()?;
                    let _ = sftp.unlink(&remote_archive);
                    if sftp.lstat(&remote_staging).is_ok() {
                        let _ = delete_recursive(&sftp, &remote_staging);
                    }
                    if sftp.lstat(&remote_backup).is_ok()
                        && sftp.lstat(Path::new(&final_destination)).is_err()
                    {
                        let _ = sftp.rename(
                            &remote_backup,
                            Path::new(&final_destination),
                            Some(RenameFlags::ATOMIC),
                        );
                    }
                }
                transfer_result
            } else {
                if let Some(root) = remote_root.as_deref() {
                    validate_mcp_boundary_on_sftp(&sftp, &source, root, false)?;
                }
                let source_path = remote_path(&source)?;
                if !sftp.lstat(&source_path)?.is_dir() {
                    return Err(AppError::Validation("下载源必须是远端文件夹".into()));
                }
                let remote_parent = wire_path(&remote_parent_path(&source)?);
                let remote_archive = remote_child_path(
                    &remote_parent,
                    &format!(".cnshell-directory-{identifier}.tar.gz"),
                )?;
                drop(sftp);
                let archive_result = run_remote_command(
                    &transport.connected().session,
                    &format!(
                        "tar -czf {} -C {} .",
                        shell_quote(&remote_archive.to_string_lossy()),
                        shell_quote(&source)
                    ),
                    "远端文件夹打包失败",
                );
                if let Err(error) = archive_result {
                    if let Ok(sftp) = transport.connected().session.sftp() {
                        let _ = sftp.unlink(&remote_archive);
                    }
                    return Err(error);
                }
                let sftp = transport.connected().session.sftp()?;
                let download_result = (|| {
                    check_directory_transfer_cancelled(&cancelled)?;
                    let mut remote = sftp.open(&remote_archive)?;
                    let mut local = std::fs::File::create(&local_archive)?;
                    let mut buffer = vec![0_u8; 256 * 1024];
                    loop {
                        check_directory_transfer_cancelled(&cancelled)?;
                        let read = remote.read(&mut buffer)?;
                        if read == 0 {
                            break;
                        }
                        local.write_all(&buffer[..read])?;
                    }
                    local.sync_all()?;
                    let mut final_destination = PathBuf::from(&destination);
                    let mut overwrite_existing = false;
                    if final_destination.exists() {
                        match conflict_policy.as_str() {
                            "skip" => return Ok(final_destination.to_string_lossy().into_owned()),
                            "rename" => {
                                let original = destination.clone();
                                let mut index = 1;
                                while final_destination.exists() {
                                    final_destination =
                                        PathBuf::from(renamed_path(&original, index));
                                    index += 1;
                                }
                            }
                            "overwrite" => overwrite_existing = true,
                            _ => unreachable!(),
                        }
                    }
                    let parent = final_destination
                        .parent()
                        .ok_or_else(|| AppError::Validation("本地目标目录无效".into()))?;
                    let staging = parent.join(format!(".cnshell-directory-{identifier}"));
                    let backup = parent.join(format!(".cnshell-directory-backup-{identifier}"));
                    std::fs::create_dir(&staging)?;
                    let extract_result = run_tar(&[
                        std::ffi::OsStr::new("-xzf"),
                        local_archive.as_os_str(),
                        std::ffi::OsStr::new("-C"),
                        staging.as_os_str(),
                    ])
                    .and_then(|_| {
                        check_directory_transfer_cancelled(&cancelled)?;
                        if overwrite_existing {
                            std::fs::rename(&final_destination, &backup)?;
                        }
                        if let Err(error) = std::fs::rename(&staging, &final_destination) {
                            if overwrite_existing {
                                let _ = std::fs::rename(&backup, &final_destination);
                            }
                            return Err(AppError::from(error));
                        }
                        if overwrite_existing {
                            remove_local_path(&backup)?;
                        }
                        Ok(())
                    });
                    if extract_result.is_err() {
                        let _ = std::fs::remove_dir_all(&staging);
                        if backup.exists() && !final_destination.exists() {
                            let _ = std::fs::rename(&backup, &final_destination);
                        }
                    }
                    extract_result?;
                    Ok(final_destination.to_string_lossy().into_owned())
                })();
                let _ = sftp.unlink(&remote_archive);
                download_result
            }
        })();
        let _ = std::fs::remove_file(&local_archive);
        if result.is_err() {
            transport.discard();
        }
        result
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()))?
}

pub fn cleanup_preview_cache() -> AppResult<()> {
    let directory = std::env::temp_dir().join("CNshellPreview");
    if directory.exists() {
        std::fs::remove_dir_all(directory)?;
    }
    Ok(())
}

fn renamed_path(path: &str, index: u32) -> String {
    let source = Path::new(path);
    let parent = source.parent().unwrap_or(Path::new(""));
    let filename = source
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    parent
        .join(renamed_filename(filename, index))
        .to_string_lossy()
        .into_owned()
}

fn renamed_remote_path(path: &str, index: u32) -> String {
    match path.rfind('/') {
        Some(separator) => format!(
            "{}{}",
            &path[..=separator],
            renamed_filename(&path[separator + 1..], index)
        ),
        None => renamed_filename(path, index),
    }
}

fn renamed_filename(filename: &str, index: u32) -> String {
    let compound = [".tar.gz", ".tar.bz2", ".tar.xz"]
        .into_iter()
        .find(|extension| filename.ends_with(extension));
    let (name, extension) = if let Some(extension) = compound {
        (&filename[..filename.len() - extension.len()], extension)
    } else if let Some(position) = filename.rfind('.').filter(|position| *position > 0) {
        (&filename[..position], &filename[position..])
    } else {
        (filename, "")
    };
    format!("{name} ({index}){extension}")
}

#[expect(
    clippy::too_many_arguments,
    reason = "The generic download primitive exposes injectable I/O hooks used by failure tests"
)]
fn download_to_path<R, W, Open, Sync, Pulse>(
    remote: &mut R,
    part: &Path,
    destination: &Path,
    expected_bytes: i64,
    buffer: &mut [u8],
    open_writer: Open,
    sync_writer: Sync,
    mut pulse: Pulse,
) -> AppResult<i64>
where
    R: Read,
    W: Write,
    Open: FnOnce(&Path) -> std::io::Result<W>,
    Sync: FnOnce(&mut W) -> std::io::Result<()>,
    Pulse: FnMut(i64) -> AppResult<()>,
{
    let result = (|| {
        let mut local = open_writer(part)?;
        let mut transferred = 0_i64;
        loop {
            pulse(transferred)?;
            let read = remote.read(buffer)?;
            if read == 0 {
                break;
            }
            local.write_all(&buffer[..read])?;
            transferred += read as i64;
            pulse(transferred)?;
        }
        sync_writer(&mut local)?;
        if transferred != expected_bytes {
            return Err(AppError::Remote(format!(
                "下载大小校验失败：预期 {expected_bytes} 字节，实际 {transferred} 字节"
            )));
        }
        std::fs::rename(part, destination)?;
        Ok(transferred)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(part);
    }
    result
}

#[derive(Clone, Default)]
pub struct TransferManager {
    controls: Arc<parking_lot::Mutex<std::collections::HashMap<String, Arc<AtomicU8>>>>,
    targets: Arc<parking_lot::Mutex<std::collections::HashMap<String, String>>>,
}

impl TransferManager {
    pub fn token(&self, id: &str, target: &str) -> AppResult<Arc<AtomicU8>> {
        let mut targets = self.targets.lock();
        if targets.values().any(|running| running == target) {
            return Err(AppError::Validation("同一目标已有传输任务正在运行".into()));
        }
        targets.insert(id.into(), target.into());
        drop(targets);
        let token = Arc::new(AtomicU8::new(0));
        self.controls.lock().insert(id.into(), token.clone());
        Ok(token)
    }
    pub fn cancel(&self, id: &str) -> bool {
        self.controls
            .lock()
            .get(id)
            .map(|token| token.swap(2, Ordering::AcqRel) != 2)
            .unwrap_or(false)
    }
    pub fn pause(&self, id: &str) -> bool {
        self.controls
            .lock()
            .get(id)
            .map(|token| {
                token
                    .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            })
            .unwrap_or(false)
    }
    pub fn resume(&self, id: &str) -> bool {
        self.controls
            .lock()
            .get(id)
            .map(|token| {
                token
                    .compare_exchange(1, 0, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            })
            .unwrap_or(false)
    }
    pub fn finish(&self, id: &str) {
        self.controls.lock().remove(id);
        self.targets.lock().remove(id);
    }
    #[cfg(test)]
    fn contains(&self, id: &str) -> bool {
        self.controls.lock().contains_key(id)
    }
}

fn transfer_with_scp(
    transport: &crate::ssh::TransportLease,
    task: &mut TransferTask,
    token: &AtomicU8,
    app: &AppHandle,
) -> AppResult<()> {
    let remote = remote_path(if task.direction == "upload" {
        &task.destination
    } else {
        &task.source
    })?;
    let mut buffer = vec![0_u8; 256 * 1024];
    if task.direction == "upload" {
        if task.conflict_policy != "overwrite" {
            return Err(AppError::Unavailable(
                "SFTP 不可用时，SCP 无法安全探测远端同名文件；请选择覆盖策略后重试".into(),
            ));
        }
        let mut local = std::fs::File::open(&task.source)?;
        task.total_bytes = local.metadata()?.len() as i64;
        let mut channel = transport.connected().session.scp_send(
            &remote,
            0o600,
            task.total_bytes as u64,
            None,
        )?;
        loop {
            if token.load(Ordering::Relaxed) == 2 {
                return Err(AppError::Remote("传输已取消".into()));
            }
            let read = local.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            channel.write_all(&buffer[..read])?;
            task.transferred_bytes += read as i64;
            let _ = app.emit("transfer-progress", task.clone());
        }
        channel.send_eof()?;
        channel.wait_eof()?;
        channel.close()?;
        channel.wait_close()?;
    } else {
        let destination_exists = Path::new(&task.destination).exists();
        if destination_exists {
            match task.conflict_policy.as_str() {
                "skip" => return Ok(()),
                "rename" => {
                    let original = task.destination.clone();
                    let mut index = 1;
                    while Path::new(&task.destination).exists() {
                        task.destination = renamed_path(&original, index);
                        index += 1;
                    }
                }
                "overwrite" => {}
                _ => {
                    return Err(AppError::Validation(
                        "本地目标已存在，请选择覆盖、跳过或重命名".into(),
                    ));
                }
            }
        }
        let (mut channel, stat) = transport.connected().session.scp_recv(&remote)?;
        task.total_bytes = stat.size() as i64;
        let temporary = format!("{}.cnshell-part-{}", task.destination, task.id);
        let mut local = std::fs::File::create(&temporary)?;
        let result = (|| -> AppResult<()> {
            loop {
                if token.load(Ordering::Relaxed) == 2 {
                    return Err(AppError::Remote("传输已取消".into()));
                }
                let read = channel.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                local.write_all(&buffer[..read])?;
                task.transferred_bytes += read as i64;
                let _ = app.emit("transfer-progress", task.clone());
            }
            local.sync_all()?;
            drop(local);
            std::fs::rename(&temporary, &task.destination)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result?;
    }
    if task.transferred_bytes != task.total_bytes {
        return Err(AppError::Remote(format!(
            "SCP 大小校验失败：预期 {} 字节，实际 {} 字节",
            task.total_bytes, task.transferred_bytes
        )));
    }
    Ok(())
}

pub async fn enqueue(
    app: AppHandle,
    db: Database,
    manager: SessionManager,
    transfers: TransferManager,
    input: TransferInput,
) -> AppResult<TransferTask> {
    if !["upload", "download"].contains(&input.direction.as_str()) {
        return Err(AppError::Validation("传输方向无效".into()));
    }
    if !["ask", "overwrite", "skip", "rename"].contains(&input.conflict_policy.as_str()) {
        return Err(AppError::Validation("同名冲突策略无效".into()));
    }
    if input.direction == "upload" {
        validate_upload_source(&input.source)?;
        validate_remote_path(&input.destination)?;
    } else {
        validate_remote_path(&input.source)?;
        validate_local_path(&input.destination)?;
    }
    let mut task = TransferTask {
        id: Uuid::new_v4().to_string(),
        session_id: input.session_id,
        direction: input.direction,
        source: input.source,
        destination: input.destination,
        total_bytes: 0,
        transferred_bytes: 0,
        status: "queued".into(),
        conflict_policy: input.conflict_policy,
        error: None,
        created_at: Utc::now().to_rfc3339(),
    };
    let target_key = format!("{}:{}", task.direction, task.destination);
    let token = transfers.token(&task.id, &target_key)?;
    if let Err(error) = db.upsert_transfer(&task).await {
        transfers.finish(&task.id);
        return Err(error);
    }
    let returned = task.clone();
    tauri::async_runtime::spawn(async move {
        task.status = "running".into();
        let _ = db.upsert_transfer(&task).await;
        let _ = app.emit("transfer-progress", task.clone());
        let profile = match manager.profile(&task.session_id) {
            Ok(value) => value,
            Err(error) => {
                task.status = "failed".into();
                task.error = Some(error.to_string());
                let _ = db.upsert_transfer(&task).await;
                let _ = app.emit("transfer-progress", task.clone());
                transfers.finish(&task.id);
                return;
            }
        };
        let transport = match manager
            .acquire_auxiliary_transport(&db, &profile, "sftp")
            .await
        {
            Ok(value) => value,
            Err(error) => {
                task.status = "failed".into();
                task.error = Some(error.to_string());
                let _ = db.upsert_transfer(&task).await;
                let _ = app.emit("transfer-progress", task.clone());
                transfers.finish(&task.id);
                return;
            }
        };
        let app_clone = app.clone();
        let mut work_task = task.clone();
        let db_clone = db.clone();
        let token_clone = token.clone();
        let result = tokio::task::spawn_blocking(move || -> AppResult<TransferTask> {
            let mut transport=transport; let result=(|| -> AppResult<TransferTask> { let sftp=match transport.connected().session.sftp(){Ok(sftp)=>sftp,Err(_)=>{transfer_with_scp(&transport,&mut work_task,&token_clone,&app_clone)?;work_task.status="completed".into();return Ok(work_task);}}; let mut buffer=vec![0_u8;256*1024];
            if work_task.direction=="download" {
                let destination_exists=Path::new(&work_task.destination).exists();if destination_exists{match work_task.conflict_policy.as_str(){"skip"=>{work_task.status="completed".into();return Ok(work_task);},"rename"=>{let original=work_task.destination.clone();let mut index=1;while Path::new(&work_task.destination).exists(){work_task.destination=renamed_path(&original,index);index+=1;}},"overwrite"=>{},_=>return Err(AppError::Validation("本地目标已存在，请选择覆盖、跳过或重命名".into()))}}
                let source=remote_path(&work_task.source)?;let stat=sftp.stat(&source)?; work_task.total_bytes=stat.size.unwrap_or(0) as i64;
                let mut remote=sftp.open(&source)?; let part=PathBuf::from(format!("{}.cnshell-part-{}",work_task.destination,work_task.id));
                let destination=PathBuf::from(&work_task.destination);
                let total_bytes=work_task.total_bytes;
                download_to_path(&mut remote,&part,&destination,total_bytes,&mut buffer,|path|std::fs::File::create(path),|local|local.sync_all(),|transferred|{
                    while token_clone.load(Ordering::Relaxed)==1{work_task.status="paused".into();let _=app_clone.emit("transfer-progress",work_task.clone());std::thread::sleep(Duration::from_millis(100));}
                    work_task.status="running".into();
                    if token_clone.load(Ordering::Relaxed)==2{return Err(AppError::Remote("传输已取消".into()));}
                    work_task.transferred_bytes=transferred;let _=app_clone.emit("transfer-progress",work_task.clone());Ok(())
                })?;
            } else {
                if sftp.stat(Path::new(&work_task.destination)).is_ok(){match work_task.conflict_policy.as_str(){"skip"=>{work_task.status="completed".into();return Ok(work_task);},"rename"=>{let original=work_task.destination.clone();let mut index=1;while sftp.stat(Path::new(&work_task.destination)).is_ok(){work_task.destination=renamed_remote_path(&original,index);index+=1;}},"overwrite"=>{},_=>return Err(AppError::Validation("远端目标已存在，请选择覆盖、跳过或重命名".into()))}}
                let mut local=std::fs::File::open(&work_task.source)?; work_task.total_bytes=local.metadata()?.len() as i64;
                let temporary=PathBuf::from(format!("{}.cnshell-part-{}",work_task.destination,work_task.id));
                let upload=(||->AppResult<()>{
                    let mut remote=sftp.open_mode(&temporary,OpenFlags::WRITE|OpenFlags::CREATE|OpenFlags::TRUNCATE,0o600,OpenType::File)?;
                    loop { while token_clone.load(Ordering::Relaxed)==1{work_task.status="paused".into();let _=app_clone.emit("transfer-progress",work_task.clone());std::thread::sleep(Duration::from_millis(100));}work_task.status="running".into();if token_clone.load(Ordering::Relaxed)==2{return Err(AppError::Remote("传输已取消".into()));} let read=local.read(&mut buffer)?;if read==0{break;}remote.write_all(&buffer[..read])?;work_task.transferred_bytes+=read as i64;let _=app_clone.emit("transfer-progress",work_task.clone()); }
                    remote.fsync()?;drop(remote);
                    let actual=sftp.stat(&temporary)?.size.unwrap_or(0)as i64;if work_task.transferred_bytes!=work_task.total_bytes||actual!=work_task.total_bytes{return Err(AppError::Remote(format!("上传大小校验失败：预期 {} 字节，本地已发送 {} 字节，远端临时文件 {} 字节",work_task.total_bytes,work_task.transferred_bytes,actual)));}
                    atomic_replace(&sftp, &temporary, Path::new(&work_task.destination))?;Ok(())
                })();
                if let Err(error)=upload{let _=sftp.unlink(&temporary);return Err(error);}
            }
            work_task.status="completed".into(); Ok(work_task) })(); if result.is_err(){transport.discard();} result
        }).await;
        task = match result {
            Ok(Ok(done)) => done,
            Ok(Err(error)) => {
                task.status = if token.load(Ordering::Relaxed) == 2 {
                    "cancelled".into()
                } else {
                    "failed".into()
                };
                task.error = Some(error.to_string());
                task
            }
            Err(error) => {
                task.status = "failed".into();
                task.error = Some(error.to_string());
                task
            }
        };
        let _ = db_clone.upsert_transfer(&task).await;
        let _ = app.emit("transfer-progress", task.clone());
        transfers.finish(&task.id);
    });
    Ok(returned)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct NoSpaceWriter(std::fs::File);
    impl Write for NoSpaceWriter {
        fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from_raw_os_error(28))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.0.flush()
        }
    }
    #[test]
    fn permission_rendering_is_posix() {
        assert_eq!(permission_string(Some(0o755), "directory"), "drwxr-xr-x");
        assert_eq!(permission_string(Some(0o640), "file"), "-rw-r-----");
    }
    #[test]
    fn shell_quoting_blocks_command_injection() {
        assert_eq!(shell_quote("a'; rm -rf /"), "'a'\\''; rm -rf /'");
    }
    #[test]
    fn conflict_rename_preserves_extension() {
        let local = std::env::temp_dir().join("file.txt");
        let expected = std::env::temp_dir().join("file (2).txt");
        assert_eq!(
            renamed_path(local.to_str().unwrap(), 2),
            expected.to_string_lossy()
        );
        assert_eq!(
            renamed_remote_path("/tmp/archive.tar.gz", 1),
            "/tmp/archive (1).tar.gz"
        );
        assert_eq!(
            renamed_remote_path("/var/log/file.txt", 2),
            "/var/log/file (2).txt"
        );
        assert!(!renamed_remote_path("/var/log/file.txt", 2).contains('\\'));
    }
    #[test]
    fn text_editor_limit_uses_utf8_bytes() {
        assert!(validate_text_size(10 * 1024 * 1024).is_ok());
        assert!(validate_text_size(10 * 1024 * 1024 + 1).is_err());
        assert!(validate_text_size("中文".len()).is_ok());
    }
    #[test]
    fn paths_and_conflict_inputs_are_bounded() {
        assert!(validate_remote_path("/tmp/file").is_ok());
        assert!(validate_remote_path("relative").is_err());
        assert!(validate_remote_path(&format!("/{}", "x".repeat(16 * 1024))).is_err());
        assert!(validate_local_path(std::env::temp_dir().join("file").to_str().unwrap()).is_ok());
        assert!(validate_local_path("relative").is_err());
        assert_eq!(
            join_path("/var/log", "system.log").unwrap(),
            "/var/log/system.log"
        );
        assert_eq!(
            wire_path(&remote_child_path("/var/log", ".cnshell-part").unwrap()),
            "/var/log/.cnshell-part"
        );
        assert_eq!(
            wire_path(&remote_parent_path("/var/log/system.log").unwrap()),
            "/var/log"
        );
    }
    #[test]
    fn file_transfer_rejects_directory_sources() {
        let directory = tempfile::tempdir().unwrap();
        assert!(validate_upload_source(directory.path().to_str().unwrap()).is_err());

        let file = directory.path().join("upload.txt");
        std::fs::write(&file, b"CNshell").unwrap();
        assert!(validate_upload_source(file.to_str().unwrap()).is_ok());
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn open_with_requires_an_existing_app_directory() {
        assert!(crate::platform::validate_application_path("relative.app").is_err());
        assert!(
            crate::platform::validate_application_path("/Applications/missing-cnshell-test.app")
                .is_err()
        );
        let directory = tempfile::tempdir().unwrap();
        let application = directory.path().join("Preview.app");
        std::fs::create_dir(&application).unwrap();
        assert!(crate::platform::validate_application_path(application.to_str().unwrap()).is_ok());
        assert!(
            crate::platform::validate_application_path(directory.path().to_str().unwrap()).is_err()
        );
    }
    #[test]
    fn remote_root_cannot_be_deleted() {
        for path in ["/", "/.", "/.."] {
            assert!(validate_deletable_path(path).is_err());
        }
        assert!(validate_deletable_path("/tmp/file").is_ok());
    }
    #[test]
    fn mcp_canonical_path_must_match_the_authorized_relative_path() {
        assert!(
            validate_mcp_resolved_path(
                Path::new("/srv/authorized/log/app.log"),
                Path::new("/srv/authorized/log/app.log")
            )
            .is_ok()
        );
        assert!(matches!(
            validate_mcp_resolved_path(
                Path::new("/srv/private/app.log"),
                Path::new("/srv/authorized/link/app.log")
            ),
            Err(AppError::PermissionDenied(_))
        ));
        assert!(matches!(
            validate_mcp_resolved_path(
                Path::new("/srv/authorized/real/app.log"),
                Path::new("/srv/authorized/link/app.log")
            ),
            Err(AppError::PermissionDenied(_))
        ));
    }
    #[cfg(unix)]
    #[test]
    fn non_utf8_paths_round_trip_without_loss() {
        let raw = PathBuf::from(std::ffi::OsString::from_vec(vec![
            b'/', b't', b'm', b'p', b'/', 0xff, b'a',
        ]));
        let token = wire_path(&raw);
        assert!(token.starts_with(RAW_PATH_PREFIX));
        assert_eq!(
            remote_path(&token).unwrap().as_os_str().as_bytes(),
            raw.as_os_str().as_bytes()
        );
        assert_eq!(display_name(&raw), "\\xFFa");
        assert_ne!(wire_path(Path::new("/tmp/\u{fffd}a")), token);
    }
    #[cfg(unix)]
    #[test]
    fn joins_names_below_non_utf8_directories() {
        let raw = PathBuf::from(std::ffi::OsString::from_vec(vec![
            b'/', b't', b'm', b'p', b'/', 0xff,
        ]));
        let joined = join_path(&wire_path(&raw), "child.txt").unwrap();
        assert_eq!(
            remote_path(&joined).unwrap().as_os_str().as_bytes(),
            b"/tmp/\xff/child.txt"
        );
        assert!(join_path(&wire_path(&raw), "../escape").is_err());
    }
    #[test]
    fn preview_cache_is_removed_on_next_launch() {
        let directory = std::env::temp_dir().join("CNshellPreview");
        let marker = directory.join(format!("test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&marker).unwrap();
        std::fs::write(marker.join("remote.txt"), b"secret").unwrap();
        cleanup_preview_cache().unwrap();
        assert!(!directory.exists());
    }
    #[test]
    fn disk_full_download_reports_storage_error_and_removes_partial_file() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("download.bin");
        let part = directory.path().join("download.bin.cnshell-part-test");
        let mut source = std::io::Cursor::new(vec![7_u8; 32]);
        let mut buffer = [0_u8; 16];
        let result = download_to_path(
            &mut source,
            &part,
            &destination,
            32,
            &mut buffer,
            |path| std::fs::File::create(path).map(NoSpaceWriter),
            |writer| writer.0.sync_all(),
            |_| Ok(()),
        );
        assert!(
            matches!(result, Err(AppError::Storage(message)) if message.contains("磁盘空间不足"))
        );
        assert!(!part.exists());
        assert!(!destination.exists());
    }
    #[test]
    fn cancelled_transfer_cannot_be_resumed() {
        let manager = TransferManager::default();
        let token = manager.token("task", "download:/tmp/file").unwrap();
        assert!(manager.contains("task"));
        assert!(manager.token("other", "download:/tmp/file").is_err());
        assert!(manager.pause("task"));
        assert_eq!(token.load(Ordering::Acquire), 1);
        assert!(manager.resume("task"));
        assert_eq!(token.load(Ordering::Acquire), 0);
        assert!(manager.pause("task"));
        assert!(manager.cancel("task"));
        assert_eq!(token.load(Ordering::Acquire), 2);
        assert!(!manager.resume("task"));
        assert!(!manager.pause("task"));
        assert_eq!(token.load(Ordering::Acquire), 2);
        manager.finish("task");
        assert!(!manager.contains("task"));
        assert!(!manager.cancel("task"));
        assert!(manager.token("other", "download:/tmp/file").is_ok());
    }
    #[test]
    fn pre_cancelled_directory_transfer_stops_before_work() {
        let cancelled = AtomicBool::new(true);
        assert!(matches!(
            check_directory_transfer_cancelled(&cancelled),
            Err(AppError::Remote(message)) if message.contains("已取消")
        ));
    }
}
