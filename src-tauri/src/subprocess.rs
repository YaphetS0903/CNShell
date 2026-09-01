use std::{ffi::OsStr, process::Command};

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub(crate) fn background_command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    configure_background(&mut command);
    command
}

pub(crate) fn background_tokio_command(program: impl AsRef<OsStr>) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(program);
    configure_background(command.as_std_mut());
    command
}

fn configure_background(_command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt as _;
        _command.creation_flags(CREATE_NO_WINDOW);
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    const CHILD_MARKER: &str = "CNSHELL_BACKGROUND_PROCESS_TEST_CHILD";

    #[test]
    fn windows_background_process_has_no_console_window() {
        if std::env::var_os(CHILD_MARKER).is_some() {
            let detached =
                unsafe { windows_sys::Win32::System::Console::GetConsoleWindow().is_null() };
            println!("CNSHELL_NO_CONSOLE={detached}");
            return;
        }

        let output = background_command(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "subprocess::tests::windows_background_process_has_no_console_window",
                "--nocapture",
            ])
            .env(CHILD_MARKER, "1")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("CNSHELL_NO_CONSOLE=true"),
            "background child unexpectedly attached a console: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}
