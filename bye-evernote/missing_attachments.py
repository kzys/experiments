# /// script
# requires-python = ">=3.10"
# dependencies = ["evernote-backup==1.14.*"]
# ///
"""Find attachments that notes refer to but the backup doesn't contain.

Usage: missing_attachments.py [-d en_backup.db] [-o missing-attachments.tsv] [--no-server]

A note body points at its attachments by MD5 hash. Sometimes Evernote has
lost the attachment while the note still refers to it; such notes export
fine, but with a hole. This lists every such reference and, unless
--no-server is given, asks Evernote whether it still has the file.
Read-only.
"""

import argparse
import re
import sys

from evernote.edam.error.ttypes import EDAMNotFoundException

from en_client import iter_notes, note_store, notebook_names, open_db

MEDIA = re.compile(r"<en-media\b[^>]*>", re.I)


def tag_attr(tag: str, attr: str) -> str | None:
    m = re.search(rf'\b{attr}\s*=\s*["\']([^"\']*)["\']', tag)
    return m.group(1) if m else None


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("-d", "--database", default="en_backup.db")
    ap.add_argument("-o", "--output", default="missing-attachments.tsv")
    ap.add_argument("--no-server", action="store_true", help="don't ask Evernote")
    args = ap.parse_args()

    db = open_db(args.database)
    ns = None if args.no_server else note_store(db)
    notebooks = notebook_names(db)
    rows = []
    notes_with_gaps = set()
    for note in iter_notes(db):
        have = {r.data.bodyHash.hex() for r in note.resources or [] if r.data and r.data.bodyHash}
        for tag in MEDIA.findall(note.content or ""):
            h = (tag_attr(tag, "hash") or "").lower()
            if not re.fullmatch(r"[0-9a-f]{32}", h) or h in have:
                continue
            have.add(h)  # report each hash once per note
            notes_with_gaps.add(note.guid)
            status = ""
            if ns:
                try:
                    r = ns.getResourceByHash(note.guid, bytes.fromhex(h), True, False, False)
                    status = f"on server ({len(r.data.body)} bytes)"
                except EDAMNotFoundException:
                    # Anything else, such as a rate limit, stops the script:
                    # an unchecked attachment isn't a lost one.
                    status = "not on server"
            rows.append(
                [
                    notebooks.get(note.notebookGuid, note.notebookGuid),
                    note.title,
                    "" if note.active else "trash",
                    tag_attr(tag, "type") or "",
                    h,
                    status,
                ]
            )

    with open(args.output, "w", encoding="utf-8") as f:
        f.write("notebook\tnote\tin\ttype\thash\tserver\n")
        for row in rows:
            f.write("\t".join(c.replace("\t", " ") for c in row) + "\n")
    on_server = sum(r[5].startswith("on server") for r in rows)
    print(
        f"{len(rows)} missing attachments in {len(notes_with_gaps)} notes"
        + ("" if args.no_server else f"; {on_server} still on the server")
        + f"; wrote {args.output}",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
