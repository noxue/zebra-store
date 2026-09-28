//! Content sniffing, image dimensions and SVG safety checks for uploads.
//!
//! `sniff` mirrors the subset of Go's `http.DetectContentType` the upload
//! policy relies on; `validate_svg` is a strict, parser-based allow-list
//! (UPL-01) instead of the original string block-list.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

/// Bytes inspected for sniffing (Go uses 512).
const SNIFF_LEN: usize = 512;

pub const MIME_SVG: &str = "image/svg+xml";

/// Detects the MIME type from magic bytes.
pub fn sniff(data: &[u8]) -> String {
    let head = &data[..data.len().min(SNIFF_LEN)];
    let starts = |sig: &[u8]| head.starts_with(sig);
    let mime = if starts(b"\xFF\xD8\xFF") {
        "image/jpeg"
    } else if starts(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        "image/gif"
    } else if head.len() >= 14 && &head[..4] == b"RIFF" && &head[8..14] == b"WEBPVP" {
        "image/webp"
    } else if starts(b"BM") {
        "image/bmp"
    } else if starts(b"\x00\x00\x01\x00") || starts(b"\x00\x00\x02\x00") {
        "image/x-icon"
    } else if starts(b"%PDF-") {
        "application/pdf"
    } else if starts(b"PK\x03\x04") {
        "application/zip"
    } else if starts(b"\x1f\x8b\x08") {
        "application/x-gzip"
    } else if starts(b"Rar!\x1a\x07") {
        "application/x-rar-compressed"
    } else if is_html(head) {
        "text/html; charset=utf-8"
    } else if trim_ws(head).starts_with(b"<?xml") {
        "text/xml; charset=utf-8"
    } else if !head.iter().any(|b| is_binary_byte(*b)) {
        "text/plain; charset=utf-8"
    } else {
        "application/octet-stream"
    };
    mime.to_owned()
}

fn trim_ws(b: &[u8]) -> &[u8] {
    let start = b
        .iter()
        .position(|c| !matches!(c, b'\t' | b'\n' | b'\x0c' | b'\r' | b' '))
        .unwrap_or(b.len());
    &b[start..]
}

fn is_binary_byte(b: u8) -> bool {
    matches!(b, 0x00..=0x08 | 0x0B | 0x0E..=0x1A | 0x1C..=0x1F)
}

fn is_html(head: &[u8]) -> bool {
    const SIGS: [&[u8]; 17] = [
        b"<!DOCTYPE HTML",
        b"<HTML",
        b"<HEAD",
        b"<SCRIPT",
        b"<IFRAME",
        b"<H1",
        b"<DIV",
        b"<FONT",
        b"<TABLE",
        b"<A",
        b"<STYLE",
        b"<TITLE",
        b"<B",
        b"<BODY",
        b"<BR",
        b"<P",
        b"<!--",
    ];
    let data = trim_ws(head);
    SIGS.iter().any(|sig| {
        data.len() > sig.len()
            && data[..sig.len()].eq_ignore_ascii_case(sig)
            && matches!(data[sig.len()], b' ' | b'>')
    })
}

/// Content looks like SVG (`<?xml` / `<svg` prefix or contains `<svg`).
pub fn looks_like_svg(data: &[u8]) -> bool {
    let head = &data[..data.len().min(SNIFF_LEN)];
    let t = trim_ws(head);
    t.starts_with(b"<?xml") || t.starts_with(b"<svg") || head.windows(4).any(|w| w == b"<svg")
}

/// Width/height of a PNG, GIF, JPEG or WebP image.
pub fn image_dimensions(data: &[u8], mime: &str) -> Result<(u32, u32), String> {
    let dims = match mime {
        "image/png" => png(data),
        "image/gif" => gif(data),
        "image/jpeg" => jpeg(data),
        "image/webp" => {
            return webp(data).ok_or_else(|| "无法解析 WebP 图片: invalid data".to_owned());
        }
        _ => None,
    };
    dims.ok_or_else(|| "无法解析图片: image: unknown format".to_owned())
}

fn be16(d: &[u8], i: usize) -> Option<u32> {
    Some(u32::from(u16::from_be_bytes([*d.get(i)?, *d.get(i + 1)?])))
}

fn le16(d: &[u8], i: usize) -> Option<u32> {
    Some(u32::from(u16::from_le_bytes([*d.get(i)?, *d.get(i + 1)?])))
}

fn png(d: &[u8]) -> Option<(u32, u32)> {
    if d.get(12..16)? != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(d.get(16..20)?.try_into().ok()?);
    let h = u32::from_be_bytes(d.get(20..24)?.try_into().ok()?);
    Some((w, h))
}

fn gif(d: &[u8]) -> Option<(u32, u32)> {
    Some((le16(d, 6)?, le16(d, 8)?))
}

fn jpeg(d: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2;
    while i + 4 <= d.len() {
        if d[i] != 0xFF {
            return None;
        }
        let marker = d[i + 1];
        if marker == 0xFF {
            i += 1;
            continue;
        }
        if matches!(marker, 0xD8 | 0x01 | 0xD0..=0xD7) {
            i += 2;
            continue;
        }
        let len = be16(d, i + 2)? as usize;
        let is_sof = matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
        if is_sof {
            return Some((be16(d, i + 7)?, be16(d, i + 5)?));
        }
        i += 2 + len;
    }
    None
}

fn webp(d: &[u8]) -> Option<(u32, u32)> {
    if d.get(0..4)? != b"RIFF" || d.get(8..12)? != b"WEBP" {
        return None;
    }
    let mut i = 12;
    loop {
        let kind = d.get(i..i + 4)?;
        let size = u32::from_le_bytes(d.get(i + 4..i + 8)?.try_into().ok()?) as usize;
        let data = d.get(i + 8..i + 8 + size)?;
        match kind {
            b"VP8X" if data.len() >= 10 => {
                let w =
                    1 + u32::from(data[4]) + (u32::from(data[5]) << 8) + (u32::from(data[6]) << 16);
                let h =
                    1 + u32::from(data[7]) + (u32::from(data[8]) << 8) + (u32::from(data[9]) << 16);
                return Some((w, h));
            }
            b"VP8 " if data.len() >= 10 => {
                return Some((le16(data, 6)? & 0x3FFF, le16(data, 8)? & 0x3FFF));
            }
            b"VP8L" if data.len() >= 5 => {
                if data[0] != 0x2f {
                    return None;
                }
                let bits = u32::from_le_bytes(data[1..5].try_into().ok()?);
                return Some(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1));
            }
            b"VP8X" | b"VP8 " | b"VP8L" => return None,
            _ => {}
        }
        i += 8 + size + (size % 2);
    }
}

// ---------------------------------------------------------------------------
// SVG
// ---------------------------------------------------------------------------

/// Elements allowed in an uploaded SVG (static graphics only: no scripting,
/// foreign content, embedding or SMIL animation).
const SVG_ELEMENTS: &[&str] = &[
    "svg",
    "a",
    "g",
    "defs",
    "symbol",
    "use",
    "title",
    "desc",
    "metadata",
    "path",
    "rect",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "text",
    "tspan",
    "textpath",
    "lineargradient",
    "radialgradient",
    "stop",
    "pattern",
    "clippath",
    "mask",
    "marker",
    "image",
    "style",
    "switch",
    "view",
    "filter",
    "feblend",
    "fecolormatrix",
    "fecomponenttransfer",
    "fecomposite",
    "feconvolvematrix",
    "fediffuselighting",
    "fedisplacementmap",
    "fedistantlight",
    "fedropshadow",
    "feflood",
    "fefunca",
    "fefuncb",
    "fefuncg",
    "fefuncr",
    "fegaussianblur",
    "feimage",
    "femerge",
    "femergenode",
    "femorphology",
    "feoffset",
    "fepointlight",
    "fespecularlighting",
    "fespotlight",
    "fetile",
    "feturbulence",
];

/// Raster data URIs allowed as `href` of `<image>`.
const SAFE_DATA_URIS: [&str; 5] = [
    "data:image/png",
    "data:image/jpeg",
    "data:image/jpg",
    "data:image/gif",
    "data:image/webp",
];

fn svg_err(msg: impl Into<String>) -> String {
    msg.into()
}

/// Lower-cased value without whitespace/control characters (browsers ignore them in URLs).
fn squash(value: &str) -> String {
    value
        .chars()
        .filter(|c| *c > ' ' && *c != '\u{7f}')
        .flat_map(char::to_lowercase)
        .collect()
}

/// Rejects script-capable content inside a value or stylesheet.
fn check_dangerous(raw: &str) -> Result<(), String> {
    let v = squash(raw);
    for bad in ["javascript:", "vbscript:", "livescript:"] {
        if v.contains(bad) {
            return Err(svg_err("SVG 文件不允许包含 javascript: 协议"));
        }
    }
    for bad in [
        "data:text/html",
        "data:application",
        "data:image/svg",
        "data:text/xml",
    ] {
        if v.contains(bad) {
            return Err(svg_err("SVG 文件不允许包含危险的 data: URI"));
        }
    }
    if v.contains("@import") || v.contains("expression(") || v.contains("-moz-binding") {
        return Err(svg_err("SVG 文件不允许引用外部样式"));
    }
    let clean: String = v.chars().filter(|c| *c != '"' && *c != '\'').collect();
    let mut rest = clean.as_str();
    while let Some(pos) = rest.find("url(") {
        rest = &rest[pos + 4..];
        if !rest.starts_with('#') {
            return Err(svg_err("SVG 文件不允许引用外部资源"));
        }
    }
    Ok(())
}

fn check_element(e: &BytesStart<'_>, depth: usize, seen_root: &mut bool) -> Result<bool, String> {
    let local = AsRef::<str>::as_ref(&e.local_name()).to_lowercase();
    if depth == 0 {
        if *seen_root || local != "svg" {
            return Err(svg_err("SVG 文件根元素必须是 <svg>"));
        }
        *seen_root = true;
    }
    if local == "script" {
        return Err(svg_err("SVG 文件不允许包含 <script> 标签"));
    }
    if local == "foreignobject" {
        return Err(svg_err("SVG 文件不允许包含 <foreignObject> 元素"));
    }
    if !SVG_ELEMENTS.contains(&local.as_str()) {
        return Err(svg_err(format!("SVG 文件不允许包含 <{local}> 元素")));
    }
    for attr in e.attributes() {
        let attr = attr.map_err(|err| svg_err(format!("SVG 文件解析失败: {err}")))?;
        let name = AsRef::<str>::as_ref(&attr.key.local_name()).to_lowercase();
        if name.starts_with("on") {
            return Err(svg_err(format!("SVG 文件不允许包含事件处理属性: {name}")));
        }
        let value = attr
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|err| svg_err(format!("SVG 文件解析失败: {err}")))?;
        check_dangerous(&value)?;
        if matches!(name.as_str(), "href" | "src") {
            let v = squash(&value);
            let safe = v.is_empty()
                || v.starts_with('#')
                || SAFE_DATA_URIS.iter().any(|p| v.starts_with(p));
            if !safe {
                return Err(svg_err("SVG 文件不允许引用外部资源"));
            }
        }
    }
    Ok(local == "style")
}

/// Validates an uploaded SVG with a strict XML parser.
///
/// Rejects malformed XML, processing instructions, entity declarations or custom
/// entity references, non-`svg` roots, script/foreign/animation/unknown elements,
/// `on*` attributes, script-capable URIs (after entity decoding) and external refs.
pub fn validate_svg(data: &[u8]) -> Result<(), String> {
    let text = std::str::from_utf8(data).map_err(|_| svg_err("SVG 文件解析失败: invalid UTF-8"))?;
    let mut reader = Reader::from_str(text);
    reader.config_mut().check_end_names = true;
    let mut depth = 0usize;
    let mut seen_root = false;
    let mut in_style = false;
    loop {
        let event = reader
            .read_event()
            .map_err(|err| svg_err(format!("SVG 文件解析失败: {err}")))?;
        match event {
            Event::Start(e) => {
                in_style = check_element(&e, depth, &mut seen_root)?;
                depth += 1;
            }
            Event::Empty(e) => {
                check_element(&e, depth, &mut seen_root)?;
                in_style = false;
            }
            Event::End(_) => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| svg_err("SVG 文件解析失败: unexpected end tag"))?;
                in_style = false;
            }
            Event::Text(t) => {
                let s: &str = &t;
                if depth == 0 && !s.trim().is_empty() {
                    return Err(svg_err("SVG 文件解析失败: text outside root"));
                }
                if in_style {
                    check_dangerous(s)?;
                }
            }
            Event::CData(c) => {
                if in_style {
                    check_dangerous(&c)?;
                }
            }
            Event::GeneralRef(r) => {
                let name: &str = &r;
                let predefined = matches!(name, "lt" | "gt" | "amp" | "apos" | "quot");
                if !predefined && !name.starts_with('#') {
                    return Err(svg_err("SVG 文件不允许声明实体"));
                }
            }
            Event::PI(_) => {
                return Err(svg_err("SVG 文件不允许包含处理指令"));
            }
            Event::DocType(d) => {
                let s = d.to_lowercase();
                if s.contains("<!entity") || s.contains('[') {
                    return Err(svg_err("SVG 文件不允许声明实体"));
                }
            }
            Event::Decl(_) | Event::Comment(_) => {}
            Event::Eof => break,
        }
    }
    if depth != 0 || !seen_root {
        return Err(svg_err("SVG 文件解析失败: unexpected EOF"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // UPL-01: every known bypass is rejected; a normal SVG passes.
    #[test]
    fn upl_01_svg_payloads() {
        let bad = [
            ("script", "<svg><script>alert(1)</script></svg>"),
            ("onload", r#"<svg onload="alert(1)"></svg>"#),
            (
                "onload tab",
                "<svg xmlns=\"http://www.w3.org/2000/svg\" onload\t=\"alert(1)\"></svg>",
            ),
            ("onload newline", "<svg onload\n=\"alert(1)\"></svg>"),
            ("onload double space", r#"<svg onload  ="alert(1)"></svg>"#),
            (
                "onbegin",
                r#"<svg><set attributeName="x" onbegin="alert(1)"/></svg>"#,
            ),
            (
                "animate",
                r#"<svg><animate attributeName="href" values="javascript:alert(1)"/></svg>"#,
            ),
            (
                "javascript href",
                r#"<svg><a href="javascript:void(0)"></a></svg>"#,
            ),
            (
                "entity javascript",
                r#"<svg><a href="&#106;avascript:alert(1)"></a></svg>"#,
            ),
            (
                "xlink javascript",
                r#"<svg xmlns:xlink="http://www.w3.org/1999/xlink"><a xlink:href="javascript:alert(1)"></a></svg>"#,
            ),
            (
                "data html",
                r#"<svg><image href="data:text/html,<h1>hi</h1>"/></svg>"#,
            ),
            (
                "foreignObject",
                "<svg><foreignObject></foreignObject></svg>",
            ),
            ("iframe", "<svg><iframe src=\"x\"></iframe></svg>"),
            ("embed", "<svg><embed src=\"x\"/></svg>"),
            (
                "external use",
                r#"<svg><use href="https://evil.example/x.svg#a"/></svg>"#,
            ),
            (
                "stylesheet pi",
                r#"<?xml-stylesheet href="http://evil/x.css"?><svg></svg>"#,
            ),
            (
                "entity decl",
                r#"<!DOCTYPE svg [<!ENTITY x "y">]><svg>&x;</svg>"#,
            ),
            ("malformed", r#"<svg onload="alert(1)""#),
            ("unclosed", "<svg><g>"),
            ("html root", "<html><svg></svg></html>"),
            (
                "style import",
                "<svg><style>@import url(http://evil/x.css);</style></svg>",
            ),
            (
                "style external url",
                "<svg><rect style=\"fill:url(http://evil/x)\"/></svg>",
            ),
        ];
        for (name, payload) in bad {
            assert!(
                validate_svg(payload.as_bytes()).is_err(),
                "{name} should be rejected"
            );
        }
        let good = r##"<?xml version="1.0"?><!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd"><svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"><style>.a{fill:red}</style><defs><linearGradient id="g"><stop offset="0"/></linearGradient></defs><rect class="a" width="10" height="10" fill="url(#g)"/><use xlink:href="#g"/><circle r="2"/></svg>"##;
        assert_eq!(validate_svg(good.as_bytes()), Ok(()));
        assert_eq!(validate_svg(b"<svg><circle/></svg>"), Ok(()));
    }

    #[test]
    fn sniffs_like_go() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\nrest"), "image/png");
        assert_eq!(sniff(b"<svg></svg>"), "text/plain; charset=utf-8");
        assert_eq!(
            sniff(b"<?xml version=\"1.0\"?><svg/>"),
            "text/xml; charset=utf-8"
        );
        assert_eq!(sniff(b"<html><body>"), "text/html; charset=utf-8");
        assert_eq!(sniff(b"\x00\x01\x02"), "application/octet-stream");
        assert!(looks_like_svg(b"  <svg/>"));
        assert!(!looks_like_svg(b"<html></html>"));
    }

    #[test]
    fn reads_dimensions() {
        let mut png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
        png.extend_from_slice(&3u32.to_be_bytes());
        png.extend_from_slice(&2u32.to_be_bytes());
        assert_eq!(image_dimensions(&png, "image/png"), Ok((3, 2)));
        let gif = b"GIF89a\x05\x00\x07\x00";
        assert_eq!(image_dimensions(gif, "image/gif"), Ok((5, 7)));
        let jpeg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00,
            0x20, 0x00, 0x40,
        ];
        assert_eq!(image_dimensions(&jpeg, "image/jpeg"), Ok((64, 32)));
        assert!(image_dimensions(b"BM....", "image/bmp").is_err());
    }
}
