use super::{parse_f32, parse_i64, Mesh, Res};

pub fn parse(data: &[u8]) -> Res<Mesh> {
    let mut positions: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut face: Vec<u32> = Vec::with_capacity(8);

    for raw in data.split(|&b| b == b'\n') {
        let line = trim(raw);
        if line.len() < 2 {
            continue;
        }
        let mut parts = line.split(|b| b.is_ascii_whitespace()).filter(|t| !t.is_empty());
        match parts.next() {
            Some(b"v") => {
                let x = parts.next().and_then(parse_f32).unwrap_or(0.0);
                let y = parts.next().and_then(parse_f32).unwrap_or(0.0);
                let z = parts.next().and_then(parse_f32).unwrap_or(0.0);
                positions.extend_from_slice(&[x, y, z]);
            }
            Some(b"f") => {
                face.clear();
                let vc = (positions.len() / 3) as i64;
                for tok in parts {
                    let first = tok.split(|&b| b == b'/').next().unwrap_or(tok);
                    if let Some(i) = parse_i64(first) {
                        let idx = if i > 0 { i - 1 } else { vc + i };
                        if idx >= 0 && idx < vc {
                            face.push(idx as u32);
                        }
                    }
                }
                for k in 1..face.len().saturating_sub(1) {
                    indices.extend_from_slice(&[face[0], face[k], face[k + 1]]);
                }
            }
            _ => {}
        }
    }

    let mut mesh = Mesh { positions, indices: Some(indices), z_up: false, colors: None };
    mesh.z_up = guess_z_up(&mesh);
    Ok(mesh)
}

/// OBJ has no up-axis convention. Exports from 3D-printing tools are usually
/// Z-up and rest on z = 0, while DCC exports (Blender etc.) are Y-up and rest
/// on y = 0.
fn guess_z_up(mesh: &Mesh) -> bool {
    let Some((mn, mx)) = mesh.bounds() else { return false };
    let size = (0..3).map(|k| mx[k] - mn[k]).fold(0.0f32, f32::max).max(1e-6);
    let eps = size * 1e-3;
    mn[2].abs() <= eps && mn[1].abs() > eps
}

fn trim(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(s.len());
    let end = s.iter().rposition(|b| !b.is_ascii_whitespace()).map_or(start, |e| e + 1);
    &s[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quads_and_relative_indices() {
        let src = b"# cube face\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nf 1/1/1 2/2/1 3/3/1 4/4/1\nf -4 -3 -2\n";
        let m = parse(src).unwrap();
        assert_eq!(m.vertex_count(), 4);
        assert_eq!(m.triangle_count(), 3);
        assert_eq!(m.indices.as_ref().unwrap()[6..9], [0, 1, 2]);
    }

    #[test]
    fn handles_crlf_and_bad_indices() {
        let src = b"v 0 0 0\r\nv 1 0 0\r\nv 0 1 0\r\nf 1 2 3\r\nf 1 2 99\r\n";
        let m = parse(src).unwrap();
        assert_eq!(m.triangle_count(), 1);
    }

    #[test]
    fn guesses_up_axis() {
        // Rests on z = 0, extends in -y: printing export.
        let m = parse(b"v 0 -5 0\nv 1 -5 0\nv 0 -4 3\nf 1 2 3\n").unwrap();
        assert!(m.z_up);
        // Rests on y = 0: Blender export.
        let m = parse(b"v 0 0 -5\nv 1 0 -5\nv 0 3 -4\nf 1 2 3\n").unwrap();
        assert!(!m.z_up);
    }
}
