#!/usr/bin/env python3
"""Freya TDE palette icons: writes <name>.svg and renders <name>.png (48 px).

Run from anywhere: python3 ide/assets/palette/make.py  (needs rsvg-convert)
Style: light bodies, grey outline, blue accent – readable on light and dark.
"""
import os, subprocess

OUT = os.path.dirname(os.path.abspath(__file__))
LINE = "#5e6670"   # outline
BODY = "#ffffff"
SOFT = "#eef1f5"   # button face
BLUE = "#3b82d6"
PALE = "#dbe8f8"
INK = "#2b2f36"
GREY = "#9aa3ad"
FONT = "font-family='DejaVu Sans, Verdana, sans-serif' font-weight='bold'"

def rect(x, y, w, h, r=3, fill=BODY, stroke=LINE, sw=2, extra=""):
    return f"<rect x='{x}' y='{y}' width='{w}' height='{h}' rx='{r}' fill='{fill}' stroke='{stroke}' stroke-width='{sw}' {extra}/>"

def line(x1, y1, x2, y2, color=GREY, w=3):
    return f"<line x1='{x1}' y1='{y1}' x2='{x2}' y2='{y2}' stroke='{color}' stroke-width='{w}' stroke-linecap='round'/>"

def text(x, y, s, size, color=INK, anchor="middle"):
    return f"<text x='{x}' y='{y}' font-size='{size}' {FONT} fill='{color}' text-anchor='{anchor}'>{s}</text>"

ICONS = {
    # --- core widgets -----------------------------------------------------------
    "form": rect(5, 7, 38, 34, 4) + f"<path d='M5 11a4 4 0 0 1 4-4h30a4 4 0 0 1 4 4v5H5z' fill='{BLUE}' stroke='{LINE}' stroke-width='2'/>"
            + "".join(f"<circle cx='{x}' cy='11.5' r='1.6' fill='#fff'/>" for x in (30, 35, 39)[:3])
            + line(11, 25, 27, 25) + line(11, 32, 21, 32),
    "container": rect(5, 11, 38, 30, 3, fill="none", extra="stroke-dasharray='4 3'") + rect(9, 7, 16, 8, 2, fill=PALE, stroke=BLUE)
                 + rect(11, 20, 12, 8, 2, fill=SOFT) + rect(26, 20, 12, 8, 2, fill=SOFT) + line(12, 34, 36, 34),
    "label": rect(4, 8, 40, 32, 4, fill=SOFT) + text(24, 32, "Aa", 20),
    "button": rect(4, 13, 40, 22, 5, fill=SOFT) + text(24, 29, "OK", 13),
    "edit": rect(4, 14, 40, 20, 3, stroke=BLUE) + text(9, 29, "ab", 12, anchor="start") + line(29, 18, 29, 30, INK, 2),
    "checkbox": rect(5, 13, 20, 20, 4, fill=BLUE, stroke=BLUE) + "<path d='M9.5 23l4.5 4.5 7.5-9' fill='none' stroke='#fff' stroke-width='3' stroke-linecap='round' stroke-linejoin='round'/>"
                + line(30, 23, 43, 23),
    # ImageBox: Bild im Rahmen mit Anfassern (Größe frei, px oder %).
    "imagebox": rect(7, 9, 34, 30, 2, fill=PALE) + f"<circle cx='31' cy='17' r='3.5' fill='#f2b632'/>"
                + f"<path d='M9 37l10-12 7 8 4-4 9 8z' fill='{BLUE}' stroke='{LINE}' stroke-width='1.5' stroke-linejoin='round'/>"
                + "".join(rect(x - 2.5, y - 2.5, 5, 5, 1, fill=BODY, stroke=BLUE, sw=1.5) for x, y in ((7, 9), (41, 9), (7, 39), (41, 39))),
    # ImageScrollBox: drei Bilder in einer Reihe mit Bildlaufleiste.
    "imagescrollbox": "".join(rect(x, 10, 12, 20, 2, fill=PALE) + f"<path d='M{x + 1} 29l4-6 3 3 2-2 2 5z' fill='{BLUE}'/>"
                              for x in (4, 18, 32))
                      + rect(4, 35, 40, 6, 3, fill=SOFT) + rect(6, 36.5, 16, 3, 1.5, fill=BLUE, stroke=BLUE, sw=1),
    # --- GTK 4 palette ---------------------------------------------------------
    "radio": f"<circle cx='15' cy='23' r='10' fill='{BODY}' stroke='{BLUE}' stroke-width='2'/><circle cx='15' cy='23' r='5' fill='{BLUE}'/>" + line(30, 23, 43, 23),
    "dropdown": rect(4, 14, 40, 20, 3) + line(9, 24, 25, 24) + f"<path d='M33 21l4 5 4-5' fill='none' stroke='{INK}' stroke-width='2.5' stroke-linecap='round' stroke-linejoin='round'/>"
                + f"<line x1='30' y1='15' x2='30' y2='33' stroke='{LINE}' stroke-width='1.5'/>",
    "spin": rect(4, 14, 40, 20, 3) + text(9, 29, "42", 12, anchor="start") + f"<line x1='31' y1='15' x2='31' y2='33' stroke='{LINE}' stroke-width='1.5'/>"
            + f"<path d='M34 22l3.5-4 3.5 4M34 26l3.5 4 3.5-4' fill='none' stroke='{INK}' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'/>",
    "switch": rect(5, 14, 38, 20, 10, fill=BLUE, stroke=BLUE) + f"<circle cx='33' cy='24' r='7.5' fill='#fff'/>",
    "slider": line(6, 24, 42, 24, GREY, 4) + line(6, 24, 26, 24, BLUE, 4) + f"<circle cx='26' cy='24' r='7' fill='{BODY}' stroke='{LINE}' stroke-width='2'/>",
    "progress": rect(4, 17, 40, 14, 7, fill=SOFT) + rect(5, 18, 24, 12, 6, fill=BLUE, stroke="none", sw=0),
    "memo": rect(6, 5, 36, 38, 3) + line(11, 13, 31, 13) + line(11, 20, 28, 20) + line(11, 27, 31, 27) + line(11, 34, 24, 34)
            + f"<rect x='35' y='9' width='4' height='14' rx='2' fill='{BLUE}'/>",
    "image": rect(5, 9, 38, 30, 3, fill=PALE) + f"<circle cx='33' cy='17' r='4' fill='#f2b632'/>"
             + f"<path d='M7 37l11-13 8 9 5-5 10 9z' fill='{BLUE}' stroke='{LINE}' stroke-width='1.5' stroke-linejoin='round'/>",
    "separator": line(5, 24, 43, 24, LINE, 3) + rect(10, 9, 28, 8, 2, fill=SOFT) + rect(10, 31, 28, 8, 2, fill=SOFT),
    "frame": rect(5, 9, 38, 32, 3) + text(9, 17, "Abc", 8, BLUE, anchor="start") + line(26, 14, 39, 14, LINE, 1.5)
             + line(11, 26, 36, 26) + line(11, 33, 30, 33),
    "tabs": f"<path d='M5 15h13v-6h-0' fill='none'/>" + rect(5, 15, 38, 26, 3) + rect(5, 8, 14, 9, 2, fill=BODY) + rect(20, 8, 12, 9, 2, fill=SOFT)
            + f"<rect x='6' y='14' width='12' height='3' fill='{BODY}'/>" + line(11, 26, 35, 26) + line(11, 33, 28, 33),
    "list": rect(6, 6, 36, 36, 3) + rect(7, 15, 34, 8, 0, fill=BLUE, stroke="none", sw=0)
            + line(11, 11, 30, 11) + line(11, 19, 32, 19, "#ffffff") + line(11, 27, 28, 27) + line(11, 35, 31, 35),
    "menu": rect(4, 6, 40, 9, 2, fill=SOFT) + text(9, 13.5, "File", 7, anchor="start") + text(25, 13.5, "Edit", 7, anchor="start")
            + rect(8, 15, 26, 27, 2) + line(13, 22, 29, 22) + line(13, 29, 26, 29) + line(13, 36, 28, 36)
            + f"<rect x='9' y='26' width='24' height='6' fill='{PALE}'/>" + line(13, 29, 26, 29, BLUE),
    "link": rect(4, 12, 40, 24, 4, fill=SOFT) + text(24, 27, "www", 12, BLUE) + line(10, 31, 38, 31, BLUE, 2),
    # --- IDE: tool bar ------------------------------------------------------------------
    "check": f"<circle cx='24' cy='24' r='18' fill='{PALE}' stroke='{LINE}' stroke-width='2'/>"
             + f"<path d='M15 24.5l6 6 12-13' fill='none' stroke='{BLUE}' stroke-width='4' stroke-linecap='round' stroke-linejoin='round'/>",
    "clippy": f"<path d='M30 14v17a7 7 0 0 1-14 0V12a5 5 0 0 1 10 0v18a2.5 2.5 0 0 1-5 0V15' fill='none' stroke='{LINE}' stroke-width='3' stroke-linecap='round'/>"
              + f"<circle cx='35' cy='10' r='5' fill='{BLUE}'/>",
    "format": rect(6, 6, 36, 36, 3) + line(12, 14, 36, 14, BLUE) + line(16, 21, 36, 21) + line(16, 28, 32, 28) + line(12, 35, 30, 35, BLUE),
    "test": f"<path d='M19 6h10M21 6v13L11 37a4 4 0 0 0 3.5 6h19a4 4 0 0 0 3.5-6L27 19V6' fill='{BODY}' stroke='{LINE}' stroke-width='2' stroke-linejoin='round'/>"
            + f"<path d='M15 31h18l3.6 6.2A2.5 2.5 0 0 1 34.4 41H13.6a2.5 2.5 0 0 1-2.2-3.8z' fill='{BLUE}'/>",
    "update": f"<path d='M38 22A14 14 0 0 0 12 15' fill='none' stroke='{BLUE}' stroke-width='3.5' stroke-linecap='round'/>"
              + f"<path d='M10 9v7h7' fill='none' stroke='{BLUE}' stroke-width='3.5' stroke-linecap='round' stroke-linejoin='round'/>"
              + f"<path d='M10 26a14 14 0 0 0 26 7' fill='none' stroke='{LINE}' stroke-width='3.5' stroke-linecap='round'/>"
              + f"<path d='M38 39v-7h-7' fill='none' stroke='{LINE}' stroke-width='3.5' stroke-linecap='round' stroke-linejoin='round'/>",
    "doc": f"<path d='M24 12c-5-4-12-4-18-3v29c6-1 13-1 18 3 5-4 12-4 18-3V9c-6-1-13-1-18 3z' fill='{BODY}' stroke='{LINE}' stroke-width='2' stroke-linejoin='round'/>"
           + line(24, 12, 24, 41, LINE, 2) + line(10, 17, 19, 17, BLUE, 2) + line(10, 23, 19, 23) + line(29, 17, 38, 17) + line(29, 23, 38, 23),
    "sqlx": f"<path d='M7 11v24c0 3 6 6 13 6s13-3 13-6V11' fill='{PALE}' stroke='{LINE}' stroke-width='2'/>"
            + f"<ellipse cx='20' cy='11' rx='13' ry='5.5' fill='{BODY}' stroke='{LINE}' stroke-width='2'/>"
            + f"<path d='M34 18l-7 12h6l-3 11 10-15h-6l4-8z' fill='{BLUE}' stroke='{LINE}' stroke-width='1.5' stroke-linejoin='round'/>",
    # --- freya (widgets/freya-gui) ---------------------------------------------------
    "accordion": rect(5, 7, 38, 11, 3, fill=SOFT) + f"<path d='M35 11l3 3 3-3' fill='none' stroke='{INK}' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'/>"
                 + line(10, 12.5, 24, 12.5) + rect(5, 21, 38, 20, 3) + line(10, 28, 34, 28) + line(10, 34, 28, 34),
    "calendar": rect(6, 9, 36, 32, 3) + f"<path d='M7 10h34v8H7z' fill='{BLUE}'/>" + line(15, 5, 15, 12, LINE, 2.5) + line(33, 5, 33, 12, LINE, 2.5)
                + "".join(f"<circle cx='{x}' cy='{y}' r='1.8' fill='{GREY}'/>" for y in (25, 32) for x in (13, 20, 27, 34))
                + rect(24, 29, 7, 6, 1.5, fill=BLUE, stroke=BLUE),
    "chip": rect(4, 15, 40, 18, 9, fill=PALE, stroke=BLUE) + text(24, 28, "tag", 10, BLUE),
    "colorpicker": f"<circle cx='24' cy='24' r='17' fill='{BODY}' stroke='{LINE}' stroke-width='2'/>"
                   + "".join(f"<circle cx='{x}' cy='{y}' r='4.5' fill='{c}'/>" for x, y, c in ((24, 14, '#e5484d'), (33, 21, '#f2b632'), (30, 32, '#30a46c'), (18, 32, BLUE), (15, 21, '#8e4ec6'))),
    "loader": f"<circle cx='24' cy='24' r='15' fill='none' stroke='{SOFT}' stroke-width='5'/>"
              + f"<path d='M24 9a15 15 0 0 1 15 15' fill='none' stroke='{BLUE}' stroke-width='5' stroke-linecap='round'/>",
    "skeleton": rect(5, 9, 38, 30, 4, fill=SOFT) + rect(10, 15, 20, 5, 2.5, fill=PALE, stroke="none", sw=0)
                + rect(10, 24, 28, 5, 2.5, fill=PALE, stroke="none", sw=0) + rect(10, 31, 16, 3, 1.5, fill=PALE, stroke="none", sw=0),
    "segmented": rect(3, 15, 42, 18, 9, fill=SOFT) + rect(4, 16, 19, 16, 8, fill=BLUE, stroke="none", sw=0)
                 + line(32, 24, 38, 24, GREY, 2.5),
    "splitter": rect(4, 8, 40, 32, 3) + line(20, 9, 20, 39, BLUE, 3) + rect(17, 19, 6, 10, 2, fill=BODY, stroke=BLUE)
                + line(9, 16, 15, 16) + line(25, 16, 39, 16),
    "popup": rect(3, 5, 42, 38, 3, fill=SOFT, stroke=GREY, extra="opacity='0.6'") + rect(9, 12, 30, 24, 4)
             + line(13, 18, 27, 18, INK, 2.5) + rect(25, 27, 10, 6, 2, fill=BLUE, stroke=BLUE),
    "terminal": rect(4, 8, 40, 32, 4, fill="#20242a", stroke=LINE) + f"<path d='M10 18l6 5-6 5' fill='none' stroke='#9fe870' stroke-width='3' stroke-linecap='round' stroke-linejoin='round'/>"
                + line(20, 29, 32, 29, "#e0e0e0", 3),
    "chart": line(8, 8, 8, 40, LINE, 2.5) + line(8, 40, 42, 40, LINE, 2.5)
             + f"<path d='M10 34l9-10 7 6 13-16' fill='none' stroke='{BLUE}' stroke-width='3.5' stroke-linecap='round' stroke-linejoin='round'/>",
    "camera": rect(4, 14, 40, 26, 5) + rect(16, 9, 14, 7, 2, fill=SOFT) + f"<circle cx='24' cy='27' r='8' fill='{PALE}' stroke='{BLUE}' stroke-width='3'/>",
    "video": rect(4, 10, 40, 28, 4, fill=PALE) + f"<path d='M20 17v14l12-7z' fill='{BLUE}' stroke='{LINE}' stroke-width='1.5' stroke-linejoin='round'/>",
    # --- IDE ------------------------------------------------------------------------
    "pointer": f"<path d='M14 6v31l8-8 5.5 12 5-2.3-5.5-11.7H38z' fill='{BODY}' stroke='{INK}' stroke-width='2.5' stroke-linejoin='round'/>",
    # --- non-visual components -----------------------------------------------------
    "component": f"<path d='M24 5l17 9v20l-17 9-17-9V14z' fill='{PALE}' stroke='{LINE}' stroke-width='2' stroke-linejoin='round'/>"
                 + f"<path d='M7 14l17 9 17-9M24 23v20' fill='none' stroke='{LINE}' stroke-width='2' stroke-linejoin='round'/>",
    "cpu": rect(12, 12, 24, 24, 3, fill=PALE) + rect(18, 18, 12, 12, 2, fill=BLUE, stroke=BLUE)
           + "".join(line(x, 5, x, 11, LINE, 2) + line(x, 37, x, 43, LINE, 2) for x in (18, 24, 30))
           + "".join(line(5, y, 11, y, LINE, 2) + line(37, y, 43, y, LINE, 2) for y in (18, 24, 30)),
    "database": f"<path d='M9 11v26c0 3 7 6 15 6s15-3 15-6V11' fill='{PALE}' stroke='{LINE}' stroke-width='2'/>"
                + f"<ellipse cx='24' cy='11' rx='15' ry='6' fill='{BLUE}' stroke='{LINE}' stroke-width='2'/>"
                + f"<path d='M9 20c0 3 7 6 15 6s15-3 15-6M9 29c0 3 7 6 15 6s15-3 15-6' fill='none' stroke='{LINE}' stroke-width='1.5'/>",
    "table": rect(5, 8, 38, 32, 3) + f"<path d='M6 9h36v8H6z' fill='{BLUE}'/>"
             + f"<path d='M5 17h38M5 25h38M5 33h38M18 8v32M31 8v32' stroke='{LINE}' stroke-width='1.5'/>",
    "globe": f"<circle cx='24' cy='24' r='18' fill='{PALE}' stroke='{LINE}' stroke-width='2'/>"
             + f"<ellipse cx='24' cy='24' rx='8' ry='18' fill='none' stroke='{BLUE}' stroke-width='2'/>"
             + f"<path d='M6 24h36M9 15h30M9 33h30' stroke='{BLUE}' stroke-width='2'/>",
    "timer": f"<circle cx='24' cy='26' r='16' fill='{BODY}' stroke='{LINE}' stroke-width='2'/>" + rect(20, 4, 8, 5, 1.5, fill=SOFT)
             + line(24, 26, 24, 16, BLUE, 3) + line(24, 26, 31, 30, INK, 2.5) + f"<circle cx='24' cy='26' r='2' fill='{INK}'/>",
}

for name, body in ICONS.items():
    svg = (f"<svg xmlns='http://www.w3.org/2000/svg' width='48' height='48' viewBox='0 0 48 48'>"
           f"<!-- Freya TDE palette icon '{name}' -->{body}</svg>\n")
    path = os.path.join(OUT, f"{name}.svg")
    with open(path, "w") as f:
        f.write(svg)
    subprocess.run(["rsvg-convert", "-w", "48", "-h", "48", path, "-o", os.path.join(OUT, f"{name}.png")], check=True)
print(len(ICONS), "icons")
