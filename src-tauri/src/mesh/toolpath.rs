//! G-code as a solid print: every extrusion becomes a ribbon with the line
//! width and layer height the slicer wrote, coloured by filament (tool).
//!
//! Understands the comment dialects of PrusaSlicer (`;TYPE:`, `;WIDTH:`,
//! `;HEIGHT:`), Bambu Studio / Orca (`; FEATURE:`, `; LINE_WIDTH:`,
//! `; LAYER_HEIGHT:`) and Cura (`;TYPE:`), absolute and relative extrusion,
//! G2/G3 arcs and `T<n>` tool changes with `filament_colour` from the config.

use super::{parse_hex_color, Mesh, Res};

/// Above this the ribbons get too heavy for the web view; the caller falls back to lines.
pub const MAX_TRIANGLES: usize = 6_000_000;
/// Chord error allowed when tessellating G2/G3 arcs (mm).
const ARC_TOLERANCE: f64 = 0.02;
/// A point this close to the straight line through its neighbours is dropped (mm).
const MERGE_TOLERANCE: f32 = 0.01;

pub fn parse(data: &[u8]) -> Res<Mesh> {
    let text = String::from_utf8_lossy(data);
    let mut p = Parser::new(filament_colours(&text));
    // Slicers that label features put the purge line of the start G-code before the
    // first label (or under "Custom"): hide it. Unlabelled files draw everything.
    p.hidden = text.lines().any(|l| l.strip_prefix(';').map(str::trim_start).is_some_and(|c| c.starts_with("TYPE:") || c.starts_with("FEATURE:")));
    for line in text.lines() {
        p.line(line)?;
    }
    p.flush()?;
    if p.out.indices.as_ref().map_or(0, |i| i.len()) == 0 {
        return Err("G-code'da baskı hattı bulunamadı".into());
    }
    Ok(p.out)
}

/// `filament_colour` (Bambu, Orca, PrusaSlicer; `extruder_colour` as a fallback),
/// wherever the slicer put its config block (top or bottom of the file).
fn filament_colours(text: &str) -> Vec<u32> {
    let mut filament = Vec::new();
    let mut extruder = Vec::new();
    for line in text.lines() {
        let Some(c) = line.strip_prefix(';') else { continue };
        let c = c.trim_start();
        let (target, rest) = if let Some(r) = c.strip_prefix("filament_colour") {
            (&mut filament, r)
        } else if let Some(r) = c.strip_prefix("extruder_colour") {
            (&mut extruder, r)
        } else {
            continue;
        };
        let Some(v) = rest.trim_start().strip_prefix('=') else { continue };
        *target = v.split([';', ',']).map(|s| parse_hex_color(s.trim().trim_matches('"')).unwrap_or(0)).collect();
    }
    if filament.iter().any(|&c| c != 0) {
        filament
    } else {
        extruder
    }
}

/// One continuous extrusion at a constant width, height and colour.
struct Poly {
    pts: Vec<[f32; 3]>,
    w: f32,
    h: f32,
    color: u32,
}

struct Parser {
    colours: Vec<u32>,
    // machine state
    pos: [f64; 4], // x, y, z, e
    abs_xyz: bool,
    abs_e: bool,
    tool: usize,
    // slicer annotations
    width: Option<f32>,
    height: Option<f32>,
    hidden: bool,
    // layer tracking for files without HEIGHT comments
    layer_z: f64,
    prev_layer_z: f64,
    cur: Option<Poly>,
    out: Mesh,
    tris: usize,
}

impl Parser {
    fn new(colours: Vec<u32>) -> Self {
        Parser {
            colours,
            pos: [0.0; 4],
            abs_xyz: true,
            abs_e: true,
            tool: 0,
            width: None,
            height: None,
            hidden: false,
            layer_z: 0.0,
            prev_layer_z: 0.0,
            cur: None,
            out: Mesh { positions: Vec::new(), indices: Some(Vec::new()), z_up: true, colors: Some(Vec::new()) },
            tris: 0,
        }
    }

    fn line(&mut self, line: &str) -> Res<()> {
        let (code, comment) = match line.find(';') {
            Some(i) => (&line[..i], Some(&line[i + 1..])),
            None => (line, None),
        };
        if let Some(c) = comment {
            self.comment(c.trim())?;
        }
        let mut words = code.split_ascii_whitespace();
        let Some(cmd) = words.next() else { return Ok(()) };
        let cmd = cmd.to_ascii_uppercase();
        let args: Vec<(u8, f64)> = words
            .filter_map(|w| {
                let (k, v) = w.split_at(1);
                Some((k.as_bytes()[0].to_ascii_uppercase(), v.parse::<f64>().ok()?))
            })
            .collect();
        let arg = |k: u8| args.iter().find(|a| a.0 == k).map(|a| a.1);
        match cmd.as_str() {
            "G0" | "G00" | "G1" | "G01" => self.linear(&arg),
            "G2" | "G02" | "G3" | "G03" => self.arc(&arg, cmd.ends_with('2')),
            "G90" => {
                self.abs_xyz = true;
                Ok(())
            }
            "G91" => {
                self.abs_xyz = false;
                Ok(())
            }
            "M82" => {
                self.abs_e = true;
                Ok(())
            }
            "M83" => {
                self.abs_e = false;
                Ok(())
            }
            "G92" => {
                for (i, k) in [b'X', b'Y', b'Z', b'E'].into_iter().enumerate() {
                    if let Some(v) = arg(k) {
                        self.pos[i] = v;
                    }
                }
                Ok(())
            }
            t if t.len() > 1 && t.starts_with('T') => {
                if let Ok(n) = t[1..].parse::<usize>() {
                    // T255 / T1000 are "unload" on Bambu printers, not a filament.
                    if n < 64 && n != self.tool {
                        self.tool = n;
                        self.flush()?;
                    }
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn comment(&mut self, c: &str) -> Res<()> {
        let num = |s: &str| s.trim().parse::<f32>().ok().filter(|v| v.is_finite() && *v > 0.0 && *v < 10.0);
        if let Some(v) = c.strip_prefix("WIDTH:").or_else(|| c.strip_prefix("LINE_WIDTH:")) {
            self.width = num(v).or(self.width);
        } else if let Some(v) = c.strip_prefix("HEIGHT:").or_else(|| c.strip_prefix("LAYER_HEIGHT:")) {
            self.height = num(v).or(self.height);
        } else if let Some(v) = c.strip_prefix("TYPE:").or_else(|| c.strip_prefix("FEATURE:")) {
            let name = v.trim().to_ascii_lowercase();
            // Inside the part and invisible in the finished print, or start/end G-code (purge line).
            let hidden = matches!(name.as_str(), "sparse infill" | "internal solid infill" | "internal infill" | "solid infill" | "fill" | "custom");
            if hidden != self.hidden {
                self.flush()?;
            }
            self.hidden = hidden;
        }
        Ok(())
    }

    fn target(&self, arg: &dyn Fn(u8) -> Option<f64>) -> [f64; 4] {
        let mut t = self.pos;
        for (i, k) in [b'X', b'Y', b'Z'].into_iter().enumerate() {
            if let Some(v) = arg(k) {
                t[i] = if self.abs_xyz { v } else { self.pos[i] + v };
            }
        }
        if let Some(v) = arg(b'E') {
            t[3] = if self.abs_e { v } else { self.pos[3] + v };
        }
        t
    }

    fn linear(&mut self, arg: &dyn Fn(u8) -> Option<f64>) -> Res<()> {
        let t = self.target(arg);
        let from = self.pos;
        self.pos = t;
        self.segment(from, t)
    }

    fn arc(&mut self, arg: &dyn Fn(u8) -> Option<f64>, clockwise: bool) -> Res<()> {
        let t = self.target(arg);
        let from = self.pos;
        let (Some(i), Some(j)) = (arg(b'I'), arg(b'J')) else {
            // R-form or malformed: draw it straight.
            self.pos = t;
            return self.segment(from, t);
        };
        let (cx, cy) = (from[0] + i, from[1] + j);
        let r = (i * i + j * j).sqrt();
        let a0 = (from[1] - cy).atan2(from[0] - cx);
        let a1 = (t[1] - cy).atan2(t[0] - cx);
        let tau = std::f64::consts::TAU;
        let mut sweep = if clockwise { a0 - a1 } else { a1 - a0 };
        sweep = sweep.rem_euclid(tau);
        if sweep < 1e-9 {
            sweep = tau; // same start and end: a full circle
        }
        let step = if r > ARC_TOLERANCE { 2.0 * (1.0 - ARC_TOLERANCE / r).acos() } else { sweep };
        let n = ((sweep / step).ceil() as usize).clamp(1, 720);
        let mut prev = from;
        for k in 1..=n {
            let f = k as f64 / n as f64;
            let a = if clockwise { a0 - sweep * f } else { a0 + sweep * f };
            let p = if k == n {
                t
            } else {
                [cx + r * a.cos(), cy + r * a.sin(), from[2] + (t[2] - from[2]) * f, from[3] + (t[3] - from[3]) * f]
            };
            self.segment(prev, p)?;
            prev = p;
        }
        self.pos = t;
        Ok(())
    }

    /// Handles one straight move from `a` to `b` (positions incl. extruder).
    fn segment(&mut self, a: [f64; 4], b: [f64; 4]) -> Res<()> {
        let de = b[3] - a[3];
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let printing = de > 1e-6 && dx * dx + dy * dy > 1e-8 && !self.hidden;
        if !printing {
            if de > 1e-6 || dx != 0.0 || dy != 0.0 || b[2] != a[2] {
                self.flush()?; // travel, retract/unretract, z hop, or hidden feature
            }
            return Ok(());
        }
        if b[2] > self.layer_z + 1e-4 {
            self.prev_layer_z = self.layer_z;
            self.layer_z = b[2];
        }
        let h = self.height.unwrap_or_else(|| {
            let d = (self.layer_z - self.prev_layer_z) as f32;
            if d > 0.02 && d < 2.0 {
                d
            } else {
                0.2
            }
        });
        let w = self.width.unwrap_or(0.45);
        let color = self.colours.get(self.tool).copied().unwrap_or(0);
        let pa = [a[0] as f32, a[1] as f32, a[2] as f32];
        let pb = [b[0] as f32, b[1] as f32, b[2] as f32];
        if let Some(cur) = &mut self.cur {
            let last = *cur.pts.last().unwrap();
            let joins = cur.w == w && cur.h == h && cur.color == color && dist2(last, pa) < 1e-8 && last[2] == pb[2];
            if joins && !sharp(cur, pb) {
                push_merged(cur, pb);
                return Ok(());
            }
        }
        self.flush()?;
        self.cur = Some(Poly { pts: vec![pa, pb], w, h, color });
        Ok(())
    }

    /// Turns the open ribbon into triangles.
    fn flush(&mut self) -> Res<()> {
        let Some(p) = self.cur.take() else { return Ok(()) };
        let n = p.pts.len();
        if n < 2 {
            return Ok(());
        }
        self.tris += 8 * (n - 1) + 4;
        if self.tris > MAX_TRIANGLES {
            return Err(format!("G-code şerit olarak çizilemeyecek kadar büyük (>{} milyon üçgen)", MAX_TRIANGLES / 1_000_000));
        }
        let pos = &mut self.out.positions;
        let idx = self.out.indices.as_mut().unwrap();
        let base = (pos.len() / 3) as u32;
        let hw = p.w / 2.0;
        for i in 0..n {
            let q = p.pts[i];
            let din = if i > 0 { dir(p.pts[i - 1], q) } else { None };
            let dout = if i + 1 < n { dir(q, p.pts[i + 1]) } else { None };
            // Miter normal between the incoming and outgoing direction.
            let (nx, ny, s) = match (din, dout) {
                (Some(a), Some(b)) => {
                    let (mx, my) = (-a.1 - b.1, a.0 + b.0);
                    let l = (mx * mx + my * my).sqrt().max(1e-6);
                    let (mx, my) = (mx / l, my / l);
                    let cos = (mx * -a.1 + my * a.0).max(0.5);
                    (mx, my, 1.0 / cos)
                }
                (Some(d), None) | (None, Some(d)) => (-d.1, d.0, 1.0),
                (None, None) => (0.0, 1.0, 1.0),
            };
            let (ox, oy) = (nx * hw * s, ny * hw * s);
            let (x, y, z) = (q[0], q[1], q[2]);
            pos.extend_from_slice(&[x + ox, y + oy, z - p.h / 2.0, x, y, z, x - ox, y - oy, z - p.h / 2.0, x, y, z - p.h]);
        }
        for i in 0..n as u32 - 1 {
            let (v, u) = (base + i * 4, base + (i + 1) * 4);
            for k in 0..4 {
                let (a, b, c, d) = (v + k, v + (k + 1) % 4, u + (k + 1) % 4, u + k);
                idx.extend_from_slice(&[a, b, c, a, c, d]);
            }
        }
        let (s, e) = (base, base + (n as u32 - 1) * 4);
        idx.extend_from_slice(&[s, s + 2, s + 1, s, s + 3, s + 2, e, e + 1, e + 2, e, e + 2, e + 3]);
        let colors = self.out.colors.as_mut().unwrap();
        colors.extend(std::iter::repeat(p.color).take(8 * (n - 1) + 4));
        Ok(())
    }
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Unit direction in the XY plane, None for zero-length moves.
fn dir(a: [f32; 3], b: [f32; 3]) -> Option<(f32, f32)> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l = (dx * dx + dy * dy).sqrt();
    (l > 1e-6).then(|| (dx / l, dy / l))
}

/// A turn of more than 120° would need an absurd miter: start a new ribbon instead.
fn sharp(p: &Poly, next: [f32; 3]) -> bool {
    let n = p.pts.len();
    match (dir(p.pts[n - 2], p.pts[n - 1]), dir(p.pts[n - 1], next)) {
        (Some(a), Some(b)) => a.0 * b.0 + a.1 * b.1 < -0.5,
        _ => false,
    }
}

/// Appends `b`, replacing the last point when it lies on the line to `b`.
fn push_merged(p: &mut Poly, b: [f32; 3]) {
    let n = p.pts.len();
    if n >= 2 {
        let (a, m) = (p.pts[n - 2], p.pts[n - 1]);
        let (ax, ay, bx, by) = (m[0] - a[0], m[1] - a[1], b[0] - a[0], b[1] - a[1]);
        let len = (bx * bx + by * by).sqrt();
        let forward = ax * bx + ay * by > 0.0 && (b[0] - m[0]) * bx + (b[1] - m[1]) * by > 0.0;
        if len > 1e-6 && forward && (ax * by - ay * bx).abs() / len < MERGE_TOLERANCE {
            p.pts[n - 1] = b;
            return;
        }
    }
    p.pts.push(b);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(g: &str) -> Mesh {
        parse(g.as_bytes()).unwrap()
    }

    fn z_range(m: &Mesh) -> (f32, f32) {
        let (mn, mx) = m.bounds().unwrap();
        (mn[2], mx[2])
    }

    const PRUSA_L: &str = ";TYPE:External perimeter\n;WIDTH:0.45\n;HEIGHT:0.2\nG90\nM82\nG92 E0\nG1 Z0.2 F600\nG1 X0 Y0\n";

    #[test]
    fn corner_becomes_one_ribbon_with_width_and_height() {
        let m = run(&format!("{PRUSA_L}G1 X10 Y0 E1\nG1 X10 Y10 E2\n"));
        // one polyline of two segments: 4 faces x 2 triangles per segment + 2 end caps x 2
        assert_eq!(m.triangle_count(), 2 * 8 + 4);
        let (mn, mx) = m.bounds().unwrap();
        assert!((mx[0] - 10.225).abs() < 1e-3 && (mn[1] + 0.225).abs() < 1e-3, "half the line width either side: {mn:?} {mx:?}");
        let (z0, z1) = z_range(&m);
        assert!((z0 - 0.0).abs() < 1e-4 && (z1 - 0.2).abs() < 1e-4, "spans the layer: {z0} {z1}");
        assert!(m.z_up);
    }

    #[test]
    fn straight_moves_merge_and_travel_splits() {
        let merged = run(&format!("{PRUSA_L}G1 X5 Y0 E0.5\nG1 X10 Y0 E1\n"));
        assert_eq!(merged.triangle_count(), 8 + 4, "collinear moves are one segment");
        let split = run(&format!("{PRUSA_L}G1 X5 Y0 E0.5\nG0 X5 Y5\nG1 X10 Y5 E1\n"));
        assert_eq!(split.triangle_count(), 2 * (8 + 4), "a travel move starts a new ribbon");
    }

    #[test]
    fn relative_extrusion_and_retractions() {
        let m = run(";WIDTH:0.4\n;HEIGHT:0.2\nM83\nG1 Z0.2\nG1 X0 Y0\nG1 E-0.8\nG1 X10 Y0 E0.5\nG1 E-0.8\nG1 X20 Y0\nG1 E0.8\nG1 X20 Y10 E0.5\n");
        assert_eq!(m.triangle_count(), 2 * (8 + 4), "retract/unretract and travel are not printed");
    }

    #[test]
    fn hidden_infill_is_skipped_but_walls_kept() {
        let m = run(&format!("{PRUSA_L}G1 X10 Y0 E1\n;TYPE:Internal infill\nG1 X10 Y10 E2\n;TYPE:Solid infill\nG1 X0 Y10 E3\n;TYPE:Top solid infill\nG1 X0 Y0 E4\n"));
        assert_eq!(m.triangle_count(), 2 * (8 + 4), "outer wall and top solid infill only");
    }

    #[test]
    fn bambu_comments_and_filament_colours_per_tool() {
        let g = "; filament_colour = #FF0000;#00FF00\n; FEATURE: Outer wall\n; LINE_WIDTH: 0.42\n; LAYER_HEIGHT: 0.2\nT0\nG1 Z0.2\nG1 X0 Y0\nM83\nG1 X10 Y0 E1\nT1\n; FEATURE: Sparse infill\nG1 X10 Y10 E1\n; FEATURE: Inner wall\nG1 X0 Y10 E1\n";
        let m = run(g);
        let c = m.colors.clone().unwrap();
        assert_eq!(m.triangle_count(), 2 * (8 + 4));
        assert!(c[..12].iter().all(|&v| v == 0xff0000ff), "T0 is red");
        assert!(c[12..].iter().all(|&v| v == 0x00ff00ff), "T1 is green");
        let (mn, mx) = m.bounds().unwrap();
        assert!((mx[1] - mn[1] - 10.42).abs() < 1e-3, "0.42 mm line width: {mn:?} {mx:?}");
    }

    #[test]
    fn arcs_are_tessellated() {
        // quarter circle of radius 10 around (0,0), counter-clockwise from (10,0) to (0,10)
        let m = run(";WIDTH:0.4\n;HEIGHT:0.2\nG1 Z0.2\nG1 X10 Y0\nM83\nG3 X0 Y10 I-10 J0 E2\n");
        assert!(m.triangle_count() > 8 * 8, "several segments: {}", m.triangle_count());
        let (mn, mx) = m.bounds().unwrap();
        assert!(mx[0] > 10.1 && mx[1] > 10.1 && mn[0] > -0.3 && mn[1] > -0.3, "stays on the arc: {mn:?} {mx:?}");
    }

    #[test]
    fn layer_height_falls_back_to_z_steps_without_comments() {
        // Cura-like: no WIDTH/HEIGHT comments, second layer 0.3 above the first
        let m = run(";TYPE:WALL-OUTER\nG1 Z0.3\nG1 X0 Y0\nM83\nG1 X10 Y0 E1\nG1 Z0.6\nG1 X10 Y10\nG1 X0 Y10 E1\n");
        let (z0, z1) = z_range(&m);
        assert!((z1 - 0.6).abs() < 1e-4 && (z0 - 0.0).abs() < 1e-3, "{z0} {z1}");
    }

    /// Manual check: `FORMIA_GCODE="a.gcode;b.gcode" cargo test --release --lib real_gcode -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_gcode() {
        let mut jobs: Vec<(String, Vec<u8>)> = Vec::new();
        for path in std::env::var("FORMIA_GCODE").expect("set FORMIA_GCODE").split(';').filter(|p| !p.is_empty()) {
            let data = std::fs::read(path).unwrap();
            if path.to_ascii_lowercase().ends_with(".3mf") {
                // Sliced project: every plate's G-code.
                for p in crate::mesh::threemf::plates(&data).iter().filter(|p| p.gcode) {
                    if let Some(g) = crate::mesh::threemf::plate_gcode(&data, Some(p.id)) {
                        jobs.push((format!("{path} #{} '{}'", p.id, p.name), g));
                    }
                }
            } else {
                jobs.push((path.to_string(), data));
            }
        }
        for (path, data) in &jobs {
            let (path, data) = (path.as_str(), data.as_slice());
            let t = std::time::Instant::now();
            match parse(&data) {
                Ok(mut m) => {
                    if std::env::var_os("FORMIA_WIRE_OUT").is_some() {
                        m.normalize_colors();
                        std::fs::write(format!("{path}.wire"), m.to_wire()).unwrap();
                    }
                    let (mn, mx) = m.bounds().unwrap();
                    let mut cols: Vec<u32> = m.colors.clone().unwrap_or_default();
                    cols.sort_unstable();
                    cols.dedup();
                    let size = (mx[0] - mn[0], mx[1] - mn[1], mx[2] - mn[2]);
                    println!("{path}: {:.1} MB, {} tris, {} verts, {:?} ms, size {:.1}x{:.1}x{:.1}, colours {:08x?}", data.len() as f64 / 1e6, m.triangle_count(), m.vertex_count(), t.elapsed().as_millis(), size.0, size.1, size.2, cols);
                }
                Err(e) => println!("{path}: {e}"),
            }
        }
    }

    #[test]
    fn start_gcode_purge_line_is_not_part_of_the_print() {
        // Purge line in the start G-code (before the first feature, and a "Custom" block),
        // then the actual wall.
        let g = ";WIDTH:0.5\n;HEIGHT:0.2\nM83\nG1 Z0.2\nG1 X0 Y-3\nG1 X60 Y-3 E9\n;TYPE:Custom\nG1 X100 Y-3 E5\n;TYPE:Perimeter\nG1 X10 Y10\nG1 X20 Y10 E1\n";
        let m = run(g);
        assert_eq!(m.triangle_count(), 8 + 4, "only the perimeter");
        let (mn, _) = m.bounds().unwrap();
        assert!(mn[0] > 9.0, "purge line at x=0..100 is gone: {mn:?}");
    }

    #[test]
    fn no_extrusion_is_an_error() {
        assert!(parse(b"G28\nG1 X10 Y10\n").is_err());
    }
}
