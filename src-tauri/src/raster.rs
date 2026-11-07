//! Small software rasterizer for model thumbnails: orthographic three-quarter
//! view, flat shaded, z-buffered, 2x supersampled, transparent background.

use image::RgbaImage;

use crate::mesh::Mesh;

const SS: u32 = 2;
const BASE: [f32; 3] = [0.72, 0.76, 0.82]; // light steel, linear-ish
const EMPTY: f32 = f32::NEG_INFINITY;

fn norm(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-20);
    [v[0] / l, v[1] / l, v[2] / l]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

pub fn render(mesh: &Mesh, out_size: u32) -> Option<RgbaImage> {
    let (mn, mx) = mesh.bounds()?;
    let center = [(mn[0] + mx[0]) * 0.5, (mn[1] + mx[1]) * 0.5, (mn[2] + mx[2]) * 0.5];
    let w = out_size * SS;
    let h = w;

    // Camera: three-quarter view from the front-right, slightly above.
    let (az, el) = (35f32.to_radians(), 24f32.to_radians());
    let d = [az.sin() * el.cos(), el.sin(), az.cos() * el.cos()];
    let f = [-d[0], -d[1], -d[2]];
    let r = norm(cross(f, [0., 1., 0.]));
    let u = cross(r, f);
    let z_up = mesh.z_up;

    // Project every vertex into camera space (x right, y up, z toward camera).
    let vc = mesh.vertex_count();
    let mut proj: Vec<[f32; 3]> = Vec::with_capacity(vc);
    let (mut sxmin, mut sxmax, mut symin, mut symax) = (f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY, f32::NEG_INFINITY);
    for i in 0..vc {
        let p = mesh.vertex(i);
        let q = [p[0] - center[0], p[1] - center[1], p[2] - center[2]];
        // Z-up sources: model (x, y, z) -> view (x, z, -y), so the model's
        // front (-Y) faces the camera.
        let v = if z_up { [q[0], q[2], -q[1]] } else { q };
        let s = [dot(v, r), dot(v, u), dot(v, d)];
        if s[0].is_finite() && s[1].is_finite() {
            sxmin = sxmin.min(s[0]);
            sxmax = sxmax.max(s[0]);
            symin = symin.min(s[1]);
            symax = symax.max(s[1]);
        }
        proj.push(s);
    }
    if !sxmin.is_finite() {
        return None;
    }
    let span = (sxmax - sxmin).max(symax - symin).max(1e-12);
    let scale = w as f32 * 0.86 / span;
    let (cx, cy) = ((sxmin + sxmax) * 0.5, (symin + symax) * 0.5);
    for s in proj.iter_mut() {
        *s = [(s[0] - cx) * scale + w as f32 * 0.5, h as f32 * 0.5 - (s[1] - cy) * scale, s[2] * scale];
    }

    let l1 = norm([-0.45, 0.8, 0.6]);
    let l2 = norm([0.75, 0.05, 0.35]);
    let half = norm([l1[0], l1[1], l1[2] + 1.0]);

    let n_px = (w * h) as usize;
    let mut depth = vec![EMPTY; n_px];
    let mut color = vec![[0f32; 3]; n_px];

    // Filament / material colours, converted once per distinct value.
    let mut palette: Vec<(u32, [f32; 3])> = Vec::new();
    let mut base_for = |rgba: u32| -> [f32; 3] {
        if rgba == 0 {
            return BASE;
        }
        if let Some(&(_, c)) = palette.iter().find(|(k, _)| *k == rgba) {
            return c;
        }
        let c = filament_base(rgba);
        palette.push((rgba, c));
        c
    };

    mesh.for_each_triangle_index(|ti, ia, ib, ic| {
        let (a, b, c) = (proj[ia], proj[ib], proj[ic]);
        let area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
        if !area.is_finite() || area.abs() < 1e-9 {
            return;
        }
        // Normal in view space (screen y points down, so flip it back).
        let e1 = [b[0] - a[0], a[1] - b[1], b[2] - a[2]];
        let e2 = [c[0] - a[0], a[1] - c[1], c[2] - a[2]];
        let mut n = norm(cross(e1, e2));
        if n[2] < 0.0 {
            n = [-n[0], -n[1], -n[2]]; // two-sided lighting
        }
        let diffuse = 0.62 * dot(n, l1).max(0.0) + 0.2 * dot(n, l2).max(0.0);
        let ambient = 0.2 + 0.08 * n[1];
        let spec = 0.22 * dot(n, half).max(0.0).powi(32);
        let k = ambient + diffuse;
        let base = base_for(mesh.triangle_color(ti));
        let col = [base[0] * k + spec, base[1] * k + spec, base[2] * k + spec];

        let minx = a[0].min(b[0]).min(c[0]).floor().max(0.0) as i64;
        let maxx = a[0].max(b[0]).max(c[0]).ceil().min(w as f32 - 1.0) as i64;
        let miny = a[1].min(b[1]).min(c[1]).floor().max(0.0) as i64;
        let maxy = a[1].max(b[1]).max(c[1]).ceil().min(h as f32 - 1.0) as i64;
        if minx > maxx || miny > maxy {
            return;
        }
        let sign = if area > 0.0 { 1.0 } else { -1.0 };
        let inv = 1.0 / area.abs();
        for py in miny..=maxy {
            let y = py as f32 + 0.5;
            for px in minx..=maxx {
                let x = px as f32 + 0.5;
                let w0 = sign * ((c[0] - b[0]) * (y - b[1]) - (c[1] - b[1]) * (x - b[0]));
                let w1 = sign * ((a[0] - c[0]) * (y - c[1]) - (a[1] - c[1]) * (x - c[0]));
                let w2 = sign * ((b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0]));
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = (w0 * a[2] + w1 * b[2] + w2 * c[2]) * inv;
                let i = py as usize * w as usize + px as usize;
                if z > depth[i] {
                    depth[i] = z;
                    color[i] = col;
                }
            }
        }
    });

    // Darken silhouettes and depth discontinuities for a crisp outline.
    let crease = w as f32 * 0.03;
    let mut shade = vec![1f32; n_px];
    for y in 0..h as usize {
        for x in 0..w as usize {
            let i = y * w as usize + x;
            let z = depth[i];
            if z == EMPTY {
                continue;
            }
            let mut edge = 1.0f32;
            for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                    continue;
                }
                let nz = depth[ny as usize * w as usize + nx as usize];
                if nz == EMPTY {
                    edge = edge.min(0.6);
                } else if (nz - z).abs() > crease {
                    edge = edge.min(0.72);
                }
            }
            shade[i] = edge;
        }
    }

    let mut img = RgbaImage::new(out_size, out_size);
    let samples = (SS * SS) as f32;
    for oy in 0..out_size {
        for ox in 0..out_size {
            let mut acc = [0f32; 3];
            let mut cov = 0f32;
            for sy in 0..SS {
                for sx in 0..SS {
                    let i = ((oy * SS + sy) * w + ox * SS + sx) as usize;
                    if depth[i] != EMPTY {
                        let s = shade[i];
                        acc[0] += color[i][0] * s;
                        acc[1] += color[i][1] * s;
                        acc[2] += color[i][2] * s;
                        cov += 1.0;
                    }
                }
            }
            if cov > 0.0 {
                let px = [to_srgb(acc[0] / cov), to_srgb(acc[1] / cov), to_srgb(acc[2] / cov), (cov / samples * 255.0).round() as u8];
                img.put_pixel(ox, oy, image::Rgba(px));
            }
        }
    }
    Some(img)
}

/// sRGB 0xRRGGBBAA -> linear base colour, lifting near-black so the shape stays
/// readable (slicers render black filament as dark grey too).
fn filament_base(rgba: u32) -> [f32; 3] {
    let lin = |v: u32| {
        let s = v as f32 / 255.0;
        if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    let c = [lin(rgba >> 24 & 0xff), lin(rgba >> 16 & 0xff), lin(rgba >> 8 & 0xff)];
    [c[0] * 0.86 + 0.03, c[1] * 0.86 + 0.03, c[2] * 0.86 + 0.03]
}

fn to_srgb(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let s = if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube() -> Mesh {
        let p = vec![
            0., 0., 0., 1., 0., 0., 1., 1., 0., 0., 1., 0., 0., 0., 1., 1., 0., 1., 1., 1., 1., 0., 1., 1.,
        ];
        let i = vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5, 2, 3, 7, 2, 7, 6, 3, 0, 4, 3, 4, 7,
        ];
        Mesh { positions: p, indices: Some(i), z_up: true, colors: None }
    }

    #[test]
    fn renders_cube_centered_with_transparent_corners() {
        let img = render(&cube(), 64).unwrap();
        assert_eq!(img.dimensions(), (64, 64));
        assert_eq!(img.get_pixel(0, 0)[3], 0, "corner should be transparent");
        let c = img.get_pixel(32, 32);
        assert_eq!(c[3], 255, "center should be covered");
        assert!(c[0] > 40, "center should be lit");
    }

    #[test]
    fn uses_triangle_colours() {
        let mut m = cube();
        m.colors = Some(vec![0xff2020ff; 12]);
        let img = render(&m, 64).unwrap();
        let c = img.get_pixel(32, 32);
        assert!(c[0] > c[1] + 40 && c[0] > c[2] + 40, "centre should be red, got {c:?}");
    }

    #[test]
    fn degenerate_mesh_does_not_panic() {
        let m = Mesh { positions: vec![1.; 9], indices: None, z_up: false, colors: None };
        assert!(render(&m, 32).is_some());
    }
}
