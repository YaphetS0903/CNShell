use crate::xymodem::ModemProgress;
use serialport::SerialPort;
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const MAX_FILES: usize = 256;
const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024 * 1024;
const MAX_DIAGNOSTIC_BYTES: usize = 64 * 1024;

pub fn helper_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("CNSHELL_KERMIT_HELPER") {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    let mut candidates = Vec::new();
    if let Ok(executable) = std::env::current_exe()
        && let Some(path) = bundled_helper_path(&executable)
    {
        candidates.push(path);
    }
    let name = if cfg!(target_os = "windows") {
        "gkermit.exe"
    } else {
        "gkermit"
    };
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("kermit")
            .join(name),
    );
    candidates.into_iter().find(|path| path.is_file())
}

#[cfg(target_os = "macos")]
fn bundled_helper_path(executable: &Path) -> Option<PathBuf> {
    executable
        .parent()?
        .parent()
        .map(|contents| contents.join("Resources/kermit/gkermit"))
}

#[cfg(target_os = "windows")]
fn bundled_helper_path(executable: &Path) -> Option<PathBuf> {
    executable
        .parent()
        .map(|directory| directory.join("kermit").join("gkermit.exe"))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn bundled_helper_path(executable: &Path) -> Option<PathBuf> {
    executable
        .parent()
        .map(|directory| directory.join("kermit").join("gkermit"))
}

pub fn available() -> bool {
    helper_path().is_some_and(|path| {
        let mut command = Command::new(&path);
        command.arg("-h").env_clear();
        configure_helper_environment(&mut command, &path);
        command.output().is_ok_and(|output| {
            String::from_utf8_lossy(&output.stderr).contains("G-Kermit 2.01")
                || String::from_utf8_lossy(&output.stdout).contains("G-Kermit 2.01")
        })
    })
}

pub fn transfer<F>(
    port: Box<dyn SerialPort>,
    cancelled: Arc<AtomicBool>,
    direction: &str,
    paths: &[PathBuf],
    mut progress: F,
) -> io::Result<u64>
where
    F: FnMut(ModemProgress),
{
    let helper = helper_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "bundled G-Kermit helper is missing",
        )
    })?;
    let (working_directory, total_bytes, initial_name) = match direction {
        "upload" => {
            validate_uploads(paths)?;
            let total = paths.iter().try_fold(0_u64, |sum, path| {
                Ok::<_, io::Error>(sum.saturating_add(fs::metadata(path)?.len()))
            })?;
            (
                None,
                Some(total),
                paths
                    .first()
                    .and_then(|path| path.file_name())
                    .and_then(|name| name.to_str())
                    .unwrap_or("Kermit upload")
                    .to_string(),
            )
        }
        "download" => {
            if paths.len() != 1 || !paths[0].is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Kermit download requires one existing directory",
                ));
            }
            (Some(tempfile::tempdir()?), None, "Kermit download".into())
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsupported Kermit direction",
            ));
        }
    };
    progress(ModemProgress {
        file_name: initial_name,
        total_bytes,
        transferred_bytes: 0,
    });
    let mut command = Command::new(&helper);
    command
        .env_clear()
        .args(["-X", "-i", "-q", "-S", "-b", "5"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_helper_environment(&mut command, &helper);
    if direction == "upload" {
        command.arg("-s");
        command.args(paths);
    } else {
        command.arg("-r");
        command.current_dir(working_directory.as_ref().unwrap().path());
    }
    let mut child = command.spawn()?;
    let child_stdin = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("G-Kermit stdin is unavailable"))?;
    let child_stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("G-Kermit stdout is unavailable"))?;
    let child_stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("G-Kermit stderr is unavailable"))?;
    let serial_reader = port.try_clone()?;
    let done = Arc::new(AtomicBool::new(false));
    let bridge_error = Arc::new(parking_lot::Mutex::new(None::<String>));
    let to_child = spawn_serial_to_child(
        serial_reader,
        child_stdin,
        done.clone(),
        cancelled.clone(),
        bridge_error.clone(),
    )?;
    let from_child = spawn_child_to_serial(
        child_stdout,
        port,
        done.clone(),
        cancelled.clone(),
        bridge_error.clone(),
    )?;
    let diagnostics = std::thread::Builder::new()
        .name("cnshell-kermit-stderr".into())
        .spawn(move || bounded_diagnostics(child_stderr))?;
    let started = Instant::now();
    let status = loop {
        if cancelled.load(Ordering::Acquire) || bridge_error.lock().is_some() {
            let _ = child.kill();
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > Duration::from_secs(60 * 60) {
            let _ = child.kill();
            cancelled.store(true, Ordering::Release);
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    done.store(true, Ordering::Release);
    let _ = to_child.join();
    let _ = from_child.join();
    let diagnostic = diagnostics.join().unwrap_or_default();
    if cancelled.load(Ordering::Acquire) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Kermit transfer cancelled",
        ));
    }
    if let Some(error) = bridge_error.lock().take() {
        return Err(io::Error::new(io::ErrorKind::BrokenPipe, error));
    }
    if !status.success() {
        return Err(io::Error::other(if diagnostic.is_empty() {
            format!("G-Kermit exited with {status}")
        } else {
            format!("G-Kermit failed: {diagnostic}")
        }));
    }
    if direction == "upload" {
        let total = total_bytes.unwrap_or(0);
        progress(ModemProgress {
            file_name: "Kermit batch".into(),
            total_bytes: Some(total),
            transferred_bytes: total,
        });
        return Ok(total);
    }
    move_received_files(working_directory.unwrap().path(), &paths[0], progress)
}

#[cfg(target_os = "windows")]
fn configure_helper_environment(command: &mut Command, helper: &Path) {
    let system_root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    let mut paths = vec![
        helper
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
    ];
    paths.push(PathBuf::from(system_root).join("System32"));
    if let Ok(path) = std::env::join_paths(paths) {
        command.env("PATH", path);
    }
}

#[cfg(not(target_os = "windows"))]
fn configure_helper_environment(command: &mut Command, _helper: &Path) {
    command.env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
}

fn spawn_serial_to_child(
    mut serial: Box<dyn SerialPort>,
    mut child: std::process::ChildStdin,
    done: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
    error: Arc<parking_lot::Mutex<Option<String>>>,
) -> io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("cnshell-kermit-input".into())
        .spawn(move || {
            let mut buffer = [0_u8; 4096];
            while !done.load(Ordering::Acquire) && !cancelled.load(Ordering::Acquire) {
                match serial.read(&mut buffer) {
                    Ok(0) => continue,
                    Ok(count) => {
                        if let Err(write_error) = child.write_all(&buffer[..count]) {
                            if write_error.kind() != io::ErrorKind::BrokenPipe {
                                *error.lock() = Some(format!("写入 G-Kermit 失败：{write_error}"));
                            }
                            break;
                        }
                        let _ = child.flush();
                    }
                    Err(read_error) if read_error.kind() == io::ErrorKind::TimedOut => continue,
                    Err(read_error) => {
                        *error.lock() = Some(format!("读取串口失败：{read_error}"));
                        break;
                    }
                }
            }
        })
}

fn spawn_child_to_serial(
    mut child: std::process::ChildStdout,
    mut serial: Box<dyn SerialPort>,
    done: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
    error: Arc<parking_lot::Mutex<Option<String>>>,
) -> io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("cnshell-kermit-output".into())
        .spawn(move || {
            let mut buffer = [0_u8; 4096];
            while !done.load(Ordering::Acquire) && !cancelled.load(Ordering::Acquire) {
                match child.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        if let Err(write_error) = serial.write_all(&buffer[..count]) {
                            *error.lock() = Some(format!("写入串口失败：{write_error}"));
                            break;
                        }
                        let _ = serial.flush();
                    }
                    Err(read_error) => {
                        *error.lock() = Some(format!("读取 G-Kermit 失败：{read_error}"));
                        break;
                    }
                }
            }
        })
}

fn bounded_diagnostics(mut stderr: std::process::ChildStderr) -> String {
    let mut result = Vec::new();
    let mut buffer = [0_u8; 4096];
    while let Ok(count) = stderr.read(&mut buffer) {
        if count == 0 {
            break;
        }
        let remaining = MAX_DIAGNOSTIC_BYTES.saturating_sub(result.len());
        result.extend_from_slice(&buffer[..count.min(remaining)]);
    }
    String::from_utf8_lossy(&result).trim().to_string()
}

fn validate_uploads(paths: &[PathBuf]) -> io::Result<()> {
    if paths.is_empty() || paths.len() > MAX_FILES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Kermit requires between 1 and 256 files",
        ));
    }
    for path in paths {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > MAX_FILE_BYTES
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Kermit upload source must be a regular file no larger than 50 GB",
            ));
        }
    }
    Ok(())
}

fn move_received_files<F>(source: &Path, destination: &Path, mut progress: F) -> io::Result<u64>
where
    F: FnMut(ModemProgress),
{
    let mut entries = fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    if entries.is_empty() || entries.len() > MAX_FILES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "G-Kermit returned an empty or oversized batch",
        ));
    }
    let mut total = 0_u64;
    for entry in entries {
        let file_type = entry.file_type()?;
        let metadata = entry.metadata()?;
        if !file_type.is_file() || metadata.len() > MAX_FILE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "G-Kermit produced an unsupported output item",
            ));
        }
        let name = entry.file_name().into_string().map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "received filename is not UTF-8")
        })?;
        if name.chars().any(char::is_control) || name.contains(['/', '\\']) || name == ".." {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "received filename is unsafe",
            ));
        }
        let target = conflict_path(destination, &name)?;
        install_received_file(&entry.path(), &target)?;
        total = total.saturating_add(metadata.len());
        progress(ModemProgress {
            file_name: name,
            total_bytes: Some(metadata.len()),
            transferred_bytes: metadata.len(),
        });
    }
    Ok(total)
}

fn install_received_file(source: &Path, destination: &Path) -> io::Result<()> {
    let staging = destination.with_file_name(format!(
        ".{}.cnshell-{}.part",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("kermit"),
        uuid::Uuid::new_v4()
    ));
    let result = (|| {
        let mut input = fs::File::open(source)?;
        let mut output = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&staging)?;
        io::copy(&mut input, &mut output)?;
        output.sync_all()?;
        fs::rename(&staging, destination)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staging);
    }
    result
}

fn conflict_path(directory: &Path, name: &str) -> io::Result<PathBuf> {
    let candidate = directory.join(name);
    if !candidate.exists() {
        return Ok(candidate);
    }
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(name);
    let extension = path.extension().and_then(|value| value.to_str());
    for index in 1..=10_000 {
        let renamed = match extension {
            Some(extension) => format!("{stem} ({index}).{extension}"),
            None => format!("{stem} ({index})"),
        };
        let candidate = directory.join(renamed);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a conflict-free Kermit filename",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::ExitStatus;

    #[test]
    fn bundled_helper_reports_the_pinned_version_when_present() {
        let Some(helper) = helper_path() else {
            return;
        };
        let output = Command::new(helper).arg("-h").output().unwrap();
        assert!(String::from_utf8_lossy(&output.stderr).contains("G-Kermit 2.01"));
    }

    #[test]
    fn bundled_helpers_interoperate_in_external_protocol_mode() {
        let Some(helper) = helper_path() else {
            return;
        };
        let root = tempfile::tempdir().unwrap();
        let receive_dir = root.path().join("receive");
        fs::create_dir(&receive_dir).unwrap();
        let source = root.path().join("interop.bin");
        let bytes = (0..12_345)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        fs::write(&source, &bytes).unwrap();
        let mut receiver = Command::new(&helper)
            .args(["-X", "-r", "-i", "-q", "-S", "-b", "2"])
            .current_dir(&receive_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut sender = Command::new(&helper)
            .args(["-X", "-i", "-q", "-S", "-b", "2", "-s"])
            .arg(&source)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut sender_out = sender.stdout.take().unwrap();
        let mut sender_in = sender.stdin.take().unwrap();
        let mut receiver_out = receiver.stdout.take().unwrap();
        let mut receiver_in = receiver.stdin.take().unwrap();
        let forward = std::thread::spawn(move || io::copy(&mut sender_out, &mut receiver_in));
        let reverse = std::thread::spawn(move || io::copy(&mut receiver_out, &mut sender_in));
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut sender_status: Option<ExitStatus> = None;
        let mut receiver_status: Option<ExitStatus> = None;
        while sender_status.is_none() || receiver_status.is_none() {
            sender_status = sender_status.or_else(|| sender.try_wait().unwrap());
            receiver_status = receiver_status.or_else(|| receiver.try_wait().unwrap());
            if Instant::now() >= deadline {
                let _ = sender.kill();
                let _ = receiver.kill();
                panic!("G-Kermit interoperability test timed out");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        forward.join().unwrap().unwrap();
        reverse.join().unwrap().unwrap();
        assert!(sender_status.unwrap().success());
        assert!(receiver_status.unwrap().success());
        assert_eq!(fs::read(receive_dir.join("interop.bin")).unwrap(), bytes);
    }

    #[test]
    fn received_files_are_confined_and_conflicts_are_renamed() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("destination");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(source.join("FILE.BIN"), b"new").unwrap();
        fs::write(destination.join("FILE.BIN"), b"old").unwrap();
        assert_eq!(
            move_received_files(&source, &destination, |_| {}).unwrap(),
            3
        );
        assert_eq!(fs::read(destination.join("FILE.BIN")).unwrap(), b"old");
        assert_eq!(fs::read(destination.join("FILE (1).BIN")).unwrap(), b"new");
    }
}
