fn main() {
    if std::env::args().nth(1).as_deref() == Some("--rdp-preflight") {
        println!("{}", cnshell_lib::rdp_preflight_json());
        return;
    }
    cnshell_lib::run();
}
