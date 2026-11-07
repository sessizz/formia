use super::{parse_f32, Mesh, Res};

pub fn parse(data: &[u8]) -> Res<Mesh> {
    let mut lines = data
        .split(|&b| b == b'\n')
        .map(|l| {
            let l = l.split(|&b| b == b'#').next().unwrap_or(&[]);
            l.split(|b| b.is_ascii_whitespace()).filter(|t| !t.is_empty()).collect::<Vec<_>>()
        })
        .filter(|t| !t.is_empty());

    let mut first = lines.next().ok_or("boş OFF")?;
    if !first[0].ends_with(b"OFF") {
        return Err("OFF başlığı yok".into());
    }
    first.remove(0);
    let counts = if first.len() >= 2 { first } else { lines.next().ok_or("OFF sayıları yok")? };
    let nv: usize = std::str::from_utf8(counts[0]).ok().and_then(|s| s.parse().ok()).ok_or("OFF köşe sayısı")?;
    let nf: usize = std::str::from_utf8(counts[1]).ok().and_then(|s| s.parse().ok()).ok_or("OFF yüz sayısı")?;

    let mut positions = Vec::with_capacity(nv * 3);
    for _ in 0..nv {
        let t = lines.next().ok_or("OFF köşeleri eksik")?;
        for k in 0..3 {
            positions.push(t.get(k).and_then(|x| parse_f32(x)).unwrap_or(0.0));
        }
    }
    let mut indices = Vec::new();
    for _ in 0..nf {
        let Some(t) = lines.next() else { break };
        let n: usize = std::str::from_utf8(t[0]).ok().and_then(|s| s.parse().ok()).unwrap_or(0);
        let f: Vec<u32> = t.iter().skip(1).take(n).filter_map(|x| std::str::from_utf8(x).ok()?.parse().ok()).collect();
        for k in 1..f.len().saturating_sub(1) {
            indices.extend_from_slice(&[f[0], f[k], f[k + 1]]);
        }
    }
    Ok(Mesh { positions, indices: Some(indices), z_up: true, colors: None })
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_off() {
        let m = super::parse(b"OFF\n# c\n4 1 0\n0 0 0\n1 0 0\n1 1 0\n0 1 0\n4 0 1 2 3\n").unwrap();
        assert_eq!(m.triangle_count(), 2);
    }
}
