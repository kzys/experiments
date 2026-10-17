# bye-evernote

Get everything out of Evernote into files you own, before cancelling:

1. Download the whole account with [evernote-backup].
2. Export it to ENEX, Evernote's own format.
3. Convert the ENEX files to plain HTML pages, with attachments (PDFs,
   images, …) saved as ordinary files under their original names.

Everything runs locally. The HTML needs nothing but a browser, so the
archive doesn't depend on another service that might go away.

```
html/
  index.html                         list of notebooks
  Recipes/
    index.html                       notes, newest first
    2019-05-04 Pancakes/
      index.html                     the note; images inline, PDFs embedded
      pancakes.pdf                   attachment, dated like the note
```

[evernote-backup]: https://github.com/vzhd1701/evernote-backup

## 1. Download

```sh
uv tool install evernote-backup     # or: pipx install evernote-backup
evernote-backup init-db
evernote-backup sync
```

`init-db` opens Evernote's login page. After you approve, the page tries
to open an `evernote://…?code=…` link. evernote-backup needs that link,
not the Evernote app. On Linux nothing handles `evernote://` links, so
the browser just stays on a "Launching Evernote…" page. Copy the link from
there in either of these ways:

- Right-click "Return to Evernote" → Copy link address.
- Open the developer console (F12) on that page and run:

  ```js
  copy([...document.links].find(a => a.href.startsWith("evernote:")).href)
  ```

Paste it into the terminal promptly; the code is single-use and expires
quickly. On a free plan, logging in this way may sign out your other
Evernote clients.

`sync` downloads every note with its attachments into `en_backup.db`.
If it's interrupted, run it again and it continues. Expect about a minute
per 50–150 notes.

## 2. Export to ENEX

```sh
evernote-backup export --include-trash --add-guid --overwrite export/
```

This writes one `.enex` file per notebook, plus `Trash.enex`; notebooks in
a stack go in a subdirectory named after the stack. Without `--overwrite`,
running it again writes `Notebook (1).enex` beside the old file. Keep these
and `en_backup.db`: they are the most complete copy, and other note apps
(Joplin, Apple Notes, Obsidian's importer, …) can import ENEX.

## 3. Convert to HTML

```sh
find export -name '*.enex' -exec python3 enex2html.py html {} +
```

With `--no-index`, it writes only the note pages, leaving the lists of
notebooks and notes to a web server that lists directories itself;
directories carry their notes' dates either way.

Needs only Python 3.10 or later, no packages. Pages link to each other by
directory (`note/` rather than `note/index.html`), so browse them through a
web server, for example `python3 -m http.server -d html`, rather than
opening the files directly. Note pages show
created/updated dates, tags, author and source URL. Checkboxes keep their
state, and Evernote-encrypted text is kept, still encrypted.

At the end it prints how many attachments a note body refers to but the
note doesn't contain; see below.

## Optional checks

These two scripts reuse evernote-backup's stored login and internal
modules. They declare evernote-backup 1.14 as a dependency, so `uv run`
installs it for them. Run them in the directory holding `en_backup.db`, or
pass `-d path/to/en_backup.db`.

**Who you share with.** ENEX doesn't record sharing. To keep a record:

```sh
uv run share_list.py
```

It writes `share-list.txt`, listing your shared notebooks and their
members, notebooks shared with you, and individually shared notes.
Checking notes takes one request per note, so it can take a few minutes.

**Attachments Evernote has lost.** Sometimes a note still refers to an
attachment that Evernote no longer has. The note exports fine, with a gap
where the attachment was (`[missing attachment <hash>]` in the HTML). To
list them and ask Evernote whether it still has each one:

```sh
uv run missing_attachments.py
```

It writes `missing-attachments.tsv`. Most entries are usually small
images from web clips. Look up the rest in the Evernote app before
cancelling; a device that hasn't synced in a while may still have them.

## License

MIT; see [LICENSE](LICENSE).
