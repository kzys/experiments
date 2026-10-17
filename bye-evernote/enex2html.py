#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Convert Evernote ENEX files to static HTML with attachments as plain files.

Usage: enex2html.py [--no-index] OUTPUT_DIR FILE.enex...

Each notebook (ENEX file) becomes a directory, and each note becomes a
directory inside it holding index.html and the note's attachments under
their original file names, dated like the note. OUTPUT_DIR/index.html
lists the notebooks. Standard library only; Python 3.10 or later.
"""

import argparse
import base64
import hashlib
import html
import mimetypes
import os
import re
import sys
import xml.etree.ElementTree as ET
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import quote, urlsplit

MAX_NAME_BYTES = 150  # well under the usual 255-byte limit, leaves room for suffixes

CSS = """
:root { color-scheme: light dark; }
body { font: 15px/1.5 system-ui, sans-serif; max-width: 960px; margin: 2em auto; padding: 0 16px; }
.meta { color: #888; font-size: 13px; margin-bottom: 1.5em; overflow-wrap: anywhere; }
.note { overflow-wrap: anywhere; }
.meta a { color: inherit; }
.note img { max-width: 100%; height: auto; }
.note embed { width: 100%; height: 85vh; border: 1px solid #8884; }
table.list { border-collapse: collapse; width: 100%; }
table.list td { padding: 4px 8px; border-bottom: 1px solid #8883; vertical-align: top; }
table.list td.n { text-align: right; color: #888; white-space: nowrap; }
td.date { white-space: nowrap; color: #888; }
.attachments { margin-top: 2em; border-top: 1px solid #8884; padding-top: 1em; }
.crypt { background: #8882; padding: 8px; white-space: pre-wrap; word-break: break-all; }
"""


def page(title: str, body: str, css_href: str) -> str:
    return (
        "<!doctype html>\n<html><head><meta charset=\"utf-8\">"
        "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">"
        f"<title>{html.escape(title)}</title>"
        f"<link rel=\"stylesheet\" href=\"{css_href}\"></head>\n"
        f"<body>\n{body}\n</body></html>\n"
    )


def parse_time(s: str | None) -> datetime | None:
    if not s:
        return None
    try:
        return datetime.strptime(s, "%Y%m%dT%H%M%SZ").replace(tzinfo=timezone.utc).astimezone()
    except ValueError:
        return None


def fmt_time(dt: datetime | None) -> str:
    return dt.strftime("%Y-%m-%d %H:%M") if dt else ""


def safe_name(name: str) -> str:
    """Return name usable as a single path component."""
    name = re.sub(r'[\x00-\x1f/\\:*?"<>|]', "_", name).strip().strip(".")
    name = re.sub(r"\s+", " ", name)
    if not name:
        name = "untitled"
    if len(name.encode()) > MAX_NAME_BYTES:
        stem, ext = os.path.splitext(name)
        if len(ext.encode()) > 16:
            stem, ext = name, ""
        budget = MAX_NAME_BYTES - len(ext.encode())
        stem = stem.encode()[:budget].decode(errors="ignore").rstrip()
        name = stem + ext
    return name


def unique(name: str, taken: set[str]) -> str:
    """Return name, or name with a " (n)" suffix, not in taken (case-insensitively)."""
    stem, ext = os.path.splitext(name)
    candidate, n = name, 2
    while candidate.lower() in taken:
        candidate = f"{stem} ({n}){ext}"
        n += 1
    taken.add(candidate.lower())
    return candidate


def attr(tag: str, name: str) -> str | None:
    m = re.search(rf'\b{name}\s*=\s*"([^"]*)"', tag) or re.search(rf"\b{name}\s*=\s*'([^']*)'", tag)
    return html.unescape(m.group(1)) if m else None


def href(name: str) -> str:
    """Return a file or directory name as a relative link in an attribute."""
    return html.escape(quote(name, safe=""), quote=True)


def media_html(name: str, mime: str, label: str) -> str:
    q = href(name)
    t = html.escape(label)
    if mime.startswith("image/"):
        return f'<a href="{q}"><img src="{q}" alt="{t}"></a>'
    if mime == "application/pdf":
        return (
            f'<div><a href="{q}">📄 {t}</a></div>'
            f'<embed src="{q}" type="application/pdf">'
        )
    if mime.startswith("audio/"):
        return f'<div>🔊 <a href="{q}">{t}</a></div><audio controls src="{q}"></audio>'
    if mime.startswith("video/"):
        return f'<div>🎞 <a href="{q}">{t}</a></div><video controls src="{q}" style="max-width:100%"></video>'
    return f'<div>📎 <a href="{q}">{t}</a></div>'


def convert_enml(enml: str, by_hash: dict[str, dict]) -> tuple[str, set[str]]:
    """Return the note body as HTML and the hashes it referenced."""
    used: set[str] = set()
    s = re.sub(r"<\?xml[^>]*\?>", "", enml)
    s = re.sub(r"<!DOCTYPE[^>]*>", "", s, flags=re.I)

    def media(m: re.Match) -> str:
        tag = m.group(0)
        h = (attr(tag, "hash") or "").lower()
        r = by_hash.get(h)
        if not r:
            return f"<div>[missing attachment {html.escape(h)}]</div>"
        used.add(h)
        return media_html(r["file"], r["mime"], r["file"])

    s = re.sub(r"<en-media\b[^>]*?/>|<en-media\b[^>]*>.*?</en-media>", media, s, flags=re.S | re.I)

    def todo(m: re.Match) -> str:
        checked = (attr(m.group(0), "checked") or "").lower() == "true"
        return f'<input type="checkbox" disabled{" checked" if checked else ""}>'

    s = re.sub(r"<en-todo\b[^>]*?/>|<en-todo\b[^>]*>\s*</en-todo>", todo, s, flags=re.I)
    s = re.sub(
        r"<en-crypt\b([^>]*)>(.*?)</en-crypt>",
        lambda m: '<div class="crypt">🔒 Encrypted text (Evernote encryption; '
        f"hint: {html.escape(attr(m.group(0), 'hint') or '')})\n{m.group(2)}</div>",
        s,
        flags=re.S | re.I,
    )
    s = re.sub(r"<en-note\b", "<div class=\"note\"", s, flags=re.I)
    s = re.sub(r"</en-note>", "</div>", s, flags=re.I)
    return s.strip(), used


def set_mtime(path: Path, dt: datetime | None) -> None:
    if dt:
        ts = dt.timestamp()
        os.utime(path, (ts, ts))


def text(el: ET.Element | None, path: str) -> str | None:
    if el is None:
        return None
    found = el.find(path)
    return found.text if found is not None and found.text is not None else None


class Stats:
    notes = 0
    resources = 0
    unreferenced = 0
    missing = 0


def convert_note(note: ET.Element, nb_name: str, nb_dir: Path, taken: set[str], stats: Stats) -> dict:
    title = (text(note, "title") or "Untitled").strip() or "Untitled"
    created = parse_time(text(note, "created"))
    updated = parse_time(text(note, "updated")) or created
    tags = [t.text for t in note.findall("tag") if t.text]
    na = note.find("note-attributes")
    source_url = text(na, "source-url")
    author = text(na, "author")

    prefix = created.strftime("%Y-%m-%d ") if created else ""
    dirname = unique(safe_name(prefix + title), taken)
    note_dir = nb_dir / dirname
    note_dir.mkdir(parents=True, exist_ok=True)

    files_taken = {"index.html"}
    by_hash: dict[str, dict] = {}
    resources = []
    for i, res in enumerate(note.findall("resource"), 1):
        data_el = res.find("data")
        raw = base64.b64decode(data_el.text or "") if data_el is not None else b""
        mime = text(res, "mime") or "application/octet-stream"
        ra = res.find("resource-attributes")
        fname = text(ra, "file-name")
        if not fname:
            ext = mimetypes.guess_extension(mime) or ".bin"
            fname = f"attachment-{i}{ext}"
        fname = unique(safe_name(fname), files_taken)
        path = note_dir / fname
        path.write_bytes(raw)
        set_mtime(path, parse_time(text(ra, "timestamp")) or created)
        h = hashlib.md5(raw).hexdigest()
        r = {"file": fname, "mime": mime, "size": len(raw)}
        by_hash.setdefault(h, r)
        resources.append((h, r))
        stats.resources += 1

    content = text(note, "content") or ""
    body, used = convert_enml(content, by_hash)
    stats.missing += len(set(re.findall(r"\[missing attachment ([0-9a-f]*)\]", body)))

    meta = [f"Created {fmt_time(created)}"]
    if updated and updated != created:
        meta.append(f"Updated {fmt_time(updated)}")
    if tags:
        meta.append("Tags: " + ", ".join(html.escape(t) for t in tags))
    if author:
        meta.append("Author: " + html.escape(author))
    if source_url:
        u = html.escape(source_url, quote=True)
        host = urlsplit(source_url).netloc or source_url[:60]
        meta.append(f'Source: <a href="{u}">{html.escape(host)}</a>')

    parts = [
        '<p><a href="../">← notebook</a></p>',
        f"<h1>{html.escape(title)}</h1>",
        f'<div class="meta">{" · ".join(meta)}</div>',
        body,
    ]
    unref = [(h, r) for h, r in resources if h not in used]
    stats.unreferenced += len(unref)
    for _, r in unref:
        parts.append(media_html(r["file"], r["mime"], r["file"]))
    if resources:
        items = "".join(
            f'<li><a href="{href(r["file"])}">{html.escape(r["file"])}</a>'
            f' <span class="meta">({r["size"]:,} bytes)</span></li>'
            for _, r in resources
        )
        parts.append(f'<div class="attachments"><b>Attachments</b><ul>{items}</ul></div>')

    index = note_dir / "index.html"
    index.write_text(page(f"{nb_name} / {title}", "\n".join(parts), "../../style.css"), encoding="utf-8")
    set_mtime(index, updated)
    # Last, since writing into the directory changes its time.
    set_mtime(note_dir, updated)
    stats.notes += 1
    return {"dir": dirname, "title": title, "created": created, "updated": updated,
            "attachments": len(resources)}


def convert_enex(enex: Path, out: Path, stats: Stats, write_index: bool) -> dict:
    nb_name = enex.stem
    nb_dir = out / safe_name(nb_name)
    nb_dir.mkdir(parents=True, exist_ok=True)
    taken = {"index.html"}
    notes = []
    for _, el in ET.iterparse(enex, events=("end",)):
        if el.tag == "note":
            notes.append(convert_note(el, nb_name, nb_dir, taken, stats))
            el.clear()

    if write_index:
        write_notebook_index(nb_dir, nb_name, notes)
    set_mtime(nb_dir, max((n["updated"] for n in notes if n["updated"]), default=None))
    return {"dir": nb_dir.name, "name": nb_name, "notes": len(notes)}


def write_notebook_index(nb_dir: Path, nb_name: str, notes: list[dict]) -> None:
    notes.sort(key=lambda n: n["created"] or datetime.min.replace(tzinfo=timezone.utc), reverse=True)
    rows = "".join(
        f'<tr><td class="date">{fmt_time(n["created"])[:10]}</td>'
        f'<td><a href="{href(n["dir"])}/">{html.escape(n["title"])}</a></td>'
        f'<td class="n">{"📎 " + str(n["attachments"]) if n["attachments"] else ""}</td></tr>'
        for n in notes
    )
    body = (
        '<p><a href="../">← all notebooks</a></p>'
        f"<h1>{html.escape(nb_name)}</h1>"
        f'<table class="list">{rows}</table>'
    )
    (nb_dir / "index.html").write_text(page(nb_name, body, "../style.css"), encoding="utf-8")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("output_dir", type=Path)
    ap.add_argument("enex", nargs="+", type=Path)
    ap.add_argument(
        "--no-index",
        action="store_true",
        help="don't write the list of notebooks or each notebook's list of notes,"
        " for a web server that lists directories itself",
    )
    args = ap.parse_args()
    out = args.output_dir
    out.mkdir(parents=True, exist_ok=True)
    (out / "style.css").write_text(CSS, encoding="utf-8")
    stats = Stats()
    notebooks = []
    for f in args.enex:
        print(f"converting {f}", file=sys.stderr, flush=True)
        notebooks.append(convert_enex(f, out, stats, write_index=not args.no_index))
    if not args.no_index:
        write_top_index(out, notebooks)
    print(
        f"notes: {stats.notes}, attachments: {stats.resources}, "
        f"not referenced in body: {stats.unreferenced}, "
        f"referenced but missing: {stats.missing}",
        file=sys.stderr,
    )
    return 0


def write_top_index(out: Path, notebooks: list[dict]) -> None:

    # Merge with notebooks converted by earlier runs into the same directory.
    existing = {nb["dir"] for nb in notebooks}
    for d in sorted(out.iterdir()):
        if d.is_dir() and d.name not in existing and (d / "index.html").exists():
            n = sum(1 for x in d.iterdir() if x.is_dir())
            notebooks.append({"dir": d.name, "name": d.name, "notes": n})
    notebooks.sort(key=lambda nb: (nb["name"] == "Trash", nb["name"].lower()))
    rows = "".join(
        f'<tr><td><a href="{href(nb["dir"])}/">{html.escape(nb["name"])}</a></td>'
        f'<td class="n">{nb["notes"]}</td></tr>'
        for nb in notebooks
    )
    body = f'<h1>Evernote archive</h1><table class="list">{rows}</table>'
    (out / "index.html").write_text(page("Evernote archive", body, "style.css"), encoding="utf-8")


if __name__ == "__main__":
    sys.exit(main())
