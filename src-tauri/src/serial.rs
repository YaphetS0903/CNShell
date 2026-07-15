use crate::{
    error::{AppError, AppResult},
    models::{
        ConnectionProfile, SerialConnectionOptions, SerialDeviceInfo, TerminalOutput,
        TerminalSession, TerminalStatus,
    },
    session_log::SessionLogManager,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use parking_lot::Mutex;
use serialport::{DataBits, FlowControl, Parity, SerialPort, SerialPortType, StopBits};
use std::{
    collections::{BTreeMap, HashMap},
    io::{Read, Write},
    sync::Arc,
    time::Duration,
};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

const RECONNECT_DELAYS: [u64; 5] = [1, 2, 5, 10, 30];
const DATA_BITS_KEY: &str = "CNSHELL_SERIAL_DATA_BITS";
const PARITY_KEY: &str = "CNSHELL_SERIAL_PARITY";
const STOP_BITS_KEY: &str = "CNSHELL_SERIAL_STOP_BITS";
const FLOW_CONTROL_KEY: &str = "CNSHELL_SERIAL_FLOW_CONTROL";
const DTR_KEY: &str = "CNSHELL_SERIAL_DTR";
const RTS_KEY: &str = "CNSHELL_SERIAL_RTS";
const OPTION_KEYS: [&str; 6] = [
    DATA_BITS_KEY,
    PARITY_KEY,
    STOP_BITS_KEY,
    FLOW_CONTROL_KEY,
    DTR_KEY,
    RTS_KEY,
];

struct ManagedSerial {
    port: Option<Box<dyn SerialPort>>,
}

#[derive(Clone, Default)]
pub struct SerialManager {
    sessions: Arc<Mutex<HashMap<String, Arc<Mutex<ManagedSerial>>>>>,
    closing: Arc<Mutex<std::collections::HashSet<String>>>,
}

impl SerialManager {
    pub fn contains(&self, id: &str) -> bool {
        self.sessions.lock().contains_key(id)
    }

    pub fn open(
        &self,
        app: AppHandle,
        profile: ConnectionProfile,
        options: SerialConnectionOptions,
        logs: SessionLogManager,
    ) -> AppResult<TerminalSession> {
        validate_options(&options)?;
        let (port, reader) = open_port(&profile, &options)?;
        let id = Uuid::new_v4().to_string();
        let managed = Arc::new(Mutex::new(ManagedSerial { port: Some(port) }));
        self.sessions.lock().insert(id.clone(), managed.clone());
        spawn_reader(
            app,
            self.clone(),
            logs,
            id.clone(),
            profile.clone(),
            options,
            managed,
            reader,
        );
        Ok(TerminalSession {
            id,
            connection_id: profile.id,
            session_type: "serial".into(),
            title: format!("{} · Serial", profile.name),
            status: "online".into(),
            started_at: chrono::Utc::now().to_rfc3339(),
            last_error: None,
        })
    }

    pub fn input(&self, id: &str, data: &str) -> AppResult<()> {
        if data.len() > 1024 * 1024 {
            return Err(AppError::Validation("单次终端输入不能超过 1 MB".into()));
        }
        let session = self
            .sessions
            .lock()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("Serial 会话 {id}")))?;
        let mut session = session.lock();
        let port = session
            .port
            .as_mut()
            .ok_or_else(|| AppError::Unavailable("串口设备已断开，正在等待重新接入".into()))?;
        port.write_all(data.as_bytes())
            .map_err(|error| AppError::Unavailable(format!("写入串口失败：{error}")))?;
        port.flush()
            .map_err(|error| AppError::Unavailable(format!("刷新串口输出失败：{error}")))
    }

    pub fn resize(&self, id: &str, cols: u32, rows: u32) -> AppResult<()> {
        if !(1..=1000).contains(&cols) || !(1..=500).contains(&rows) {
            return Err(AppError::Validation("终端尺寸超出允许范围".into()));
        }
        if !self.contains(id) {
            return Err(AppError::NotFound(format!("Serial 会话 {id}")));
        }
        Ok(())
    }

    pub fn close(&self, id: &str) -> AppResult<()> {
        let session = self
            .sessions
            .lock()
            .remove(id)
            .ok_or_else(|| AppError::NotFound(format!("Serial 会话 {id}")))?;
        self.closing.lock().insert(id.into());
        session.lock().port.take();
        Ok(())
    }

    pub fn close_all(&self) {
        for id in self.sessions.lock().keys().cloned().collect::<Vec<_>>() {
            let _ = self.close(&id);
        }
    }
}

pub fn default_options(connection_id: String) -> SerialConnectionOptions {
    SerialConnectionOptions {
        connection_id,
        data_bits: 8,
        parity: "none".into(),
        stop_bits: 1,
        flow_control: "none".into(),
        dtr: true,
        rts: true,
    }
}

pub fn has_persisted_options(profile: &ConnectionProfile) -> bool {
    OPTION_KEYS
        .iter()
        .any(|key| profile.environment.contains_key(*key))
}

pub fn is_option_key(key: &str) -> bool {
    OPTION_KEYS.contains(&key)
}

pub fn options_from_profile(profile: &ConnectionProfile) -> AppResult<SerialConnectionOptions> {
    let mut options = default_options(profile.id.clone());
    if let Some(value) = profile.environment.get(DATA_BITS_KEY) {
        options.data_bits = value
            .parse()
            .map_err(|_| AppError::Validation("保存的串口数据位无效".into()))?;
    }
    if let Some(value) = profile.environment.get(PARITY_KEY) {
        options.parity = value.clone();
    }
    if let Some(value) = profile.environment.get(STOP_BITS_KEY) {
        options.stop_bits = value
            .parse()
            .map_err(|_| AppError::Validation("保存的串口停止位无效".into()))?;
    }
    if let Some(value) = profile.environment.get(FLOW_CONTROL_KEY) {
        options.flow_control = value.clone();
    }
    if let Some(value) = profile.environment.get(DTR_KEY) {
        options.dtr = parse_bool(value, "DTR")?;
    }
    if let Some(value) = profile.environment.get(RTS_KEY) {
        options.rts = parse_bool(value, "RTS")?;
    }
    validate_options(&options)?;
    Ok(options)
}

pub fn environment_with_options(
    current: &BTreeMap<String, String>,
    options: &SerialConnectionOptions,
) -> AppResult<BTreeMap<String, String>> {
    validate_options(options)?;
    let mut environment = current.clone();
    environment.insert(DATA_BITS_KEY.into(), options.data_bits.to_string());
    environment.insert(PARITY_KEY.into(), options.parity.clone());
    environment.insert(STOP_BITS_KEY.into(), options.stop_bits.to_string());
    environment.insert(FLOW_CONTROL_KEY.into(), options.flow_control.clone());
    environment.insert(DTR_KEY.into(), options.dtr.to_string());
    environment.insert(RTS_KEY.into(), options.rts.to_string());
    Ok(environment)
}

fn parse_bool(value: &str, name: &str) -> AppResult<bool> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(AppError::Validation(format!("保存的串口 {name} 参数无效"))),
    }
}

pub fn validate_options(options: &SerialConnectionOptions) -> AppResult<()> {
    if options.connection_id.is_empty()
        || options.connection_id.len() > 128
        || !(5..=8).contains(&options.data_bits)
        || !["none", "odd", "even"].contains(&options.parity.as_str())
        || ![1, 2].contains(&options.stop_bits)
        || !["none", "software", "hardware"].contains(&options.flow_control.as_str())
    {
        return Err(AppError::Validation("串口参数无效".into()));
    }
    Ok(())
}

pub fn devices() -> AppResult<Vec<SerialDeviceInfo>> {
    let mut devices = serialport::available_ports()
        .map_err(|error| AppError::Unavailable(format!("枚举串口设备失败：{error}")))?
        .into_iter()
        .filter(|port| enumerable_path(&port.port_name) && port.port_name.len() <= 16 * 1024)
        .map(|port| match port.port_type {
            SerialPortType::UsbPort(info) => {
                let product = bounded(info.product, 256);
                let manufacturer = bounded(info.manufacturer, 256);
                let serial_number = bounded(info.serial_number, 256);
                let label = product
                    .clone()
                    .or_else(|| manufacturer.clone())
                    .unwrap_or_else(|| "USB Serial".into());
                SerialDeviceInfo {
                    path: port.port_name,
                    kind: "usb".into(),
                    label,
                    vendor_id: Some(info.vid),
                    product_id: Some(info.pid),
                    serial_number,
                    manufacturer,
                    product,
                }
            }
            SerialPortType::BluetoothPort => SerialDeviceInfo {
                path: port.port_name,
                kind: "bluetooth".into(),
                label: "Bluetooth Serial".into(),
                vendor_id: None,
                product_id: None,
                serial_number: None,
                manufacturer: None,
                product: None,
            },
            SerialPortType::PciPort => SerialDeviceInfo {
                path: port.port_name,
                kind: "pci".into(),
                label: "PCI Serial".into(),
                vendor_id: None,
                product_id: None,
                serial_number: None,
                manufacturer: None,
                product: None,
            },
            SerialPortType::Unknown => SerialDeviceInfo {
                path: port.port_name,
                kind: "unknown".into(),
                label: "Serial Device".into(),
                vendor_id: None,
                product_id: None,
                serial_number: None,
                manufacturer: None,
                product: None,
            },
        })
        .collect::<Vec<_>>();
    devices.sort_by(|left, right| {
        let left_rank = if left.path.starts_with("/dev/cu.") {
            0
        } else {
            1
        };
        let right_rank = if right.path.starts_with("/dev/cu.") {
            0
        } else {
            1
        };
        left_rank.cmp(&right_rank).then(left.path.cmp(&right.path))
    });
    Ok(devices)
}

fn bounded(value: Option<String>, max_bytes: usize) -> Option<String> {
    value.filter(|item| item.len() <= max_bytes && !item.chars().any(char::is_control))
}

fn enumerable_path(path: &str) -> bool {
    if cfg!(target_os = "macos") {
        path.starts_with("/dev/cu.")
    } else {
        path.starts_with("/dev/")
    }
}

fn open_port(
    profile: &ConnectionProfile,
    options: &SerialConnectionOptions,
) -> AppResult<(Box<dyn SerialPort>, Box<dyn SerialPort>)> {
    validate_options(options)?;
    let baud_rate =
        u32::try_from(profile.port).map_err(|_| AppError::Validation("串口波特率无效".into()))?;
    let data_bits = match options.data_bits {
        5 => DataBits::Five,
        6 => DataBits::Six,
        7 => DataBits::Seven,
        _ => DataBits::Eight,
    };
    let parity = match options.parity.as_str() {
        "odd" => Parity::Odd,
        "even" => Parity::Even,
        _ => Parity::None,
    };
    let stop_bits = if options.stop_bits == 2 {
        StopBits::Two
    } else {
        StopBits::One
    };
    let flow_control = match options.flow_control.as_str() {
        "software" => FlowControl::Software,
        "hardware" => FlowControl::Hardware,
        _ => FlowControl::None,
    };
    let mut port = serialport::new(&profile.host, baud_rate)
        .data_bits(data_bits)
        .parity(parity)
        .stop_bits(stop_bits)
        .flow_control(flow_control)
        .timeout(Duration::from_millis(200))
        .dtr_on_open(options.dtr)
        .open()
        .map_err(|error| AppError::Unavailable(format!("打开串口失败：{error}")))?;
    port.write_data_terminal_ready(options.dtr)
        .map_err(|error| AppError::Unavailable(format!("设置串口 DTR 失败：{error}")))?;
    port.write_request_to_send(options.rts)
        .map_err(|error| AppError::Unavailable(format!("设置串口 RTS 失败：{error}")))?;
    let reader = port
        .try_clone()
        .map_err(|error| AppError::Unavailable(format!("创建串口读取句柄失败：{error}")))?;
    Ok((port, reader))
}

#[allow(clippy::too_many_arguments)]
fn spawn_reader(
    app: AppHandle,
    manager: SerialManager,
    logs: SessionLogManager,
    id: String,
    profile: ConnectionProfile,
    options: SerialConnectionOptions,
    session: Arc<Mutex<ManagedSerial>>,
    mut reader: Box<dyn SerialPort>,
) {
    std::thread::Builder::new()
        .name(format!("cnshell-serial-{}", &id[..id.len().min(8)]))
        .spawn(move || {
            let mut buffer = [0_u8; 32 * 1024];
            let mut failure = None;
            loop {
                if manager.closing.lock().contains(&id) {
                    break;
                }
                match reader.read(&mut buffer) {
                    Ok(0) => continue,
                    Ok(size) => {
                        logs.write_output(&id, &buffer[..size]);
                        let _ = app.emit(
                            "terminal-output",
                            TerminalOutput {
                                session_id: id.clone(),
                                data_base64: STANDARD.encode(&buffer[..size]),
                            },
                        );
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::TimedOut => continue,
                    Err(error) => {
                        session.lock().port.take();
                        let initial_error = error.to_string();
                        match reconnect(&app, &manager, &id, &profile, &options, &session) {
                            Some(new_reader) => {
                                reader = new_reader;
                                continue;
                            }
                            None => {
                                if !manager.closing.lock().contains(&id) {
                                    failure =
                                        Some(format!("串口设备断开且重连失败：{initial_error}"));
                                }
                                break;
                            }
                        }
                    }
                }
            }
            let requested = manager.closing.lock().remove(&id);
            manager.sessions.lock().remove(&id);
            let _ = logs.stop(&id);
            let _ = app.emit(
                "terminal-status",
                TerminalStatus {
                    session_id: id,
                    status: if requested { "closed" } else { "failed" }.into(),
                    last_error: if requested {
                        None
                    } else {
                        failure.or_else(|| Some("串口设备已断开".into()))
                    },
                    attempt: None,
                },
            );
        })
        .ok();
}

fn reconnect(
    app: &AppHandle,
    manager: &SerialManager,
    id: &str,
    profile: &ConnectionProfile,
    options: &SerialConnectionOptions,
    session: &Arc<Mutex<ManagedSerial>>,
) -> Option<Box<dyn SerialPort>> {
    for (index, delay) in RECONNECT_DELAYS.into_iter().enumerate() {
        let attempt = (index + 1) as u8;
        let _ = app.emit(
            "terminal-status",
            TerminalStatus {
                session_id: id.into(),
                status: "reconnecting".into(),
                last_error: Some(format!("串口设备已断开，{delay} 秒后尝试重新接入")),
                attempt: Some(attempt),
            },
        );
        for _ in 0..delay * 10 {
            if manager.closing.lock().contains(id) {
                return None;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        match open_port(profile, options) {
            Ok((port, reader)) => {
                session.lock().port = Some(port);
                let _ = app.emit(
                    "terminal-status",
                    TerminalStatus {
                        session_id: id.into(),
                        status: "online".into(),
                        last_error: None,
                        attempt: None,
                    },
                );
                return Some(reader);
            }
            Err(_) => continue,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_defaults_and_enum_validation_are_stable() {
        let options = default_options("serial-1".into());
        assert!(validate_options(&options).is_ok());
        let mut invalid = options;
        invalid.data_bits = 9;
        assert!(validate_options(&invalid).is_err());
        invalid.data_bits = 8;
        invalid.flow_control = "unsafe".into();
        assert!(validate_options(&invalid).is_err());
    }

    #[test]
    fn serial_options_round_trip_through_profile_environment() {
        let mut profile = ConnectionProfile {
            id: "serial-1".into(),
            folder_id: None,
            protocol: "serial".into(),
            name: "USB console".into(),
            host: "/dev/cu.usbserial".into(),
            port: 115_200,
            username: "serial".into(),
            auth_type: "none".into(),
            private_key_path: None,
            certificate_path: None,
            host_key_policy: "strict".into(),
            note: String::new(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: BTreeMap::from([("KEEP_ME".into(), "yes".into())]),
            has_credential: false,
            created_at: String::new(),
            updated_at: String::new(),
            last_connected_at: None,
        };
        let options = SerialConnectionOptions {
            connection_id: profile.id.clone(),
            data_bits: 7,
            parity: "even".into(),
            stop_bits: 2,
            flow_control: "hardware".into(),
            dtr: false,
            rts: true,
        };
        profile.environment = environment_with_options(&profile.environment, &options).unwrap();
        assert!(has_persisted_options(&profile));
        assert!(is_option_key(DATA_BITS_KEY));
        assert!(!is_option_key("KEEP_ME"));
        assert_eq!(options_from_profile(&profile).unwrap(), options);
        assert_eq!(profile.environment.get("KEEP_ME").unwrap(), "yes");
    }

    #[test]
    fn reconnect_schedule_is_bounded() {
        assert_eq!(RECONNECT_DELAYS, [1, 2, 5, 10, 30]);
        assert_eq!(RECONNECT_DELAYS.iter().sum::<u64>(), 48);
    }

    #[test]
    fn local_device_enumeration_returns_bounded_paths() {
        let devices = devices().unwrap();
        assert!(devices.iter().all(|device| {
            device.path.starts_with("/dev/")
                && device.path.len() <= 16 * 1024
                && device.label.len() <= 256
        }));
    }
}
