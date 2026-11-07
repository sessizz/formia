//! Extracts the preview images slicers embed in G-code comments
//! (`; thumbnail begin WxH len` ... `; thumbnail end`, PNG or JPG, base64).

use base64::Engine;

pub fn embedded_thumbnail(head: &[u8]) -> Option<Vec<u8>> {
    let text = String::from_utf8_lossy(head);
    let mut best: Option<(u64, Vec<u8>)> = None;
    let mut cur: Option<(u64, String)> = None;
    for line in text.lines() {
        let l = line.trim_start_matches(';').trim();
        if let Some(rest) = l.strip_prefix("thumbnail begin").or_else(|| l.strip_prefix("thumbnail_JPG begin")).or_else(|| l.strip_prefix("thumbnail_PNG begin")) {
            let dims = rest.split_whitespace().next().unwrap_or("");
            let area = dims.split_once('x').and_then(|(w, h)| Some(w.parse::<u64>().ok()? * h.parse::<u64>().ok()?)).unwrap_or(1);
            cur = Some((area, String::new()));
        } else if l.starts_with("thumbnail end") || l.starts_with("thumbnail_JPG end") || l.starts_with("thumbnail_PNG end") {
            if let Some((area, b64)) = cur.take() {
                if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64.as_bytes()) {
                    if best.as_ref().map_or(true, |(a, _)| area > *a) {
                        best = Some((area, bytes));
                    }
                }
            }
        } else if let Some((_, b64)) = cur.as_mut() {
            b64.push_str(l);
        }
    }
    best.map(|(_, b)| b)
}

#[cfg(test)]
mod tests {
    #[test]
    fn picks_largest_thumbnail() {
        let src = "; generated\n;\n; thumbnail begin 16x16 8\n; AAECAw==\n; thumbnail end\n; thumbnail begin 300x300 12\n; BAUG\n; BwgJ\n; thumbnail end\nG28\n";
        let t = super::embedded_thumbnail(src.as_bytes()).unwrap();
        assert_eq!(t, vec![4, 5, 6, 7, 8, 9]);
    }
}
