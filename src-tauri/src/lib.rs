mod fs_ops;
mod gcode;
mod mesh;
mod migrate;
mod raster;
mod scan;
mod thumbs;
mod vfs;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine;
use percent_encoding::percent_decode_str;
use serde::Serialize;
use tauri::http::{Request, Response};
use tauri::ipc::Channel;
use tauri::{Manager, State};

use thumbs::ThumbService;

struct AppState {
    thumbs: Arc<ThumbService>,
    config_dir: PathBuf,
    scans: Mutex<HashMap<u32, Arc<AtomicBool>>>,
    next_scan: AtomicU32,
}

type CmdResult<T> = Result<T, String>;

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> CmdResult<T> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Places {
    quick: Vec<fs_ops::Place>,
    drives: Vec<fs_ops::Drive>,
    os: &'static str,
    sep: char,
}

#[tauri::command]
async fn get_places() -> CmdResult<Places> {
    blocking(|| Ok(Places { quick: fs_ops::places(), drives: fs_ops::drives(), os: std::env::consts::OS, sep: std::path::MAIN_SEPARATOR })).await
}

#[tauri::command]
async fn list_dir(path: String, show_hidden: bool) -> CmdResult<fs_ops::Listing> {
    blocking(move || fs_ops::list_dir(&path, show_hidden)).await
}

#[tauri::command]
async fn scan_start(state: State<'_, AppState>, root: String, exts: Vec<String>, show_hidden: bool, include_archives: bool, on_event: Channel<scan::ScanEvent>) -> CmdResult<u32> {
    let id = state.next_scan.fetch_add(1, Ordering::Relaxed);
    let cancel = Arc::new(AtomicBool::new(false));
    state.scans.lock().unwrap().insert(id, cancel.clone());
    let opts = scan::Options { exts: exts.into_iter().map(|e| e.to_ascii_lowercase()).collect::<HashSet<_>>(), show_hidden, include_archives };
    std::thread::spawn(move || {
        scan::run(&root, &opts, &cancel, |ev| {
            let _ = on_event.send(ev);
        });
    });
    Ok(id)
}

#[tauri::command]
async fn scan_cancel(state: State<'_, AppState>, id: u32) -> CmdResult<()> {
    if let Some(flag) = state.scans.lock().unwrap().remove(&id) {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

/// Real file for a virtual path; entries inside archives are extracted to a temp folder.
fn materialize(path: &str) -> CmdResult<PathBuf> {
    match vfs::resolve(path)? {
        vfs::Loc::Real(p) => Ok(p),
        loc @ vfs::Loc::Zip { .. } => {
            let bytes = vfs::read_all(&loc)?;
            let name = vfs::display_path(&loc).file_name().map(|n| n.to_owned()).ok_or("dosya adı yok")?;
            let dir = std::env::temp_dir().join("formia").join(&blake3::hash(path.as_bytes()).to_hex()[..16]);
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let out = dir.join(name);
            std::fs::write(&out, &*bytes).map_err(|e| e.to_string())?;
            Ok(out)
        }
    }
}

#[tauri::command]
async fn open_path(path: String) -> CmdResult<()> {
    blocking(move || {
        let p = materialize(&path)?;
        tauri_plugin_opener::open_path(p, None::<&str>).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
async fn reveal_path(path: String) -> CmdResult<()> {
    blocking(move || {
        let target = match vfs::resolve(&path)? {
            vfs::Loc::Real(p) => p,
            vfs::Loc::Zip { archive, .. } => archive,
        };
        tauri_plugin_opener::reveal_item_in_dir(target).map_err(|e| e.to_string())
    })
    .await
}

/// Extracts an archive entry (or returns the real path) so it can be dragged to other apps.
#[tauri::command]
async fn real_path(path: String) -> CmdResult<String> {
    blocking(move || materialize(&path).map(|p| p.to_string_lossy().into_owned())).await
}

#[derive(Serialize)]
struct DeleteFailure {
    path: String,
    error: String,
}

#[derive(Serialize)]
struct DeleteReport {
    deleted: Vec<String>,
    failed: Vec<DeleteFailure>,
}

/// Moves files and folders to the Recycle Bin (Trash on macOS).
#[tauri::command]
async fn delete_paths(paths: Vec<String>) -> CmdResult<DeleteReport> {
    blocking(move || {
        let mut report = DeleteReport { deleted: Vec::new(), failed: Vec::new() };
        let mut real = Vec::new();
        for p in paths {
            match vfs::resolve(&p) {
                Ok(vfs::Loc::Real(_)) => real.push(p),
                Ok(vfs::Loc::Zip { .. }) => report.failed.push(DeleteFailure { path: p, error: "Arşiv içindeki dosyalar silinemez".into() }),
                Err(e) => report.failed.push(DeleteFailure { path: p, error: e }),
            }
        }
        if real.is_empty() {
            return Ok(report);
        }
        // One shell operation for the whole batch; fall back to one by one to report failures.
        if trash::delete_all(&real).is_ok() {
            report.deleted = real;
        } else {
            for p in real {
                match trash::delete(&p) {
                    Ok(()) => report.deleted.push(p),
                    Err(e) => report.failed.push(DeleteFailure { path: p, error: e.to_string() }),
                }
            }
        }
        Ok(report)
    })
    .await
}

/// Renames a file or folder in place and returns its new path.
#[tauri::command]
async fn rename_path(path: String, new_name: String) -> CmdResult<String> {
    blocking(move || {
        let name = new_name.trim();
        const BAD: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
        if name.is_empty() || name == "." || name == ".." || name.contains(BAD) || name.chars().any(char::is_control) {
            return Err("Geçersiz ad: \\ / : * ? \" < > | karakterleri kullanılamaz".into());
        }
        if cfg!(windows) && name.ends_with('.') {
            return Err("Ad nokta ile bitemez".into());
        }
        let vfs::Loc::Real(src) = vfs::resolve(&path)? else {
            return Err("Arşiv içindeki dosyalar yeniden adlandırılamaz".into());
        };
        let dst = src.with_file_name(name);
        if dst == src {
            return Ok(path);
        }
        let only_case = dst.to_string_lossy().to_lowercase() == src.to_string_lossy().to_lowercase();
        if !only_case && std::fs::symlink_metadata(&dst).is_ok() {
            return Err(format!("“{name}” adında bir öğe zaten var"));
        }
        std::fs::rename(&src, &dst).map_err(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied => "Erişim reddedildi (dosya başka bir programda açık olabilir)".to_string(),
            _ => e.to_string(),
        })?;
        Ok(dst.to_string_lossy().into_owned())
    })
    .await
}

const DRAG_ICON: &[u8] = include_bytes!("../icons/128x128.png");

/// Starts a native drag of the given files out of the window (e.g. into a slicer).
#[tauri::command]
async fn start_drag(app: tauri::AppHandle, window: tauri::WebviewWindow, state: State<'_, AppState>, paths: Vec<String>) -> CmdResult<()> {
    let svc = state.thumbs.clone();
    let (files, icon) = blocking(move || {
        let files = paths.iter().take(200).map(|p| materialize(p)).collect::<CmdResult<Vec<_>>>()?;
        let icon = paths.first().and_then(|p| svc.drag_icon(p, 112)).unwrap_or_else(|| DRAG_ICON.to_vec());
        Ok((files, icon))
    })
    .await?;
    app.run_on_main_thread(move || {
        if let Err(e) = drag::start_drag(&window, drag::DragItem::Files(files), drag::Image::Raw(icon), |_, _| {}, drag::Options::default()) {
            eprintln!("drag failed: {e}");
        }
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn thumb_state(state: State<'_, AppState>, path: String) -> CmdResult<&'static str> {
    let svc = state.thumbs.clone();
    blocking(move || {
        Ok(match svc.cache_state(&path)?.1 {
            thumbs::CacheState::Hit => "hit",
            thumbs::CacheState::Failed => "failed",
            thumbs::CacheState::Miss => "miss",
        })
    })
    .await
}

#[tauri::command]
async fn thumb_store(state: State<'_, AppState>, path: String, png_base64: Option<String>) -> CmdResult<()> {
    let svc = state.thumbs.clone();
    blocking(move || {
        let bytes = match png_base64 {
            Some(b) => Some(base64::engine::general_purpose::STANDARD.decode(b.as_bytes()).map_err(|e| e.to_string())?),
            None => None,
        };
        svc.store(&path, bytes.as_deref())
    })
    .await
}

#[derive(Serialize)]
struct CacheInfo {
    bytes: u64,
    files: u64,
}

#[tauri::command]
async fn cache_info(state: State<'_, AppState>) -> CmdResult<CacheInfo> {
    let svc = state.thumbs.clone();
    blocking(move || {
        let (bytes, files) = svc.cache_size();
        Ok(CacheInfo { bytes, files })
    })
    .await
}

#[tauri::command]
async fn cache_clear(state: State<'_, AppState>) -> CmdResult<()> {
    let svc = state.thumbs.clone();
    blocking(move || {
        svc.clear();
        Ok(())
    })
    .await
}

#[derive(Serialize)]
struct PlateOut {
    id: u32,
    name: String,
    objects: usize,
    /// Slicer render of the plate as a PNG data URL (only when there are several plates).
    thumb: Option<String>,
    /// Sliced export: the preview shows this plate's G-code as a finished print.
    toolpath: bool,
}

/// Build plates of a 3MF project. The preview lists them when there are two or more;
/// `toolpath` tells it a sliced `.gcode.3mf` is drawn from G-code.
#[tauri::command]
async fn plate_list(path: String) -> CmdResult<Vec<PlateOut>> {
    blocking(move || {
        let loc = vfs::resolve(&path)?;
        let bytes = vfs::read_all(&loc)?;
        let plates = mesh::threemf::plates(&bytes);
        let with_thumbs = plates.len() > 1;
        let b64 = base64::engine::general_purpose::STANDARD;
        Ok(plates
            .into_iter()
            .map(|p| {
                let file = p.thumb.clone().unwrap_or_else(|| format!("Metadata/plate_{}.png", p.id));
                let thumb = with_thumbs
                    .then(|| mesh::threemf::entry(&bytes, &file))
                    .flatten()
                    .and_then(|img| thumbs::embedded_thumb_sized(&img, 192))
                    .map(|png| format!("data:image/png;base64,{}", b64.encode(png)));
                PlateOut { id: p.id, name: p.name, objects: p.objects, thumb, toolpath: p.gcode }
            })
            .collect())
    })
    .await
}

#[tauri::command]
async fn set_prefer_embedded(state: State<'_, AppState>, value: bool) -> CmdResult<()> {
    state.thumbs.set_prefer_embedded(value);
    Ok(())
}

#[tauri::command]
async fn get_settings(state: State<'_, AppState>) -> CmdResult<serde_json::Value> {
    let p = state.config_dir.join("settings.json");
    Ok(std::fs::read(&p).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(serde_json::Value::Null))
}

#[tauri::command]
async fn save_settings(state: State<'_, AppState>, value: serde_json::Value) -> CmdResult<()> {
    let dir = state.config_dir.clone();
    blocking(move || {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let tmp = dir.join("settings.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, dir.join("settings.json")).map_err(|e| e.to_string())
    })
    .await
}

fn request_path(req: &Request<Vec<u8>>) -> String {
    let p = req.uri().path().trim_start_matches('/');
    percent_decode_str(p).decode_utf8_lossy().into_owned()
}

fn mime_for(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "txt" | "md" | "log" | "ini" | "cfg" | "csv" | "scad" | "gcode" | "gco" | "json" | "xml" | "yaml" | "yml" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn error_response(code: u16, msg: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(code)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("Access-Control-Allow-Origin", "*")
        .header("Cache-Control", "no-store")
        .body(msg.as_bytes().to_vec())
        .unwrap()
}

/// Serves raw file bytes (with HTTP Range support for media).
fn raw_response(req: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let path = request_path(req);
    let loc = match vfs::resolve(&path) {
        Ok(l) => l,
        Err(e) => return error_response(404, &e),
    };
    let meta = match vfs::stat(&loc) {
        Ok(m) if !m.is_dir => m,
        Ok(_) => return error_response(400, "klasör"),
        Err(e) => return error_response(404, &e),
    };
    let mime = mime_for(&path);
    let range = req.headers().get("range").and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("bytes=")).and_then(|v| {
        let (s, e) = v.split_once('-')?;
        let start: u64 = s.trim().parse().ok()?;
        let end: u64 = if e.trim().is_empty() { meta.size.saturating_sub(1) } else { e.trim().parse().ok()? };
        Some((start, end.min(meta.size.saturating_sub(1))))
    });
    let builder = Response::builder()
        .header("Content-Type", mime)
        .header("Access-Control-Allow-Origin", "*")
        .header("Accept-Ranges", "bytes")
        .header("Cache-Control", "no-store");
    match range {
        Some((start, end)) if start <= end && start < meta.size => {
            // Cap media chunks so huge videos stream instead of loading at once.
            let end = end.min(start + (8 << 20) - 1);
            match vfs::read_range(&loc, start, end - start + 1) {
                Ok(bytes) => builder
                    .status(206)
                    .header("Content-Range", format!("bytes {start}-{}/{}", start + bytes.len() as u64 - 1, meta.size))
                    .body(bytes)
                    .unwrap(),
                Err(e) => error_response(500, &e),
            }
        }
        _ => match vfs::read_all(&loc) {
            Ok(bytes) => builder.status(200).body(bytes.to_vec()).unwrap(),
            Err(e) => error_response(500, &e),
        },
    }
}

/// Parses a model natively and returns the compact mesh wire format.
fn mesh_response(req: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let path = request_path(req);
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    let gcode = matches!(ext.as_str(), "gcode" | "gco" | "g");
    let kind = mesh::Kind::from_ext(&ext);
    if kind.is_none() && !gcode {
        return error_response(415, "desteklenmeyen biçim");
    }
    let loc = match vfs::resolve(&path) {
        Ok(l) => l,
        Err(e) => return error_response(404, &e),
    };
    let bytes = match vfs::read_all(&loc) {
        Ok(b) => b,
        Err(e) => return error_response(500, &e),
    };
    let base = match &loc {
        vfs::Loc::Real(p) => p.parent().map(|p| p.to_path_buf()),
        _ => None,
    };
    // `mesh://…/project.3mf?plate=2` builds just that plate of a multi-plate project.
    let plate = req.uri().query().and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("plate=")?.parse::<u32>().ok()));
    // Sliced .gcode.3mf: the plate's G-code, drawn like a .gcode file.
    let sliced = if kind == Some(mesh::Kind::ThreeMf) { mesh::threemf::plate_gcode(&bytes, plate) } else { None };
    let ribbons = |g: &[u8]| {
        mesh::toolpath::parse(g).map(|mut m| {
            m.normalize_colors();
            m
        })
    };
    let parsed = match (kind, plate) {
        // G-code: the finished print as extrusion ribbons (too big → 422, the viewer draws lines).
        (None, _) => ribbons(&bytes),
        _ if sliced.is_some() => ribbons(sliced.as_deref().unwrap_or_default()),
        (Some(mesh::Kind::ThreeMf), Some(p)) => mesh::parse_3mf_plate(&bytes, p),
        (Some(k), _) => mesh::parse(k, &bytes, base.as_deref()),
    };
    match parsed {
        Ok(m) => Response::builder()
            .status(200)
            .header("Content-Type", "application/octet-stream")
            .header("Access-Control-Allow-Origin", "*")
            .header("Cache-Control", "no-store")
            .body(m.to_wire())
            .unwrap(),
        Err(e) => error_response(422, &e),
    }
}

#[cfg(windows)]
fn quiet_drive_errors() {
    use windows_sys::Win32::System::Diagnostics::Debug::SetErrorMode;
    // SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX: no "insert a disk" dialogs.
    unsafe { SetErrorMode(0x0001 | 0x8000) };
}

#[cfg(not(windows))]
fn quiet_drive_errors() {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    quiet_drive_errors();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // Size/position only: restoring DECORATIONS would bring back the native frame
        // that the Windows build turns off (tauri.windows.conf.json).
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(tauri_plugin_window_state::StateFlags::all() & !tauri_plugin_window_state::StateFlags::DECORATIONS)
                .build(),
        )
        .setup(|app| {
            let cache = app.path().app_cache_dir()?.join("thumbs");
            let config_dir = app.path().app_config_dir()?;
            migrate::carry_over(&config_dir, &cache);
            app.manage(AppState { thumbs: ThumbService::start(cache), config_dir, scans: Mutex::new(HashMap::new()), next_scan: AtomicU32::new(1) });
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol("thumb", |ctx, req, responder| {
            let state = ctx.app_handle().state::<AppState>();
            state.thumbs.request(request_path(&req), responder);
        })
        .register_asynchronous_uri_scheme_protocol("raw", |_ctx, req, responder| {
            std::thread::spawn(move || responder.respond(raw_response(&req)));
        })
        .register_asynchronous_uri_scheme_protocol("mesh", |_ctx, req, responder| {
            std::thread::spawn(move || responder.respond(mesh_response(&req)));
        })
        .invoke_handler(tauri::generate_handler![
            get_places,
            list_dir,
            scan_start,
            scan_cancel,
            open_path,
            reveal_path,
            real_path,
            delete_paths,
            rename_path,
            start_drag,
            thumb_state,
            thumb_store,
            cache_info,
            cache_clear,
            set_prefer_embedded,
            plate_list,
            get_settings,
            save_settings
        ])
        .run(tauri::generate_context!())
        .expect("uygulama başlatılamadı");
}
