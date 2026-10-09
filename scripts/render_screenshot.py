#!/usr/bin/env python3
"""Rasterize a terminal screen captured with `tmux capture-pane -p -e -N` into a PNG.

    tmux capture-pane -t SESSION -p -e -N > screen.ans
    python3 scripts/render_screenshot.py screen.ans out.png [--title TEXT]

Each terminal cell is drawn on a fixed grid, the way a terminal does it: text with JetBrains Mono
(falling back to DejaVu Sans Mono / Noto Sans Mono for missing glyphs), and box-drawing lines as
real strokes so vertical and horizontal guides connect. Needs Pillow.
"""
import argparse
import os
import re
import unicodedata

from PIL import Image, ImageDraw, ImageFont

SCALE = 2
FONT_PX = 14 * SCALE
LINE_H = round(FONT_PX * 1.28)
PAD = 18 * SCALE
BAR_H = 34 * SCALE
RADIUS = 10 * SCALE

FONT_DIRS = [os.path.expanduser("~/.local/share/fonts"), os.path.expanduser("~/.fonts"), "/usr/share/fonts", "/usr/local/share/fonts"]
FAMILY = {
    "regular": ["JetBrainsMono-Regular.ttf", "DejaVuSansMono.ttf", "NotoSansMono-Regular.ttf"],
    "bold": ["JetBrainsMono-Bold.ttf", "DejaVuSansMono-Bold.ttf", "NotoSansMono-Bold.ttf"],
}
DEFAULT_FG, DEFAULT_BG = (205, 214, 244), (30, 30, 46)  # Catppuccin Mocha text / base
FRAME_BG, BAR_FG = (17, 17, 27), (127, 132, 156)
ANSI16 = [(69, 71, 90), (243, 139, 168), (166, 227, 161), (249, 226, 175), (137, 180, 250), (245, 194, 231), (148, 226, 213), (186, 194, 222),
          (88, 91, 112), (243, 139, 168), (166, 227, 161), (249, 226, 175), (137, 180, 250), (245, 194, 231), (148, 226, 213), (166, 173, 200)]


def find_font(name):
    for base in FONT_DIRS:
        for root, _, files in os.walk(base):
            if name in files:
                return os.path.join(root, name)
    return None


def load_fonts(kind):
    fonts = [ImageFont.truetype(p, FONT_PX) for p in (find_font(n) for n in FAMILY[kind]) if p]
    if not fonts:
        raise SystemExit(f"no monospace font found for {kind}")
    return fonts


def has_glyph(font, ch, cache={}):
    """A font lacks `ch` when it renders the same bitmap as an unassigned code point (.notdef)."""
    def bitmap(c):
        im = Image.new("L", (FONT_PX * 2, FONT_PX * 2), 0)
        ImageDraw.Draw(im).text((0, 0), c, font=font, fill=255)
        return im.tobytes()

    key = (id(font), ch)
    if key not in cache:
        nd = (id(font), None)
        if nd not in cache:
            cache[nd] = bitmap("\U0010fffd")
        cache[key] = bitmap(ch) != cache[nd]
    return cache[key]


def xterm256(n):
    if n < 16:
        return ANSI16[n]
    if n < 232:
        n -= 16
        return tuple(0 if v == 0 else 55 + 40 * v for v in (n // 36, n // 6 % 6, n % 6))
    g = 8 + 10 * (n - 232)
    return (g, g, g)


def parse(text):
    """→ rows of cells (char, fg, bg, bold); wide characters take two cells."""
    rows = []
    sgr = re.compile(r"\x1b\[([0-9;]*)m")
    for line in text.split("\n"):
        fg, bg, bold, rev = None, None, False, False
        cells, i = [], 0
        while i < len(line):
            m = sgr.match(line, i)
            if m:
                codes = [int(c) if c else 0 for c in m.group(1).split(";")]
                j = 0
                while j < len(codes):
                    c = codes[j]
                    if c == 0:
                        fg, bg, bold, rev = None, None, False, False
                    elif c == 1:
                        bold = True
                    elif c == 22:
                        bold = False
                    elif c == 7:
                        rev = True
                    elif c == 27:
                        rev = False
                    elif c in (38, 48) and j + 1 < len(codes):
                        if codes[j + 1] == 2 and j + 4 < len(codes):
                            col = tuple(codes[j + 2:j + 5])
                            j += 4
                        elif codes[j + 1] == 5 and j + 2 < len(codes):
                            col = xterm256(codes[j + 2])
                            j += 2
                        else:
                            col = None
                        if c == 38:
                            fg = col
                        else:
                            bg = col
                    elif c == 39:
                        fg = None
                    elif c == 49:
                        bg = None
                    elif 30 <= c <= 37 or 90 <= c <= 97:
                        fg = ANSI16[c - 30 if c < 90 else c - 82]
                    elif 40 <= c <= 47 or 100 <= c <= 107:
                        bg = ANSI16[c - 40 if c < 100 else c - 92]
                    j += 1
                i = m.end()
                continue
            ch = line[i]
            i += 1
            if ch == "\x1b":
                continue
            f, b = fg or DEFAULT_FG, bg or DEFAULT_BG
            if rev:
                f, b = b, f
            wide = unicodedata.east_asian_width(ch) in "WF"
            cells.append((ch, f, b, bold))
            if wide:
                cells.append(("", f, b, bold))
        rows.append(cells)
    while rows and not rows[-1]:
        rows.pop()
    return rows


# box drawing: which arms each glyph has (up, down, left, right) and whether it is heavy / dashed
BOX = {
    "─": "lr", "│": "ud", "├": "udr", "┤": "udl", "└": "ur", "┘": "ul", "┌": "dr", "┐": "dl", "┬": "dlr", "┴": "ulr", "┼": "udlr",
    "╭": "dr", "╮": "dl", "╰": "ur", "╯": "ul", "┄": "lr", "┆": "ud", "━": "lr", "┃": "ud",
}
DASHED, HEAVY = set("┄┆"), set("━┃")


def draw_box(d, ch, x, y, cw, ch_h, color):
    arms = BOX[ch]
    w = (2 if ch in HEAVY else 1) * SCALE
    cx, cy = x + cw // 2, y + ch_h // 2
    segs = []
    if "l" in arms:
        segs.append(((x, cy), (cx, cy)))
    if "r" in arms:
        segs.append(((cx, cy), (x + cw, cy)))
    if "u" in arms:
        segs.append(((cx, y), (cx, cy)))
    if "d" in arms:
        segs.append(((cx, cy), (cx, y + ch_h)))
    for (x0, y0), (x1, y1) in segs:
        if ch in DASHED:  # three dashes per cell, like the glyph
            n, horiz = 3, y0 == y1
            length = (x1 - x0) if horiz else (y1 - y0)
            step = max(1, (cw if horiz else ch_h) // n)
            pos = 0
            while pos < length:
                a, b = pos, min(pos + step * 0.6, length)
                if horiz:
                    d.rectangle([x0 + a, cy - w // 2, x0 + b, cy - w // 2 + w - 1], fill=color)
                else:
                    d.rectangle([cx - w // 2, y0 + a, cx - w // 2 + w - 1, y0 + b], fill=color)
                pos += step
        elif y0 == y1:
            d.rectangle([min(x0, x1), cy - w // 2, max(x0, x1), cy - w // 2 + w - 1], fill=color)
        else:
            d.rectangle([cx - w // 2, min(y0, y1), cx - w // 2 + w - 1, max(y0, y1)], fill=color)


def render(rows, out, title):
    regular, bold = load_fonts("regular"), load_fonts("bold")
    cw = round(regular[0].getlength("M"))
    cols = max(len(r) for r in rows)
    width = cols * cw + 2 * PAD
    height = len(rows) * LINE_H + 2 * PAD + BAR_H
    img = Image.new("RGBA", (width, height), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([0, 0, width - 1, height - 1], RADIUS, fill=FRAME_BG)
    # title bar: three dots and a title
    for k, col in enumerate([(243, 139, 168), (249, 226, 175), (166, 227, 161)]):
        r = 6 * SCALE
        cx = PAD + r + k * 20 * SCALE
        d.ellipse([cx - r, BAR_H // 2 - r, cx + r, BAR_H // 2 + r], fill=col)
    if title:
        tf = regular[0]
        tw = tf.getlength(title)
        d.text(((width - tw) / 2, BAR_H / 2), title, font=tf, fill=BAR_FG, anchor="lm")
    # terminal surface
    top = BAR_H
    d.rectangle([PAD // 2, top, width - PAD // 2, height - PAD // 2], fill=DEFAULT_BG)
    oy = top + PAD // 2
    ox = PAD
    for r, cells in enumerate(rows):
        y = oy + r * LINE_H
        for c, (ch, fg, bg, is_bold) in enumerate(cells):
            x = ox + c * cw
            if bg != DEFAULT_BG:
                d.rectangle([x, y, x + cw - 1, y + LINE_H - 1], fill=bg)
            if not ch or ch == " ":
                continue
            if ch in BOX:
                draw_box(d, ch, x, y, cw, LINE_H, fg)
                continue
            chain = bold if is_bold else regular
            font = next((f for f in chain if has_glyph(f, ch)), chain[0])
            gw = font.getlength(ch)
            span = cw * (2 if unicodedata.east_asian_width(ch) in "WF" else 1)
            # center glyphs from fallback fonts in their cell; shrink any that would overflow
            if gw > span * 1.05:
                font = ImageFont.truetype(font.path, int(FONT_PX * span / gw))
                gw = font.getlength(ch)
            d.text((x + (span - gw) / 2, y + LINE_H / 2), ch, font=font, fill=fg, anchor="lm")
    img.save(out)
    print(f"{out}: {width}×{height}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("capture")
    ap.add_argument("out")
    ap.add_argument("--title", default="")
    a = ap.parse_args()
    with open(a.capture, encoding="utf-8") as f:
        render(parse(f.read().rstrip("\n")), a.out, a.title)


if __name__ == "__main__":
    main()
