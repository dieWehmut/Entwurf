use crate::tray_actions::{restore, Action};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

const TRAY_ID: &str = "entwurf-tray";

fn activate(app: &tauri::AppHandle, close: bool) {
    if let Some(window) = app.get_webview_window("main") {
        let result = restore(
            |action| match action {
                Action::Show => window.show(),
                Action::Unminimize => window.unminimize(),
                Action::Focus => window.set_focus(),
                // close() emits CloseRequested; the frontend draft/busy guard remains authoritative.
                Action::Close => window.close(),
            },
            close,
        );
        if let Err(error) = result {
            eprintln!("Entwurf tray window action failed: {error}");
        }
    }
}

pub fn install(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "tray-show", "显示窗口", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "tray-quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let Some(icon) = app.default_window_icon().cloned() else {
        eprintln!("Entwurf tray unavailable: no application icon");
        return Ok(());
    };
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("Entwurf")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "tray-show" => activate(app, false),
            "tray-quit" => activate(app, true),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                activate(tray.app_handle(), false);
            }
        })
        .build(app)?;
    // Register hiding only after successful creation. Tauri owns the registered tray.
    if let Some(window) = app.get_webview_window("main") {
        let main = window.clone();
        window.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::Resized(_))
                && main.app_handle().tray_by_id(TRAY_ID).is_some()
                && main.is_minimized().unwrap_or(false)
            {
                if let Err(error) = main.hide() {
                    eprintln!("Entwurf minimize to tray failed: {error}");
                }
            }
        });
    }
    Ok(())
}
