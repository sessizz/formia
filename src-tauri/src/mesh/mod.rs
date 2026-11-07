//! Native mesh loaders. Everything here works on in-memory bytes so the same
//! code serves plain files, memory-mapped files and entries inside zip archives.

pub mod gltf_load;
pub mod obj;
pub mod off;
pub mod ply;
pub mod stl;
pub mod threemf;
pub mod toolpath;

use std::path::Path;

pub type Res<T> = Result<T, String>;

/// Triangle mesh. `indices == None` means `positions` is a triangle soup
/// (every 9 floats form one triangle).
#[derive(Debug, Default, Clone)]
pub struct Mesh {
    pub positions: Vec<f32>,
    pub indices: Option<Vec<u32>>,
    /// True when the source format is conventionally Z-up (3D printing formats).
    pub z_up: bool,
    /// Optional colour per triangle (indexed meshes only) as 0xRRGGBBAA in
    /// sRGB; 0 means "no colour, use the viewer default".
    pub colors: Option<Vec<u32>>,
}

/// Parses `#RGB`, `#RRGGBB` or `#RRGGBBAA` into 0xRRGGBBAA (alpha forced opaque).
pub fn parse_hex_color(s: &str) -> Option<u32> {
    let h = s.trim().trim_start_matches('#');
    let v = match h.len() {
        3 => {
            let n = u32::from_str_radix(h, 16).ok()?;
            let (r, g, b) = ((n >> 8) & 0xf, (n >> 4) & 0xf, n & 0xf);
            (r * 17) << 16 | (g * 17) << 8 | (b * 17)
        }
        6 | 8 => u32::from_str_radix(&h[..6], 16).ok()?,
        _ => return None,
    };
    Some(v << 8 | 0xff)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Stl,
    Obj,
    ThreeMf,
    Ply,
    Off,
    Gltf,
}

impl Kind {
    pub fn from_ext(ext: &str) -> Option<Kind> {
        Some(match ext.to_ascii_lowercase().as_str() {
            "stl" => Kind::Stl,
            "obj" => Kind::Obj,
            "3mf" => Kind::ThreeMf,
            "ply" => Kind::Ply,
            "off" => Kind::Off,
            "glb" | "gltf" => Kind::Gltf,
            _ => return None,
        })
    }
}

pub fn parse(kind: Kind, data: &[u8], base_dir: Option<&Path>) -> Res<Mesh> {
    let mut mesh = match kind {
        Kind::Stl => stl::parse(data)?,
        Kind::Obj => obj::parse(data)?,
        Kind::ThreeMf => threemf::parse(data)?,
        Kind::Ply => ply::parse(data)?,
        Kind::Off => off::parse(data)?,
        Kind::Gltf => gltf_load::parse(data, base_dir)?,
    };
    if mesh.triangle_count() == 0 {
        return Err("modelde üçgen bulunamadı".into());
    }
    mesh.normalize_colors();
    Ok(mesh)
}

/// One build plate of a multi-plate 3MF project (see [`threemf::plates`]).
pub fn parse_3mf_plate(data: &[u8], plate: u32) -> Res<Mesh> {
    let mut mesh = threemf::parse_plate(data, Some(plate))?;
    if mesh.triangle_count() == 0 {
        return Err("bu plakada model yok".into());
    }
    mesh.normalize_colors();
    Ok(mesh)
}

impl Mesh {
    pub fn vertex_count(&self) -> usize {
        self.positions.len() / 3
    }

    pub fn triangle_count(&self) -> usize {
        match &self.indices {
            Some(idx) => idx.len() / 3,
            None => self.positions.len() / 9,
        }
    }

    #[inline]
    pub fn vertex(&self, i: usize) -> [f32; 3] {
        [self.positions[i * 3], self.positions[i * 3 + 1], self.positions[i * 3 + 2]]
    }

    /// Calls `f(triangle, a, b, c)` for every valid triangle.
    pub fn for_each_triangle_index(&self, mut f: impl FnMut(usize, usize, usize, usize)) {
        let vc = self.vertex_count();
        match &self.indices {
            Some(idx) => {
                for (ti, t) in idx.chunks_exact(3).enumerate() {
                    let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
                    if a < vc && b < vc && c < vc {
                        f(ti, a, b, c);
                    }
                }
            }
            None => {
                for t in 0..vc / 3 {
                    f(t, t * 3, t * 3 + 1, t * 3 + 2);
                }
            }
        }
    }

    /// Colour of triangle `t` (0 when the mesh has none).
    #[inline]
    pub fn triangle_color(&self, t: usize) -> u32 {
        self.colors.as_ref().and_then(|c| c.get(t).copied()).unwrap_or(0)
    }

    /// Drops the colour table when no triangle actually carries a colour.
    pub fn normalize_colors(&mut self) {
        let keep = self.indices.is_some() && self.colors.as_ref().is_some_and(|c| c.len() == self.triangle_count() && c.iter().any(|&v| v != 0));
        if !keep {
            self.colors = None;
        }
    }

    /// Axis-aligned bounds over all finite vertices.
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let mut mn = [f32::INFINITY; 3];
        let mut mx = [f32::NEG_INFINITY; 3];
        for p in self.positions.chunks_exact(3) {
            if !(p[0].is_finite() && p[1].is_finite() && p[2].is_finite()) {
                continue;
            }
            for k in 0..3 {
                mn[k] = mn[k].min(p[k]);
                mx[k] = mx[k].max(p[k]);
            }
        }
        if mn[0].is_finite() {
            Some((mn, mx))
        } else {
            None
        }
    }

    /// Binary layout consumed by the web viewer (all little endian):
    /// `b"M3DV"`, u32 version, u32 flags (bit0 indexed, bit1 z-up, bit2 colour
    /// groups), u32 vertex count, u32 index count, f32 positions, u32 indices,
    /// then (bit2) u32 group count and per group u32 first index, u32 index
    /// count, u32 colour. Coloured triangles are reordered so groups are contiguous.
    pub fn to_wire(&self) -> Vec<u8> {
        let idx_len = self.indices.as_ref().map_or(0, |i| i.len());
        let grouped = self.colors.is_some() && self.indices.is_some();
        let mut out = Vec::with_capacity(32 + self.positions.len() * 4 + idx_len * 4);
        out.extend_from_slice(b"M3DV");
        out.extend_from_slice(&2u32.to_le_bytes());
        let flags = (self.indices.is_some() as u32) | ((self.z_up as u32) << 1) | ((grouped as u32) << 2);
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&(self.vertex_count() as u32).to_le_bytes());
        out.extend_from_slice(&(idx_len as u32).to_le_bytes());
        for v in &self.positions[..self.vertex_count() * 3] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        let Some(idx) = &self.indices else { return out };
        if !grouped {
            for i in idx {
                out.extend_from_slice(&i.to_le_bytes());
            }
            return out;
        }
        let tris = idx.len() / 3;
        let mut order: Vec<usize> = (0..tris).collect();
        order.sort_by_key(|&t| self.triangle_color(t));
        let mut groups: Vec<(u32, u32, u32)> = Vec::new();
        for (pos, &t) in order.iter().enumerate() {
            for k in 0..3 {
                out.extend_from_slice(&idx[t * 3 + k].to_le_bytes());
            }
            let c = self.triangle_color(t);
            match groups.last_mut() {
                Some(g) if g.2 == c => g.1 += 3,
                _ => groups.push((pos as u32 * 3, 3, c)),
            }
        }
        out.extend_from_slice(&(groups.len() as u32).to_le_bytes());
        for (start, count, color) in groups {
            out.extend_from_slice(&start.to_le_bytes());
            out.extend_from_slice(&count.to_le_bytes());
            out.extend_from_slice(&color.to_le_bytes());
        }
        out
    }
}

/// Parses an ASCII float/int token without allocating.
#[inline]
pub(crate) fn parse_f32(tok: &[u8]) -> Option<f32> {
    std::str::from_utf8(tok).ok()?.parse::<f32>().ok()
}

#[inline]
pub(crate) fn parse_i64(tok: &[u8]) -> Option<i64> {
    std::str::from_utf8(tok).ok()?.parse::<i64>().ok()
}

/// Splits on ASCII whitespace.
pub(crate) fn tokens(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    data.split(|b| b.is_ascii_whitespace()).filter(|t| !t.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format_header() {
        let m = Mesh {
            positions: vec![0., 0., 0., 1., 0., 0., 0., 1., 0.],
            indices: Some(vec![0, 1, 2]),
            z_up: true,
            colors: None,
        };
        let w = m.to_wire();
        assert_eq!(&w[0..4], b"M3DV");
        assert_eq!(u32::from_le_bytes(w[8..12].try_into().unwrap()), 0b11);
        assert_eq!(u32::from_le_bytes(w[12..16].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(w[16..20].try_into().unwrap()), 3);
        assert_eq!(w.len(), 20 + 9 * 4 + 3 * 4);
    }

    #[test]
    fn wire_groups_triangles_by_colour() {
        let red = 0xff0000ff;
        let m = Mesh {
            positions: vec![0.; 12],
            indices: Some(vec![0, 1, 2, 1, 2, 3, 2, 3, 0]),
            z_up: true,
            colors: Some(vec![red, 0, red]),
        };
        let w = m.to_wire();
        let u = |o: usize| u32::from_le_bytes(w[o..o + 4].try_into().unwrap());
        assert_eq!(u(8) & 0b100, 0b100);
        let idx0 = 20 + 4 * 12;
        // uncoloured triangle (1,2,3) sorts first
        assert_eq!((u(idx0), u(idx0 + 4), u(idx0 + 8)), (1, 2, 3));
        let g = idx0 + 9 * 4;
        assert_eq!(u(g), 2);
        assert_eq!((u(g + 4), u(g + 8), u(g + 12)), (0, 3, 0));
        assert_eq!((u(g + 16), u(g + 20), u(g + 24)), (3, 6, red));
    }

    #[test]
    fn hex_colors() {
        assert_eq!(parse_hex_color("#FF6A13"), Some(0xff6a13ff));
        assert_eq!(parse_hex_color("#0086D6FF"), Some(0x0086d6ff));
        assert_eq!(parse_hex_color("#fff"), Some(0xffffffff));
        assert_eq!(parse_hex_color("nope"), None);
    }

    #[test]
    fn out_of_range_indices_are_skipped() {
        let m = Mesh { positions: vec![0.; 9], indices: Some(vec![0, 1, 2, 0, 1, 99]), z_up: false, colors: None };
        let mut n = 0;
        m.for_each_triangle_index(|_, _, _, _| n += 1);
        assert_eq!(n, 1);
    }
}
