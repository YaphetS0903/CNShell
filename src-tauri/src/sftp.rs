use crate::{
    db::Database,
    error::{AppError, AppResult},
    models::{RemoteFile, TransferInput, TransferTask},
    ssh::SessionManager,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use ssh2::{FileStat, OpenFlags, OpenType, RenameFlags, Sftp};
#[cfg(unix)]
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::{
    io::{Read, Write},
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

const RAW_PATH_PREFIX: &str = "cnshell-raw-path:";
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
pub fn join_path(parent: &str, name: &str) -> AppResult<String> {
    if name.is_empty()
        || name.len() > 4096
        || name.contains(['\0', '/'])
        || name == "."
        || name == ".."
    {
        return Err(AppError::Validation("远端文件名无效".into()));
    }
    let mut path = remote_path(parent)?;
    path.push(name);
    Ok(wire_path(&path))
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
    let profile = manager.profile(&session_id)?;
    let mut transport = manager.acquire_transport(&db, &profile, true).await?;
    tokio::task::spawn_blocking(move || {
        let result = transport
            .connected()
            .session
            .sftp()
            .map_err(AppError::from)
            .and_then(operation);
        if result.is_err() {
            transport.discard();
        }
        result
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()))?
}

pub async fn list(
    db: Database,
    manager: SessionManager,
    session_id: String,
    path: String,
    show_hidden: bool,
) -> AppResult<Vec<RemoteFile>> {
    validate_remote_path(&path)?;
    with_sftp(db, manager, session_id, move |sftp| {
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
    })
    .await
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
        sftp.rename(
            &remote_path(&source)?,
            &remote_path(&destination)?,
            Some(RenameFlags::OVERWRITE | RenameFlags::ATOMIC),
        )?;
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
        if let Err(error) = sftp.rename(
            &temp,
            &target,
            Some(RenameFlags::OVERWRITE | RenameFlags::ATOMIC),
        ) {
            let _ = sftp.unlink(&temp);
            return Err(AppError::from(error));
        }
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
    let mut transport = manager.acquire_transport(&db, &profile, true).await?;
    tokio::task::spawn_blocking(move || {
        let result = (|| {
            let mut channel = transport.connected().session.channel_session()?;
            let command = if extract {
                let parent = Path::new(&path).parent().unwrap_or(Path::new("/"));
                format!(
                    "tar -xzf {} -C {}",
                    shell_quote(&path),
                    shell_quote(&parent.to_string_lossy())
                )
            } else {
                let source = Path::new(&path);
                let parent = source.parent().unwrap_or(Path::new("/"));
                let name = source
                    .file_name()
                    .and_then(|value| value.to_str())
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
) -> AppResult<String> {
    validate_remote_path(&path)?;
    let profile = manager.profile(&session_id)?;
    let mut transport = manager.acquire_transport(&db, &profile, true).await?;
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
            Command::new("open")
                .arg(&local)
                .spawn()
                .map_err(|error| AppError::Unavailable(format!("无法打开本地预览：{error}")))?;
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

pub async fn transfer_directory(
    db: Database,
    manager: SessionManager,
    session_id: String,
    direction: String,
    source: String,
    destination: String,
    conflict_policy: String,
    cancelled: Arc<AtomicBool>,
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
    if source.starts_with(RAW_PATH_PREFIX) || destination.starts_with(RAW_PATH_PREFIX) {
        return Err(AppError::Validation(
            "文件夹打包传输暂不支持非 UTF-8 路径".into(),
        ));
    }
    let profile = manager.profile(&session_id)?;
    let mut transport = manager.acquire_transport(&db, &profile, true).await?;
    tokio::task::spawn_blocking(move || {
        let identifier = Uuid::new_v4().to_string();
        let local_archive =
            std::env::temp_dir().join(format!("cnshell-directory-{identifier}.tar.gz"));
        let result = (|| {
            check_directory_transfer_cancelled(&cancelled)?;
            let sftp = transport.connected().session.sftp()?;
            if direction == "upload" {
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
                                final_destination = renamed_path(&original, index);
                                index += 1;
                            }
                        }
                        "overwrite" => overwrite_existing = true,
                        _ => unreachable!(),
                    }
                }
                let parent = Path::new(&final_destination)
                    .parent()
                    .unwrap_or(Path::new("/"));
                let remote_archive = parent.join(format!(".cnshell-directory-{identifier}.tar.gz"));
                let remote_staging = parent.join(format!(".cnshell-directory-{identifier}"));
                let remote_backup = parent.join(format!(".cnshell-directory-backup-{identifier}"));
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
                let source_path = remote_path(&source)?;
                if !sftp.lstat(&source_path)?.is_dir() {
                    return Err(AppError::Validation("下载源必须是远端文件夹".into()));
                }
                let remote_parent = source_path.parent().unwrap_or(Path::new("/"));
                let remote_archive =
                    remote_parent.join(format!(".cnshell-directory-{identifier}.tar.gz"));
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
    let compound = [".tar.gz", ".tar.bz2", ".tar.xz"]
        .into_iter()
        .find(|extension| filename.ends_with(extension));
    let (name, extension) = if let Some(extension) = compound {
        (&filename[..filename.len() - extension.len()], extension)
    } else if let Some(extension) = source.extension().and_then(|value| value.to_str()) {
        (
            &filename[..filename.len() - extension.len() - 1],
            &filename[filename.len() - extension.len() - 1..],
        )
    } else {
        (filename, "")
    };
    parent
        .join(format!("{name} ({index}){extension}"))
        .to_string_lossy()
        .into_owned()
}

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
        validate_local_path(&input.source)?;
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
        let transport = match manager.acquire_transport(&db, &profile, true).await {
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
            let mut transport=transport; let result=(|| -> AppResult<TransferTask> { let sftp=transport.connected().session.sftp()?; let mut buffer=vec![0_u8;256*1024];
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
                if sftp.stat(Path::new(&work_task.destination)).is_ok(){match work_task.conflict_policy.as_str(){"skip"=>{work_task.status="completed".into();return Ok(work_task);},"rename"=>{let original=work_task.destination.clone();let mut index=1;while sftp.stat(Path::new(&work_task.destination)).is_ok(){work_task.destination=renamed_path(&original,index);index+=1;}},"overwrite"=>{},_=>return Err(AppError::Validation("远端目标已存在，请选择覆盖、跳过或重命名".into()))}}
                let mut local=std::fs::File::open(&work_task.source)?; work_task.total_bytes=local.metadata()?.len() as i64;
                let temporary=PathBuf::from(format!("{}.cnshell-part-{}",work_task.destination,work_task.id));
                let upload=(||->AppResult<()>{
                    let mut remote=sftp.open_mode(&temporary,OpenFlags::WRITE|OpenFlags::CREATE|OpenFlags::TRUNCATE,0o600,OpenType::File)?;
                    loop { while token_clone.load(Ordering::Relaxed)==1{work_task.status="paused".into();let _=app_clone.emit("transfer-progress",work_task.clone());std::thread::sleep(Duration::from_millis(100));}work_task.status="running".into();if token_clone.load(Ordering::Relaxed)==2{return Err(AppError::Remote("传输已取消".into()));} let read=local.read(&mut buffer)?;if read==0{break;}remote.write_all(&buffer[..read])?;work_task.transferred_bytes+=read as i64;let _=app_clone.emit("transfer-progress",work_task.clone()); }
                    remote.fsync()?;drop(remote);
                    let actual=sftp.stat(&temporary)?.size.unwrap_or(0)as i64;if work_task.transferred_bytes!=work_task.total_bytes||actual!=work_task.total_bytes{return Err(AppError::Remote(format!("上传大小校验失败：预期 {} 字节，本地已发送 {} 字节，远端临时文件 {} 字节",work_task.total_bytes,work_task.transferred_bytes,actual)));}
                    sftp.rename(&temporary,Path::new(&work_task.destination),Some(RenameFlags::OVERWRITE|RenameFlags::ATOMIC))?;Ok(())
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
        assert_eq!(renamed_path("/tmp/file.txt", 2), "/tmp/file (2).txt");
        assert_eq!(
            renamed_path("/tmp/archive.tar.gz", 1),
            "/tmp/archive (1).tar.gz"
        );
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
        assert!(validate_local_path("/tmp/file").is_ok());
        assert!(validate_local_path("relative").is_err());
    }
    #[test]
    fn remote_root_cannot_be_deleted() {
        for path in ["/", "/.", "/.."] {
            assert!(validate_deletable_path(path).is_err());
        }
        assert!(validate_deletable_path("/tmp/file").is_ok());
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
