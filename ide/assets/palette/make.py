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
