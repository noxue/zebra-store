#!/usr/bin/env python3
"""Generates original anime-style product cover images (no third-party logos).

Usage: scripts/gen_covers.py [out_dir]   (default: scripts/covers)
Each cover is 1200x900 PNG: pastel gradient, bokeh, sparkles, a simple
geometric mascot icon, the product title and a tagline.
"""
import math
import os
import random
import sys

from PIL import Image, ImageDraw, ImageFilter, ImageFont

OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), "covers")
W, H = 1200, 900
FONT_BOLD = "/System/Library/Fonts/Hiragino Sans GB.ttc"

# slug, title, tagline, gradient top, gradient bottom, icon kind
COVERS = [
    ("chatgpt-plus", "ChatGPT Plus", "成品账号 · 即买即用", "#a78bfa", "#34d399", "bot"),
    ("chatgpt-team", "ChatGPT Team", "团队席位 · 稳定独享", "#60a5fa", "#a78bfa", "bot"),
    ("claude-pro", "Claude Pro", "成品账号 · 长文神器", "#fb923c", "#f472b6", "spark"),
    ("claude-max", "Claude Max", "高额度 · 重度用户", "#f472b6", "#8b5cf6", "spark"),
    ("aws-account", "AWS 账号", "云服务 · 多规格可选", "#fbbf24", "#f97316", "cloud"),
    ("gcp-account", "GCP 账号", "云服务 · 试用额度", "#38bdf8", "#22c55e", "cloud"),
    ("google-account", "Google 账号", "邮箱账号 · 新号 / 老号", "#f87171", "#60a5fa", "key"),
]


def hex_rgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def gradient(top, bottom):
    img = Image.new("RGB", (W, H))
    t, b = hex_rgb(top), hex_rgb(bottom)
    px = img.load()
    for y in range(H):
        for x in range(W):
            k = min(1.0, (y / H) * 0.75 + (x / W) * 0.25)
            px[x, y] = tuple(int(t[i] + (b[i] - t[i]) * k) for i in range(3))
    return img.convert("RGBA")


def bokeh(img, rng):
    layer = Image.new("RGBA", img.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    for _ in range(18):
        r = rng.randint(30, 140)
        x, y = rng.randint(-50, W + 50), rng.randint(-50, H + 50)
        d.ellipse((x - r, y - r, x + r, y + r), fill=(255, 255, 255, rng.randint(18, 50)))
    return Image.alpha_composite(img, layer.filter(ImageFilter.GaussianBlur(12)))


def star(d, cx, cy, r, fill):
    pts = []
    for i in range(8):
        rad = r if i % 2 == 0 else r * 0.28
        a = math.pi / 4 * i - math.pi / 2
        pts.append((cx + rad * math.cos(a), cy + rad * math.sin(a)))
    d.polygon(pts, fill=fill)


def sparkles(img, rng):
    d = ImageDraw.Draw(img)
    for _ in range(26):
        star(d, rng.randint(0, W), rng.randint(0, H), rng.randint(6, 22), (255, 255, 255, rng.randint(150, 240)))


def icon(img, kind, accent):
    """Draws a round white badge with a simple geometric mascot."""
    cx, cy, R = 300, 450, 190
    shadow = Image.new("RGBA", img.size, (0, 0, 0, 0))
    ImageDraw.Draw(shadow).ellipse((cx - R + 10, cy - R + 24, cx + R + 10, cy + R + 24), fill=(40, 20, 80, 70))
    img.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(18)))
    d = ImageDraw.Draw(img)
    d.ellipse((cx - R, cy - R, cx + R, cy + R), fill=(255, 255, 255, 245))
    a = hex_rgb(accent) + (255,)
    if kind == "bot":
        d.rounded_rectangle((cx - 110, cy - 80, cx + 110, cy + 90), 50, fill=a)
        d.line((cx, cy - 80, cx, cy - 130), fill=a, width=12)
        d.ellipse((cx - 18, cy - 150, cx + 18, cy - 114), fill=a)
        for ex in (-50, 50):  # sparkling anime eyes
            d.ellipse((cx + ex - 26, cy - 30, cx + ex + 26, cy + 30), fill="white")
            d.ellipse((cx + ex - 14, cy - 14, cx + ex + 14, cy + 22), fill=(40, 30, 70, 255))
            d.ellipse((cx + ex - 6, cy - 10, cx + ex + 4, cy), fill="white")
        d.arc((cx - 30, cy + 25, cx + 30, cy + 65), 20, 160, fill="white", width=8)
    elif kind == "spark":
        star(d, cx, cy, 130, a)
        star(d, cx + 95, cy - 95, 34, a)
        star(d, cx - 100, cy + 90, 24, a)
        d.ellipse((cx - 22, cy - 22, cx + 22, cy + 22), fill="white")
    elif kind == "cloud":
        for (x, y, r) in ((-70, 20, 70), (10, -30, 90), (85, 25, 65)):
            d.ellipse((cx + x - r, cy + y - r, cx + x + r, cy + y + r), fill=a)
        d.rounded_rectangle((cx - 140, cy + 20, cx + 150, cy + 90), 35, fill=a)
        for ex in (-35, 35):
            d.ellipse((cx + ex - 10, cy + 10, cx + ex + 10, cy + 32), fill="white")
        d.arc((cx - 18, cy + 30, cx + 18, cy + 60), 20, 160, fill="white", width=6)
    else:  # key
        d.ellipse((cx - 120, cy - 70, cx + 20, cy + 70), outline=a, width=34)
        d.rounded_rectangle((cx + 10, cy - 17, cx + 140, cy + 17), 10, fill=a)
        d.rectangle((cx + 90, cy + 10, cx + 110, cy + 60), fill=a)
        d.rectangle((cx + 120, cy + 10, cx + 140, cy + 45), fill=a)


def text(img, title, tagline):
    d = ImageDraw.Draw(img)
    x = 560
    size = 110
    ft = ImageFont.truetype(FONT_BOLD, size, index=1)
    while d.textlength(title, font=ft) > W - x - 50:  # shrink long titles to fit
        size -= 4
        ft = ImageFont.truetype(FONT_BOLD, size, index=1)
    fs = ImageFont.truetype(FONT_BOLD, 46, index=0)
    for off in ((4, 6),):  # soft drop shadow
        d.text((x + off[0], 330 + off[1]), title, font=ft, fill=(50, 20, 90, 90))
    d.text((x, 330), title, font=ft, fill="white", stroke_width=3, stroke_fill=(90, 50, 150, 255))
    tw = d.textlength(tagline, font=fs)
    d.rounded_rectangle((x - 6, 490, x + tw + 42, 566), 38, fill=(255, 255, 255, 225))
    d.text((x + 18, 500), tagline, font=fs, fill=(90, 50, 150, 255))
    fb = ImageFont.truetype(FONT_BOLD, 30, index=0)
    d.text((x, 610), "★ Zebra Store · 测试商品", font=fb, fill=(255, 255, 255, 230))


def main():
    os.makedirs(OUT, exist_ok=True)
    for slug, title, tagline, top, bottom, kind in COVERS:
        rng = random.Random(slug)  # deterministic per product
        img = bokeh(gradient(top, bottom), rng)
        sparkles(img, rng)
        icon(img, kind, bottom)
        text(img, title, tagline)
        path = os.path.join(OUT, f"{slug}.png")
        img.convert("RGB").save(path, optimize=True)
        print(path)


if __name__ == "__main__":
    main()
