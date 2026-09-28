"""Distinct logos for the reseller subsites (same spirit as backend/scripts/gen_covers.py)."""
import io
import math
import random

from PIL import Image, ImageDraw, ImageFilter, ImageFont

SIZE = 256


def _gradient(size, top, bottom):
    img = Image.new("RGB", (size, size), top)
    draw = ImageDraw.Draw(img)
    for y in range(size):
        t = y / (size - 1)
        color = tuple(round(a + (b - a) * t) for a, b in zip(top, bottom))
        draw.line([(0, y), (size, y)], fill=color)
    return img


def _hex(color):
    color = color.lstrip("#")
    return tuple(int(color[i : i + 2], 16) for i in (0, 2, 4))


def logo_png(letter, primary, secondary, motif, seed=7):
    """Rounded-square badge: gradient, motif shapes, big initial. Returns PNG bytes."""
    rnd = random.Random(seed)
    base = _gradient(SIZE, _hex(primary), _hex(secondary)).convert("RGBA")
    layer = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    draw = ImageDraw.Draw(layer)
    if motif == "petals":  # sakura: five-petal flowers
        for _ in range(7):
            cx, cy, r = rnd.randint(10, 246), rnd.randint(10, 246), rnd.randint(10, 26)
            for k in range(5):
                a = k * 2 * math.pi / 5
                px, py = cx + r * math.cos(a), cy + r * math.sin(a)
                draw.ellipse([px - r * 0.6, py - r * 0.6, px + r * 0.6, py + r * 0.6], fill=(255, 255, 255, 70))
    elif motif == "grid":  # neon: synthwave grid + sun
        draw.ellipse([58, 30, 198, 170], fill=(255, 230, 120, 90))
        for x in range(-256, 512, 32):
            draw.line([(128, 150), (x, 256)], fill=(255, 255, 255, 80), width=2)
        for y in (170, 190, 215, 245):
            draw.line([(0, y), (256, y)], fill=(255, 255, 255, 80), width=2)
    else:  # matcha: leaves / bubbles
        for _ in range(9):
            cx, cy, r = rnd.randint(0, 256), rnd.randint(0, 256), rnd.randint(8, 30)
            draw.ellipse([cx - r, cy - r * 0.55, cx + r, cy + r * 0.55], fill=(255, 255, 255, 60))
    layer = layer.filter(ImageFilter.GaussianBlur(1.2))
    img = Image.alpha_composite(base, layer)

    draw = ImageDraw.Draw(img)
    font = ImageFont.load_default(size=150)
    box = draw.textbbox((0, 0), letter, font=font)
    w, h = box[2] - box[0], box[3] - box[1]
    pos = ((SIZE - w) / 2 - box[0], (SIZE - h) / 2 - box[1])
    shadow = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).text((pos[0] + 5, pos[1] + 6), letter, font=font, fill=(0, 0, 0, 110))
    img = Image.alpha_composite(img, shadow.filter(ImageFilter.GaussianBlur(4)))
    ImageDraw.Draw(img).text(pos, letter, font=font, fill=(255, 255, 255, 255))

    mask = Image.new("L", (SIZE, SIZE), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, SIZE - 1, SIZE - 1], radius=56, fill=255)
    out = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    out.paste(img, (0, 0), mask)
    buf = io.BytesIO()
    out.save(buf, "PNG")
    return buf.getvalue()


if __name__ == "__main__":
    import sys

    for name, args in {
        "sakura": ("S", "#ff8fb8", "#ffc6dc", "petals"),
        "neon": ("N", "#7c3aed", "#06b6d4", "grid"),
        "matcha": ("M", "#4d7c0f", "#a3c585", "leaves"),
    }.items():
        with open(f"{sys.argv[1] if len(sys.argv) > 1 else '.'}/{name}.png", "wb") as f:
            f.write(logo_png(*args))
