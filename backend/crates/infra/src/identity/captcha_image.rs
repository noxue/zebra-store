//! PNG image captcha renderer (digits drawn from a 5×7 bitmap font with noise).

use base64::Engine;
use rand::Rng;
use zs_domain::identity::captcha::{ImageChallenge, ImageRenderer, ImageSetting};
use zs_domain::{Error, Result};

/// Glyph width of the bitmap font.
const GLYPH_W: u32 = 5;
/// Glyph height of the bitmap font.
const GLYPH_H: u32 = 7;
/// Largest rendered image side, to bound memory use.
const MAX_SIDE: u32 = 800;
/// Noise dots drawn per configured `noise_count` unit.
const DOTS_PER_NOISE: u32 = 40;

/// 5×7 glyphs of the digits 0–9 (bit 4 = leftmost column).
const FONT: [[u8; 7]; 10] = [
    [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
    [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
    [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
    [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
    [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
    [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
    [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
    [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
    [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
    [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
];

/// Renders numeric captchas as PNG data URIs.
#[derive(Debug, Clone, Copy, Default)]
pub struct PngCaptchaRenderer;

struct Canvas {
    w: u32,
    h: u32,
    px: Vec<u8>,
}

impl Canvas {
    fn new(w: u32, h: u32, bg: [u8; 3]) -> Self {
        let mut px = Vec::with_capacity((w * h * 3) as usize);
        for _ in 0..w * h {
            px.extend_from_slice(&bg);
        }
        Self { w, h, px }
    }

    fn set(&mut self, x: i64, y: i64, c: [u8; 3]) {
        if x < 0 || y < 0 || x >= i64::from(self.w) || y >= i64::from(self.h) {
            return;
        }
        let i = ((y as u64 * u64::from(self.w) + x as u64) * 3) as usize;
        self.px[i..i + 3].copy_from_slice(&c);
    }

    fn line(&mut self, (x0, y0): (i64, i64), (x1, y1): (i64, i64), c: [u8; 3]) {
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let (mut x, mut y, mut err) = (x0, y0, dx + dy);
        loop {
            self.set(x, y, c);
            self.set(x, y + 1, c);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    fn png(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, self.w, self.h);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc.write_header().map_err(Error::internal)?;
            writer.write_image_data(&self.px).map_err(Error::internal)?;
        }
        Ok(out)
    }
}

fn dark(rng: &mut impl Rng) -> [u8; 3] {
    [
        rng.random_range(20..110),
        rng.random_range(20..110),
        rng.random_range(20..110),
    ]
}

impl ImageRenderer for PngCaptchaRenderer {
    fn render(&self, s: &ImageSetting) -> Result<ImageChallenge> {
        let mut rng = rand::rng();
        let w = u32::try_from(s.width).unwrap_or(240).clamp(100, MAX_SIDE);
        let h = u32::try_from(s.height).unwrap_or(80).clamp(40, MAX_SIDE);
        let len = u32::try_from(s.length).unwrap_or(5).clamp(1, 8);
        let answer: String = (0..len)
            .map(|_| char::from(b'0' + rng.random_range(0..10u8)))
            .collect();
        let bg = [
            rng.random_range(225..=255),
            rng.random_range(225..=255),
            rng.random_range(225..=255),
        ];
        let mut canvas = Canvas::new(w, h, bg);
        let noise = u32::try_from(s.noise_count).unwrap_or(0).min(50);
        for _ in 0..noise * DOTS_PER_NOISE {
            let c = dark(&mut rng);
            let (x, y) = (rng.random_range(0..w), rng.random_range(0..h));
            canvas.set(i64::from(x), i64::from(y), c);
        }
        let cell = w / len;
        let scale = ((h * 6 / 10) / GLYPH_H).min(cell * 7 / 10 / GLYPH_W).max(1);
        for (i, ch) in answer.bytes().enumerate() {
            let glyph = FONT[usize::from(ch - b'0')];
            let color = dark(&mut rng);
            let gw = GLYPH_W * scale;
            let gh = GLYPH_H * scale;
            let x0 = i as u32 * cell + (cell.saturating_sub(gw)) / 2;
            let y_room = h.saturating_sub(gh);
            let y0 = if y_room > 0 {
                rng.random_range(0..=y_room)
            } else {
                0
            };
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..GLYPH_W {
                    if bits & (0x10 >> col) == 0 {
                        continue;
                    }
                    for dy in 0..scale {
                        for dx in 0..scale {
                            canvas.set(
                                i64::from(x0 + col * scale + dx),
                                i64::from(y0 + row as u32 * scale + dy),
                                color,
                            );
                        }
                    }
                }
            }
        }
        let lines = u32::try_from(s.show_line).unwrap_or(0).min(10);
        for _ in 0..lines {
            let c = dark(&mut rng);
            let a = (0, i64::from(rng.random_range(0..h)));
            let b = (i64::from(w) - 1, i64::from(rng.random_range(0..h)));
            canvas.line(a, b, c);
        }
        let png = canvas.png()?;
        Ok(ImageChallenge {
            answer,
            image_base64: format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png)
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_png_data_uri() {
        let setting = ImageSetting {
            length: 5,
            width: 240,
            height: 80,
            noise_count: 2,
            show_line: 2,
            expire_seconds: 300,
            max_store: 10240,
        };
        let c = PngCaptchaRenderer.render(&setting).unwrap();
        assert_eq!(c.answer.len(), 5);
        assert!(c.answer.chars().all(|ch| ch.is_ascii_digit()));
        let b64 = c
            .image_base64
            .strip_prefix("data:image/png;base64,")
            .unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    }
}
