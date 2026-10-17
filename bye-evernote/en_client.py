"""Shared helpers for scripts that talk to Evernote using an evernote-backup database.

These scripts reuse the login stored by evernote-backup and import its
internal modules, so they pin the evernote-backup version they were tested
with. Run them with uv, which installs it:

    uv run share_list.py
"""

import lzma
import pickle
import sqlite3
from collections.abc import Iterator

from evernote_backup.cli_app_auth import get_sync_client
from evernote_backup.token_util import resolve_auth_token


def open_db(path: str) -> sqlite3.Connection:
    return sqlite3.connect(path)


def note_store(db: sqlite3.Connection):
    """Return an authenticated NoteStore client, saving a refreshed login to db."""
    token = db.execute("select value from config where name='auth_token'").fetchone()[0]
    auth = resolve_auth_token(token)
    if auth.updated:
        db.execute(
            "update config set value=? where name='auth_token'", (auth.auth_for_storage,)
        )
        db.commit()
    client = get_sync_client(
        auth_token=auth.monolith_token,
        jwt_token=auth.jwt_token,
        backend="evernote",
        network_error_retry_count=5,
        use_system_ssl_ca=False,
        max_chunk_results=200,
    )
    return client.note_store


def iter_notes(db: sqlite3.Connection) -> Iterator:
    """Yield the full Note objects stored in db, attachments included."""
    for (raw,) in db.execute("select raw_note from notes where raw_note is not null"):
        yield pickle.loads(lzma.decompress(raw))


def notebook_names(db: sqlite3.Connection) -> dict[str, str]:
    return dict(db.execute("select guid, name from notebooks"))
