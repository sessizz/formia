//! Virtual file system: real paths plus paths that reach inside .zip archives,
//! written the way Windows Explorer shows them (`D:\models\pack.zip\parts\a.stl`).

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use zip::ZipArchive;

pub type Res<T> = Result<T, String>;

#[derive(Debug, Clone, PartialEq)]
pub enum Loc {
    Real(PathBuf),
    /// `inner` uses '/' separators, no leading or trailing slash; "" is the archive root.
    Zip { archive: PathBuf, inner: String },
}

#[derive(Debug, Clone)]
pub struct Meta {
    pub size: u64,
    /// Milliseconds since the Unix epoch.
    pub mtime: i64,
    pub is_dir: bool,
    /// Extra identity for cache keys (zip entry CRC).
    pub tag: u64,
}

#[derive(Debug, Clone)]
pub struct ZipEntryInfo {
    pub name: String,
    pub size: u64,
    pub mtime: i64,
    pub crc: u32,
    pub is_dir: bool,
    pub index: usize,
}

#[derive(Debug)]
pub struct ZipIndex {
    pub entries: Vec<ZipEntryInfo>,
    by_name: HashMap<String, usize>,
}

pub fn is_archive_name(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".zip")
}

fn is_archive(p: &Path) -> bool {
    p.file_name().map(|n| is_archive_name(&n.to_string_lossy())).unwrap_or(false)
}

/// Resolves a user-visible path. A real .zip file resolves to `Real`; use
/// [`as_container`] when the archive should be treated as a folder.
pub fn resolve(path: &str) -> Res<Loc> {
    let p = PathBuf::from(path);
    if fs::metadata(&p).is_ok() {
        return Ok(Loc::Real(p));
    }
    let mut anc = p.parent();
    while let Some(a) = anc {
        if let Ok(m) = fs::metadata(a) {
            if m.is_file() && is_archive(a) {
                let rest = p.strip_prefix(a).map_err(|e| e.to_string())?;
                let inner = rest.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/");
                return Ok(Loc::Zip { archive: a.to_path_buf(), inner });
            }
            break;
        }
        anc = a.parent();
    }
    Err(format!("Yol bulunamadı: {path}"))
}

/// Turns a real archive file into its root folder location.
pub fn as_container(loc: Loc) -> Loc {
    match loc {
        Loc::Real(p) if is_archive(&p) && p.is_file() => Loc::Zip { archive: p, inner: String::new() },
        other => other,
    }
}

pub fn display_path(loc: &Loc) -> PathBuf {
    match loc {
        Loc::Real(p) => p.clone(),
        Loc::Zip { archive, inner } => {
            let mut p = archive.clone();
            for seg in inner.split('/').filter(|s| !s.is_empty()) {
                p.push(seg);
            }
            p
        }
    }
}

pub fn file_mtime_ms(m: &fs::Metadata) -> i64 {
    m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0)
}

pub fn stat(loc: &Loc) -> Res<Meta> {
    match loc {
        Loc::Real(p) => {
            let m = fs::metadata(p).map_err(|e| e.to_string())?;
            Ok(Meta { size: m.len(), mtime: file_mtime_ms(&m), is_dir: m.is_dir(), tag: 0 })
        }
        Loc::Zip { archive, inner } => {
            let idx = zip_index(archive)?;
            if inner.is_empty() {
                return Ok(Meta { size: 0, mtime: 0, is_dir: true, tag: 0 });
            }
            match idx.get(inner) {
                Some(e) => Ok(Meta { size: e.size, mtime: e.mtime, is_dir: e.is_dir, tag: e.crc as u64 }),
                None if idx.has_dir(inner) => Ok(Meta { size: 0, mtime: 0, is_dir: true, tag: 0 }),
                None => Err(format!("Arşivde bulunamadı: {inner}")),
            }
        }
    }
}

/// File contents, memory-mapped when possible.
pub enum Bytes {
    Map(memmap2::Mmap),
    Vec(Vec<u8>),
}

impl Deref for Bytes {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Bytes::Map(m) => m,
            Bytes::Vec(v) => v,
        }
    }
}

pub fn read_all(loc: &Loc) -> Res<Bytes> {
    match loc {
        Loc::Real(p) => {
            let f = File::open(p).map_err(|e| e.to_string())?;
            let len = f.metadata().map(|m| m.len()).unwrap_or(0);
            if len > 0 {
                // SAFETY: read-only map; a concurrent writer can at worst give us garbage geometry.
                if let Ok(m) = unsafe { memmap2::Mmap::map(&f) } {
                    return Ok(Bytes::Map(m));
                }
            }
            fs::read(p).map(Bytes::Vec).map_err(|e| e.to_string())
        }
        Loc::Zip { archive, inner } => read_zip_entry(archive, inner, u64::MAX).map(Bytes::Vec),
    }
}

/// Reads at most `max` bytes from the start of the file.
pub fn read_head(loc: &Loc, max: u64) -> Res<Vec<u8>> {
    match loc {
        Loc::Real(p) => {
            let f = File::open(p).map_err(|e| e.to_string())?;
            let mut buf = Vec::new();
            f.take(max).read_to_end(&mut buf).map_err(|e| e.to_string())?;
            Ok(buf)
        }
        Loc::Zip { archive, inner } => read_zip_entry(archive, inner, max),
    }
}

/// Reads `len` bytes at `start` (real files seek; zip entries are decompressed).
pub fn read_range(loc: &Loc, start: u64, len: u64) -> Res<Vec<u8>> {
    match loc {
        Loc::Real(p) => {
            let mut f = File::open(p).map_err(|e| e.to_string())?;
            f.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
            let mut buf = Vec::with_capacity(len.min(64 << 20) as usize);
            f.take(len).read_to_end(&mut buf).map_err(|e| e.to_string())?;
            Ok(buf)
        }
        Loc::Zip { .. } => {
            let all = read_all(loc)?;
            let s = (start as usize).min(all.len());
            let e = (s + len as usize).min(all.len());
            Ok(all[s..e].to_vec())
        }
    }
}

fn read_zip_entry(archive: &Path, inner: &str, max: u64) -> Res<Vec<u8>> {
    let idx = zip_index(archive)?;
    let e = idx.get(inner).ok_or_else(|| format!("Arşivde bulunamadı: {inner}"))?;
    if e.is_dir {
        return Err("Bu bir klasör".into());
    }
    let f = File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = ZipArchive::new(BufReader::new(f)).map_err(|e| e.to_string())?;
    let entry = zip.by_index(e.index).map_err(|e| format!("Arşiv girdisi açılamadı: {e}"))?;
    let mut buf = Vec::with_capacity(e.size.min(max).min(1 << 30) as usize);
    entry.take(max).read_to_end(&mut buf).map_err(|e| format!("Arşiv girdisi okunamadı: {e}"))?;
    Ok(buf)
}

impl ZipIndex {
    pub fn get(&self, inner: &str) -> Option<&ZipEntryInfo> {
        self.by_name.get(&inner.to_ascii_lowercase()).map(|&i| &self.entries[i])
    }

    pub fn has_dir(&self, inner: &str) -> bool {
        let prefix = format!("{}/", inner.to_ascii_lowercase());
        self.entries.iter().any(|e| e.name.to_ascii_lowercase().starts_with(&prefix))
    }

    /// Direct children of `inner` ("" = root): (name, entry or None for implicit dirs).
    pub fn children(&self, inner: &str) -> Vec<(String, Option<&ZipEntryInfo>)> {
        let prefix = if inner.is_empty() { String::new() } else { format!("{}/", inner.to_ascii_lowercase()) };
        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut out: Vec<(String, Option<&ZipEntryInfo>)> = Vec::new();
        for e in &self.entries {
            let lower = e.name.to_ascii_lowercase();
            if !lower.starts_with(&prefix) || lower.len() == prefix.len() {
                continue;
            }
            let rest = &e.name[prefix.len()..];
            let (first, deeper) = match rest.find('/') {
                Some(i) => (&rest[..i], true),
                None => (rest, false),
            };
            if first.is_empty() {
                continue;
            }
            let key = first.to_ascii_lowercase();
            match seen.get(&key) {
                Some(&i) => {
                    if !deeper && out[i].1.is_none() {
                        out[i].1 = Some(e);
                    }
                }
                None => {
                    seen.insert(key, out.len());
                    out.push((first.to_string(), if deeper { None } else { Some(e) }));
                }
            }
        }
        out
    }
}

type ZipCache = Mutex<HashMap<PathBuf, (i64, u64, Arc<ZipIndex>)>>;

fn cache() -> &'static ZipCache {
    static C: OnceLock<ZipCache> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn zip_index(archive: &Path) -> Res<Arc<ZipIndex>> {
    let m = fs::metadata(archive).map_err(|e| e.to_string())?;
    let (mtime, size) = (file_mtime_ms(&m), m.len());
    if let Some((t, s, idx)) = cache().lock().unwrap().get(archive) {
        if *t == mtime && *s == size {
            return Ok(idx.clone());
        }
    }
    let f = File::open(archive).map_err(|e| e.to_string())?;
    let zip = ZipArchive::new(BufReader::new(f)).map_err(|e| format!("Arşiv okunamadı: {e}"))?;
    let mut entries = Vec::with_capacity(zip.len());
    let mut by_name = HashMap::with_capacity(zip.len());
    for i in 0..zip.len() {
        let Ok(e) = zip.by_index_data(i) else { continue };
        let Ok(raw) = e.name() else { continue };
        let name = raw.replace('\\', "/").trim_matches('/').to_string();
        if name.is_empty() || name.split('/').any(|s| s == "..") {
            continue;
        }
        let info = ZipEntryInfo {
            is_dir: e.is_dir(),
            size: e.size(),
            mtime: e.last_modified().map(|d| civil_to_ms(d.year() as i64, d.month() as i64, d.day() as i64, d.hour() as i64, d.minute() as i64, d.second() as i64)).unwrap_or(mtime),
            crc: e.crc32(),
            index: i,
            name,
        };
        by_name.insert(info.name.to_ascii_lowercase(), entries.len());
        entries.push(info);
    }
    let idx = Arc::new(ZipIndex { entries, by_name });
    let mut c = cache().lock().unwrap();
    if c.len() > 24 {
        c.clear();
    }
    c.insert(archive.to_path_buf(), (mtime, size, idx.clone()));
    Ok(idx)
}

/// Days-from-civil (Howard Hinnant) to Unix milliseconds; zip times are local, treated as UTC.
fn civil_to_ms(y: i64, m: i64, d: i64, hh: i64, mm: i64, ss: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    ((days * 86400) + hh * 3600 + mm * 60 + ss) * 1000
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mg-vfs-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_zip(path: &Path, files: &[(&str, &[u8])]) {
        let f = File::create(path).unwrap();
        let mut w = zip::ZipWriter::new(f);
        for (n, b) in files {
            if n.ends_with('/') {
                w.add_directory(n.trim_end_matches('/'), zip::write::SimpleFileOptions::default()).unwrap();
            } else {
                w.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
                w.write_all(b).unwrap();
            }
        }
        w.finish().unwrap();
    }

    #[test]
    fn civil_epoch() {
        assert_eq!(civil_to_ms(1970, 1, 1, 0, 0, 0), 0);
        assert_eq!(civil_to_ms(2000, 3, 1, 0, 0, 1), 951868801000);
    }

    #[test]
    fn resolves_and_lists_inside_zip() {
        let d = temp_dir("list");
        let z = d.join("Pack.zip");
        write_zip(&z, &[("readme.txt", b"hi"), ("parts/a.stl", b"solid"), ("parts/sub/b.obj", b"v"), ("empty/", b"")]);

        let root = as_container(resolve(z.to_str().unwrap()).unwrap());
        assert_eq!(root, Loc::Zip { archive: z.clone(), inner: String::new() });

        let idx = zip_index(&z).unwrap();
        let mut names: Vec<_> = idx.children("").into_iter().map(|(n, e)| (n, e.map(|e| e.is_dir))).collect();
        names.sort();
        assert_eq!(names, vec![("empty".into(), Some(true)), ("parts".into(), None), ("readme.txt".into(), Some(false))]);
        let parts: Vec<_> = idx.children("parts").into_iter().map(|(n, _)| n).collect();
        assert_eq!(parts.len(), 2);

        let inner = resolve(z.join("parts").join("a.stl").to_str().unwrap()).unwrap();
        assert_eq!(inner, Loc::Zip { archive: z.clone(), inner: "parts/a.stl".into() });
        assert_eq!(&*read_all(&inner).unwrap(), b"solid");
        assert!(stat(&resolve(z.join("parts").to_str().unwrap()).unwrap()).unwrap().is_dir);
        assert_eq!(display_path(&inner), z.join("parts").join("a.stl"));
    }

    #[test]
    fn missing_paths_error() {
        let d = temp_dir("missing");
        assert!(resolve(d.join("nope").join("x.stl").to_str().unwrap()).is_err());
    }
}
