pub mod core;
pub mod model;
pub mod network;
pub mod storage;

use crate::{core::Core, model::*};
use std::sync::Arc;
use tauri::{Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
fn get_state(core: State<'_, Arc<Core>>) -> Result<Snapshot, String> {
    core.snapshot()
}
#[tauri::command]
fn get_messages(
    core: State<'_, Arc<Core>>,
    peer_id: String,
    limit: usize,
) -> Result<Vec<Message>, String> {
    core.store.lock().messages(&peer_id, limit.clamp(1, 1000))
}
#[tauri::command]
fn mark_read(
    app: tauri::AppHandle,
    core: State<'_, Arc<Core>>,
    peer_id: String,
) -> Result<(), String> {
    let viewing = app
        .get_webview_window("main")
        .map(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false))
        .unwrap_or(false);
    if !viewing {
        return Ok(());
    }
    let store = core.store.lock();
    if store.unread(&peer_id) > 0 {
        store.mark_read(&peer_id)?;
        drop(store);
        core.changed();
    }
    Ok(())
}
#[tauri::command]
fn rename_device(core: State<'_, Arc<Core>>, name: String) -> Result<(), String> {
    core.rename(&name)
}
#[tauri::command]
async fn add_peer(core: State<'_, Arc<Core>>, address: String) -> Result<String, String> {
    network::add_address(core.inner(), &address).await
}
#[tauri::command]
async fn start_pair(core: State<'_, Arc<Core>>, peer_id: String) -> Result<String, String> {
    network::begin_pair(core.inner().clone(), &peer_id).await
}
#[tauri::command]
fn confirm_pair(core: State<'_, Arc<Core>>, id: String, accept: bool) -> Result<(), String> {
    network::confirm_pair(core.inner(), &id, accept)
}
#[tauri::command]
fn forget_peer(core: State<'_, Arc<Core>>, peer_id: String) -> Result<(), String> {
    core.store.lock().forget(&peer_id)?;
    core.changed();
    Ok(())
}
#[tauri::command]
async fn send_message(
    core: State<'_, Arc<Core>>,
    peer_id: String,
    text: String,
    image_ids: Vec<String>,
) -> Result<Message, String> {
    network::send(core.inner().clone(), peer_id, text, image_ids).await
}
#[tauri::command]
async fn retry_message(
    core: State<'_, Arc<Core>>,
    peer_id: String,
    id: String,
) -> Result<Message, String> {
    network::retry(core.inner().clone(), peer_id, id).await
}
#[tauri::command]
async fn stage_image(
    core: State<'_, Arc<Core>>,
    bytes: Vec<u8>,
    name: String,
) -> Result<Attachment, String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || core.stage(&bytes, &name))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn image_url(core: State<'_, Arc<Core>>, id: String) -> Result<String, String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || core.image_url(&id))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn choose_images(
    app: tauri::AppHandle,
    core: State<'_, Arc<Core>>,
) -> Result<Vec<Attachment>, String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let paths = app
            .dialog()
            .file()
            .add_filter("图片", &["png", "jpg", "jpeg", "gif", "webp"])
            .blocking_pick_files()
            .unwrap_or_default();
        if paths.len() > MAX_IMAGES {
            return Err("每条消息最多 8 张图片".into());
        }
        paths
            .into_iter()
            .map(|p| {
                let path = p.into_path().map_err(|e| e.to_string())?;
                let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
                if size > MAX_IMAGE as u64 {
                    return Err("单张图片最多 15 MB".into());
                }
                let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                core.stage(
                    &bytes,
                    &path.file_name().unwrap_or_default().to_string_lossy(),
                )
            })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn stage_paths(
    core: State<'_, Arc<Core>>,
    paths: Vec<String>,
) -> Result<Vec<Attachment>, String> {
    if paths.len() > MAX_IMAGES {
        return Err("每条消息最多 8 张图片".into());
    }
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        paths
            .into_iter()
            .map(|p| {
                let path = std::path::PathBuf::from(p);
                let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
                if size > MAX_IMAGE as u64 {
                    return Err("单张图片最多 15 MB".into());
                }
                let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                core.stage(
                    &bytes,
                    &path.file_name().unwrap_or_default().to_string_lossy(),
                )
            })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn clipboard_image(core: State<'_, Arc<Core>>) -> Result<Option<Attachment>, String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        let image = match clipboard.get_image() {
            Ok(image) => image,
            Err(arboard::Error::ContentNotAvailable) => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        if image.width.saturating_mul(image.height) > 25_000_000 {
            return Err("图片尺寸过大".into());
        }
        let rgba = image::RgbaImage::from_raw(
            image.width as u32,
            image.height as u32,
            image.bytes.into_owned(),
        )
        .ok_or("截图读取失败")?;
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(rgba)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        core.stage(bytes.get_ref(), "截图.png").map(Some)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn save_image(
    app: tauri::AppHandle,
    core: State<'_, Arc<Core>>,
    id: String,
) -> Result<bool, String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let attachment = core.attachment(&id)?;
        let Some(file) = app
            .dialog()
            .file()
            .set_file_name(&attachment.name)
            .blocking_save_file()
        else {
            return Ok(false);
        };
        let path = file.into_path().map_err(|e| e.to_string())?;
        let bytes = core.image_bytes(&id)?;
        std::fs::write(path, bytes).map_err(|e| e.to_string())?;
        Ok(true)
    })
    .await
    .map_err(|e| e.to_string())?
}

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_state,
            get_messages,
            mark_read,
            rename_device,
            add_peer,
            start_pair,
            confirm_pair,
            forget_peer,
            send_message,
            retry_message,
            stage_image,
            image_url,
            choose_images,
            stage_paths,
            clipboard_image,
            save_image
        ])
        .setup(|app| {
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
            use tauri_plugin_notification::NotificationExt;
            let root = app.path().app_data_dir()?;
            let handle = app.handle().clone();
            let events: core::Events = Arc::new(move |name, payload| {
                let _ = handle.emit(name, &payload);
                if name == "message-received"
                    && !handle
                        .get_webview_window("main")
                        .map(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false))
                        .unwrap_or(false)
                {
                    let _ = handle
                        .notification()
                        .builder()
                        .title("PCMessage")
                        .body("收到一条新消息")
                        .show();
                }
            });
            let core = Core::create(root, DEFAULT_PORT, events).map_err(std::io::Error::other)?;
            let running =
                tauri::async_runtime::block_on(network::Running::start(core.clone(), true))
                    .map_err(std::io::Error::other)?;
            app.manage(core);
            app.manage(parking_lot::Mutex::new(Some(running)));
            let show = MenuItem::with_id(app, "show", "打开 PCMessage", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let mut tray = TrayIconBuilder::new()
                .tooltip("PCMessage")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if let Some(w) = tray.app_handle().get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        });
    let app = builder
        .build(tauri::generate_context!())
        .expect("PCMessage 启动失败");
    app.run(|_app, _event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = _event {
            if let Some(window) = _app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
    });
}
