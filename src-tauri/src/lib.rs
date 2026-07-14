mod automation;
mod backup;
mod batch;
mod bookmark;
mod commands;
mod db;
mod diagnostics;
mod error;
mod external_edit;
mod models;
mod monitor;
mod openssh;
mod protocols;
mod rdp;
mod session_log;
mod sftp;
mod ssh;
mod task;
mod tunnel;
mod zmodem;

use batch::BatchManager;
use db::Database;
use external_edit::ExternalEditManager;
use monitor::MonitorState;
use rdp::RdpManager;
use session_log::SessionLogManager;
use sftp::TransferManager;
use ssh::SessionManager;
use task::TaskManager;
use tauri::{
    Emitter, Manager,
    menu::{AboutMetadata, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder},
};
use tunnel::TunnelManager;

pub struct AppState {
    db: Database,
    sessions: SessionManager,
    transfers: TransferManager,
    monitor: MonitorState,
    tunnels: TunnelManager,
    tasks: TaskManager,
    rdp: RdpManager,
    logs: SessionLogManager,
    batches: BatchManager,
    external_edits: ExternalEditManager,
}

pub fn rdp_preflight_json() -> String {
    serde_json::to_string(&rdp::preflight()).expect("RDP preflight is serializable")
}

fn build_menu(app: &tauri::App) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let about_metadata = AboutMetadata {
        name: Some("CNshell".into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        ..Default::default()
    };
    let about = PredefinedMenuItem::about(app, Some("关于 CNshell"), Some(about_metadata))?;
    let app_menu = SubmenuBuilder::new(app, "CNshell")
        .item(&about)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;
    let file = SubmenuBuilder::new(app, "文件")
        .item(
            &MenuItemBuilder::with_id("new_connection", "新建连接")
                .accelerator("CmdOrCtrl+N")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("new_terminal", "新建终端")
                .accelerator("CmdOrCtrl+T")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("close_session", "关闭当前会话")
                .accelerator("CmdOrCtrl+W")
                .build(app)?,
        )
        .build()?;
    let edit = SubmenuBuilder::new(app, "编辑")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;
    let view = SubmenuBuilder::new(app, "显示")
        .item(
            &MenuItemBuilder::with_id("toggle_files", "切换文件面板")
                .accelerator("CmdOrCtrl+J")
                .build(app)?,
        )
        .fullscreen()
        .build()?;
    let help = SubmenuBuilder::new(app, "帮助")
        .item(
            &MenuItemBuilder::with_id("show_help", "CNshell 使用帮助")
                .accelerator("CmdOrCtrl+?")
                .build(app)?,
        )
        .build()?;
    MenuBuilder::new(app)
        .items(&[&app_menu, &file, &edit, &view, &help])
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let _ = sftp::cleanup_preview_cache();
            let _ = external_edit::cleanup_cache();
            let data_dir = handle.path().app_data_dir()?;
            let db =
                tauri::async_runtime::block_on(Database::open(&data_dir.join("cnshell.sqlite")))
                    .map_err(|error| Box::<dyn std::error::Error>::from(error.to_string()))?;
            app.manage(AppState {
                db,
                sessions: SessionManager::default(),
                transfers: TransferManager::default(),
                monitor: MonitorState::default(),
                tunnels: TunnelManager::default(),
                tasks: TaskManager::default(),
                rdp: RdpManager::default(),
                logs: SessionLogManager::new(data_dir.join("session-logs"))
                    .map_err(|error| Box::<dyn std::error::Error>::from(error.to_string()))?,
                batches: BatchManager::default(),
                external_edits: ExternalEditManager::default(),
            });
            app.set_menu(build_menu(app)?)?;
            Ok(())
        })
        .on_menu_event(|app, event| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.emit("menu-action", event.id().as_ref());
            }
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                window.state::<AppState>().rdp.close_all();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::connection_list,
            commands::connection_deleted_list,
            commands::folder_list,
            commands::folder_save,
            commands::folder_delete,
            commands::connection_move,
            commands::connection_save,
            commands::connection_duplicate,
            commands::connection_delete,
            commands::connection_restore,
            commands::connection_purge,
            commands::connection_test_start,
            commands::connection_trust_host,
            commands::openssh_import,
            commands::openssh_generate_key,
            commands::openssh_deploy_key,
            commands::protocol_capabilities,
            commands::protocol_options_get,
            commands::protocol_options_save,
            commands::automation_validate,
            commands::automation_start,
            commands::sync_write,
            commands::sync_read,
            commands::terminal_open,
            commands::terminal_input,
            commands::terminal_resize,
            commands::terminal_close,
            commands::zmodem_start,
            commands::zmodem_cancel,
            commands::terminal_log_start,
            commands::terminal_log_stop,
            commands::terminal_log_status,
            commands::terminal_log_export,
            commands::batch_start,
            commands::batch_get,
            commands::batch_cancel,
            commands::external_edit_start,
            commands::external_edit_read,
            commands::external_edit_discard,
            commands::sftp_list,
            commands::sftp_join_path,
            commands::sftp_mkdir,
            commands::sftp_rename,
            commands::sftp_delete,
            commands::sftp_chmod,
            commands::sftp_open_text,
            commands::sftp_save_text,
            commands::sftp_create_text,
            commands::sftp_archive_start,
            commands::sftp_open_local_start,
            commands::sftp_directory_transfer_start,
            commands::task_get,
            commands::task_cancel,
            commands::transfer_enqueue,
            commands::transfer_list,
            commands::transfer_cancel,
            commands::transfer_pause,
            commands::transfer_resume,
            commands::transfer_retry,
            commands::proxy_list,
            commands::proxy_save,
            commands::proxy_delete,
            commands::tunnel_list,
            commands::tunnel_save,
            commands::tunnel_start,
            commands::tunnel_stop,
            commands::tunnel_delete,
            commands::snippet_list,
            commands::snippet_save,
            commands::snippet_delete,
            commands::history_add,
            commands::history_list,
            commands::history_clear,
            commands::workspace_save,
            commands::workspace_load,
            commands::connection_export,
            commands::connection_export_one,
            commands::connection_import,
            commands::monitor_snapshot,
            commands::monitor_process_signal,
            commands::monitor_network_sockets,
            commands::monitor_network_diagnostic_start,
            commands::monitor_system_info,
            commands::monitor_export_system_info,
            commands::rdp_preflight,
            commands::rdp_open,
            commands::rdp_close,
            commands::settings_get,
            commands::settings_save,
            commands::diagnostics_export,
            commands::diagnostics_environment,
            commands::diagnostics_reveal
        ])
        .run(tauri::generate_context!())
        .expect("CNshell 启动失败");
}
