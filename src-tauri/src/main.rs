#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let mut arguments = std::env::args_os();
    let _executable = arguments.next();
    match arguments.next().as_deref().and_then(|value| value.to_str()) {
        Some("--rdp-preflight") => {
            println!("{}", cnshell_lib::rdp_preflight_json());
            return;
        }
        Some("--rdp-displays") => {
            println!("{}", cnshell_lib::rdp_displays_json());
            return;
        }
        Some("--serial-devices") => {
            println!("{}", cnshell_lib::serial_devices_json());
            return;
        }
        Some("--verify-updater-signature") => {
            let Some(archive) = arguments.next() else {
                eprintln!("updater 验签失败：缺少归档路径");
                std::process::exit(2);
            };
            let Some(signature) = arguments.next() else {
                eprintln!("updater 验签失败：缺少签名路径");
                std::process::exit(2);
            };
            let Some(config) = arguments.next() else {
                eprintln!("updater 验签失败：缺少 release 配置路径");
                std::process::exit(2);
            };
            if arguments.next().is_some() {
                eprintln!("updater 验签失败：参数数量无效");
                std::process::exit(2);
            }
            match cnshell_lib::verify_updater_signature(
                std::path::Path::new(&archive),
                std::path::Path::new(&signature),
                std::path::Path::new(&config),
            ) {
                Ok(()) => println!("updater 签名与 release 公钥匹配"),
                Err(error) => {
                    eprintln!("updater 验签失败：{error}");
                    std::process::exit(1);
                }
            }
            return;
        }
        _ => {}
    }
    cnshell_lib::run();
}
