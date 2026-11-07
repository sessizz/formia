//! Recursive archive scan: walks a folder tree (and optionally the inside of
//! .zip files) and streams matching files back in batches.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::fs_ops::{is_hidden_meta, normalize, Entry};
use crate::vfs::{self, Loc};

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ScanEvent {
    #[serde(rename_all = "camelCase")]
    Batch { entries: Vec<Entry>, dirs: u64, files: u64 },
    #[serde(rename_all = "camelCase")]
    Done { dirs: u64, files: u64, matched: u64, cancelled: bool, error: Option<String> },
}

pub struct Options {
    pub exts: HashSet<String>,
    pub show_hidden: bool,
    pub include_archives: bool,
}

struct Sink<'a, F: FnMut(ScanEvent)> {
    emit: &'a mut F,
    batch: Vec<Entry>,
    last: Instant,
    dirs: u64,
    files: u64,
    matched: u64,
}

impl<F: FnMut(ScanEvent)> Sink<'_, F> {
    fn push(&mut self, e: Entry) {
        self.matched += 1;
        self.batch.push(e);
        self.maybe_flush();
    }
    fn maybe_flush(&mut self) {
        if self.batch.len() >= 300 || (!self.batch.is_empty() && self.last.elapsed() > Duration::from_millis(150)) {
            self.flush();
        }
    }
    fn flush(&mut self) {
        let entries = std::mem::take(&mut self.batch);
        (self.emit)(ScanEvent::Batch { entries, dirs: self.dirs, files: self.files });
        self.last = Instant::now();
    }
}

fn ext_of(name: &str) -> String {
    name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
}

pub fn run(root: &str, opts: &Options, cancel: &Arc<AtomicBool>, mut emit: impl FnMut(ScanEvent)) {
    let mut sink = Sink { emit: &mut emit, batch: Vec::new(), last: Instant::now(), dirs: 0, files: 0, matched: 0 };
    let error = match vfs::resolve(&normalize(root)).map(vfs::as_container) {
        Ok(Loc::Real(dir)) => {
            let walker = walkdir::WalkDir::new(&dir).follow_links(false).into_iter().filter_entry(|e| {
                e.depth() == 0 || opts.show_hidden || e.metadata().map(|m| !is_hidden_meta(&e.file_name().to_string_lossy(), &m)).unwrap_or(false)
            });
            for e in walker.flatten() {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                if e.file_type().is_dir() {
                    sink.dirs += 1;
                    sink.maybe_flush();
                    continue;
                }
                sink.files += 1;
                let name = e.file_name().to_string_lossy().into_owned();
                let ext = ext_of(&name);
                let is_zip = ext == "zip";
                if opts.exts.contains(&ext) || (is_zip && opts.exts.contains("zip")) {
                    let meta = e.metadata().ok();
                    sink.push(Entry {
                        path: e.path().to_string_lossy().into_owned(),
                        size: meta.as_ref().map_or(0, |m| m.len()),
                        mtime: meta.as_ref().map_or(0, vfs::file_mtime_ms),
                        is_dir: false,
                        archive: is_zip,
                        hidden: false,
                        name,
                    });
                } else if is_zip && opts.include_archives {
                    scan_zip(e.path(), "", opts, cancel, &mut sink);
                } else {
                    sink.maybe_flush();
                }
            }
            None
        }
        Ok(Loc::Zip { archive, inner }) => {
            scan_zip(&archive, &inner, opts, cancel, &mut sink);
            None
        }
        Err(e) => Some(e),
    };
    sink.flush();
    let (dirs, files, matched) = (sink.dirs, sink.files, sink.matched);
    emit(ScanEvent::Done { dirs, files, matched, cancelled: cancel.load(Ordering::Relaxed), error });
}

fn scan_zip<F: FnMut(ScanEvent)>(archive: &std::path::Path, inner: &str, opts: &Options, cancel: &Arc<AtomicBool>, sink: &mut Sink<F>) {
    let Ok(idx) = vfs::zip_index(archive) else { return };
    let prefix = if inner.is_empty() { String::new() } else { format!("{}/", inner.to_ascii_lowercase()) };
    for e in &idx.entries {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        if e.is_dir || !e.name.to_ascii_lowercase().starts_with(&prefix) {
            continue;
        }
        if !opts.show_hidden && e.name.split('/').any(|s| s.starts_with('.') || s == "__MACOSX") {
            continue;
        }
        sink.files += 1;
        let name = e.name.rsplit('/').next().unwrap_or(&e.name).to_string();
        if opts.exts.contains(&ext_of(&name)) {
            let path = vfs::display_path(&Loc::Zip { archive: archive.to_path_buf(), inner: e.name.clone() });
            sink.push(Entry { path: path.to_string_lossy().into_owned(), size: e.size, mtime: e.mtime, is_dir: false, archive: false, hidden: false, name });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn finds_models_in_tree_and_zips() {
        let d = std::env::temp_dir().join(format!("mg-scan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("a").join("b")).unwrap();
        fs::write(d.join("a").join("one.stl"), b"x").unwrap();
        fs::write(d.join("a").join("b").join("two.3MF"), b"x").unwrap();
        fs::write(d.join("a").join("skip.txt"), b"x").unwrap();
        {
            let f = fs::File::create(d.join("pack.zip")).unwrap();
            let mut w = zip::ZipWriter::new(f);
            w.start_file("deep/three.obj", zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(b"v").unwrap();
            w.start_file("__MACOSX/deep/._three.obj", zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(b"v").unwrap();
            w.finish().unwrap();
        }
        let opts = Options { exts: ["stl", "3mf", "obj"].iter().map(|s| s.to_string()).collect(), show_hidden: false, include_archives: true };
        let cancel = Arc::new(AtomicBool::new(false));
        let mut found = Vec::new();
        let mut done = None;
        run(d.to_str().unwrap(), &opts, &cancel, |ev| match ev {
            ScanEvent::Batch { entries, .. } => found.extend(entries.into_iter().map(|e| e.name)),
            ScanEvent::Done { matched, cancelled, .. } => done = Some((matched, cancelled)),
        });
        found.sort();
        assert_eq!(found, vec!["one.stl", "three.obj", "two.3MF"]);
        assert_eq!(done, Some((3, false)));
    }
}
