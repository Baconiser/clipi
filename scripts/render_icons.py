#!/usr/bin/env python3
"""Render clipi app and tray icons from the UI palette."""

from pathlib import Path

from PIL import Image, ImageDraw

# Matches src/app.rs
PAPER = (0x1C, 0x19, 0x16, 255)
INK = (0xE8, 0xE0, 0xD4, 255)
MUTE = (0x8A, 0x80, 0x74, 255)
RULE = (0x3A, 0x34, 0x2C, 255)
MARK = (0xC4, 0x5C, 0x26, 255)
CLIP_LIT = (0xD9, 0x78, 0x3A, 255)
CLIP_SLOT = (0x8A, 0x3E, 0x18, 255)
STACK_MID = (0xC4, 0xB8, 0xA8, 255)
STACK_BACK = (0x9A, 0x8C, 0x7A, 255)
BLACK = (0, 0, 0, 255)
CLEAR = (0, 0, 0, 0)

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "assets"
SCALE = 4


def box(s, x, y, w, h):
    return [s * x, s * y, s * (x + w), s * (y + h)]


def draw_round(draw, rect, radius, fill, outline=None, width=1):
    r = max(1, int(radius))
    kwargs = {"fill": fill}
    if outline is not None:
        kwargs["outline"] = outline
        kwargs["width"] = width
    draw.rounded_rectangle(rect, radius=r, **kwargs)


def layout(compact):
    """Paper and clamp in normalized coordinates."""
    if compact:
        px, py, pw, ph = 0.22, 0.20, 0.56, 0.62
    else:
        px, py, pw, ph = 0.28, 0.30, 0.44, 0.50
    cx = px + pw * 0.15
    cw = pw * 0.70
    ch = 0.11 if compact else 0.10
    cy = py - ch * 0.38
    return px, py, pw, ph, cx, cy, cw, ch


def draw_glyph(draw, s, colors, compact):
    px, py, pw, ph, cx, cy, cw, ch = layout(compact)
    pr = s * (0.05 if compact else 0.04)
    mark_w = 0.07 if compact else 0.048
    shift = 0.028

    if not compact:
        draw_round(
            draw,
            box(s, px + shift * 2, py + shift * 2, pw, ph),
            pr,
            colors["stack_back"],
        )
        draw_round(
            draw,
            box(s, px + shift, py + shift, pw, ph),
            pr,
            colors["stack_mid"],
        )
    draw_round(draw, box(s, px, py, pw, ph), pr, colors["paper"])

    mx0, my0, mx1, my1 = box(s, px, py, mark_w, ph)
    inset = pr * 0.18
    draw.rectangle([mx0, my0 + inset, mx1, my1 - inset], fill=colors["mark"])

    if not compact:
        lx0 = s * (px + mark_w + 0.07)
        lx1 = s * (px + pw - 0.08)
        rows = (
            (0.22, 0.026, colors["mark"], 1.00),
            (0.36, 0.016, colors["mute"], 0.80),
            (0.48, 0.016, colors["mute"], 0.58),
        )
        for yt, thick, color, length in rows:
            y = s * (py + yt)
            h = max(2, s * thick)
            draw_round(
                draw,
                [lx0, y, lx0 + (lx1 - lx0) * length, y + h],
                h * 0.45,
                color,
            )

    cr = s * (0.04 if compact else 0.028)
    draw_round(draw, box(s, cx, cy, cw, ch), cr, colors["clip"])

    hole = colors["hole"]
    if hole[3] > 0:
        slot_h = ch * 0.26
        slot_y = max(py + 0.01, cy + ch * 0.42)
        slot_x = cx + cw * 0.18
        slot_w = cw * 0.64
        draw_round(
            draw,
            box(s, slot_x, slot_y, slot_w, slot_h),
            s * 0.01,
            hole,
        )

    if colors["clip_lit"] != colors["clip"]:
        ridge_h = ch * 0.14
        draw_round(
            draw,
            box(s, cx + cw * 0.14, cy + ch * 0.14, cw * 0.72, ridge_h),
            s * 0.008,
            colors["clip_lit"],
        )


def render(size, kind):
    src = size * SCALE
    img = Image.new("RGBA", (src, src), CLEAR)
    draw = ImageDraw.Draw(img)

    if kind == "app":
        pad = src * 0.03
        draw_round(
            draw,
            [pad, pad, src - 1 - pad, src - 1 - pad],
            src * 0.18,
            PAPER,
            outline=RULE,
            width=max(2, src // 180),
        )
        colors = {
            "paper": INK,
            "mark": MARK,
            "mute": MUTE,
            "clip": MARK,
            "clip_lit": CLIP_LIT,
            "stack_mid": STACK_MID,
            "stack_back": STACK_BACK,
            "hole": CLIP_SLOT,
        }
        draw_glyph(draw, src, colors, compact=False)
    elif kind == "tray":
        colors = {
            "paper": INK,
            "mark": MARK,
            "mute": MUTE,
            "clip": MARK,
            "clip_lit": CLIP_LIT,
            "stack_mid": STACK_MID,
            "stack_back": STACK_BACK,
            "hole": CLIP_SLOT,
        }
        draw_glyph(draw, src, colors, compact=True)
    elif kind == "template":
        colors = {
            "paper": BLACK,
            "mark": BLACK,
            "mute": BLACK,
            "clip": BLACK,
            "clip_lit": BLACK,
            "stack_mid": BLACK,
            "stack_back": BLACK,
            "hole": CLEAR,
        }
        draw_glyph(draw, src, colors, compact=True)
    else:
        raise ValueError(kind)

    return img.resize((size, size), Image.Resampling.LANCZOS)


def main():
    ASSETS.mkdir(parents=True, exist_ok=True)
    render(512, "app").save(ASSETS / "icon.png", optimize=True)
    render(64, "tray").save(ASSETS / "tray.png", optimize=True)
    render(64, "template").save(ASSETS / "tray-template.png", optimize=True)
    print(f"wrote icons in {ASSETS}")


if __name__ == "__main__":
    main()
