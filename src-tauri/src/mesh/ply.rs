use super::{Mesh, Res};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Ty {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    F32,
    F64,
}

impl Ty {
    fn parse(s: &str) -> Option<Ty> {
        Some(match s {
            "char" | "int8" => Ty::I8,
            "uchar" | "uint8" => Ty::U8,
            "short" | "int16" => Ty::I16,
            "ushort" | "uint16" => Ty::U16,
            "int" | "int32" => Ty::I32,
            "uint" | "uint32" => Ty::U32,
            "float" | "float32" => Ty::F32,
            "double" | "float64" => Ty::F64,
            _ => return None,
        })
    }
    fn size(self) -> usize {
        match self {
            Ty::I8 | Ty::U8 => 1,
            Ty::I16 | Ty::U16 => 2,
            Ty::I32 | Ty::U32 | Ty::F32 => 4,
            Ty::F64 => 8,
        }
    }
}

#[derive(Debug)]
enum Prop {
    Scalar(String, Ty),
    List(String, Ty, Ty),
}

#[derive(Debug)]
struct Element {
    name: String,
    count: usize,
    props: Vec<Prop>,
}

#[derive(PartialEq)]
enum Format {
    Ascii,
    Le,
    Be,
}

pub fn parse(data: &[u8]) -> Res<Mesh> {
    if !data.starts_with(b"ply") {
        return Err("PLY başlığı yok".into());
    }
    let end = find(data, b"end_header").ok_or("PLY başlığı bitmiyor")?;
    let mut body_start = end + b"end_header".len();
    if data.get(body_start) == Some(&b'\r') {
        body_start += 1;
    }
    if data.get(body_start) == Some(&b'\n') {
        body_start += 1;
    }
    let header = String::from_utf8_lossy(&data[..end]);
    let mut format = Format::Ascii;
    let mut elements: Vec<Element> = Vec::new();
    for line in header.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        match parts.as_slice() {
            ["format", f, ..] => {
                format = match *f {
                    "ascii" => Format::Ascii,
                    "binary_little_endian" => Format::Le,
                    "binary_big_endian" => Format::Be,
                    _ => return Err("bilinmeyen PLY biçimi".into()),
                }
            }
            ["element", name, count] => elements.push(Element { name: name.to_string(), count: count.parse().map_err(|_| "PLY eleman sayısı")?, props: vec![] }),
            ["property", "list", ct, it, name] => {
                let el = elements.last_mut().ok_or("PLY özelliği elemansız")?;
                el.props.push(Prop::List(name.to_string(), Ty::parse(ct).ok_or("PLY tipi")?, Ty::parse(it).ok_or("PLY tipi")?));
            }
            ["property", ty, name] => {
                let el = elements.last_mut().ok_or("PLY özelliği elemansız")?;
                el.props.push(Prop::Scalar(name.to_string(), Ty::parse(ty).ok_or("PLY tipi")?));
            }
            _ => {}
        }
    }

    let body = &data[body_start..];
    let mut src: Box<dyn Source> = match format {
        Format::Ascii => Box::new(AsciiSource { toks: Box::new(super::tokens(body)) }),
        Format::Le => Box::new(BinSource { data: body, pos: 0, le: true }),
        Format::Be => Box::new(BinSource { data: body, pos: 0, le: false }),
    };

    let mut positions: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut list: Vec<u32> = Vec::new();
    for el in &elements {
        let is_vertex = el.name == "vertex";
        let is_face = el.name == "face";
        let xyz: Vec<Option<usize>> = ["x", "y", "z"]
            .iter()
            .map(|n| el.props.iter().position(|p| matches!(p, Prop::Scalar(pn, _) if pn == n)))
            .collect();
        if is_vertex {
            positions.reserve(el.count * 3);
        }
        for _ in 0..el.count {
            let mut v = [0f32; 3];
            for (pi, p) in el.props.iter().enumerate() {
                match p {
                    Prop::Scalar(_, ty) => {
                        let val = src.read(*ty).ok_or("PLY verisi eksik")?;
                        if is_vertex {
                            for k in 0..3 {
                                if xyz[k] == Some(pi) {
                                    v[k] = val as f32;
                                }
                            }
                        }
                    }
                    Prop::List(name, ct, it) => {
                        let n = src.read(*ct).ok_or("PLY verisi eksik")? as usize;
                        list.clear();
                        for _ in 0..n {
                            list.push(src.read(*it).ok_or("PLY verisi eksik")? as u32);
                        }
                        if is_face && (name == "vertex_indices" || name == "vertex_index") {
                            for k in 1..list.len().saturating_sub(1) {
                                indices.extend_from_slice(&[list[0], list[k], list[k + 1]]);
                            }
                        }
                    }
                }
            }
            if is_vertex {
                positions.extend_from_slice(&v);
            }
        }
    }
    if indices.is_empty() {
        return Err("PLY dosyasında yüz yok (nokta bulutu)".into());
    }
    Ok(Mesh { positions, indices: Some(indices), z_up: true, colors: None })
}

trait Source {
    fn read(&mut self, ty: Ty) -> Option<f64>;
}

struct AsciiSource<'a> {
    toks: Box<dyn Iterator<Item = &'a [u8]> + 'a>,
}

impl Source for AsciiSource<'_> {
    fn read(&mut self, _ty: Ty) -> Option<f64> {
        std::str::from_utf8(self.toks.next()?).ok()?.parse().ok()
    }
}

struct BinSource<'a> {
    data: &'a [u8],
    pos: usize,
    le: bool,
}

impl Source for BinSource<'_> {
    fn read(&mut self, ty: Ty) -> Option<f64> {
        let n = ty.size();
        let b = self.data.get(self.pos..self.pos + n)?;
        self.pos += n;
        let mut a = [0u8; 8];
        a[..n].copy_from_slice(b);
        if !self.le {
            a[..n].reverse();
        }
        Some(match ty {
            Ty::I8 => a[0] as i8 as f64,
            Ty::U8 => a[0] as f64,
            Ty::I16 => i16::from_le_bytes([a[0], a[1]]) as f64,
            Ty::U16 => u16::from_le_bytes([a[0], a[1]]) as f64,
            Ty::I32 => i32::from_le_bytes([a[0], a[1], a[2], a[3]]) as f64,
            Ty::U32 => u32::from_le_bytes([a[0], a[1], a[2], a[3]]) as f64,
            Ty::F32 => f32::from_le_bytes([a[0], a[1], a[2], a[3]]) as f64,
            Ty::F64 => f64::from_le_bytes(a),
        })
    }
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_with_extra_props_and_quad() {
        let src = b"ply\nformat ascii 1.0\ncomment hi\nelement vertex 4\nproperty float x\nproperty float y\nproperty float z\nproperty uchar red\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n0 0 0 255\n1 0 0 255\n1 1 0 255\n0 1 2.5 255\n4 0 1 2 3\n";
        let m = parse(src).unwrap();
        assert_eq!(m.vertex_count(), 4);
        assert_eq!(m.triangle_count(), 2);
        assert_eq!(m.positions[11], 2.5);
    }

    #[test]
    fn binary_little_endian() {
        let mut d = b"ply\r\nformat binary_little_endian 1.0\r\nelement vertex 3\r\nproperty float x\r\nproperty float y\r\nproperty float z\r\nelement face 1\r\nproperty list uchar uint vertex_index\r\nend_header\r\n".to_vec();
        for v in [0f32, 0., 0., 1., 0., 0., 0., 2., 0.] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.push(3);
        for i in [0u32, 1, 2] {
            d.extend_from_slice(&i.to_le_bytes());
        }
        let m = parse(&d).unwrap();
        assert_eq!(m.triangle_count(), 1);
        assert_eq!(m.positions[7], 2.0);
    }
}
