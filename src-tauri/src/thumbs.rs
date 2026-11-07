//! Thumbnail service behind the `thumb://` protocol.
//!
//! Requests go through two LIFO stages so the most recently requested (i.e.
//! currently visible) items are served first:
//! 1. lookup threads stat the source and answer straight from the disk cache;
//! 2. render workers produce missing thumbnails and write them to the cache.

use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};
use tauri::http::Response;
use tauri::UriSchemeResponder;

use crate::mesh::{self, Kind};
use crate::{gcode, raster, vfs};

pub const THUMB_SIZE: u32 = 384;
const VERSION: u32 = 3;
const MAX_PENDING: usize = 800;
const BIG_FILE: u64 = 150 << 20;
const MAX_IMAGE: u64 = 256 << 20;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Category {
    Mesh(Kind),
    Image,
    Gcode,
    /// Rendered by the web view (three.js loaders) and uploaded back.
    Client,
    Unsupported,
}

pub fn category(name: &str) -> Category {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    if let Some(k) = Kind::from_ext(&ext) {
        return Category::Mesh(k);
    }
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tga" => Category::Image,
        "gcode" | "gco" | "g" => Category::Gcode,
        "fbx" | "dae" | "3ds" | "amf" | "wrl" | "vrml" | "step" | "stp" | "iges" | "igs" | "brep" => Category::Client,
        _ => Category::Unsupported,
    }
}

struct Lookup {
    vpath: String,
    responder: UriSchemeResponder,
}

struct Render {
    key: String,
    loc: vfs::Loc,
    name: String,
    meta: vfs::Meta,
}

struct Stack<T> {
    items: Mutex<Vec<T>>,
    cv: Condvar,
}

impl<T> Stack<T> {
    fn new() -> Self {
        Stack { items: Mutex::new(Vec::new()), cv: Condvar::new() }
    }
    /// Pushes and returns the oldest item if the stack overflowed.
    fn push(&self, item: T) -> Option<T> {
        let mut v = self.items.lock().unwrap();
        v.push(item);
        let dropped = if v.len() > MAX_PENDING { Some(v.remove(0)) } else { None };
        self.cv.notify_one();
        dropped
    }
    fn pop(&self) -> T {
        let mut v = self.items.lock().unwrap();
        loop {
            if let Some(x) = v.pop() {
                return x;
            }
            v = self.cv.wait(v).unwrap();
        }
    }
}

pub struct ThumbService {
    dir: PathBuf,
    prefer_embedded: AtomicBool,
    lookups: Stack<Lookup>,
    renders: Stack<Render>,
    waiting: Mutex<HashMap<String, Vec<UriSchemeResponder>>>,
    big: Mutex<()>,
}

pub enum CacheState {
    Hit,
    Failed,
    Miss,
}

impl ThumbService {
    pub fn start(dir: PathBuf) -> Arc<ThumbService> {
        let _ = fs::create_dir_all(&dir);
        let svc = Arc::new(ThumbService {
            dir,
            prefer_embedded: AtomicBool::new(true),
            lookups: Stack::new(),
            renders: Stack::new(),
            waiting: Mutex::new(HashMap::new()),
            big: Mutex::new(()),
        });
        for _ in 0..2 {
            let s = svc.clone();
            std::thread::Builder::new().name("thumb-lookup".into()).spawn(move || loop {
                let job = s.lookups.pop();
                s.lookup(job);
            }).expect("thread");
        }
        let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).saturating_sub(1).clamp(2, 8);
        for _ in 0..workers {
            let s = svc.clone();
            std::thread::Builder::new().name("thumb-render".into()).spawn(move || loop {
                let job = s.renders.pop();
                s.render(job);
            }).expect("thread");
        }
        svc
    }

    pub fn set_prefer_embedded(&self, v: bool) {
        self.prefer_embedded.store(v, Ordering::Relaxed);
    }

    pub fn request(&self, vpath: String, responder: UriSchemeResponder) {
        if let Some(old) = self.lookups.push(Lookup { vpath, responder }) {
            old.responder.respond(status(503));
        }
    }

    fn key_for(&self, vpath: &str, meta: &vfs::Meta) -> String {
        let pe = self.prefer_embedded.load(Ordering::Relaxed);
        let mut s = format!("{VERSION}|{THUMB_SIZE}|{pe}|{}|{}|{}|{vpath}", meta.size, meta.mtime, meta.tag);
        // Re-render only 3MFs after embedded-image handling changed (flat backdrop removal);
        // keys of every other file stay the same so their cached thumbnails are kept.
        if vpath.to_ascii_lowercase().ends_with(".3mf") {
            s.push_str("|embed2");
        }
        blake3::hash(s.as_bytes()).to_hex().to_string()
    }

    fn paths(&self, key: &str) -> (PathBuf, PathBuf) {
        let d = self.dir.join(&key[..2]);
        (d.join(format!("{key}.png")), d.join(format!("{key}.none")))
    }

    pub fn cache_state(&self, vpath: &str) -> vfs::Res<(String, CacheState)> {
        let loc = vfs::resolve(vpath)?;
        let meta = vfs::stat(&loc)?;
        let key = self.key_for(vpath, &meta);
        let (png, none) = self.paths(&key);
        let st = if png.exists() {
            CacheState::Hit
        } else if none.exists() {
            CacheState::Failed
        } else {
            CacheState::Miss
        };
        Ok((key, st))
    }

    /// Stores a thumbnail rendered by the web view (or marks it as failed).
    pub fn store(&self, vpath: &str, png: Option<&[u8]>) -> vfs::Res<()> {
        let (key, _) = self.cache_state(vpath)?;
        self.write(&key, png);
        Ok(())
    }

    fn write(&self, key: &str, png: Option<&[u8]>) {
        let (p, n) = self.paths(key);
        if let Some(parent) = p.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match png {
            Some(b) => {
                let tmp = p.with_extension("tmp");
                if fs::write(&tmp, b).is_ok() {
                    let _ = fs::rename(&tmp, &p);
                }
            }
            None => {
                let _ = fs::write(&n, b"");
            }
        }
    }

    fn lookup(&self, job: Lookup) {
        let name = job.vpath.rsplit(['\\', '/']).next().unwrap_or("").to_string();
        let cat = category(&name);
        if cat == Category::Unsupported {
            return job.responder.respond(status(404));
        }
        let Ok(loc) = vfs::resolve(&job.vpath) else { return job.responder.respond(status(404)) };
        let Ok(meta) = vfs::stat(&loc) else { return job.responder.respond(status(404)) };
        if meta.is_dir {
            return job.responder.respond(status(404));
        }
        let key = self.key_for(&job.vpath, &meta);
        let (png, none) = self.paths(&key);
        if let Ok(bytes) = fs::read(&png) {
            return job.responder.respond(image_response(bytes));
        }
        if none.exists() || cat == Category::Client {
            return job.responder.respond(status(404));
        }
        let mut waiting = self.waiting.lock().unwrap();
        if let Some(list) = waiting.get_mut(&key) {
            list.push(job.responder);
            return;
        }
        waiting.insert(key.clone(), vec![job.responder]);
        drop(waiting);
        if let Some(old) = self.renders.push(Render { key, loc, name, meta }) {
            for r in self.waiting.lock().unwrap().remove(&old.key).unwrap_or_default() {
                r.respond(status(503));
            }
        }
    }

    fn render(&self, job: Render) {
        let pe = self.prefer_embedded.load(Ordering::Relaxed);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.generate(&job, pe))).unwrap_or(None);
        self.write(&job.key, result.as_deref());
        let responders = self.waiting.lock().unwrap().remove(&job.key).unwrap_or_default();
        for r in responders {
            match &result {
                Some(b) => r.respond(image_response(b.clone())),
                None => r.respond(status(404)),
            }
        }
    }

    fn generate(&self, job: &Render, prefer_embedded: bool) -> Option<Vec<u8>> {
        match category(&job.name) {
            Category::Mesh(kind) => {
                let _guard = if job.meta.size > BIG_FILE { Some(self.big.lock().unwrap_or_else(|e| e.into_inner())) } else { None };
                let bytes = vfs::read_all(&job.loc).ok()?;
                if kind == Kind::ThreeMf && prefer_embedded {
                    if let Some(png) = mesh::threemf::embedded_thumbnail(&bytes).and_then(|b| embedded_thumb(&b)) {
                        return Some(png);
                    }
                }
                let base = match &job.loc {
                    vfs::Loc::Real(p) => p.parent().map(|p| p.to_path_buf()),
                    _ => None,
                };
                let m = mesh::parse(kind, &bytes, base.as_deref()).ok()?;
                drop(bytes);
                let img = raster::render(&m, THUMB_SIZE)?;
                encode_png(img.as_raw(), img.width(), img.height())
            }
            Category::Gcode => {
                let head = vfs::read_head(&job.loc, 8 << 20).ok()?;
                image_thumb(&gcode::embedded_thumbnail(&head)?)
            }
            Category::Image => {
                if job.meta.size > MAX_IMAGE {
                    return None;
                }
                image_thumb(&vfs::read_all(&job.loc).ok()?)
            }
            Category::Client | Category::Unsupported => None,
        }
    }

    /// Small PNG of the cached thumbnail, used as the drag-and-drop image.
    pub fn drag_icon(&self, vpath: &str, size: u32) -> Option<Vec<u8>> {
        let (key, state) = self.cache_state(vpath).ok()?;
        if !matches!(state, CacheState::Hit) {
            return None;
        }
        let bytes = fs::read(self.paths(&key).0).ok()?;
        let img = image::load_from_memory(&bytes).ok()?.thumbnail(size, size).to_rgba8();
        encode_png(img.as_raw(), img.width(), img.height())
    }

    pub fn cache_size(&self) -> (u64, u64) {
        let mut bytes = 0;
        let mut files = 0;
        for e in walkdir::WalkDir::new(&self.dir).into_iter().flatten() {
            if e.file_type().is_file() {
                files += 1;
                bytes += e.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
        (bytes, files)
    }

    pub fn clear(&self) {
        let _ = fs::remove_dir_all(&self.dir);
        let _ = fs::create_dir_all(&self.dir);
    }
}

pub fn image_thumb(bytes: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory(bytes).ok()?;
    let img = if img.width() > THUMB_SIZE || img.height() > THUMB_SIZE { img.thumbnail(THUMB_SIZE, THUMB_SIZE) } else { img };
    let rgba = img.to_rgba8();
    encode_png(rgba.as_raw(), rgba.width(), rgba.height())
}

/// Thumbnail from an image a slicer embedded in a 3MF. Older Bambu Studio builds
/// render the plate on an opaque flat grey backdrop; that backdrop is removed so
/// these match the transparent renders around them.
pub fn embedded_thumb(bytes: &[u8]) -> Option<Vec<u8>> {
    embedded_thumb_sized(bytes, THUMB_SIZE)
}

/// [`embedded_thumb`] scaled to fit `max` pixels (plate list in the preview).
pub fn embedded_thumb_sized(bytes: &[u8], max: u32) -> Option<Vec<u8>> {
    let mut rgba = image::load_from_memory(bytes).ok()?.to_rgba8();
    clear_flat_background(&mut rgba);
    let img = image::DynamicImage::ImageRgba8(rgba);
    let img = if img.width() > max || img.height() > max { img.thumbnail(max, max) } else { img };
    let rgba = img.to_rgba8();
    encode_png(rgba.as_raw(), rgba.width(), rgba.height())
}

/// Makes a flat, opaque backdrop transparent. It only acts when the image has no
/// transparency yet and nearly its whole border is a single colour (so photos are
/// left alone). The backdrop is flood-filled from the border, so parts of the model
/// in a similar colour survive. The one-pixel rim around it gets a soft alpha.
fn clear_flat_background(img: &mut image::RgbaImage) {
    const TOL: i32 = 6;
    let (w, h) = img.dimensions();
    if w < 3 || h < 3 {
        return;
    }
    let border: Vec<(u32, u32)> = (0..w).flat_map(|x| [(x, 0), (x, h - 1)]).chain((1..h - 1).flat_map(|y| [(0, y), (w - 1, y)])).collect();
    if border.iter().any(|&(x, y)| img.get_pixel(x, y)[3] < 250) {
        return;
    }
    let median = |c: usize| {
        let mut v: Vec<u8> = border.iter().map(|&(x, y)| img.get_pixel(x, y)[c]).collect();
        v.sort_unstable();
        v[v.len() / 2] as i32
    };
    let bg = [median(0), median(1), median(2)];
    let diff = |p: &image::Rgba<u8>| (0..3).map(|c| (p[c] as i32 - bg[c]).abs()).max().unwrap_or(0);
    let flat = border.iter().filter(|&&(x, y)| diff(img.get_pixel(x, y)) <= TOL).count();
    if flat * 10 < border.len() * 9 {
        return;
    }

    let idx = |x: u32, y: u32| (y * w + x) as usize;
    let mut is_bg = vec![false; (w * h) as usize];
    let mut stack: Vec<(u32, u32)> = border.into_iter().filter(|&(x, y)| diff(img.get_pixel(x, y)) <= TOL).collect();
    while let Some((x, y)) = stack.pop() {
        if is_bg[idx(x, y)] || diff(img.get_pixel(x, y)) > TOL {
            continue;
        }
        is_bg[idx(x, y)] = true;
        if x > 0 { stack.push((x - 1, y)); }
        if x + 1 < w { stack.push((x + 1, y)); }
        if y > 0 { stack.push((x, y - 1)); }
        if y + 1 < h { stack.push((x, y + 1)); }
    }

    for y in 0..h {
        for x in 0..w {
            if is_bg[idx(x, y)] {
                continue;
            }
            let touches_bg = (x > 0 && is_bg[idx(x - 1, y)]) || (x + 1 < w && is_bg[idx(x + 1, y)]) || (y > 0 && is_bg[idx(x, y - 1)]) || (y + 1 < h && is_bg[idx(x, y + 1)]);
            if touches_bg {
                // Colour-to-alpha: treat the rim pixel as the part colour blended over the backdrop.
                let p = *img.get_pixel(x, y);
                let a = (0..3)
                    .map(|c| {
                        let d = p[c] as f32 - bg[c] as f32;
                        let room = if d > 0.0 { 255.0 - bg[c] as f32 } else { bg[c] as f32 };
                        if room > 0.0 { d.abs() / room } else { 0.0 }
                    })
                    .fold(0.0f32, f32::max)
                    .clamp(0.0, 1.0);
                if a > 0.0 && a < 1.0 {
                    let un = |c: usize| (bg[c] as f32 + (p[c] as f32 - bg[c] as f32) / a).round().clamp(0.0, 255.0) as u8;
                    img.put_pixel(x, y, image::Rgba([un(0), un(1), un(2), (a * 255.0).round() as u8]));
                }
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            if is_bg[idx(x, y)] {
                img.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));
            }
        }
    }
}

fn encode_png(raw: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    PngEncoder::new_with_quality(&mut out, CompressionType::Fast, FilterType::Adaptive).write_image(raw, w, h, ExtendedColorType::Rgba8).ok()?;
    Some(out.into_inner())
}

fn image_response(bytes: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(200)
        .header("Content-Type", "image/png")
        .header("Cache-Control", "max-age=604800")
        .header("Access-Control-Allow-Origin", "*")
        .body(bytes)
        .unwrap()
}

pub fn status(code: u16) -> Response<Vec<u8>> {
    Response::builder()
        .status(code)
        .header("Cache-Control", "no-store")
        .header("Access-Control-Allow-Origin", "*")
        .body(Vec::new())
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories() {
        assert_eq!(category("Benchy.STL"), Category::Mesh(Kind::Stl));
        assert_eq!(category("a.gcode"), Category::Gcode);
        assert_eq!(category("plate.3mf"), Category::Mesh(Kind::ThreeMf));
        assert_eq!(category("part.step"), Category::Client);
        assert_eq!(category("photo.JPG"), Category::Image);
        assert_eq!(category("notes.txt"), Category::Unsupported);
        assert_eq!(category("noext"), Category::Unsupported);
    }

    #[test]
    fn image_thumb_downscales() {
        let img = image::RgbaImage::from_pixel(1000, 500, image::Rgba([10, 20, 30, 255]));
        let png = encode_png(img.as_raw(), 1000, 500).unwrap();
        let t = image::load_from_memory(&image_thumb(&png).unwrap()).unwrap();
        assert_eq!((t.width(), t.height()), (THUMB_SIZE, THUMB_SIZE / 2));
    }

    const BG: image::Rgba<u8> = image::Rgba([231, 231, 231, 255]);

    /// Older Bambu Studio plate render: green part on an opaque flat grey backdrop,
    /// with a one-pixel anti-aliased rim and a backdrop-coloured hole inside the part.
    fn opaque_plate() -> image::RgbaImage {
        let mut img = image::RgbaImage::from_pixel(64, 64, BG);
        for y in 16..48 {
            for x in 16..48 {
                let rim = x == 16 || x == 47 || y == 16 || y == 47;
                img.put_pixel(x, y, if rim { image::Rgba([135, 195, 135, 255]) } else { image::Rgba([40, 160, 40, 255]) });
            }
        }
        for y in 28..36 {
            for x in 28..36 {
                img.put_pixel(x, y, BG);
            }
        }
        img
    }

    #[test]
    fn flat_backdrop_becomes_transparent() {
        let mut img = opaque_plate();
        clear_flat_background(&mut img);
        assert_eq!(img.get_pixel(0, 0)[3], 0, "corner");
        assert_eq!(img.get_pixel(10, 40)[3], 0, "backdrop next to the part");
        assert_eq!(*img.get_pixel(30, 20), image::Rgba([40, 160, 40, 255]), "part interior untouched");
        let rim = img.get_pixel(16, 30);
        assert!(rim[3] > 0 && rim[3] < 255, "anti-aliased rim is feathered, got alpha {}", rim[3]);
        assert!(rim[1] > rim[0] + 60, "rim keeps the part colour instead of the grey halo: {rim:?}");
        assert_eq!(img.get_pixel(31, 31)[3], 255, "enclosed hole is not reachable from the border and stays");
    }

    #[test]
    fn transparent_or_busy_images_are_left_alone() {
        let mut transparent = opaque_plate();
        transparent.put_pixel(0, 0, image::Rgba([0, 0, 0, 0]));
        let before = transparent.clone();
        clear_flat_background(&mut transparent);
        assert_eq!(transparent, before, "already has transparency");

        // Designer cover photo: the border is not one flat colour.
        let mut photo = image::RgbaImage::from_fn(64, 64, |x, y| image::Rgba([(x * 4) as u8, (y * 4) as u8, 120, 255]));
        let before = photo.clone();
        clear_flat_background(&mut photo);
        assert_eq!(photo, before, "busy border");
    }

    /// Manual check against real files:
    /// `FORMIA_3MF="/path/a.3mf" cargo test --lib real_3mf_thumb -- --ignored --nocapture`
    /// writes `<file>.thumb.png` next to each listed file (separate several with `;`).
    #[test]
    #[ignore]
    fn real_3mf_thumb() {
        let list = std::env::var("FORMIA_3MF").expect("set FORMIA_3MF");
        for path in list.split(';').filter(|p| !p.is_empty()) {
            let data = fs::read(path).unwrap();
            let Some(png) = mesh::threemf::embedded_thumbnail(&data).and_then(|b| embedded_thumb(&b)) else {
                println!("{path}: no embedded image (rendered from the mesh instead)");
                continue;
            };
            let t = image::load_from_memory(&png).unwrap().to_rgba8();
            let clear = t.pixels().filter(|p| p[3] == 0).count() * 100 / t.pixels().count();
            fs::write(format!("{path}.thumb.png"), &png).unwrap();
            println!("{path}: {}x{}, {clear}% transparent", t.width(), t.height());
        }
    }

    #[test]
    fn embedded_thumb_clears_backdrop_and_downscales() {
        let img = image::imageops::resize(&opaque_plate(), 512, 512, image::imageops::FilterType::Nearest);
        let png = encode_png(img.as_raw(), 512, 512).unwrap();
        let t = image::load_from_memory(&embedded_thumb(&png).unwrap()).unwrap().to_rgba8();
        assert_eq!((t.width(), t.height()), (THUMB_SIZE, THUMB_SIZE));
        assert_eq!(t.get_pixel(2, 2)[3], 0);
        assert_eq!(t.get_pixel(THUMB_SIZE / 2 - 30, THUMB_SIZE / 2 - 60)[3], 255);
    }
}
