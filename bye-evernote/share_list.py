# /// script
# requires-python = ">=3.10"
# dependencies = ["evernote-backup==1.14.*"]
# ///
"""List what an Evernote account shares and with whom.

Usage: share_list.py [-d en_backup.db] [-o share-list.txt] [NOTE_TITLE...]

ENEX exports don't record sharing, so this asks Evernote directly. It
lists your notebooks shared with others, notebooks others share with you,
and individual notes you shared. Checking individual notes takes one
request per note; pass note titles to check only those. Read-only.
"""

import argparse
import sys

from evernote.edam.error.ttypes import EDAMErrorCode, EDAMNotFoundException, EDAMUserException
from evernote.edam.notestore.ttypes import NoteResultSpec

from en_client import note_store, notebook_names, open_db

# SharedNotebookPrivilegeLevel
NOTEBOOK_PRIVILEGE = {
    0: "read",
    1: "edit (+activity)",
    2: "read (+activity)",
    3: "group",
    4: "full access",
    5: "business full access",
}
# ShareRelationshipPrivilegeLevel
MEMBER_PRIVILEGE = {10: "read", 20: "read (+activity)", 30: "edit", 40: "full access"}
# SharedNotePrivilegeLevel
NOTE_PRIVILEGE = {0: "read", 1: "edit", 2: "full access"}


def name(table: dict[int, str], level: int) -> str:
    return table.get(level, str(level))


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("-d", "--database", default="en_backup.db")
    ap.add_argument("-o", "--output", default="share-list.txt")
    ap.add_argument("titles", nargs="*", help="only check these notes for individual shares")
    args = ap.parse_args()

    db = open_db(args.database)
    ns = note_store(db)
    notebooks = {nb.guid: nb.name for nb in ns.listNotebooks()}
    lines = ["== Your notebooks shared with others =="]

    by_notebook: dict[str, list] = {}
    for share in ns.listSharedNotebooks():
        by_notebook.setdefault(share.notebookGuid, []).append(share)
    for guid, shares in sorted(by_notebook.items(), key=lambda kv: notebooks.get(kv[0], "")):
        lines.append(notebooks.get(guid, guid))
        # User ids and emails already listed, so a share isn't listed twice.
        seen = set()
        try:
            rel = ns.getNotebookShares(guid)
            for m in rel.memberships or []:
                lines.append(f"  member: {m.displayName} ({name(MEMBER_PRIVILEGE, m.bestPrivilege)})")
                seen.add(m.recipientUserId)
            for i in rel.invitations or []:
                lines.append(f"  invited: {i.displayName} ({name(NOTEBOOK_PRIVILEGE, i.privilege)})")
                if i.recipientUserIdentity:
                    seen.add(i.recipientUserIdentity.stringIdentifier)
                    seen.add(i.recipientUserIdentity.longIdentifier)
        except Exception as e:
            lines.append(f"  (getNotebookShares failed: {e})")
        seen.discard(None)
        for share in shares:
            if share.recipientUserId not in seen and share.email not in seen:
                lines.append(f"  share: {share.email} ({name(NOTEBOOK_PRIVILEGE, share.privilege)})")
    if not by_notebook:
        lines.append("(none)")

    lines += ["", "== Notebooks others shared with you =="]
    linked = ns.listLinkedNotebooks()
    lines += [f"{ln.shareName}  (owner: {ln.username})" for ln in linked] or ["(none)"]

    lines += ["", "== Individual notes you shared =="]
    query = "select guid, title, notebook_guid from notes where is_active=1"
    if args.titles:
        query += " and title in (%s)" % ",".join("?" * len(args.titles))
    rows = db.execute(query, args.titles).fetchall()
    all_notebooks = notebook_names(db)
    spec = NoteResultSpec(includeSharedNotes=True)
    found = 0
    for n, (guid, title, nb_guid) in enumerate(rows, 1):
        if n % 100 == 0:
            print(f"notes checked: {n}/{len(rows)}", file=sys.stderr, flush=True)
        try:
            note = ns.getNoteWithResultSpec(guid, spec)
        except EDAMNotFoundException:
            continue
        except EDAMUserException as e:
            # Notes in notebooks shared with you live on another shard.
            # Anything else, such as a rate limit or an expired login, stops
            # the script rather than leaving notes unchecked.
            if e.errorCode in (EDAMErrorCode.AUTH_EXPIRED, EDAMErrorCode.INVALID_AUTH):
                raise
            continue
        for s in note.sharedNotes or []:
            r = s.recipientIdentity
            if r and r.contact:
                who = r.contact.name or r.contact.id
            elif r and r.userId:
                who = f"user id {r.userId}"
            else:
                who = "?"
            lines.append(
                f"{all_notebooks.get(nb_guid, nb_guid)} / {title}  → {who}"
                f" ({name(NOTE_PRIVILEGE, s.privilege)})"
            )
            found += 1
    if not found:
        lines.append("(none)")

    with open(args.output, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
    print(f"wrote {args.output}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
