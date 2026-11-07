use std::path::Path;

use super::{Mesh, Res};

type Mat = [[f32; 4]; 4];
const IDENTITY: Mat = [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.], [0., 0., 0., 1.]];

pub fn parse(data: &[u8], base_dir: Option<&Path>) -> Res<Mesh> {
    let g = gltf::Gltf::from_slice(data).map_err(|e| format!("glTF okunamadı: {e}"))?;
    let buffers = gltf::import_buffers(&g.document, base_dir, g.blob.clone()).map_err(|e| format!("glTF verisi okunamadı: {e}"))?;
    let mut out = Mesh { positions: Vec::new(), indices: Some(Vec::new()), z_up: false, colors: Some(Vec::new()) };

    let scene = g.document.default_scene().or_else(|| g.document.scenes().next());
    match scene {
        Some(scene) => {
            for node in scene.nodes() {
                visit(&node, &IDENTITY, &buffers, &mut out, 0);
            }
        }
        None => {
            for mesh in g.document.meshes() {
                add_mesh(&mesh, &IDENTITY, &buffers, &mut out);
            }
        }
    }
    Ok(out)
}

fn visit(node: &gltf::Node, parent: &Mat, buffers: &[gltf::buffer::Data], out: &mut Mesh, depth: u32) {
    if depth > 64 {
        return;
    }
    let m = mul(parent, &node.transform().matrix());
    if let Some(mesh) = node.mesh() {
        add_mesh(&mesh, &m, buffers, out);
    }
    for child in node.children() {
        visit(&child, &m, buffers, out, depth + 1);
    }
}

fn add_mesh(mesh: &gltf::Mesh, m: &Mat, buffers: &[gltf::buffer::Data], out: &mut Mesh) {
    for prim in mesh.primitives() {
        if prim.mode() != gltf::mesh::Mode::Triangles {
            continue;
        }
        let reader = prim.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
        let Some(pos) = reader.read_positions() else { continue };
        let base = (out.positions.len() / 3) as u32;
        let mut count = 0u32;
        for p in pos {
            // glTF matrices are column major: m[col][row].
            out.positions.push(m[0][0] * p[0] + m[1][0] * p[1] + m[2][0] * p[2] + m[3][0]);
            out.positions.push(m[0][1] * p[0] + m[1][1] * p[1] + m[2][1] * p[2] + m[3][1]);
            out.positions.push(m[0][2] * p[0] + m[1][2] * p[1] + m[2][2] * p[2] + m[3][2]);
            count += 1;
        }
        let idx = out.indices.get_or_insert_with(Vec::new);
        let before = idx.len();
        match reader.read_indices() {
            Some(ind) => idx.extend(ind.into_u32().map(|i| base + i)),
            None => idx.extend((0..count - count % 3).map(|i| base + i)),
        }
        idx.truncate(before + (idx.len() - before) / 3 * 3);
        let tris = (idx.len() - before) / 3;
        let color = material_color(&prim.material());
        out.colors.get_or_insert_with(Vec::new).extend(std::iter::repeat(color).take(tris));
    }
}

/// Base colour of an untextured material as sRGB 0xRRGGBBAA (0 = default/textured).
fn material_color(mat: &gltf::Material) -> u32 {
    if mat.index().is_none() {
        return 0;
    }
    let pbr = mat.pbr_metallic_roughness();
    if pbr.base_color_texture().is_some() {
        return 0;
    }
    let to8 = |v: f32| {
        let v = v.clamp(0.0, 1.0);
        let s = if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
        (s * 255.0).round() as u32
    };
    let [r, g, b, _] = pbr.base_color_factor();
    to8(r) << 24 | to8(g) << 16 | to8(b) << 8 | 0xff
}

fn mul(a: &Mat, b: &Mat) -> Mat {
    // column-major: (a*b)[c][r] = sum_k a[k][r] * b[c][k]
    let mut r = [[0f32; 4]; 4];
    for c in 0..4 {
        for row in 0..4 {
            r[c][row] = (0..4).map(|k| a[k][row] * b[c][k]).sum();
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_embedded_gltf_with_translation() {
        // One triangle, positions in a base64 data URI, node translated by (0, 5, 0).
        let mut bin = Vec::new();
        for v in [0f32, 0., 0., 1., 0., 0., 0., 1., 0.] {
            bin.extend_from_slice(&v.to_le_bytes());
        }
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bin);
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0,"translation":[0,5,0]}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}}}}]}}],
            "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}}],
            "bufferViews":[{{"buffer":0,"byteLength":36}}],
            "buffers":[{{"byteLength":36,"uri":"data:application/octet-stream;base64,{b64}"}}]}}"#
        );
        let m = parse(json.as_bytes(), None).unwrap();
        assert_eq!(m.triangle_count(), 1);
        assert_eq!(m.positions[7], 6.0);
        assert!(!m.z_up);
    }
}
