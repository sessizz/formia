//! 3MF loader with support for build items, components, transforms and the
//! production extension (components living in other .model parts, as written
//! by Bambu Studio / Orca Slicer).
//!
//! Colours, most specific first: painted triangles (Bambu/Orca `paint_color`,
//! PrusaSlicer `mmu_segmentation`), PrusaSlicer volume extruders, Bambu part
//! extruders, object extruders (all mapped through the project's filament
//! colours), then core-spec `basematerials` / `colorgroup` colours.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use zip::ZipArchive;

use super::{parse_f32, parse_hex_color, Mesh, Res};

type Mat = [[f32; 4]; 4];
const IDENTITY: Mat = [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.], [0., 0., 0., 1.]];
const MAX_DEPTH: u32 = 32;

#[derive(Clone, Debug)]
struct Component {
    path: Option<String>,
    id: u32,
    transform: Mat,
}

#[derive(Default, Debug)]
struct Object {
    positions: Vec<f32>,
    indices: Vec<u32>,
    components: Vec<Component>,
    /// Painted filament per triangle (0 = unpainted); empty when nothing is painted.
    paint: Vec<u8>,
    /// Material colour per triangle (0 = none); empty when no triangle has one.
    tri_color: Vec<u32>,
    /// Object-level material colour (`pid`/`pindex`).
    color: u32,
    pid: Option<u32>,
}

#[derive(Default, Debug)]
struct Model {
    objects: HashMap<u32, Object>,
    build: Vec<Component>,
    /// `basematerials` / `colorgroup` id -> colours.
    materials: HashMap<u32, Vec<u32>>,
}

/// Filament assignments stored by slicers next to the model.
#[derive(Default, Debug)]
struct Slicer {
    /// Colour of filament 1, 2, … (0xRRGGBBAA).
    filaments: Vec<u32>,
    /// Root object id -> extruder (Bambu, Orca, PrusaSlicer).
    object_extruder: HashMap<u32, u32>,
    /// (root object id, component object id) -> extruder (Bambu, Orca).
    part_extruder: HashMap<(u32, u32), u32>,
    /// Root object id -> (first triangle, last triangle, extruder) (PrusaSlicer).
    volumes: HashMap<u32, Vec<(u32, u32, u32)>>,
    /// Build plates (Bambu, Orca), in file order.
    plates: Vec<Plate>,
}

/// One build plate of a Bambu Studio / Orca Slicer project.
#[derive(Default, Debug, Clone)]
pub struct Plate {
    pub id: u32,
    pub name: String,
    /// Zip entry of the slicer's render of this plate.
    pub thumb: Option<String>,
    /// Number of object instances on the plate.
    pub objects: usize,
    /// (build object id, instance index among that object's build items).
    instances: Vec<(u32, u32)>,
    /// Sliced export: the plate is shown from its `Metadata/plate_N.gcode`.
    pub gcode: bool,
}

/// `Metadata/plate_N.gcode` entries of a sliced project (Bambu/Orca `.gcode.3mf`), by plate number.
fn gcode_entries(names: &HashMap<String, String>) -> Vec<(u32, String)> {
    let mut v: Vec<(u32, String)> = names
        .iter()
        .filter_map(|(lower, real)| Some((lower.strip_prefix("metadata/plate_")?.strip_suffix(".gcode")?.parse().ok()?, real.clone())))
        .collect();
    v.sort();
    v
}

/// G-code of one plate of a sliced project (`None`: the first plate); `None` when unsliced.
pub fn plate_gcode(data: &[u8], plate: Option<u32>) -> Option<Vec<u8>> {
    let mut zip = ZipArchive::new(Cursor::new(data)).ok()?;
    let names = name_index(&zip);
    let entries = gcode_entries(&names);
    let (_, name) = entries.iter().find(|(id, _)| plate.is_none_or(|p| p == *id))?;
    read_entry(&mut zip, &names, name)
}

/// Raw bytes of one zip entry (case-insensitive name), e.g. a plate render.
pub fn entry(data: &[u8], name: &str) -> Option<Vec<u8>> {
    let mut zip = ZipArchive::new(Cursor::new(data)).ok()?;
    let names = name_index(&zip);
    read_entry(&mut zip, &names, name)
}

/// Plates declared in the project's model_settings.config, sorted by number.
/// Empty for files without plate information (PrusaSlicer, plain 3MF).
pub fn plates(data: &[u8]) -> Vec<Plate> {
    let Ok(mut zip) = ZipArchive::new(Cursor::new(data)) else { return Vec::new() };
    let names = name_index(&zip);
    let mut p = load_slicer(&mut zip, &names).plates;
    p.sort_by_key(|p| p.id);
    let sliced = gcode_entries(&names);
    if sliced.is_empty() {
        return p;
    }
    // Sliced export: one plate per G-code file, named from the project where it says so.
    sliced
        .into_iter()
        .map(|(id, _)| {
            let known = p.iter().find(|q| q.id == id);
            Plate {
                id,
                name: known.map(|q| q.name.clone()).unwrap_or_default(),
                thumb: known.and_then(|q| q.thumb.clone()).or_else(|| Some(format!("Metadata/plate_{id}.png"))),
                objects: known.map_or(1, |q| q.objects.max(1)),
                instances: Vec::new(),
                gcode: true,
            }
        })
        .collect()
}

impl Slicer {
    fn filament(&self, extruder: u32) -> u32 {
        if extruder == 0 {
            return 0;
        }
        self.filaments.get(extruder as usize - 1).copied().unwrap_or(0)
    }
}

struct Ctx<'a> {
    zip: ZipArchive<Cursor<&'a [u8]>>,
    names: HashMap<String, String>,
    models: HashMap<String, Model>,
    slicer: Slicer,
}

/// Colour context handed down the component tree.
#[derive(Clone, Copy)]
struct Inherit {
    root: Option<u32>,
    color: u32,
}

pub fn parse(data: &[u8]) -> Res<Mesh> {
    parse_plate(data, None)
}

/// Like [`parse`], but with `Some(plate)` only that plate's object instances are built.
pub fn parse_plate(data: &[u8], plate: Option<u32>) -> Res<Mesh> {
    let mut zip = ZipArchive::new(Cursor::new(data)).map_err(|e| format!("3MF açılamadı: {e}"))?;
    let names = name_index(&zip);
    let slicer = load_slicer(&mut zip, &names);
    let mut ctx = Ctx { zip, names, models: HashMap::new(), slicer };
    let root = root_model_path(&mut ctx).ok_or("3MF içinde model bulunamadı")?;
    ctx.ensure(&root)?;

    let mut out = Mesh { positions: Vec::new(), indices: Some(Vec::new()), z_up: true, colors: Some(Vec::new()) };
    let build = ctx.models[&root].build.clone();
    let wanted: Option<std::collections::HashSet<(u32, u32)>> = plate.map(|id| ctx.slicer.plates.iter().filter(|p| p.id == id).flat_map(|p| p.instances.iter().copied()).collect());
    let mut instance_of: HashMap<u32, u32> = HashMap::new();
    for item in &build {
        let n = instance_of.entry(item.id).or_insert(0);
        let instance = *n;
        *n += 1;
        if wanted.as_ref().is_some_and(|w| !w.contains(&(item.id, instance))) {
            continue;
        }
        let path = item.path.clone().unwrap_or_else(|| root.clone());
        let color = if ctx.slicer.filaments.is_empty() {
            0
        } else {
            ctx.slicer.filament(ctx.slicer.object_extruder.get(&item.id).copied().filter(|&e| e > 0).unwrap_or(1))
        };
        emit(&mut ctx, &path, item.id, &item.transform, 0, &mut out, Inherit { root: Some(item.id), color });
    }

    if out.triangle_count() == 0 && plate.is_none() {
        // No usable build section: show every mesh object we can find.
        let all: Vec<String> = ctx.names.values().filter(|n| n.to_ascii_lowercase().ends_with(".model")).cloned().collect();
        for p in all {
            let _ = ctx.ensure(&p);
        }
        for model in ctx.models.values() {
            for obj in model.objects.values() {
                append(obj, &IDENTITY, &mut out, 0, None, &ctx.slicer);
            }
        }
    }
    Ok(out)
}

/// Returns the bytes of the thumbnail image embedded by slicers, if any.
pub fn embedded_thumbnail(data: &[u8]) -> Option<Vec<u8>> {
    let mut zip = ZipArchive::new(Cursor::new(data)).ok()?;
    let names = name_index(&zip);
    // Slicer plate renders first (they show the actual file content), then
    // whatever the package declares, then designer cover photos.
    let mut candidates: Vec<String> = vec!["Metadata/plate_1.png".into(), "Metadata/thumbnail.png".into()];
    if let Some(rels) = read_entry(&mut zip, &names, "_rels/.rels") {
        for (ty, target) in relationships(&rels) {
            if ty.to_ascii_lowercase().ends_with("thumbnail") {
                candidates.push(target);
            }
        }
    }
    for c in ["Metadata/thumbnail.jpg", "Auxiliaries/.thumbnails/thumbnail_middle.png", "Auxiliaries/.thumbnails/thumbnail_3mf.png"] {
        candidates.push(c.to_string());
    }
    candidates.iter().filter_map(|c| read_entry(&mut zip, &names, c)).find(|b| b.len() > 64)
}

impl Ctx<'_> {
    fn ensure(&mut self, path: &str) -> Res<()> {
        if self.models.contains_key(path) {
            return Ok(());
        }
        let xml = read_entry(&mut self.zip, &self.names, path).ok_or_else(|| format!("3MF parçası eksik: {path}"))?;
        let model = parse_model(&xml)?;
        self.models.insert(path.to_string(), model);
        Ok(())
    }
}

fn emit(ctx: &mut Ctx, path: &str, id: u32, m: &Mat, depth: u32, out: &mut Mesh, inh: Inherit) {
    if depth > MAX_DEPTH || ctx.ensure(path).is_err() {
        return;
    }
    let comps = {
        let Some(obj) = ctx.models.get(path).and_then(|md| md.objects.get(&id)) else { return };
        // PrusaSlicer volumes are triangle ranges of the root object's own mesh.
        let volumes = if depth == 0 { ctx.slicer.volumes.get(&id) } else { None };
        append(obj, m, out, inh.color, volumes, &ctx.slicer);
        obj.components.clone()
    };
    for c in comps {
        let p = c.path.clone().unwrap_or_else(|| path.to_string());
        let mut child = inh;
        if depth == 0 {
            if let Some(&e) = inh.root.and_then(|r| ctx.slicer.part_extruder.get(&(r, c.id))) {
                let col = ctx.slicer.filament(e);
                if col != 0 {
                    child.color = col;
                }
            }
        }
        emit(ctx, &p, c.id, &mul(&c.transform, m), depth + 1, out, child);
    }
}

fn append(obj: &Object, m: &Mat, out: &mut Mesh, inherited: u32, volumes: Option<&Vec<(u32, u32, u32)>>, slicer: &Slicer) {
    if obj.indices.is_empty() {
        return;
    }
    let Mesh { positions, indices, colors, .. } = out;
    let base = (positions.len() / 3) as u32;
    let vc = (obj.positions.len() / 3) as u32;
    for p in obj.positions.chunks_exact(3) {
        let (x, y, z) = (p[0], p[1], p[2]);
        positions.push(x * m[0][0] + y * m[1][0] + z * m[2][0] + m[3][0]);
        positions.push(x * m[0][1] + y * m[1][1] + z * m[2][1] + m[3][1]);
        positions.push(x * m[0][2] + y * m[1][2] + z * m[2][2] + m[3][2]);
    }
    let idx = indices.get_or_insert_with(Vec::new);
    let cols = colors.get_or_insert_with(Vec::new);
    for (ti, t) in obj.indices.chunks_exact(3).enumerate() {
        if t[0] < vc && t[1] < vc && t[2] < vc {
            idx.extend_from_slice(&[base + t[0], base + t[1], base + t[2]]);
            cols.push(triangle_color(obj, ti, inherited, volumes, slicer));
        }
    }
}

fn triangle_color(obj: &Object, ti: usize, inherited: u32, volumes: Option<&Vec<(u32, u32, u32)>>, slicer: &Slicer) -> u32 {
    if let Some(&p) = obj.paint.get(ti) {
        let c = slicer.filament(p as u32);
        if c != 0 {
            return c;
        }
    }
    if let Some(vols) = volumes {
        let t = ti as u32;
        if let Some(&(_, _, e)) = vols.iter().find(|&&(a, b, e)| t >= a && t <= b && e != 0) {
            let c = slicer.filament(e);
            if c != 0 {
                return c;
            }
        }
    }
    if inherited != 0 {
        return inherited;
    }
    match obj.tri_color.get(ti) {
        Some(&c) if c != 0 => c,
        _ => obj.color,
    }
}

/// Decodes the triangle-subdivision bitstream PrusaSlicer (`mmu_segmentation`)
/// and Bambu/Orca (`paint_color`) store per triangle: hex digits read from the
/// end, one nibble each. A nibble's low two bits give the number of split
/// sides (0 = leaf); a leaf's state is in the high two bits, `0b11` meaning
/// "next nibble + 3". Returns the state covering the largest area (0 = unpainted).
pub(crate) fn decode_paint(s: &str) -> u8 {
    let nibbles: Vec<u8> = s.bytes().rev().filter_map(|c| (c as char).to_digit(16)).map(|d| d as u8).collect();
    let mut acc = [0f32; 19];
    fn node(n: &[u8], pos: &mut usize, weight: f32, acc: &mut [f32; 19], depth: u32) -> bool {
        if depth > 24 {
            return false;
        }
        let Some(&code) = n.get(*pos) else { return false };
        *pos += 1;
        let splits = (code & 0b11) as usize;
        if splits == 0 {
            let state = if code & 0b1100 == 0b1100 {
                let Some(&x) = n.get(*pos) else { return false };
                *pos += 1;
                x as usize + 3
            } else {
                (code >> 2) as usize
            };
            acc[state.min(18)] += weight;
            return true;
        }
        let children = splits + 1;
        (0..children).all(|_| node(n, pos, weight / children as f32, acc, depth + 1))
    }
    let mut pos = 0;
    node(&nibbles, &mut pos, 1.0, &mut acc, 0);
    let (best, w) = acc.iter().enumerate().fold((0, 0f32), |b, (i, &w)| if w > b.1 { (i, w) } else { b });
    if w > 0.0 {
        best as u8
    } else {
        0
    }
}

/// Row-vector convention used by 3MF: p' = p * a * b.
fn mul(a: &Mat, b: &Mat) -> Mat {
    let mut r = [[0f32; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            r[i][j] = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}

fn parse_transform(s: &[u8]) -> Mat {
    let v: Vec<f32> = s.split(|b| b.is_ascii_whitespace()).filter(|t| !t.is_empty()).filter_map(parse_f32).collect();
    if v.len() != 12 {
        return IDENTITY;
    }
    [[v[0], v[1], v[2], 0.], [v[3], v[4], v[5], 0.], [v[6], v[7], v[8], 0.], [v[9], v[10], v[11], 1.]]
}

struct ModelParser {
    model: Model,
    current: Option<(u32, Object)>,
    material: Option<(u32, Vec<u32>)>,
    in_build: bool,
}

fn parse_model(xml: &[u8]) -> Res<Model> {
    let mut reader = Reader::from_reader(xml);
    let mut p = ModelParser { model: Model::default(), current: None, material: None, in_build: false };
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => p.element(&e, false),
            Ok(Event::Empty(e)) => p.element(&e, true),
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                "object" => {
                    if let Some((id, obj)) = p.current.take() {
                        p.model.objects.insert(id, obj);
                    }
                }
                "basematerials" | "colorgroup" => {
                    if let Some((id, cols)) = p.material.take() {
                        p.model.materials.insert(id, cols);
                    }
                }
                "build" => p.in_build = false,
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("3MF XML hatası: {e}")),
            _ => {}
        }
    }
    Ok(p.model)
}

impl ModelParser {
    fn material_color(&self, pid: Option<u32>, index: Option<u32>) -> u32 {
        match (pid, index) {
            (Some(pid), Some(i)) => self.model.materials.get(&pid).and_then(|m| m.get(i as usize)).copied().unwrap_or(0),
            _ => 0,
        }
    }

    fn element(&mut self, e: &BytesStart, empty: bool) {
        match e.local_name().as_ref() {
            "object" => {
                let id = attr(e, "id").and_then(|v| parse_u32(&v)).unwrap_or(0);
                let pid = attr(e, "pid").and_then(|v| parse_u32(&v));
                let pindex = attr(e, "pindex").and_then(|v| parse_u32(&v));
                let obj = Object { pid, color: self.material_color(pid, pindex.or(Some(0))), ..Default::default() };
                if empty {
                    self.model.objects.insert(id, obj);
                } else {
                    self.current = Some((id, obj));
                }
            }
            "vertex" => {
                if let Some((_, obj)) = self.current.as_mut() {
                    let mut xyz = [0f32; 3];
                    for a in e.attributes().flatten() {
                        let k = match a.key.local_name().as_ref() {
                            "x" => 0,
                            "y" => 1,
                            "z" => 2,
                            _ => continue,
                        };
                        xyz[k] = parse_f32(a.value.as_bytes()).unwrap_or(0.0);
                    }
                    obj.positions.extend_from_slice(&xyz);
                }
            }
            "triangle" => self.triangle(e),
            "component" => {
                if let Some((_, obj)) = self.current.as_mut() {
                    if let Some(c) = component(e) {
                        obj.components.push(c);
                    }
                }
            }
            "basematerials" | "colorgroup" if !empty => {
                let id = attr(e, "id").and_then(|v| parse_u32(&v)).unwrap_or(0);
                self.material = Some((id, Vec::new()));
            }
            "base" | "color" => {
                let key = if e.local_name().as_ref() == "base" { "displaycolor" } else { "color" };
                if let Some((_, cols)) = self.material.as_mut() {
                    cols.push(attr(e, key).and_then(|v| parse_hex_color(&String::from_utf8_lossy(&v))).unwrap_or(0));
                }
            }
            "build" => self.in_build = !empty,
            "item" if self.in_build => {
                if let Some(c) = component(e) {
                    self.model.build.push(c);
                }
            }
            _ => {}
        }
    }

    fn triangle(&mut self, e: &BytesStart) {
        let mut v = [u32::MAX; 3];
        let (mut pid, mut p1, mut paint) = (None, None, 0u8);
        for a in e.attributes().flatten() {
            match a.key.local_name().as_ref() {
                "v1" => v[0] = parse_u32(a.value.as_bytes()).unwrap_or(u32::MAX),
                "v2" => v[1] = parse_u32(a.value.as_bytes()).unwrap_or(u32::MAX),
                "v3" => v[2] = parse_u32(a.value.as_bytes()).unwrap_or(u32::MAX),
                "pid" => pid = parse_u32(a.value.as_bytes()),
                "p1" => p1 = parse_u32(a.value.as_bytes()),
                "paint_color" | "mmu_segmentation" => paint = decode_paint(&String::from_utf8_lossy(a.value.as_bytes())),
                _ => {}
            }
        }
        let color = if pid.is_some() || p1.is_some() {
            let opid = self.current.as_ref().and_then(|(_, o)| o.pid);
            self.material_color(pid.or(opid), p1)
        } else {
            0
        };
        let Some((_, obj)) = self.current.as_mut() else { return };
        let ti = obj.indices.len() / 3;
        obj.indices.extend_from_slice(&v);
        if paint != 0 {
            obj.paint.resize(ti, 0);
            obj.paint.push(paint);
        }
        if color != 0 {
            obj.tri_color.resize(ti, 0);
            obj.tri_color.push(color);
        }
    }
}

/// Reads filament colours and extruder assignments written by Bambu Studio,
/// Orca Slicer and PrusaSlicer.
fn load_slicer<R: Read + std::io::Seek>(zip: &mut ZipArchive<R>, names: &HashMap<String, String>) -> Slicer {
    let mut s = Slicer::default();
    if let Some(json) = read_entry(zip, names, "Metadata/project_settings.config") {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&json) {
            if let Some(arr) = v.get("filament_colour").and_then(|a| a.as_array()) {
                s.filaments = arr.iter().map(|c| c.as_str().and_then(parse_hex_color).unwrap_or(0)).collect();
            }
        }
    }
    if s.filaments.is_empty() {
        if let Some(cfg) = read_entry(zip, names, "Metadata/Slic3r_PE.config") {
            let text = String::from_utf8_lossy(&cfg);
            let get = |key: &str| {
                text.lines()
                    .find_map(|l| l.trim_start_matches(';').trim().strip_prefix(key)?.trim_start().strip_prefix('=').map(|v| v.trim().trim_matches('"').to_string()))
                    .unwrap_or_default()
            };
            let (ext, fil) = (get("extruder_colour"), get("filament_colour"));
            let e: Vec<&str> = ext.split(';').collect();
            let f: Vec<&str> = fil.split(';').collect();
            s.filaments = (0..e.len().max(f.len()))
                .map(|i| e.get(i).and_then(|c| parse_hex_color(c)).or_else(|| f.get(i).and_then(|c| parse_hex_color(c))).unwrap_or(0))
                .collect();
            if s.filaments.iter().all(|&c| c == 0) {
                s.filaments.clear();
            }
        }
    }
    for cfg in ["Metadata/model_settings.config", "Metadata/Slic3r_PE_model.config"] {
        if let Some(xml) = read_entry(zip, names, cfg) {
            parse_settings_xml(&xml, &mut s);
        }
    }
    s
}

/// `<object id>` / `<part id>` (Bambu, Orca) / `<volume firstid lastid>`
/// (PrusaSlicer) with `<metadata key="extruder" value="N"/>` children.
fn parse_settings_xml(xml: &[u8], s: &mut Slicer) {
    let mut reader = Reader::from_reader(xml);
    let mut obj: Option<u32> = None;
    let mut part: Option<u32> = None;
    let mut volume: Option<(u32, u32, u32)> = None;
    // Bambu/Orca: <plate> with plater_id / plater_name / thumbnail_file and
    // <model_instance> children naming object_id + instance_id.
    let mut plate: Option<Plate> = None;
    let mut instance: Option<(Option<u32>, u32)> = None;
    loop {
        let (e, empty) = match reader.read_event() {
            Ok(Event::Start(e)) => (e, false),
            Ok(Event::Empty(e)) => (e, true),
            Ok(Event::End(e)) => {
                match e.local_name().as_ref() {
                    "model_instance" => {
                        if let (Some(p), Some((Some(o), i))) = (plate.as_mut(), instance.take()) {
                            p.instances.push((o, i));
                            p.objects += 1;
                        }
                    }
                    "plate" => {
                        if let Some(p) = plate.take().filter(|p| p.id > 0) {
                            s.plates.push(p);
                        }
                    }
                    "object" => obj = None,
                    "part" => part = None,
                    "volume" => {
                        if let (Some(o), Some(v)) = (obj, volume.take()) {
                            s.volumes.entry(o).or_default().push(v);
                        }
                    }
                    _ => {}
                }
                continue;
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => continue,
        };
        let id = || attr(&e, "id").and_then(|v| parse_u32(&v));
        match e.local_name().as_ref() {
            "object" if !empty => {
                obj = id();
                part = None;
            }
            "part" if !empty => part = id(),
            "volume" if !empty => {
                let first = attr(&e, "firstid").and_then(|v| parse_u32(&v)).unwrap_or(0);
                let last = attr(&e, "lastid").and_then(|v| parse_u32(&v)).unwrap_or(u32::MAX);
                volume = Some((first, last, 0));
            }
            "plate" | "assemble" => {
                obj = None;
                part = None;
                if !empty && matches!(e.local_name().as_ref(), "plate") {
                    plate = Some(Plate::default());
                }
            }
            "model_instance" if !empty && plate.is_some() => instance = Some((None, 0)),
            "metadata" if plate.is_some() => {
                let (Some(key), Some(val)) = (attr(&e, "key"), attr(&e, "value")) else { continue };
                // Attribute values are raw XML ("Fast&amp;Easy"): unescape entities.
                let raw = String::from_utf8_lossy(&val).into_owned();
                let text = quick_xml::escape::unescape(&raw).map(|c| c.into_owned()).unwrap_or(raw);
                let (p, inst) = (plate.as_mut().unwrap(), instance.as_mut());
                match (key.as_slice(), inst) {
                    (b"object_id", Some(i)) => i.0 = parse_u32(&val),
                    (b"instance_id", Some(i)) => i.1 = parse_u32(&val).unwrap_or(0),
                    (b"plater_id", None) => p.id = parse_u32(&val).unwrap_or(0),
                    (b"plater_name", None) => p.name = text.trim().to_string(),
                    (b"thumbnail_file", None) if !text.trim().is_empty() => p.thumb = Some(text.trim().trim_start_matches('/').to_string()),
                    _ => {}
                }
            }
            "metadata" => {
                if attr(&e, "key").as_deref() != Some(&b"extruder"[..]) {
                    continue;
                }
                let Some(val) = attr(&e, "value").and_then(|v| parse_u32(&v)) else { continue };
                if let Some(v) = volume.as_mut() {
                    v.2 = val;
                } else if let (Some(o), Some(p)) = (obj, part) {
                    if val > 0 {
                        s.part_extruder.insert((o, p), val);
                    }
                } else if let Some(o) = obj {
                    s.object_extruder.insert(o, val);
                }
            }
            _ => {}
        }
    }
}

fn component(e: &BytesStart) -> Option<Component> {
    let id = attr(e, "objectid").and_then(|v| parse_u32(&v))?;
    let transform = attr(e, "transform").map(|t| parse_transform(&t)).unwrap_or(IDENTITY);
    let path = attr(e, "path").map(|p| normalize(&String::from_utf8_lossy(&p)));
    Some(Component { path, id, transform })
}

fn attr(e: &BytesStart, name: &str) -> Option<Vec<u8>> {
    e.attributes().flatten().find(|a| a.key.local_name().as_ref() == name).map(|a| a.value.as_bytes().to_vec())
}

fn parse_u32(v: &[u8]) -> Option<u32> {
    std::str::from_utf8(v).ok()?.trim().parse().ok()
}

fn normalize(p: &str) -> String {
    p.trim_start_matches('/').replace('\\', "/")
}

/// Lower-case entry name -> real entry name, for case-insensitive lookups.
fn name_index<R: Read + std::io::Seek>(zip: &ZipArchive<R>) -> HashMap<String, String> {
    zip.file_names().flatten().map(|n| (n.to_ascii_lowercase(), n.into_owned())).collect()
}

fn read_entry<R: Read + std::io::Seek>(zip: &mut ZipArchive<R>, names: &HashMap<String, String>, path: &str) -> Option<Vec<u8>> {
    let p = normalize(path);
    let real = names.get(&p.to_ascii_lowercase())?;
    let mut f = zip.by_name(real).ok()?;
    let mut buf = Vec::with_capacity(f.size().min(1 << 30) as usize);
    f.read_to_end(&mut buf).ok()?;
    Some(buf)
}

fn relationships(xml: &[u8]) -> Vec<(String, String)> {
    let mut reader = Reader::from_reader(xml);
    let mut out = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.local_name().as_ref() == "Relationship" => {
                let ty = attr(&e, "Type").map(|v| String::from_utf8_lossy(&v).into_owned()).unwrap_or_default();
                if let Some(t) = attr(&e, "Target") {
                    out.push((ty, normalize(&String::from_utf8_lossy(&t))));
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

fn root_model_path(ctx: &mut Ctx) -> Option<String> {
    if let Some(rels) = read_entry(&mut ctx.zip, &ctx.names, "_rels/.rels") {
        for (ty, target) in relationships(&rels) {
            if ty.to_ascii_lowercase().ends_with("3dmodel") && ctx.names.contains_key(&target.to_ascii_lowercase()) {
                return Some(ctx.names[&target.to_ascii_lowercase()].clone());
            }
        }
    }
    let mut models: Vec<&String> = ctx.names.values().filter(|n| n.to_ascii_lowercase().ends_with(".model")).collect();
    models.sort_by_key(|n| (n.matches('/').count(), n.len()));
    models.first().map(|s| s.to_string())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    pub fn make_zip(files: &[(&str, &str)]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            for (name, body) in files {
                w.start_file(*name, SimpleFileOptions::default()).unwrap();
                w.write_all(body.as_bytes()).unwrap();
            }
            w.finish().unwrap();
        }
        buf.into_inner()
    }

    const RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/></Relationships>"#;

    const TRI: &str = r#"<mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/></vertices><triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh>"#;

    #[test]
    fn simple_build_item_with_transform() {
        let model = format!(
            r#"<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources><object id="1" type="model">{TRI}</object></resources><build><item objectid="1" transform="1 0 0 0 1 0 0 0 1 10 20 30"/></build></model>"#
        );
        let data = make_zip(&[("_rels/.rels", RELS), ("3D/3dmodel.model", &model)]);
        let m = parse(&data).unwrap();
        assert_eq!(m.triangle_count(), 1);
        assert_eq!(&m.positions[0..3], &[10., 20., 30.]);
        assert_eq!(&m.positions[3..6], &[11., 20., 30.]);
        assert_eq!(m.colors.as_deref(), Some(&[0u32][..]), "no colour information");
    }

    #[test]
    fn production_extension_components_in_other_part() {
        let root = r#"<model xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02" xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06"><resources><object id="2" type="model"><components><component p:path="/3D/Objects/object_1.model" objectid="1" transform="2 0 0 0 2 0 0 0 2 0 0 0"/></components></object></resources><build><item objectid="2" transform="1 0 0 0 1 0 0 0 1 5 0 0"/></build></model>"#;
        let part = format!(r#"<model xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources><object id="1" type="model">{TRI}</object></resources><build/></model>"#);
        let data = make_zip(&[("_rels/.rels", RELS), ("3D/3dmodel.model", root), ("3D/Objects/object_1.model", &part)]);
        let m = parse(&data).unwrap();
        assert_eq!(m.triangle_count(), 1);
        // scaled by 2 then moved by 5 on x
        assert_eq!(&m.positions[3..6], &[7., 0., 0.]);
    }

    #[test]
    fn falls_back_to_all_objects_without_build() {
        let model = format!(r#"<model><resources><object id="1">{TRI}</object><object id="2">{TRI}</object></resources></model>"#);
        let data = make_zip(&[("3D/3DModel.model", &model)]);
        let m = parse(&data).unwrap();
        assert_eq!(m.triangle_count(), 2);
    }

    #[test]
    fn finds_embedded_thumbnail() {
        let png = "x".repeat(200);
        let rels = r#"<Relationships><Relationship Target="/Metadata/thumbnail.png" Id="t" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail"/></Relationships>"#;
        let data = make_zip(&[("_rels/.rels", rels), ("Metadata/thumbnail.png", &png)]);
        assert_eq!(embedded_thumbnail(&data).unwrap().len(), 200);
    }

    #[test]
    fn decodes_paint_bitstreams() {
        assert_eq!(decode_paint(""), 0);
        assert_eq!(decode_paint("4"), 1); // leaf, state 1
        assert_eq!(decode_paint("8"), 2); // leaf, state 2
        assert_eq!(decode_paint("4C"), 7); // leaf, extended: 4 + 3
        // split into 2 children (code 1): child A state 2 ("8"), child B state 1 ("4") -> tie, first max wins
        // nibbles are read from the end: "1", then "8", then "4" => string "481"
        assert_eq!(decode_paint("481"), 1);
        // split into 3 (code 2): states 2, 2, 1 -> 2 dominates; string reversed from [2, 8, 8, 4]
        assert_eq!(decode_paint("4882"), 2);
        // truncated stream does not panic
        assert_eq!(decode_paint("3"), 0);
    }

    /// Bambu Studio layout: filament colours in project_settings.config, extruders
    /// per object/part in model_settings.config, painted triangles in the part.
    #[test]
    fn bambu_filament_colours() {
        let root = r#"<model xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06"><resources>
            <object id="2"><components><component p:path="/3D/Objects/object_1.model" objectid="1"/></components></object>
            <object id="4"><components><component p:path="/3D/Objects/object_1.model" objectid="1"/><component p:path="/3D/Objects/object_1.model" objectid="3"/></components></object>
          </resources><build><item objectid="2"/><item objectid="4"/></build></model>"#;
        let part = r#"<model><resources>
            <object id="1"><mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/></vertices>
              <triangles><triangle v1="0" v2="1" v3="2"/><triangle v1="0" v2="2" v3="1" paint_color="4C"/></triangles></mesh></object>
            <object id="3"><mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/></vertices>
              <triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh></object>
          </resources></model>"#;
        let settings = r#"<config>
            <object id="2"><metadata key="extruder" value="1"/><part id="1" subtype="normal_part"><metadata key="name" value="a"/></part></object>
            <object id="4"><metadata key="extruder" value="2"/><part id="1"/><part id="3"><metadata key="extruder" value="3"/></part></object>
            <plate><metadata key="extruder" value="9"/></plate></config>"#;
        let project = r##"{"filament_colour": ["#000000", "#FF6A13", "#0086D6", "#111111", "#222222", "#333333", "#00AE42"]}"##;
        let data = make_zip(&[
            ("_rels/.rels", RELS),
            ("3D/3dmodel.model", root),
            ("3D/Objects/object_1.model", part),
            ("Metadata/model_settings.config", settings),
            ("Metadata/project_settings.config", project),
        ]);
        let m = parse(&data).unwrap();
        // object 2: two triangles (filament 1, painted filament 7); object 4: part 1 (filament 2, painted 7), part 3 (filament 3)
        assert_eq!(m.colors.unwrap(), vec![0x000000ff, 0x00ae42ff, 0xff6a13ff, 0x00ae42ff, 0x0086d6ff]);
    }

    /// Bambu/Orca multi-plate project: two instances of object 1 and one object 2,
    /// spread over plate 1 and plate 2; plate 3 is empty.
    fn plate_project() -> Vec<u8> {
        let model = format!(
            r#"<model><resources><object id="1">{TRI}</object><object id="2">{TRI}</object></resources>
            <build><item objectid="1"/><item objectid="1" transform="1 0 0 0 1 0 0 0 1 300 0 0"/><item objectid="2"/></build></model>"#
        );
        let settings = r#"<config>
            <object id="1"><metadata key="extruder" value="1"/></object>
            <plate><metadata key="plater_id" value="1"/><metadata key="plater_name" value=""/><metadata key="thumbnail_file" value="Metadata/plate_1.png"/>
              <model_instance><metadata key="object_id" value="1"/><metadata key="instance_id" value="0"/></model_instance></plate>
            <plate><metadata key="plater_id" value="2"/><metadata key="plater_name" value="Kapak &amp; taban"/><metadata key="thumbnail_file" value="Metadata/plate_2.png"/>
              <model_instance><metadata key="object_id" value="1"/><metadata key="instance_id" value="1"/></model_instance>
              <model_instance><metadata key="object_id" value="2"/><metadata key="instance_id" value="0"/></model_instance></plate>
            <plate><metadata key="plater_id" value="3"/></plate>
          </config>"#;
        make_zip(&[("_rels/.rels", RELS), ("3D/3dmodel.model", &model), ("Metadata/model_settings.config", settings)])
    }

    /// Manual check against real projects:
    /// `FORMIA_3MF="a.3mf;b.3mf" cargo test --lib real_plates -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_plates() {
        for path in std::env::var("FORMIA_3MF").expect("set FORMIA_3MF").split(';').filter(|p| !p.is_empty()) {
            let data = std::fs::read(path).unwrap();
            let all = parse(&data).map(|m| m.triangle_count()).unwrap_or(0);
            let list = plates(&data);
            let per: Vec<String> = list
                .iter()
                .map(|p| format!("#{} '{}' obj={} thumb={} tris={}", p.id, p.name, p.objects, p.thumb.is_some(), parse_plate(&data, Some(p.id)).map(|m| m.triangle_count()).unwrap_or(0)))
                .collect();
            println!("{path}: all={all} plates={} [{}]", list.len(), per.join(", "));
        }
    }

    /// Bambu/Orca sliced export (.gcode.3mf): G-code per plate, empty model.
    fn sliced_project() -> Vec<u8> {
        let settings = r#"<config>
            <plate><metadata key="plater_id" value="1"/><metadata key="plater_name" value="Gövde"/><metadata key="gcode_file" value="Metadata/plate_1.gcode"/></plate>
            <plate><metadata key="plater_id" value="2"/><metadata key="gcode_file" value="Metadata/plate_2.gcode"/></plate>
          </config>"#;
        make_zip(&[
            ("_rels/.rels", RELS),
            ("3D/3dmodel.model", "<model><resources/><build/></model>"),
            ("Metadata/model_settings.config", settings),
            ("Metadata/plate_2.gcode", "; plate two\n"),
            ("Metadata/plate_1.gcode", "; plate one\n"),
            ("Metadata/plate_1.gcode.md5", "abc"),
            ("Metadata/plate_1.png", "png"),
        ])
    }

    #[test]
    fn sliced_project_lists_gcode_plates() {
        let data = sliced_project();
        let p = plates(&data);
        let got: Vec<(u32, &str, bool, usize)> = p.iter().map(|p| (p.id, p.name.as_str(), p.gcode, p.objects)).collect();
        assert_eq!(got, vec![(1, "Gövde", true, 1), (2, "", true, 1)]);
        assert_eq!(p[1].thumb.as_deref(), Some("Metadata/plate_2.png"), "slicer's default render name");
        assert_eq!(plate_gcode(&data, Some(2)).as_deref(), Some(&b"; plate two\n"[..]));
        assert_eq!(plate_gcode(&data, None).as_deref(), Some(&b"; plate one\n"[..]), "first plate by default");
        assert!(plate_gcode(&data, Some(7)).is_none());
        assert!(plate_gcode(&plate_project(), None).is_none(), "an unsliced project has no G-code");
        assert!(plates(&plate_project()).iter().all(|p| !p.gcode));
    }

    #[test]
    fn plates_are_listed_with_names_and_thumbnails() {
        let p = plates(&plate_project());
        let got: Vec<(u32, &str, Option<&str>, usize)> = p.iter().map(|p| (p.id, p.name.as_str(), p.thumb.as_deref(), p.objects)).collect();
        assert_eq!(got, vec![(1, "", Some("Metadata/plate_1.png"), 1), (2, "Kapak & taban", Some("Metadata/plate_2.png"), 2), (3, "", None, 0)]);
        assert!(plates(&make_zip(&[("_rels/.rels", RELS)])).is_empty(), "no slicer settings, no plates");
    }

    #[test]
    fn parse_plate_keeps_only_that_plates_instances() {
        let data = plate_project();
        let one = parse_plate(&data, Some(1)).unwrap();
        assert_eq!(one.triangle_count(), 1);
        assert_eq!(&one.positions[0..3], &[0., 0., 0.], "first instance of object 1");
        let two = parse_plate(&data, Some(2)).unwrap();
        assert_eq!(two.triangle_count(), 2);
        assert_eq!(&two.positions[0..3], &[300., 0., 0.], "second instance of object 1, moved");
        assert_eq!(parse_plate(&data, Some(3)).unwrap().triangle_count(), 0, "empty plate stays empty");
        assert_eq!(parse(&data).unwrap().triangle_count(), 3, "without a plate everything is shown");
    }

    #[test]
    fn core_spec_base_materials() {
        let model = r##"<model><resources>
            <basematerials id="5"><base name="red" displaycolor="#FF0000"/><base name="green" displaycolor="#00FF00FF"/></basematerials>
            <object id="1" pid="5" pindex="1"><mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/></vertices>
              <triangles><triangle v1="0" v2="1" v3="2"/><triangle v1="0" v2="2" v3="1" p1="0"/></triangles></mesh></object>
          </resources><build><item objectid="1"/></build></model>"##;
        let data = make_zip(&[("_rels/.rels", RELS), ("3D/3dmodel.model", model)]);
        let m = parse(&data).unwrap();
        assert_eq!(m.colors.unwrap(), vec![0x00ff00ff, 0xff0000ff]);
    }

    #[test]
    fn prusa_volume_extruders() {
        let model = r#"<model><resources><object id="1"><mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/></vertices>
              <triangles><triangle v1="0" v2="1" v3="2"/><triangle v1="0" v2="2" v3="1"/></triangles></mesh></object>
          </resources><build><item objectid="1"/></build></model>"#;
        let cfg = r#"<config><object id="1" instances_count="1"><metadata type="object" key="extruder" value="1"/>
            <volume firstid="0" lastid="0"><metadata type="volume" key="extruder" value="0"/></volume>
            <volume firstid="1" lastid="1"><metadata type="volume" key="extruder" value="2"/></volume></object></config>"#;
        let pe = "; extruder_colour = \"\";\"\"\n; filament_colour = \"#FF8000;#202020\"\n";
        let data = make_zip(&[("_rels/.rels", RELS), ("3D/3dmodel.model", model), ("Metadata/Slic3r_PE_model.config", cfg), ("Metadata/Slic3r_PE.config", pe)]);
        let m = parse(&data).unwrap();
        assert_eq!(m.colors.unwrap(), vec![0xff8000ff, 0x202020ff]);
    }
}
