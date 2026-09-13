#!/bin/bash
# EmulationStation Ports launcher for music, the Jellyfin audio client.
# Shows up as "Music" in the Ports list (ES derives the name from the
# filename, same as Terminal.sh does for lowkey's terminal entry).

# Server/username/password come from here rather than being hardcoded in
# this script, since this file is what gets committed/deployed — see
# README.md for the format. Quick Connect (PLAN.md) will remove the need
# for this file entirely; until then, this is how the device gets them.
[ -f /storage/.config/music.env ] && . /storage/.config/music.env

exec /storage/.local/bin/music
