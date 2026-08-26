mod ae;
mod archive;
pub mod cli;
mod commands;
mod error;
mod importer;
mod installer;
mod library;
mod models;

#[cfg(test)]
mod tests;

// The native menu is macOS-only: macOS shows it in the system top bar (expected
// there), whereas on Windows it would attach an unwanted menu bar to the window.
// All menu actions are also reachable from the in-app toolbar.
#[cfg(target_os = "macos")]
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
#[cfg(target_os = "macos")]
use tauri::Emitter;

/// Build the native application menu (macOS system top bar).
/// Custom items emit a `menu-action` event to the webview; predefined items
/// keep standard behaviour (copy/paste/quit, needed once we replace the
/// default menu).
#[cfg(target_os = "macos")]
fn build_menu(handle: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let file = Submenu::with_items(
        handle,
        "File",
        true,
        &[
            &MenuItem::with_id(handle, "import_package", "パッケージを追加 (.ppf)", true, None::<&str>)?,
            &MenuItem::with_id(handle, "import_assets", "スクリプト・プラグインを追加", true, None::<&str>)?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::close_window(handle, None)?,
        ],
    )?;
    let edit = Submenu::with_items(
        handle,
        "Edit",
        true,
        &[
            &MenuItem::with_id(handle, "toggle_select", "複数選択モード", true, None::<&str>)?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::undo(handle, None)?,
            &PredefinedMenuItem::redo(handle, None)?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::cut(handle, None)?,
            &PredefinedMenuItem::copy(handle, None)?,
            &PredefinedMenuItem::paste(handle, None)?,
            &PredefinedMenuItem::select_all(handle, None)?,
        ],
    )?;

    let menu = Menu::new(handle)?;
    #[cfg(target_os = "macos")]
    {
        let app_menu = Submenu::with_items(
            handle,
            "Pluxel",
            true,
            &[
                &PredefinedMenuItem::about(handle, None, None)?,
                &PredefinedMenuItem::separator(handle)?,
                &PredefinedMenuItem::services(handle, None)?,
                &PredefinedMenuItem::separator(handle)?,
                &PredefinedMenuItem::hide(handle, None)?,
                &PredefinedMenuItem::hide_others(handle, None)?,
                &PredefinedMenuItem::separator(handle)?,
                &PredefinedMenuItem::quit(handle, None)?,
            ],
        )?;
        menu.append(&app_menu)?;
    }
    menu.append(&file)?;
    menu.append(&edit)?;
    #[cfg(target_os = "macos")]
    {
        let window = Submenu::with_items(
            handle,
            "Window",
            true,
            &[
                &PredefinedMenuItem::minimize(handle, None)?,
                &PredefinedMenuItem::separator(handle)?,
                &PredefinedMenuItem::fullscreen(handle, None)?,
            ],
        )?;
        menu.append(&window)?;
    }
    Ok(menu)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();
    // macOS keeps the native menu (system top bar); Windows shows no menu bar.
    #[cfg(target_os = "macos")]
    {
        builder = builder.menu(build_menu).on_menu_event(|app, event| {
            let _ = app.emit("menu-action", event.id().as_ref());
        });
    }
    // Auto-updater (desktop only): checks GitHub Releases and applies signed updates.
    #[cfg(desktop)]
    {
        builder = builder
            .plugin(tauri_plugin_updater::Builder::new().build())
            .plugin(tauri_plugin_process::init());
    }
    builder
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::admin_status,
            commands::admin_unlock,
            commands::admin_lock,
            commands::get_library,
            commands::get_data_dir,
            commands::import_paths,
            commands::add_files,
            commands::replace_files,
            commands::remove_file,
            commands::remove_package,
            commands::replace_package,
            commands::update_manifest,
            commands::set_banner,
            commands::clear_banner,
            commands::read_banner,
            commands::detect_ae,
            commands::reveal_path,
            commands::run_installer,
            commands::install_package,
            commands::install_all_ae,
            commands::install_packages,
            commands::uninstall_package,
            commands::export_library,
            commands::export_package,
            commands::import_archive,
            commands::create_bundle,
            commands::update_bundle,
            commands::delete_bundle,
            commands::install_bundle,
            commands::export_bundle,
            commands::set_bundle_banner,
            commands::clear_bundle_banner,
            commands::read_bundle_banner,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
