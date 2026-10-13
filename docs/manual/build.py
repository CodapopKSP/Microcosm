"""Build the manual as a PDF: docs/manual/manual.md -> docs/manual/Microcosm-Manual.pdf.

    python3 docs/manual/build.py          # the PDF
    python3 docs/manual/build.py --html   # also write manual.html (for checking the layout)

Needs the Python packages `markdown` and `weasyprint`. Figures are made by figures.py (charts
and diagrams) and screenshots.py (captures from the game); build.py only uses what's in figures/.

The Markdown stays plain so other readers (GitHub, the game) can show it: a figure is an image
alone in its paragraph, and its alt text becomes the numbered caption here.
"""

import html
import re
import sys
from pathlib import Path

import markdown
from weasyprint import HTML

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
SOURCE = HERE / "manual.md"
OUT = HERE / "Microcosm-Manual.pdf"


def version():
    m = re.search(r'^version\s*=\s*"([^"]+)"', (ROOT / "Cargo.toml").read_text(), re.M)
    return m.group(1) if m else ""


def slug(text):
    return re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")


def render_body(md_text):
    body = markdown.markdown(md_text, extensions=["tables", "fenced_code"], output_format="html")

    # Figures: an image alone in a paragraph, captioned with its alt text.
    count = 0

    def figure(m):
        nonlocal count
        count += 1
        src, alt = m.group("src"), m.group("alt")
        alt = re.sub(r"\*([^*]+)\*", r"<i>\1</i>", alt)  # alt text isn't parsed as Markdown
        kind = "shot" if src.endswith((".png", ".jpg")) else "chart"
        return (f'<figure class="{kind}"><img src="{src}" alt="">'
                f'<figcaption><b>Figure {count}.</b> {alt}</figcaption></figure>')

    body = re.sub(r'<p><img alt="(?P<alt>[^"]*)" src="(?P<src>[^"]+)"\s*/?></p>', figure, body)

    # Headings get ids (for the contents) and chapters are wrapped so each starts a page.
    toc, used = [], set()

    def heading(m):
        level, inner = int(m.group(1)), m.group(2)
        plain = html.unescape(re.sub(r"<[^>]+>", "", inner))
        hid, n = slug(plain), 2
        while hid in used:  # the same heading in two chapters
            hid, n = f"{slug(plain)}-{n}", n + 1
        used.add(hid)
        if level <= 2:
            toc.append((level, hid, inner))
        return f'<h{level} id="{hid}">{inner}</h{level}>'

    body = re.sub(r"<h([1-3])>(.*?)</h\1>", heading, body)
    chapters = re.split(r"(?=<h1 )", body)
    body = "".join(f'<section class="chapter">{c}</section>' for c in chapters if c.strip())
    return body, toc


def contents(toc):
    rows = []
    for level, hid, inner in toc:
        rows.append(f'<li class="toc-{level}"><a href="#{hid}">{inner}</a></li>')
    return f'<nav class="toc"><h1 class="toc-title">Contents</h1><ul>{"".join(rows)}</ul></nav>'


def page(md_text):
    body, toc = render_body(md_text)
    cover = f"""
<section class="cover">
  <div class="cover-title">Microcosm</div>
  <div class="cover-sub">How the World Works</div>
  <img class="cover-img" src="figures/shot-cover.jpg" alt="">
  <div class="cover-foot">A guide to the simulation · Version {version()}</div>
</section>"""
    return f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Microcosm: How the World Works</title>
<link rel="stylesheet" href="style.css"></head>
<body>{cover}{contents(toc)}{body}</body></html>"""


def main():
    doc = page(SOURCE.read_text())
    if "--html" in sys.argv:
        (HERE / "manual.html").write_text(doc)
    HTML(string=doc, base_url=str(HERE)).write_pdf(OUT)
    print("wrote", OUT.relative_to(ROOT))


if __name__ == "__main__":
    main()
