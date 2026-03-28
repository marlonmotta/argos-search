//! Argos Search — Tauri Desktop App
//! System tray, close-to-tray, global shortcut, file actions, scope presets.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use argos_core::engine::SearchOptions;
use argos_core::{ArgosConfig, ArgosEngine, SearchScope};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, State, WindowEvent,
};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
};

struct AppState {
    engine: Arc<Mutex<Option<ArgosEngine>>>,
    scope: Mutex<SearchScope>,
    custom_roots: Mutex<Vec<String>>,
}

#[derive(Serialize, Clone)]
struct StatusResponse {
    ready: bool,
    message: String,
    indexed_count: u64,
    roots: Vec<String>,
    scope: String,
}

#[derive(Serialize, Clone)]
struct IndexResponse {
    success: bool,
    message: String,
    indexed: u64,
    skipped: u64,
    errors: u64,
    took_ms: u64,
    total_files: u64,
}

#[derive(Deserialize)]
struct SearchRequest {
    query: String,
    limit: Option<usize>,
}

#[derive(Serialize)]
struct SearchHitResponse {
    path: String,
    name: String,
    score: f32,
    size_bytes: Option<u64>,
    modified: Option<String>,
    extension: Option<String>,
}

#[derive(Serialize)]
struct SearchResponse {
    query: String,
    total_hits: usize,
    took_ms: u64,
    hits: Vec<SearchHitResponse>,
}

// ─── Preferences ─────────────────────────────────────────────

#[derive(Serialize, Deserialize, Default)]
struct UserPrefs {
    scope: Option<String>,
    custom_roots: Vec<String>,
    shortcut: Option<String>,
    mode: Option<String>,
}

impl UserPrefs {
    fn path() -> PathBuf {
        ArgosConfig::global_data_dir().join("preferences.json")
    }
    fn load() -> Self {
        let path = Self::path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            Self::default()
        }
    }
    fn save(&self) {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}

// ─── Engine helpers ──────────────────────────────────────────

fn build_engine_from_scope_and_custom(
    scope: &SearchScope,
    custom_roots: &[String],
) -> Result<ArgosEngine, String> {
    let config = ArgosConfig::load_global().unwrap_or_default();
    let mut roots = scope.roots();
    for cr in custom_roots {
        let p = PathBuf::from(cr);
        if p.exists() && !roots.contains(&p) {
            roots.push(p);
        }
    }
    ArgosEngine::new_multi(roots, config).map_err(|e| e.to_string())
}

fn make_status(engine: &ArgosEngine, scope: &SearchScope) -> StatusResponse {
    let count = engine.indexed_count().unwrap_or(0);
    let roots: Vec<String> = engine
        .roots()
        .iter()
        .map(|r| r.to_string_lossy().to_string())
        .collect();
    StatusResponse {
        ready: true,
        message: format!("{} — {} dirs", scope.label(), roots.len()),
        indexed_count: count,
        roots,
        scope: format!("{:?}", scope),
    }
}

// ─── Tauri Commands ──────────────────────────────────────────

#[tauri::command]
fn init_engine(state: State<AppState>) -> Result<StatusResponse, String> {
    let prefs = UserPrefs::load();
    if let Some(ref s) = prefs.scope {
        *state.scope.lock().unwrap() = SearchScope::from_str_name(s);
    }
    *state.custom_roots.lock().unwrap() = prefs.custom_roots.clone();
    let scope = *state.scope.lock().unwrap();
    let custom = state.custom_roots.lock().unwrap().clone();
    match build_engine_from_scope_and_custom(&scope, &custom) {
        Ok(engine) => {
            let resp = make_status(&engine, &scope);
            *state.engine.lock().unwrap() = Some(engine);
            Ok(resp)
        }
        Err(e) => Ok(StatusResponse {
            ready: false,
            message: format!("Error: {}", e),
            indexed_count: 0,
            roots: vec![],
            scope: format!("{:?}", scope),
        }),
    }
}

#[tauri::command]
fn set_scope(scope_name: String, state: State<AppState>) -> Result<StatusResponse, String> {
    let new_scope = SearchScope::from_str_name(&scope_name);
    *state.scope.lock().unwrap() = new_scope;
    let custom = state.custom_roots.lock().unwrap().clone();
    let mut prefs = UserPrefs::load();
    prefs.scope = Some(scope_name);
    prefs.save();
    match build_engine_from_scope_and_custom(&new_scope, &custom) {
        Ok(engine) => {
            let resp = make_status(&engine, &new_scope);
            *state.engine.lock().unwrap() = Some(engine);
            Ok(resp)
        }
        Err(e) => Ok(StatusResponse {
            ready: false,
            message: format!("Error: {}", e),
            indexed_count: 0,
            roots: vec![],
            scope: format!("{:?}", new_scope),
        }),
    }
}

#[tauri::command]
fn add_root(path: String, state: State<AppState>) -> Result<StatusResponse, String> {
    let new_root = PathBuf::from(&path);
    if !new_root.exists() {
        let scope = *state.scope.lock().unwrap();
        return Ok(StatusResponse {
            ready: false,
            message: format!("Not found: {}", path),
            indexed_count: 0,
            roots: vec![],
            scope: format!("{:?}", scope),
        });
    }
    {
        let mut custom = state.custom_roots.lock().unwrap();
        if !custom.contains(&path) {
            custom.push(path.clone());
        }
    }
    let custom = state.custom_roots.lock().unwrap().clone();
    let mut prefs = UserPrefs::load();
    prefs.custom_roots = custom.clone();
    prefs.save();
    let scope = *state.scope.lock().unwrap();
    match build_engine_from_scope_and_custom(&scope, &custom) {
        Ok(engine) => {
            let resp = make_status(&engine, &scope);
            *state.engine.lock().unwrap() = Some(engine);
            Ok(resp)
        }
        Err(e) => Ok(StatusResponse {
            ready: false,
            message: format!("Error: {}", e),
            indexed_count: 0,
            roots: vec![],
            scope: format!("{:?}", scope),
        }),
    }
}

#[tauri::command]
fn remove_root(path: String, state: State<AppState>) -> Result<StatusResponse, String> {
    {
        let mut custom = state.custom_roots.lock().unwrap();
        custom.retain(|r| r != &path);
    }
    let custom = state.custom_roots.lock().unwrap().clone();
    let mut prefs = UserPrefs::load();
    prefs.custom_roots = custom.clone();
    prefs.save();
    let scope = *state.scope.lock().unwrap();
    match build_engine_from_scope_and_custom(&scope, &custom) {
        Ok(engine) => {
            let resp = make_status(&engine, &scope);
            *state.engine.lock().unwrap() = Some(engine);
            Ok(resp)
        }
        Err(e) => Ok(StatusResponse {
            ready: false,
            message: format!("Error: {}", e),
            indexed_count: 0,
            roots: vec![],
            scope: format!("{:?}", scope),
        }),
    }
}

#[tauri::command]
fn get_custom_roots(state: State<AppState>) -> Vec<String> {
    state.custom_roots.lock().unwrap().clone()
}

#[tauri::command]
async fn index_build(state: State<'_, AppState>) -> Result<IndexResponse, String> {
    let engine_arc = state.engine.clone();
    let result = tokio::task::spawn_blocking(move || {
        let guard = engine_arc.lock().unwrap();
        let engine = guard
            .as_ref()
            .ok_or_else(|| "Engine not initialized".to_string())?;
        engine.index_build().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Thread error: {}", e))?;
    match result {
        Ok(stats) => Ok(IndexResponse {
            success: true,
            message: "Index complete".into(),
            indexed: stats.indexed,
            skipped: stats.skipped,
            errors: stats.errors,
            took_ms: stats.took_ms,
            total_files: stats.total_found,
        }),
        Err(e) => Ok(IndexResponse {
            success: false,
            message: format!("Error: {}", e),
            indexed: 0,
            skipped: 0,
            errors: 0,
            took_ms: 0,
            total_files: 0,
        }),
    }
}

#[tauri::command]
fn search(request: SearchRequest, state: State<AppState>) -> Result<SearchResponse, String> {
    let guard = state.engine.lock().unwrap();
    let engine = guard.as_ref().ok_or("Engine not initialized")?;
    let options = SearchOptions::default().with_limit(request.limit.unwrap_or(50));
    match engine.search(&request.query, &options) {
        Ok(result) => {
            let hits: Vec<SearchHitResponse> = result
                .hits
                .into_iter()
                .map(|h| {
                    let extension = std::path::Path::new(&h.path)
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.to_lowercase());
                    SearchHitResponse {
                        path: h.path,
                        name: h.name,
                        score: h.score,
                        size_bytes: h.size_bytes,
                        modified: h.modified,
                        extension,
                    }
                })
                .collect();
            Ok(SearchResponse {
                query: result.query,
                total_hits: result.total_hits,
                took_ms: result.took_ms,
                hits,
            })
        }
        Err(e) => Err(format!("Search error: {}", e)),
    }
}

#[tauri::command]
fn open_file(path: String) -> Result<bool, String> {
    open::that(&path).map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
fn open_containing_folder(path: String) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg("/select,")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let p = std::path::Path::new(&path);
        let folder = p.parent().unwrap_or(p);
        open::that(folder.to_string_lossy().to_string()).map_err(|e| e.to_string())?;
    }
    Ok(true)
}

#[tauri::command]
fn get_shortcut() -> String {
    UserPrefs::load()
        .shortcut
        .unwrap_or_else(|| "Ctrl + Space".to_string())
}

#[tauri::command]
fn set_shortcut(shortcut: String) -> Result<bool, String> {
    let mut prefs = UserPrefs::load();
    prefs.shortcut = Some(shortcut);
    prefs.save();
    Ok(true)
}

#[tauri::command]
fn pick_folder() -> Result<Option<String>, String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Add-Type -AssemblyName System.Windows.Forms; $f = New-Object System.Windows.Forms.FolderBrowserDialog; $f.Description = 'Add folder to Argos Search'; if ($f.ShowDialog() -eq 'OK') { $f.SelectedPath }",
            ])
            .output()
            .map_err(|e| e.to_string())?;
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(if path.is_empty() { None } else { Some(path) })
    }
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let output = Command::new("osascript")
            .args([
                "-e",
                "POSIX path of (choose folder with prompt \"Add folder to Argos Search\")",
            ])
            .output()
            .map_err(|e| e.to_string())?;
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(if path.is_empty() { None } else { Some(path) })
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Ok(None)
    }
}

#[tauri::command]
fn get_status(state: State<AppState>) -> Result<StatusResponse, String> {
    let guard = state.engine.lock().unwrap();
    let scope = *state.scope.lock().unwrap();
    match guard.as_ref() {
        Some(engine) => Ok(make_status(engine, &scope)),
        None => Ok(StatusResponse {
            ready: false,
            message: "Not initialized".into(),
            indexed_count: 0,
            roots: vec![],
            scope: format!("{:?}", scope),
        }),
    }
}

#[tauri::command]
fn get_launch_mode() -> String {
    UserPrefs::load()
        .mode
        .unwrap_or_else(|| "launcher".to_string())
}

#[tauri::command]
fn set_launch_mode(mode: String) -> Result<bool, String> {
    let mut prefs = UserPrefs::load();
    prefs.mode = Some(mode);
    prefs.save();
    Ok(true)
}

#[tauri::command]
fn hide_launcher_window(app_handle: tauri::AppHandle) {
    if let Some(window) = app_handle.get_webview_window("launcher") {
        let _ = window.hide();
    }
}

// ─── Shortcut parser ─────────────────────────────────────────

fn parse_shortcut_string(s: &str) -> Option<Shortcut> {
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    let mut mods = Modifiers::empty();
    let mut key_code: Option<Code> = None;

    for part in &parts {
        match part.to_lowercase().as_str() {
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "alt" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            "super" | "meta" | "win" => mods |= Modifiers::META,
            "space" => key_code = Some(Code::Space),
            "enter" | "return" => key_code = Some(Code::Enter),
            "tab" => key_code = Some(Code::Tab),
            "backspace" => key_code = Some(Code::Backspace),
            "delete" => key_code = Some(Code::Delete),
            "escape" | "esc" => key_code = Some(Code::Escape),
            "f1" => key_code = Some(Code::F1),
            "f2" => key_code = Some(Code::F2),
            "f3" => key_code = Some(Code::F3),
            "f4" => key_code = Some(Code::F4),
            "f5" => key_code = Some(Code::F5),
            "f6" => key_code = Some(Code::F6),
            "f7" => key_code = Some(Code::F7),
            "f8" => key_code = Some(Code::F8),
            "f9" => key_code = Some(Code::F9),
            "f10" => key_code = Some(Code::F10),
            "f11" => key_code = Some(Code::F11),
            "f12" => key_code = Some(Code::F12),
            s if s.len() == 1 => {
                let ch = s.chars().next().unwrap().to_ascii_uppercase();
                key_code = match ch {
                    'A' => Some(Code::KeyA), 'B' => Some(Code::KeyB),
                    'C' => Some(Code::KeyC), 'D' => Some(Code::KeyD),
                    'E' => Some(Code::KeyE), 'F' => Some(Code::KeyF),
                    'G' => Some(Code::KeyG), 'H' => Some(Code::KeyH),
                    'I' => Some(Code::KeyI), 'J' => Some(Code::KeyJ),
                    'K' => Some(Code::KeyK), 'L' => Some(Code::KeyL),
                    'M' => Some(Code::KeyM), 'N' => Some(Code::KeyN),
                    'O' => Some(Code::KeyO), 'P' => Some(Code::KeyP),
                    'Q' => Some(Code::KeyQ), 'R' => Some(Code::KeyR),
                    'S' => Some(Code::KeyS), 'T' => Some(Code::KeyT),
                    'U' => Some(Code::KeyU), 'V' => Some(Code::KeyV),
                    'W' => Some(Code::KeyW), 'X' => Some(Code::KeyX),
                    'Y' => Some(Code::KeyY), 'Z' => Some(Code::KeyZ),
                    '0' => Some(Code::Digit0), '1' => Some(Code::Digit1),
                    '2' => Some(Code::Digit2), '3' => Some(Code::Digit3),
                    '4' => Some(Code::Digit4), '5' => Some(Code::Digit5),
                    '6' => Some(Code::Digit6), '7' => Some(Code::Digit7),
                    '8' => Some(Code::Digit8), '9' => Some(Code::Digit9),
                    _ => None,
                };
            }
            _ => {}
        }
    }

    key_code.map(|code| {
        if mods.is_empty() {
            Shortcut::new(None, code)
        } else {
            Shortcut::new(Some(mods), code)
        }
    })
}

// ─── Main ────────────────────────────────────────────────────

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState {
            engine: Arc::new(Mutex::new(None)),
            scope: Mutex::new(SearchScope::Personal),
            custom_roots: Mutex::new(Vec::new()),
        })
        .setup(|app| {
            // ── System Tray ──
            let show_item =
                MenuItem::with_id(app, "show", "👁️ Show Argos", true, None::<&str>)?;
            let quit_item =
                MenuItem::with_id(app, "quit", "❌ Quit", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&tray_menu)
                .tooltip("Argos Search")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // ── Global Shortcut ──
            let prefs = UserPrefs::load();
            let shortcut_str = prefs.shortcut.unwrap_or_else(|| "Ctrl + Space".to_string());
            if let Some(shortcut) = parse_shortcut_string(&shortcut_str) {
                let _ = app.global_shortcut().on_shortcut(shortcut, |app, _scut, event| {
                    if event.state == ShortcutState::Pressed {
                        let prefs = UserPrefs::load();
                        let mode = prefs.mode.unwrap_or_else(|| "launcher".to_string());
                        let window_label = if mode == "launcher" { "launcher" } else { "main" };
                        
                        if let Some(window) = app.get_webview_window(window_label) {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                // Esconde a outra janela por precaução
                                let other_label = if mode == "launcher" { "main" } else { "launcher" };
                                if let Some(other) = app.get_webview_window(other_label) {
                                    let _ = other.hide();
                                }
                                
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                            }
                        }
                    }
                });
            }

            Ok(())
        })
        // ── Window events: close → tray (clear search), minimize → tray (keep search) ──
        .on_window_event(|window, event| {
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.emit("clear-search", ());
                    let _ = window.hide();
                }
                _ => {
                    // Check for minimize → also go to tray
                    if window.is_minimized().unwrap_or(false) {
                        let _ = window.hide();
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            init_engine,
            set_scope,
            add_root,
            remove_root,
            get_custom_roots,
            index_build,
            search,
            get_status,
            pick_folder,
            open_file,
            open_containing_folder,
            set_shortcut,
            get_launch_mode,
            set_launch_mode,
            hide_launcher_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Argos Search");
}
