fn main() {
    match std::env::args().nth(1).as_deref() {
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
        _ => {}
    }
    cnshell_lib::run();
}
