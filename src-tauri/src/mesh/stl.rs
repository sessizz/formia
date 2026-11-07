use super::{parse_f32, tokens, Mesh, Res};

pub fn parse(data: &[u8]) -> Res<Mesh> {
    if data.len() >= 84 {
        let n = u32::from_le_bytes([data[80], data[81], data[82], data[83]]) as usize;
        if 84 + n.saturating_mul(50) == data.len() {
            return Ok(binary(data, n));
        }
    }
    if looks_ascii(data) {
        let m = ascii(data);
        if !m.positions.is_empty() {
            return Ok(m);
        }
    }
    if data.len() >= 84 {
        // Header count disagrees with the file size (truncated or padded file):
        // read as many whole triangles as the data actually holds.
        let n = (data.len() - 84) / 50;
        return Ok(binary(data, n));
    }
    Err("geçersiz STL".into())
}

fn looks_ascii(data: &[u8]) -> bool {
    let head = &data[..data.len().min(1024)];
    let start = head.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(0);
    if !head[start..].starts_with(b"solid") {
        return false;
    }
    // Many binary files also begin with "solid" in their 80 byte header.
    head.windows(5).any(|w| w == b"facet") || head.windows(8).any(|w| w == b"endsolid")
}

fn binary(data: &[u8], n: usize) -> Mesh {
    let mut positions = Vec::with_capacity(n * 9);
    for i in 0..n {
        let base = 84 + i * 50 + 12;
        let tri = &data[base..base + 36];
        for k in 0..9 {
            let o = k * 4;
            positions.push(f32::from_le_bytes([tri[o], tri[o + 1], tri[o + 2], tri[o + 3]]));
        }
    }
    Mesh { positions, indices: None, z_up: true, colors: None }
}

fn ascii(data: &[u8]) -> Mesh {
    let mut positions = Vec::new();
    let mut it = tokens(data);
    while let Some(t) = it.next() {
        if t.eq_ignore_ascii_case(b"vertex") {
            for _ in 0..3 {
                positions.push(it.next().and_then(parse_f32).unwrap_or(0.0));
            }
        }
    }
    positions.truncate(positions.len() / 9 * 9);
    Mesh { positions, indices: None, z_up: true, colors: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn binary_stl(tris: &[[f32; 9]]) -> Vec<u8> {
        let mut v = vec![0u8; 80];
        v[..5].copy_from_slice(b"solid"); // nasty but common header
        v.extend_from_slice(&(tris.len() as u32).to_le_bytes());
        for t in tris {
            v.extend_from_slice(&[0u8; 12]);
            for f in t {
                v.extend_from_slice(&f.to_le_bytes());
            }
            v.extend_from_slice(&[0u8; 2]);
        }
        v
    }

    #[test]
    fn parses_binary_even_with_solid_header() {
        let data = binary_stl(&[[0., 0., 0., 1., 0., 0., 0., 1., 0.], [0., 0., 1., 1., 0., 1., 0., 1., 1.]]);
        let m = parse(&data).unwrap();
        assert_eq!(m.triangle_count(), 2);
        assert_eq!(m.positions[7], 1.0); // y of the first triangle's third vertex
        assert_eq!(m.positions[17], 1.0); // z of the second triangle's third vertex
        assert!(m.z_up);
    }

    #[test]
    fn parses_ascii() {
        let src = b"solid cube\n facet normal 0 0 1\n  outer loop\n   vertex 0 0 0\n   vertex 1 0 0\n   vertex 0 1.5 0\n  endloop\n endfacet\nendsolid cube\n";
        let m = parse(src).unwrap();
        assert_eq!(m.triangle_count(), 1);
        assert_eq!(m.positions[7], 1.5);
    }

    #[test]
    fn truncated_binary_reads_whole_triangles() {
        let mut data = binary_stl(&[[0.; 9], [1.; 9], [2.; 9]]);
        data.truncate(84 + 50 * 2 + 10);
        let m = parse(&data).unwrap();
        assert_eq!(m.triangle_count(), 2);
    }
}
