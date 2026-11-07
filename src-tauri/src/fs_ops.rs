//! Folder listing, drives and well-known places.

use std::fs;
use std::path::PathBuf;

use serde::Serialize;

use crate::vfs::{self, Loc, Res};

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub mtime: i64,
    /// A .zip file that can be opened like a folder.
    pub archive: bool,
    pub hidden: bool,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub path: String,
    pub parent: Option<String>,
    pub in_archive: bool,
    pub entries: Vec<Entry>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub id: &'static str,
    pub name: String,
    pub path: String,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Drive {
    pub path: String,
    pub label: String,
    pub kind: &'static str,
    pub total: u64,
    pub free: u64,
}

/// "C:" -> "C:\", strips quotes and redundant trailing separators.
pub fn normalize(path: &str) -> String {
    let p = path.trim().trim_matches('"').to_string();
    #[cfg(windows)]
    {
        let p = p.replace('/', "\\");
        let b = p.as_bytes();
        if b.len() == 2 && b[1] == b':' {
            return format!("{p}\\");
        }
        if b.len() > 3 && p.ends_with('\\') && !p.starts_with("\\\\?\\") {
            return p.trim_end_matches('\\').to_string();
        }
        p
    }
    #[cfg(not(windows))]
    {
        if p.len() > 1 && p.ends_with('/') {
            return p.trim_end_matches('/').to_string();
        }
        p
    }
}

pub fn is_hidden_meta(name: &str, m: &fs::Metadata) -> bool {
    if name.starts_with('.') {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const HIDDEN: u32 = 0x2;
        const SYSTEM: u32 = 0x4;
        if m.file_attributes() & (HIDDEN | SYSTEM) != 0 {
            return true;
        }
    }
    let _ = m;
    false
}

pub fn list_dir(path: &str, show_hidden: bool) -> Res<Listing> {
    let path = normalize(path);
    let loc = vfs::as_container(vfs::resolve(&path)?);
    match &loc {
        Loc::Real(dir) => {
            let rd = fs::read_dir(dir).map_err(|e| friendly_io(&e))?;
            let mut entries = Vec::new();
            for de in rd.flatten() {
                let name = de.file_name().to_string_lossy().into_owned();
                let Ok(mut meta) = de.metadata() else { continue };
                let hidden = is_hidden_meta(&name, &meta);
                if hidden && !show_hidden {
                    continue;
                }
                if meta.file_type().is_symlink() {
                    if let Ok(m) = fs::metadata(de.path()) {
                        meta = m;
                    }
                }
                let is_dir = meta.is_dir();
                entries.push(Entry {
                    archive: !is_dir && vfs::is_archive_name(&name),
                    path: de.path().to_string_lossy().into_owned(),
                    size: if is_dir { 0 } else { meta.len() },
                    mtime: vfs::file_mtime_ms(&meta),
                    is_dir,
                    hidden,
                    name,
                });
            }
            Ok(Listing {
                parent: dir.parent().map(|p| p.to_string_lossy().into_owned()),
                path: dir.to_string_lossy().into_owned(),
                in_archive: false,
                entries,
            })
        }
        Loc::Zip { archive, inner } => {
            let idx = vfs::zip_index(archive)?;
            if !inner.is_empty() && !idx.has_dir(inner) && idx.get(inner).map_or(true, |e| !e.is_dir) {
                return Err("Bu bir klasör değil".into());
            }
            let here = vfs::display_path(&loc);
            let entries = idx
                .children(inner)
                .into_iter()
                .filter(|(name, _)| show_hidden || !name.starts_with('.') && name != "__MACOSX")
                .map(|(name, e)| {
                    let is_dir = e.map_or(true, |e| e.is_dir);
                    Entry {
                        path: here.join(&name).to_string_lossy().into_owned(),
                        size: e.map_or(0, |e| e.size),
                        mtime: e.map_or(0, |e| e.mtime),
                        archive: false,
                        hidden: name.starts_with('.'),
                        is_dir,
                        name,
                    }
                })
                .collect();
            Ok(Listing {
                parent: here.parent().map(|p| p.to_string_lossy().into_owned()),
                path: here.to_string_lossy().into_owned(),
                in_archive: true,
                entries,
            })
        }
    }
}

fn friendly_io(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => "Bu klasöre erişim izni yok".into(),
        std::io::ErrorKind::NotFound => "Klasör bulunamadı".into(),
        _ => e.to_string(),
    }
}

pub fn places() -> Vec<Place> {
    let mut out = Vec::new();
    let mut add = |id: &'static str, name: &str, p: Option<PathBuf>| {
        if let Some(p) = p {
            if p.is_dir() {
                out.push(Place { id, name: name.into(), path: p.to_string_lossy().into_owned() });
            }
        }
    };
    add("home", "Ana klasör", dirs::home_dir());
    add("desktop", "Masaüstü", dirs::desktop_dir());
    add("documents", "Belgeler", dirs::document_dir());
    add("downloads", "İndirilenler", dirs::download_dir());
    add("pictures", "Resimler", dirs::picture_dir());
    out
}

#[cfg(windows)]
pub fn drives() -> Vec<Drive> {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let mask = unsafe { GetLogicalDrives() };
    let mut roots = Vec::new();
    for i in 0..26u32 {
        if mask & (1 << i) != 0 {
            roots.push(format!("{}:\\", (b'A' + i as u8) as char));
        }
    }
    let (tx, rx) = mpsc::channel();
    for (order, root) in roots.iter().enumerate() {
        let tx = tx.clone();
        let root = root.clone();
        std::thread::spawn(move || {
            let w = wide(&root);
            let kind = match unsafe { GetDriveTypeW(w.as_ptr()) } {
                2 => "removable",
                3 => "fixed",
                4 => "network",
                5 => "cdrom",
                6 => "ram",
                _ => "unknown",
            };
            let mut label = [0u16; 261];
            let ok = unsafe { GetVolumeInformationW(w.as_ptr(), label.as_mut_ptr(), label.len() as u32, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), 0) };
            let label = if ok != 0 { String::from_utf16_lossy(&label[..label.iter().position(|&c| c == 0).unwrap_or(0)]) } else { String::new() };
            let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
            unsafe { GetDiskFreeSpaceExW(w.as_ptr(), &mut avail, &mut total, &mut free) };
            let _ = tx.send((order, Drive { path: root, label, kind, total, free: avail }));
        });
    }
    drop(tx);
    let deadline = Instant::now() + Duration::from_millis(1500);
    let mut found: Vec<Option<Drive>> = vec![None; roots.len()];
    while let Ok((i, d)) = rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        found[i] = Some(d);
    }
    roots
        .into_iter()
        .zip(found)
        .map(|(root, d)| d.unwrap_or(Drive { path: root, label: String::new(), kind: "network", total: 0, free: 0 }))
        .filter(|d| d.kind != "cdrom" || d.total > 0)
        .collect()
}

#[cfg(not(windows))]
pub fn drives() -> Vec<Drive> {
    drives_in(std::path::Path::new("/Volumes"))
}

/// "/" first, named after the /Volumes entry that points at it (e.g. "Macintosh HD",
/// as Finder shows it), then the other mounted volumes by name.
#[cfg(not(windows))]
fn drives_in(volumes: &std::path::Path) -> Vec<Drive> {
    let mut root_label = None;
    let mut others = Vec::new();
    if let Ok(rd) = fs::read_dir(volumes) {
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if fs::canonicalize(&p).ok().as_deref() == Some(std::path::Path::new("/")) {
                root_label = Some(name);
                continue;
            }
            others.push(Drive { path: p.to_string_lossy().into_owned(), label: name, kind: "removable", total: 0, free: 0 });
        }
    }
    others.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    let mut out = vec![Drive { path: "/".into(), label: root_label.unwrap_or_else(|| "Sistem".into()), kind: "fixed", total: 0, free: 0 }];
    out.extend(others);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_real_dir_and_zip() {
        let d = std::env::temp_dir().join(format!("mg-list-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("a.stl"), b"solid x").unwrap();
        fs::write(d.join(".hidden"), b"").unwrap();
        {
            use std::io::Write;
            let f = fs::File::create(d.join("pack.zip")).unwrap();
            let mut w = zip::ZipWriter::new(f);
            w.start_file("in/b.obj", zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(b"v 0 0 0").unwrap();
            w.finish().unwrap();
        }
        let l = list_dir(d.to_str().unwrap(), false).unwrap();
        let mut names: Vec<_> = l.entries.iter().map(|e| (e.name.clone(), e.is_dir, e.archive)).collect();
        names.sort();
        assert_eq!(names, vec![("a.stl".into(), false, false), ("pack.zip".into(), false, true), ("sub".into(), true, false)]);
        assert_eq!(list_dir(d.to_str().unwrap(), true).unwrap().entries.len(), 4);

        let z = list_dir(d.join("pack.zip").to_str().unwrap(), false).unwrap();
        assert!(z.in_archive);
        assert_eq!(z.entries.len(), 1);
        assert!(z.entries[0].is_dir);
        let inner = list_dir(&z.entries[0].path, false).unwrap();
        assert_eq!(inner.entries[0].name, "b.obj");
        assert_eq!(inner.entries[0].size, 7);
        assert_eq!(inner.parent.as_deref(), Some(d.join("pack.zip").to_str().unwrap()));
    }

    #[cfg(unix)]
    #[test]
    fn root_volume_takes_its_finder_name() {
        let v = std::env::temp_dir().join(format!("mg-volumes-{}", std::process::id()));
        let _ = fs::remove_dir_all(&v);
        fs::create_dir_all(v.join("USB Bellek")).unwrap();
        fs::create_dir_all(v.join("Formia")).unwrap();
        std::os::unix::fs::symlink("/", v.join("Macintosh HD")).unwrap();
        let d = drives_in(&v);
        let names: Vec<(&str, &str)> = d.iter().map(|d| (d.path.as_str(), d.label.as_str())).collect();
        assert_eq!(names[0], ("/", "Macintosh HD"));
        assert_eq!(names[1].1, "Formia");
        assert_eq!(names[2].1, "USB Bellek");
        assert_eq!(d.len(), 3, "the root link is not listed twice");
        let _ = fs::remove_dir_all(&v);
    }

    #[cfg(windows)]
    #[test]
    fn normalizes_drive_roots() {
        assert_eq!(normalize("c:"), "c:\\");
        assert_eq!(normalize("C:\\foo\\"), "C:\\foo");
        assert_eq!(normalize("\"D:/x/y\""), "D:\\x\\y");
        assert_eq!(normalize("C:\\"), "C:\\");
    }
}
