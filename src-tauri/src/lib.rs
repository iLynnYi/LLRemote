#![allow(
    non_snake_case,
    reason = "The library crate intentionally uses the LLRemoteLib product name."
)]

pub mod app_log;
mod app_shutdown;
mod app_storage;
mod app_version;
pub mod background_share;
mod commands;
pub mod credential_vault;
pub mod file_access;
pub mod files;
#[cfg(any(target_os = "windows", test))]
mod gui_single_instance;
pub mod host_store;
mod host_store_storage;
#[cfg(any(target_os = "macos", windows, test))]
mod keyboard_input_source;
pub mod local_agent_service;
pub mod local_vnc_server;
mod local_vnc_server_platform;
mod local_vnc_server_protocol;
pub mod remote_desktop_core;
mod remote_message;
pub mod remote_session;
pub mod server;
pub mod server_discovery;
pub mod session_artifacts;
mod session_keyboard_capture;
pub mod startup_attempt;
pub mod tray;
mod window_bounds;
pub mod windows_firewall;
pub mod windows_service;
#[cfg(windows)]
mod windows_storage;
pub mod windows_uninstall;

pub use files::transfer as file_transfer;

#[cfg(target_os = "macos")]
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
#[cfg(target_os = "macos")]
use tauri::Emitter;
use tauri::Manager;

#[cfg(target_os = "macos")]
fn setup_app_menu(app: &tauri::AppHandle) -> tauri::Result<()> {
    let app_name = app
        .config()
        .product_name
        .clone()
        .unwrap_or_else(|| "LL Remote".into());

    let about = MenuItemBuilder::with_id("app:about", "关于 LL Remote").build(app)?;
    let services = PredefinedMenuItem::services(app, Some("服务"))?;
    let hide = PredefinedMenuItem::hide(app, Some("隐藏 LL Remote"))?;
    let hide_others = PredefinedMenuItem::hide_others(app, Some("隐藏其他"))?;
    let quit = PredefinedMenuItem::quit(app, Some("退出 LL Remote"))?;

    let app_submenu = SubmenuBuilder::new(app, app_name)
        .item(&about)
        .separator()
        .item(&services)
        .separator()
        .item(&hide)
        .item(&hide_others)
        .separator()
        .item(&quit)
        .build()?;

    // Cocoa routes local WebView editing shortcuts through these native actions.
    // The session event tap suppresses them only while the remote viewport owns input.
    let edit_submenu = SubmenuBuilder::new(app, "编辑")
        .undo_with_text("撤销")
        .redo_with_text("重做")
        .separator()
        .cut_with_text("剪切")
        .copy_with_text("复制")
        .paste_with_text("粘贴")
        .select_all_with_text("全选")
        .build()?;
    let menu = MenuBuilder::new(app)
        .item(&app_submenu)
        .item(&edit_submenu)
        .build()?;

    app.set_menu(menu)?;
    app.on_menu_event(|app, event| {
        if event.id().as_ref() != "app:about" {
            return;
        }

        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
            let _ = window.emit_to("main", "app-action-open-about", ());
        }
    });
    Ok(())
}

fn configure_gui_builder<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    // Service and background-share modes return from main before reaching here.
    // Register first so a second GUI exits before opening stores or creating a tray.
    #[cfg(all(desktop, not(target_os = "windows")))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        let _ = tray::show_main_window(app);
    }));
    #[cfg(target_os = "windows")]
    let builder = builder.plugin(gui_single_instance::init());

    builder
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(windows)]
    if session_keyboard_capture::run_worker_if_requested() {
        return;
    }
    // Load the project .env before any backend service reads its configuration.
    // dotenvy keeps variables explicitly provided by the process unchanged.
    let _ = dotenvy::dotenv();

    // Delay WebView creation until version validation and store initialization
    // succeed. A rejected downgrade must never dispatch frontend store commands.
    let mut context = tauri::generate_context!();
    #[cfg(windows)]
    windows_storage::configure_directories(context.config_mut());
    let initial_windows: Vec<_> = context
        .config()
        .app
        .windows
        .iter()
        .filter(|window| window.create)
        .cloned()
        .collect();
    for window in &mut context.config_mut().app.windows {
        window.create = false;
    }

    configure_gui_builder(tauri::Builder::default())
        .manage(app_shutdown::AppShutdown::default())
        .manage(remote_session::RemoteSessionManager::default())
        .manage(session_keyboard_capture::SessionKeyboardCapture::default())
        .manage(window_bounds::WindowBoundsManager::default())
        .manage(session_artifacts::SessionRecordingFiles::default())
        .manage(file_transfer::FileTransferScans::default())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            window_bounds::WindowBoundsManager::on_event(window, event);
            let capture = window.state::<session_keyboard_capture::SessionKeyboardCapture>();
            match event {
                tauri::WindowEvent::Focused(false) => capture.disable_unfocused_window(window),
                tauri::WindowEvent::Destroyed => capture.disable_window(window),
                _ => (),
            }
        })
        .on_page_load(|webview, payload| {
            if payload.event() == tauri::webview::PageLoadEvent::Finished {
                window_bounds::WindowBoundsManager::request(&webview.window(), true);
            }
        })
        .setup(move |app| {
            let handle = app.handle().clone();
            let app_data_dir = app_storage::application_directory(&handle.path().app_data_dir()?)?;
            let version = handle
                .config()
                .version
                .as_deref()
                .unwrap_or(env!("CARGO_PKG_VERSION"));
            if let Err(message) = app_version::check_and_record(&app_data_dir, version) {
                use tauri_plugin_dialog::DialogExt;
                let exit = handle.clone();
                handle
                    .dialog()
                    .message(message)
                    .title("LL Remote 无法启动")
                    .kind(tauri_plugin_dialog::MessageDialogKind::Error)
                    .show(move |_| exit.exit(0));
                return Ok(());
            }
            let default_log_dir =
                app_storage::application_directory(&handle.path().app_log_dir()?)?;
            let logger = app_log::AppLogger::initialize(default_log_dir)
                .map_err(|error| tauri::Error::Io(std::io::Error::other(error)))?;
            app.manage(logger.clone());
            logger.info(
                "app",
                "app_started",
                "LL Remote started.",
                serde_json::json!({
                    "version": handle.package_info().version.to_string(),
                    "platform": std::env::consts::OS,
                }),
            );
            if let Err(error) = app
                .state::<session_artifacts::SessionRecordingFiles>()
                .configure_index(app_data_dir.join("recording-directories.json"))
            {
                logger.submit(
                    app_log::LogLevel::Warn,
                    "recording",
                    "recovery_index_unavailable",
                    "Recording recovery index is unavailable; remote sessions remain usable.",
                    serde_json::json!({ "error": error }),
                );
            }
            let host_repository =
                host_store::HostRepository::open(app_data_dir.join("hosts.sqlite3"))
                    .map_err(|error| tauri::Error::Io(std::io::Error::other(error)))?;
            #[cfg(windows)]
            let builtin_directory = windows_storage::default_artifacts(&app_data_dir);
            #[cfg(not(windows))]
            let builtin_directory = app_data_dir.join("Artifacts");
            let file_access_store = file_access::FileAccessStore::open_with_builtin(
                app_data_dir.join("file_access.json"),
                builtin_directory,
            )
            .map_err(|error| tauri::Error::Io(std::io::Error::other(error)))?;
            let transfer_task_store = files::transfer_task_store::TransferTaskRepository::open(
                app_data_dir.join("file_transfer_tasks.sqlite3"),
            )
            .map_err(|error| tauri::Error::Io(std::io::Error::other(error)))?;
            app.manage(host_repository);
            app.manage(file_access_store);
            app.manage(transfer_task_store);

            for window in &initial_windows {
                tauri::WebviewWindowBuilder::from_config(&handle, window)?.build()?;
            }

            #[cfg(target_os = "macos")]
            setup_app_menu(&handle)?;
            tray::setup_tray(&handle)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }

            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            session_keyboard_capture::is_session_keyboard_capture_focused,
            session_keyboard_capture::set_session_keyboard_capture,
            commands::report_frontend_startup_failure,
            commands::log_connection_model,
            commands::append_frontend_log_batch,
            commands::open_log_directory,
            commands::update_tray_state,
            commands::focus_session_external_window,
            commands::background_share::get_background_share_status,
            commands::background_share::get_local_share_disconnect_lock,
            commands::background_share::set_local_share_disconnect_lock,
            commands::background_share::prepare_background_share,
            commands::background_share::start_background_share,
            commands::background_share::stop_background_share,
            commands::background_share::set_background_share_auto_start,
            commands::background_share::restart_background_share,
            commands::background_share::update_background_share_view,
            commands::background_share::disconnect_background_share_connection,
            commands::background_share::request_background_share_permission,
            commands::windows_service::get_windows_service_status,
            commands::windows_firewall::inspect_windows_firewall,
            commands::windows_firewall::open_windows_firewall_settings,
            commands::windows_service::get_windows_service_server_status,
            commands::windows_service::disconnect_windows_service_connection,
            commands::windows_service::install_windows_service,
            commands::windows_service::uninstall_windows_service,
            commands::windows_service::configure_windows_service,
            commands::windows_service::authorize_windows_service_password_management,
            commands::windows_service::set_windows_service_password,
            commands::windows_service::start_windows_service,
            commands::windows_service::start_windows_service_with_fallback,
            commands::windows_service::stop_windows_service,
            commands::windows_service::enable_windows_service_sas,
            commands::windows_service::repair_windows_service,
            commands::windows_service::set_windows_service_auto_start,
            commands::agent::invoke_remote_agent,
            commands::agent::upload_remote_agent_file,
            commands::files::select_file_transfer_sources,
            commands::files::start_file_transfer_source_scan,
            commands::files::next_file_transfer_source_page,
            commands::files::cancel_file_transfer_source_scan,
            commands::files::load_file_transfer_tasks,
            commands::files::apply_file_transfer_task_changes,
            commands::remote_session::start_remote_session,
            commands::remote_session::update_remote_session,
            commands::remote_session::connect_remote_session_rfb,
            commands::remote_session::send_remote_session_rfb_input,
            commands::remote_session::disconnect_remote_session_rfb,
            commands::remote_session::stop_remote_session,
            commands::remote_session::attach_session_surface,
            commands::remote_session::detach_session_surface,
            commands::remote_session::set_active_session_surface,
            commands::remote_session::get_remote_session_snapshot,
            commands::remote_session::list_remote_session_snapshots,
            commands::hosts::list_hosts,
            commands::hosts::create_host,
            commands::hosts::update_host,
            commands::hosts::delete_host,
            commands::hosts::record_host_connection_attempt,
            commands::hosts::list_host_groups,
            commands::hosts::create_host_group,
            commands::hosts::resolve_host_password,
            commands::hosts::rename_host_group,
            commands::hosts::delete_host_group,
            commands::files::inspect_session_artifact_directory,
            commands::files::write_session_artifact_file,
            commands::files::start_session_recording_file,
            commands::files::append_session_recording_chunk,
            commands::files::finish_session_recording_file,
            commands::files::release_session_recording_file,
            commands::files::inspect_session_recording_space,
            commands::files::list_session_recording_jobs,
            commands::files::recover_session_recordings,
            commands::files::retry_session_recording_save,
            commands::files::relocate_session_recording_file,
            commands::files::list_file_access_directories,
            commands::files::grant_file_access_directory,
            commands::files::revoke_file_access_directory,
            commands::files::set_builtin_file_access_directory,
            commands::files::reveal_authorized_path,
            commands::files::open_authorized_path
        ])
        .build(context)
        .expect("error while building tauri application")
        .run(|app, event| match event {
            // Closing the main window hides it; Dock reopen must show it again.
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => {
                let _ = tray::show_main_window(app);
            }
            event => app_shutdown::handle_event(app, event),
        });
}

#[cfg(all(test, any(target_os = "macos", target_os = "windows")))]
#[path = "../tests/support/gui_single_instance_tests.rs"]
mod gui_single_instance_tests;
