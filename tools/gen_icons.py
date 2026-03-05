#!/usr/bin/env python3
"""Generate a few Superview icon candidates as PNG.

No external dependencies besides Pillow.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


OUT_DIR = Path(__file__).resolve().parents[1] / "assets" / "icons"


@dataclass(frozen=True)
class Palette:
    bg: tuple[int, int, int, int]
    panel: tuple[int, int, int, int]
    white: tuple[int, int, int, int]
    accent: tuple[int, int, int, int]


PALETTE = Palette(
    bg=(15, 18, 25, 255),
    panel=(26, 31, 44, 255),
    white=(242, 244, 248, 255),
    accent=(56, 189, 248, 255),
)


def rounded_rect(draw: ImageDraw.ImageDraw, xy, radius: int, fill):
    x0, y0, x1, y1 = xy
    draw.rounded_rectangle((x0, y0, x1, y1), radius=radius, fill=fill)


def icon_frame(size: int) -> Image.Image:
    img = Image.new("RGBA", (size, size), PALETTE.bg)
    d = ImageDraw.Draw(img)

    pad = int(size * 0.08)
    rounded_rect(d, (pad, pad, size - pad, size - pad), radius=int(size * 0.14), fill=PALETTE.panel)

    # Wide trapezoid (superview frame)
    cx = size // 2
    top_y = int(size * 0.33)
    bot_y = int(size * 0.70)
    top_w = int(size * 0.48)
    bot_w = int(size * 0.72)

    trap = [
        (cx - top_w // 2, top_y),
        (cx + top_w // 2, top_y),
        (cx + bot_w // 2, bot_y),
        (cx - bot_w // 2, bot_y),
    ]
    d.polygon(trap, outline=PALETTE.white, width=max(2, size // 64))

    # Accent horizon line with slight "stretch" hints
    line_y = int(size * 0.55)
    left = cx - int(bot_w * 0.42)
    right = cx + int(bot_w * 0.42)
    d.line((left, line_y, right, line_y), fill=PALETTE.accent, width=max(3, size // 48))

    # Curved-ish hints using short angled segments (keeps it simple)
    seg = int(size * 0.07)
    for offset in (-1, 1):
        x = cx + offset * int(size * 0.22)
        d.line((x - offset * seg, line_y - seg, x + offset * seg, line_y + seg), fill=PALETTE.accent, width=max(3, size // 64))

    return img


def icon_lens(size: int) -> Image.Image:
    img = Image.new("RGBA", (size, size), PALETTE.bg)
    d = ImageDraw.Draw(img)

    pad = int(size * 0.08)
    rounded_rect(d, (pad, pad, size - pad, size - pad), radius=int(size * 0.14), fill=PALETTE.panel)

    # Lens circle
    cx = cy = size // 2
    r = int(size * 0.30)
    stroke = max(3, size // 56)
    d.ellipse((cx - r, cy - r, cx + r, cy + r), outline=PALETTE.white, width=stroke)

    # Inner ring
    r2 = int(r * 0.70)
    d.ellipse((cx - r2, cy - r2, cx + r2, cy + r2), outline=PALETTE.accent, width=max(3, stroke - 1))

    # Horizon line across the lens
    y = cy + int(r * 0.15)
    d.line((cx - int(r * 0.95), y, cx + int(r * 0.95), y), fill=PALETTE.white, width=max(3, size // 72))

    # Subtle superview "spread" marks
    mark = int(r * 0.75)
    d.line((cx - mark, y - int(r * 0.35), cx - int(r * 0.35), y + int(r * 0.35)), fill=PALETTE.accent, width=max(3, size // 80))
    d.line((cx + mark, y - int(r * 0.35), cx + int(r * 0.35), y + int(r * 0.35)), fill=PALETTE.accent, width=max(3, size // 80))

    return img


def _load_font(size: int) -> ImageFont.FreeTypeFont | ImageFont.ImageFont:
    candidates = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ]
    for path in candidates:
        try:
            return ImageFont.truetype(path, size=size)
        except OSError:
            continue
    return ImageFont.load_default()


def icon_monogram(size: int) -> Image.Image:
    img = Image.new("RGBA", (size, size), PALETTE.bg)
    d = ImageDraw.Draw(img)

    pad = int(size * 0.08)
    rounded_rect(d, (pad, pad, size - pad, size - pad), radius=int(size * 0.14), fill=PALETTE.panel)

    font = _load_font(int(size * 0.34))
    text = "SV"

    bbox = d.textbbox((0, 0), text, font=font)
    tw = bbox[2] - bbox[0]
    th = bbox[3] - bbox[1]

    x = (size - tw) // 2
    y = int(size * 0.34)

    # Accent shadow to give a tiny bit of depth
    d.text((x + int(size * 0.01), y + int(size * 0.01)), text, font=font, fill=PALETTE.accent)
    d.text((x, y), text, font=font, fill=PALETTE.white)

    # Small wide frame underline
    cx = size // 2
    line_y = int(size * 0.72)
    top_w = int(size * 0.44)
    bot_w = int(size * 0.66)
    top_y = line_y - int(size * 0.05)
    bot_y = line_y + int(size * 0.07)
    trap = [
        (cx - top_w // 2, top_y),
        (cx + top_w // 2, top_y),
        (cx + bot_w // 2, bot_y),
        (cx - bot_w // 2, bot_y),
    ]
    d.polygon(trap, outline=PALETTE.accent, width=max(3, size // 96))

    return img


def save_variants(name: str, img: Image.Image):
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    # Save a big master, plus a few common sizes.
    for s in (1024, 512, 256, 128):
        resized = img.resize((s, s), resample=Image.Resampling.LANCZOS) if img.size != (s, s) else img
        (OUT_DIR / f"{name}-{s}.png").write_bytes(_to_png_bytes(resized))


def _to_png_bytes(img: Image.Image) -> bytes:
    import io

    buf = io.BytesIO()
    img.save(buf, format="PNG", optimize=True)
    return buf.getvalue()


def main() -> int:
    OUT_DIR.mkdir(parents=True, exist_ok=True)

    save_variants("icon-frame", icon_frame(1024))
    save_variants("icon-lens", icon_lens(1024))
    save_variants("icon-monogram", icon_monogram(1024))

    print(f"Wrote icons to: {OUT_DIR}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
