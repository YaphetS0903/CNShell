use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const SOH: u8 = 0x01;
const STX: u8 = 0x02;
const EOT: u8 = 0x04;
const ACK: u8 = 0x06;
const NAK: u8 = 0x15;
const CAN: u8 = 0x18;
const CRC_REQUEST: u8 = b'C';
const PAD: u8 = 0x1a;
const MAX_RETRIES: usize = 16;
const MAX_FILES: usize = 256;
const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmodemChecksum {
    Checksum,
    Crc16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModemProgress {
    pub file_name: String,
    pub total_bytes: Option<u64>,
    pub transferred_bytes: u64,
}

pub fn xmodem_send<D, F>(
    device: &mut D,
    path: &Path,
    use_1k: bool,
    mut progress: F,
) -> io::Result<u64>
where
    D: Read + Write,
    F: FnMut(ModemProgress),
{
    let mut source = File::open(path)?;
    let metadata = source.metadata()?;
    validate_upload(path, &metadata)?;
    let total = metadata.len();
    let name = local_file_name(path)?;
    let checksum = wait_for_receiver(device)?;
    let sent = send_data_blocks(device, &mut source, total, use_1k, checksum, |bytes| {
        progress(ModemProgress {
            file_name: name.clone(),
            total_bytes: Some(total),
            transferred_bytes: bytes,
        });
    })?;
    finish_xmodem_send(device)?;
    Ok(sent)
}

pub fn xmodem_receive<D, F>(
    device: &mut D,
    destination: &Path,
    checksum: XmodemChecksum,
    mut progress: F,
) -> io::Result<u64>
where
    D: Read + Write,
    F: FnMut(ModemProgress),
{
    validate_xmodem_destination(destination)?;
    let temporary = temporary_path(destination);
    let result = (|| {
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        write_control(
            device,
            if checksum == XmodemChecksum::Crc16 {
                CRC_REQUEST
            } else {
                NAK
            },
        )?;
        let name = destination
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("download")
            .to_string();
        let received = receive_data_blocks(device, &mut output, None, checksum, false, |bytes| {
            progress(ModemProgress {
                file_name: name.clone(),
                total_bytes: None,
                transferred_bytes: bytes,
            });
        })?;
        output.sync_all()?;
        atomic_replace(&temporary, destination)?;
        Ok(received)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn ymodem_send<D, F>(device: &mut D, paths: &[PathBuf], mut progress: F) -> io::Result<u64>
where
    D: Read + Write,
    F: FnMut(ModemProgress),
{
    if paths.is_empty() || paths.len() > MAX_FILES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Ymodem requires between 1 and 256 files",
        ));
    }
    let mut total_sent = 0_u64;
    wait_for_crc(device)?;
    for path in paths {
        let mut source = File::open(path)?;
        let metadata = source.metadata()?;
        validate_upload(path, &metadata)?;
        let size = metadata.len();
        let name = local_file_name(path)?;
        let header = ymodem_header(&name, size)?;
        send_block(device, 0, &header, XmodemChecksum::Crc16)?;
        wait_for_crc(device)?;
        let sent = send_data_blocks(
            device,
            &mut source,
            size,
            true,
            XmodemChecksum::Crc16,
            |bytes| {
                progress(ModemProgress {
                    file_name: name.clone(),
                    total_bytes: Some(size),
                    transferred_bytes: bytes,
                });
            },
        )?;
        finish_ymodem_file_send(device)?;
        total_sent = total_sent.saturating_add(sent);
        wait_for_crc(device)?;
    }
    send_block(device, 0, &[0; 128], XmodemChecksum::Crc16)?;
    Ok(total_sent)
}

pub fn ymodem_receive<D, F>(device: &mut D, destination: &Path, mut progress: F) -> io::Result<u64>
where
    D: Read + Write,
    F: FnMut(ModemProgress),
{
    if !destination.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Ymodem destination must be an existing directory",
        ));
    }
    write_control(device, CRC_REQUEST)?;
    let mut total_received = 0_u64;
    let mut files = 0_usize;
    loop {
        let header = receive_header(device)?;
        let Some((name, size)) = parse_ymodem_header(&header)? else {
            write_control(device, ACK)?;
            return Ok(total_received);
        };
        files += 1;
        if files > MAX_FILES {
            cancel_peer(device);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Ymodem batch contains too many files",
            ));
        }
        let final_path = conflict_path(destination, &name)?;
        let temporary = temporary_path(&final_path);
        let file_result = (|| {
            let mut output = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            write_control(device, ACK)?;
            write_control(device, CRC_REQUEST)?;
            let received = receive_data_blocks(
                device,
                &mut output,
                Some(size),
                XmodemChecksum::Crc16,
                true,
                |bytes| {
                    progress(ModemProgress {
                        file_name: name.clone(),
                        total_bytes: Some(size),
                        transferred_bytes: bytes,
                    });
                },
            )?;
            output.sync_all()?;
            atomic_replace(&temporary, &final_path)?;
            Ok(received)
        })();
        if file_result.is_err() {
            let _ = fs::remove_file(&temporary);
            return file_result.map(|_| total_received);
        }
        total_received = total_received.saturating_add(file_result?);
    }
}

fn send_data_blocks<D, R, F>(
    device: &mut D,
    source: &mut R,
    total: u64,
    use_1k: bool,
    checksum: XmodemChecksum,
    mut progress: F,
) -> io::Result<u64>
where
    D: Read + Write,
    R: Read,
    F: FnMut(u64),
{
    let block_size = if use_1k { 1024 } else { 128 };
    let mut block_number = 1_u8;
    let mut sent = 0_u64;
    loop {
        let mut data = vec![PAD; block_size];
        let count = read_up_to(source, &mut data)?;
        if count == 0 {
            break;
        }
        send_block(device, block_number, &data, checksum)?;
        sent = sent.saturating_add(count as u64).min(total);
        progress(sent);
        block_number = block_number.wrapping_add(1);
    }
    Ok(sent)
}

fn receive_data_blocks<D, W, F>(
    device: &mut D,
    output: &mut W,
    expected_size: Option<u64>,
    checksum: XmodemChecksum,
    ymodem_eot: bool,
    mut progress: F,
) -> io::Result<u64>
where
    D: Read + Write,
    W: Write,
    F: FnMut(u64),
{
    let mut expected_block = 1_u8;
    let mut received = 0_u64;
    let mut errors = 0_usize;
    loop {
        match read_byte(device) {
            Ok(EOT) => {
                if ymodem_eot {
                    write_control(device, NAK)?;
                    if wait_for_control(device, &[EOT])? != EOT {
                        unreachable!();
                    }
                }
                write_control(device, ACK)?;
                if ymodem_eot {
                    write_control(device, CRC_REQUEST)?;
                }
                return Ok(received);
            }
            Ok(CAN) => {
                if matches!(read_byte(device), Ok(CAN)) {
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionAborted,
                        "transfer cancelled by peer",
                    ));
                }
            }
            Ok(start @ (SOH | STX)) => match receive_block(device, start, checksum) {
                Ok((number, data)) if number == expected_block => {
                    if expected_size.is_none()
                        && received.saturating_add(data.len() as u64) > MAX_FILE_BYTES
                    {
                        cancel_peer(device);
                        return Err(io::Error::new(
                            io::ErrorKind::FileTooLarge,
                            "Xmodem download exceeds 50 GB",
                        ));
                    }
                    let remaining = expected_size
                        .map(|size| size.saturating_sub(received))
                        .unwrap_or(data.len() as u64);
                    let write_len =
                        usize::try_from(remaining.min(data.len() as u64)).unwrap_or(data.len());
                    output.write_all(&data[..write_len])?;
                    received = received.saturating_add(write_len as u64);
                    expected_block = expected_block.wrapping_add(1);
                    errors = 0;
                    write_control(device, ACK)?;
                    progress(received);
                }
                Ok((number, _)) if number == expected_block.wrapping_sub(1) => {
                    write_control(device, ACK)?;
                }
                Ok(_) | Err(_) => {
                    errors += 1;
                    write_control(device, NAK)?;
                }
            },
            Ok(_) | Err(_) => {
                errors += 1;
                write_control(device, NAK)?;
            }
        }
        if errors >= MAX_RETRIES {
            cancel_peer(device);
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "too many receive retries",
            ));
        }
    }
}

fn receive_header<D: Read + Write>(device: &mut D) -> io::Result<Vec<u8>> {
    for _ in 0..MAX_RETRIES {
        match read_byte(device) {
            Ok(start @ (SOH | STX)) => match receive_block(device, start, XmodemChecksum::Crc16) {
                Ok((0, data)) => return Ok(data),
                _ => write_control(device, NAK)?,
            },
            Ok(CAN) if matches!(read_byte(device), Ok(CAN)) => {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "transfer cancelled by peer",
                ));
            }
            _ => write_control(device, CRC_REQUEST)?,
        }
    }
    cancel_peer(device);
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "Ymodem header was not received",
    ))
}

fn send_block<D: Read + Write>(
    device: &mut D,
    number: u8,
    data: &[u8],
    checksum: XmodemChecksum,
) -> io::Result<()> {
    if !matches!(data.len(), 128 | 1024) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid modem block size",
        ));
    }
    let start = if data.len() == 1024 { STX } else { SOH };
    for _ in 0..MAX_RETRIES {
        device.write_all(&[start, number, !number])?;
        device.write_all(data)?;
        match checksum {
            XmodemChecksum::Checksum => {
                device.write_all(&[data.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte))])?;
            }
            XmodemChecksum::Crc16 => device.write_all(&crc16(data).to_be_bytes())?,
        }
        device.flush()?;
        match read_byte(device) {
            Ok(ACK) => return Ok(()),
            Ok(CAN) if matches!(read_byte(device), Ok(CAN)) => {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "transfer cancelled by peer",
                ));
            }
            _ => continue,
        }
    }
    cancel_peer(device);
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "block was not acknowledged",
    ))
}

fn receive_block<D: Read + Write>(
    device: &mut D,
    start: u8,
    checksum: XmodemChecksum,
) -> io::Result<(u8, Vec<u8>)> {
    let number = read_byte(device)?;
    let complement = read_byte(device)?;
    if number != !complement {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "block number complement mismatch",
        ));
    }
    let mut data = vec![0_u8; if start == STX { 1024 } else { 128 }];
    device.read_exact(&mut data)?;
    let valid = match checksum {
        XmodemChecksum::Checksum => {
            read_byte(device)? == data.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte))
        }
        XmodemChecksum::Crc16 => {
            let received = u16::from_be_bytes([read_byte(device)?, read_byte(device)?]);
            received == crc16(&data)
        }
    };
    if !valid {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "block checksum mismatch",
        ));
    }
    Ok((number, data))
}

fn finish_xmodem_send<D: Read + Write>(device: &mut D) -> io::Result<()> {
    for _ in 0..MAX_RETRIES {
        write_control(device, EOT)?;
        match read_byte(device) {
            Ok(ACK) => return Ok(()),
            Ok(CAN) if matches!(read_byte(device), Ok(CAN)) => {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "cancelled",
                ));
            }
            _ => continue,
        }
    }
    cancel_peer(device);
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "Xmodem EOT was not acknowledged",
    ))
}

fn finish_ymodem_file_send<D: Read + Write>(device: &mut D) -> io::Result<()> {
    let mut received_nak = false;
    for _ in 0..MAX_RETRIES {
        write_control(device, EOT)?;
        if matches!(read_byte(device), Ok(NAK)) {
            received_nak = true;
            break;
        }
    }
    if !received_nak {
        cancel_peer(device);
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "Ymodem first EOT was not rejected",
        ));
    }
    for _ in 0..MAX_RETRIES {
        write_control(device, EOT)?;
        if matches!(read_byte(device), Ok(ACK)) {
            return Ok(());
        }
    }
    cancel_peer(device);
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "Ymodem EOT handshake failed",
    ))
}

fn wait_for_receiver<D: Read + Write>(device: &mut D) -> io::Result<XmodemChecksum> {
    match wait_for_control(device, &[CRC_REQUEST, NAK])? {
        CRC_REQUEST => Ok(XmodemChecksum::Crc16),
        NAK => Ok(XmodemChecksum::Checksum),
        _ => unreachable!(),
    }
}

fn wait_for_crc<D: Read + Write>(device: &mut D) -> io::Result<()> {
    wait_for_control(device, &[CRC_REQUEST]).map(|_| ())
}

fn wait_for_control<D: Read + Write>(device: &mut D, accepted: &[u8]) -> io::Result<u8> {
    for _ in 0..MAX_RETRIES {
        match read_byte(device) {
            Ok(value) if accepted.contains(&value) => return Ok(value),
            Ok(CAN) if matches!(read_byte(device), Ok(CAN)) => {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "transfer cancelled by peer",
                ));
            }
            _ => continue,
        }
    }
    cancel_peer(device);
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "peer did not enter modem transfer mode",
    ))
}

fn read_byte<D: Read>(device: &mut D) -> io::Result<u8> {
    let mut byte = [0_u8; 1];
    device.read_exact(&mut byte)?;
    Ok(byte[0])
}

fn write_control<D: Write>(device: &mut D, value: u8) -> io::Result<()> {
    device.write_all(&[value])?;
    device.flush()
}

pub fn cancel_peer<D: Write>(device: &mut D) {
    let _ = device.write_all(&[CAN, CAN]);
    let _ = device.flush();
}

fn read_up_to<R: Read>(source: &mut R, buffer: &mut [u8]) -> io::Result<usize> {
    let mut total = 0;
    while total < buffer.len() {
        match source.read(&mut buffer[total..]) {
            Ok(0) => break,
            Ok(count) => total += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(total)
}

fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0_u16;
    for byte in data {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn ymodem_header(name: &str, size: u64) -> io::Result<[u8; 128]> {
    if name.is_empty() || name.len() > 100 || name.contains(['/', '\\', '\0']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Ymodem filename is invalid or too long",
        ));
    }
    let mut header = [0_u8; 128];
    header[..name.len()].copy_from_slice(name.as_bytes());
    let size = size.to_string();
    let start = name.len() + 1;
    if start + size.len() >= header.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Ymodem metadata is too long",
        ));
    }
    header[start..start + size.len()].copy_from_slice(size.as_bytes());
    Ok(header)
}

fn parse_ymodem_header(header: &[u8]) -> io::Result<Option<(String, u64)>> {
    let name_end = header.iter().position(|value| *value == 0).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Ymodem filename is not terminated",
        )
    })?;
    if name_end == 0 {
        return Ok(None);
    }
    let name = std::str::from_utf8(&header[..name_end])
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "filename is not UTF-8"))?;
    if name.len() > 255
        || name == "."
        || name == ".."
        || name.chars().any(char::is_control)
        || name.contains(['/', '\\'])
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsafe Ymodem filename",
        ));
    }
    let metadata = &header[name_end + 1..];
    let size_end = metadata
        .iter()
        .position(|value| *value == 0 || *value == b' ')
        .unwrap_or(metadata.len());
    if size_end == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Ymodem file size is missing",
        ));
    }
    let size = std::str::from_utf8(&metadata[..size_end])
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value <= MAX_FILE_BYTES)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid Ymodem file size"))?;
    Ok(Some((name.to_string(), size)))
}

fn validate_upload(path: &Path, metadata: &fs::Metadata) -> io::Result<()> {
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "transfer source must be a regular file no larger than 50 GB",
        ));
    }
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "symbolic links are not accepted as modem upload sources",
        ));
    }
    Ok(())
}

fn validate_xmodem_destination(path: &Path) -> io::Result<()> {
    if !path.is_absolute() || path.file_name().is_none() || path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Xmodem destination must be an absolute file path",
        ));
    }
    if !path.parent().is_some_and(Path::is_dir) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Xmodem destination directory does not exist",
        ));
    }
    Ok(())
}

fn local_file_name(path: &Path) -> io::Result<String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "filename must be valid UTF-8")
        })?;
    if name.chars().any(char::is_control) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "filename contains control characters",
        ));
    }
    Ok(name.to_string())
}

fn temporary_path(destination: &Path) -> PathBuf {
    destination.with_file_name(format!(
        ".{}.cnshell-{}.part",
        destination
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("download"),
        uuid::Uuid::new_v4()
    ))
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
        "could not allocate a conflict-free filename",
    ))
}

fn atomic_replace(temporary: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(temporary, destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Cursor,
        net::{TcpListener, TcpStream},
        thread,
        time::Duration,
    };

    struct ScriptedDevice {
        input: Cursor<Vec<u8>>,
        output: Vec<u8>,
    }

    impl Read for ScriptedDevice {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.input.read(buffer)
        }
    }

    impl Write for ScriptedDevice {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.output.extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn duplex() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let left = TcpStream::connect(address).unwrap();
        let (right, _) = listener.accept().unwrap();
        left.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        right
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        (left, right)
    }

    #[test]
    fn xmodem_crc_and_one_k_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.bin");
        let destination = directory.path().join("destination.bin");
        let bytes = (0..3_333)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        fs::write(&source, &bytes).unwrap();
        let (mut sender, mut receiver) = duplex();
        let send_path = source.clone();
        let send = thread::spawn(move || xmodem_send(&mut sender, &send_path, true, |_| {}));
        let received =
            xmodem_receive(&mut receiver, &destination, XmodemChecksum::Crc16, |_| {}).unwrap();
        assert_eq!(send.join().unwrap().unwrap(), bytes.len() as u64);
        assert_eq!(received, 4_096);
        let downloaded = fs::read(destination).unwrap();
        assert_eq!(&downloaded[..bytes.len()], bytes);
        assert!(downloaded[bytes.len()..].iter().all(|byte| *byte == PAD));
    }

    #[test]
    fn xmodem_legacy_checksum_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("legacy.bin");
        let destination = directory.path().join("legacy-download.bin");
        let bytes = (0..257)
            .map(|index| (index % 239) as u8)
            .collect::<Vec<_>>();
        fs::write(&source, &bytes).unwrap();
        let (mut sender, mut receiver) = duplex();
        let send = thread::spawn(move || xmodem_send(&mut sender, &source, false, |_| {}));
        assert_eq!(
            xmodem_receive(
                &mut receiver,
                &destination,
                XmodemChecksum::Checksum,
                |_| {}
            )
            .unwrap(),
            384
        );
        assert_eq!(send.join().unwrap().unwrap(), bytes.len() as u64);
        assert_eq!(&fs::read(destination).unwrap()[..bytes.len()], bytes);
    }

    #[test]
    fn ymodem_batch_round_trip_preserves_sizes_and_renames_conflicts() {
        let directory = tempfile::tempdir().unwrap();
        let upload = directory.path().join("upload");
        let download = directory.path().join("download");
        fs::create_dir(&upload).unwrap();
        fs::create_dir(&download).unwrap();
        let first = upload.join("alpha.txt");
        let second = upload.join("beta.bin");
        fs::write(&first, b"alpha").unwrap();
        fs::write(&second, vec![7_u8; 2_049]).unwrap();
        fs::write(download.join("alpha.txt"), b"existing").unwrap();
        let (mut sender, mut receiver) = duplex();
        let paths = vec![first.clone(), second.clone()];
        let send = thread::spawn(move || ymodem_send(&mut sender, &paths, |_| {}));
        assert_eq!(
            ymodem_receive(&mut receiver, &download, |_| {}).unwrap(),
            2_054
        );
        assert_eq!(send.join().unwrap().unwrap(), 2_054);
        assert_eq!(fs::read(download.join("alpha.txt")).unwrap(), b"existing");
        assert_eq!(fs::read(download.join("alpha (1).txt")).unwrap(), b"alpha");
        assert_eq!(
            fs::read(download.join("beta.bin")).unwrap(),
            vec![7_u8; 2_049]
        );
    }

    #[test]
    fn ymodem_header_rejects_traversal_and_oversized_files() {
        let mut header = [0_u8; 128];
        let unsafe_name = b"../escape\0";
        header[..unsafe_name.len()].copy_from_slice(unsafe_name);
        header[unsafe_name.len()] = b'1';
        assert!(parse_ymodem_header(&header).is_err());
        let header = ymodem_header("safe.bin", MAX_FILE_BYTES + 1).unwrap();
        assert!(parse_ymodem_header(&header).is_err());
    }

    #[test]
    fn crc16_matches_xmodem_reference_vector() {
        assert_eq!(crc16(b"123456789"), 0x31c3);
    }

    #[test]
    fn bad_crc_then_peer_cancel_removes_partial_download() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("broken.bin");
        let mut input = vec![SOH, 1, !1];
        input.extend([9_u8; 128]);
        input.extend([0_u8, 0_u8]);
        input.extend([CAN, CAN]);
        let mut device = ScriptedDevice {
            input: Cursor::new(input),
            output: Vec::new(),
        };
        assert!(matches!(
            xmodem_receive(
                &mut device,
                &destination,
                XmodemChecksum::Crc16,
                |_| {}
            ),
            Err(error) if error.kind() == io::ErrorKind::ConnectionAborted
        ));
        assert!(!destination.exists());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
        assert_eq!(device.output, vec![CRC_REQUEST, NAK]);
    }

    #[test]
    fn duplicate_data_block_is_acknowledged_without_being_written_twice() {
        let data = [4_u8; 128];
        let mut frame = vec![SOH, 1, !1];
        frame.extend(data);
        frame.extend(crc16(&data).to_be_bytes());
        let mut input = frame.clone();
        input.extend(frame);
        input.push(EOT);
        let mut device = ScriptedDevice {
            input: Cursor::new(input),
            output: Vec::new(),
        };
        let mut output = Vec::new();
        assert_eq!(
            receive_data_blocks(
                &mut device,
                &mut output,
                None,
                XmodemChecksum::Crc16,
                false,
                |_| {}
            )
            .unwrap(),
            128
        );
        assert_eq!(output, data);
        assert_eq!(device.output, vec![ACK, ACK, ACK]);
    }
}
