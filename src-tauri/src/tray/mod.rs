mod events;
mod model;

pub use model::{
    build_local_share_tray_snapshot, build_tray_menu_model, TrayIconState, TrayLocalConnection,
    TrayLocalShareAction, TrayLocalShareStatus, TrayMenuItemKind, TrayMenuItemModel, TrayMenuModel,
    TrayStateSnapshot,
};

use events::{
    EVENT_COPY_LOCAL_SHARE, EVENT_DISCONNECT_LOCAL_CONNECTION, EVENT_OPEN_LOCAL_SHARE,
    EVENT_OPEN_MAIN, EVENT_OPEN_SETTINGS, EVENT_START_LOCAL_SHARE, EVENT_STOP_LOCAL_SHARE,
};
use tauri::image::Image;
use tauri::include_image;
use tauri::menu::{Menu, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime};

const TRAY_ID: &str = "main-status-tray";

pub fn setup_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let snapshot = TrayStateSnapshot::default();
    let menu = build_tauri_menu(app, &snapshot)?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon_for_state(snapshot.icon_state)?)
        .tooltip(&snapshot.tooltip)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            handle_tray_menu_event(app, event.id().as_ref());
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

pub fn update_tray<R: Runtime>(
    app: &AppHandle<R>,
    snapshot: TrayStateSnapshot,
) -> Result<(), String> {
    let tray = app
        .tray_by_id(TRAY_ID)
        .ok_or_else(|| "tray_not_ready".to_string())?;
    let menu = build_tauri_menu(app, &snapshot).map_err(|error| error.to_string())?;

    tray.set_icon(Some(
        icon_for_state(snapshot.icon_state).map_err(|error| error.to_string())?,
    ))
    .map_err(|error| error.to_string())?;
    tray.set_tooltip(Some(&snapshot.tooltip))
        .map_err(|error| error.to_string())?;
    tray.set_menu(Some(menu))
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn build_tauri_menu<R: Runtime>(
    app: &AppHandle<R>,
    snapshot: &TrayStateSnapshot,
) -> tauri::Result<Menu<R>> {
    let model = build_tray_menu_model(snapshot);
    let menu = Menu::new(app)?;

    for line in model.summary {
        let summary_item = MenuItemBuilder::new(line).enabled(false).build(app)?;
        menu.append(&summary_item)?;
    }

    menu.append(&PredefinedMenuItem::separator(app)?)?;

    for item_model in model.items {
        if matches!(item_model.kind, TrayMenuItemKind::OpenMain) {
            menu.append(&PredefinedMenuItem::separator(app)?)?;
        }

        if item_model.children.is_empty() {
            let menu_item =
                MenuItemBuilder::with_id(model::menu_item_id(&item_model), item_model.label)
                    .enabled(item_model.enabled)
                    .build(app)?;
            menu.append(&menu_item)?;
        } else {
            let submenu = SubmenuBuilder::new(app, item_model.label).enabled(item_model.enabled);
            let submenu = item_model
                .children
                .into_iter()
                .try_fold(submenu, |builder, child| {
                    let child_item =
                        MenuItemBuilder::with_id(model::menu_item_id(&child), child.label)
                            .enabled(child.enabled)
                            .build(app)?;
                    Ok::<_, tauri::Error>(builder.item(&child_item))
                })?;
            menu.append(&submenu.build()?)?;
        }
    }

    Ok(menu)
}

fn handle_tray_menu_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match id {
        "tray:open-main" => {
            let _ = show_main_window(app);
            let _ = app.emit(EVENT_OPEN_MAIN, ());
        }
        "tray:open-local-share" => {
            let _ = show_main_window(app);
            let _ = app.emit(EVENT_OPEN_LOCAL_SHARE, ());
        }
        "tray:start-local-share" => {
            let _ = app.emit(EVENT_START_LOCAL_SHARE, ());
        }
        "tray:stop-local-share" => {
            let _ = app.emit(EVENT_STOP_LOCAL_SHARE, ());
        }
        "tray:copy-local-share" => {
            let _ = app.emit(EVENT_COPY_LOCAL_SHARE, ());
        }
        id if id.starts_with("tray:disconnect-local-connection:") => {
            let connection_id = id.trim_start_matches("tray:disconnect-local-connection:");
            let _ = app.emit(EVENT_DISCONNECT_LOCAL_CONNECTION, connection_id.to_string());
        }
        "tray:open-settings" => {
            let _ = show_main_window(app);
            let _ = app.emit(EVENT_OPEN_SETTINGS, ());
        }

        "tray:quit" => app.exit(0),
        _ => {}
    }
}

pub(crate) fn show_main_window<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        window.show()?;
        window.unminimize()?;
        window.set_focus()?;
    }
    Ok(())
}

fn icon_for_state(state: TrayIconState) -> tauri::Result<Image<'static>> {
    let image = match state {
        TrayIconState::Idle => include_image!("icons/tray/idle.png"),
        TrayIconState::Sharing => include_image!("icons/tray/sharing.png"),
        TrayIconState::SharingActive1
        | TrayIconState::SharingActive2
        | TrayIconState::SharingActive3
        | TrayIconState::SharingActive9Plus => include_image!("icons/tray/sharing-active.png"),
        TrayIconState::Error => include_image!("icons/tray/error.png"),
    };
    Ok(image)
}
