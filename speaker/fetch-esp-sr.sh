#!/bin/sh
# Downloads ESP-SR and the two components it needs from Espressif's
# component registry into vendor/.
set -eu
cd "$(dirname "$0")"
mkdir -p vendor
fetch() {
    [ -d "vendor/$1" ] && return
    rm -rf "vendor/$1.tmp"
    curl -fsSL -o "vendor/$1.zip" \
        "https://components-file.espressif.com/components/espressif/$1/$2/espressif__$1-v$(echo "$2" | tr "~" _).zip"
    # Unpack beside, so a failed run is not mistaken for a finished one.
    unzip -q -d "vendor/$1.tmp" "vendor/$1.zip"
    mv "vendor/$1.tmp" "vendor/$1"
    rm "vendor/$1.zip"
}
fetch esp-sr 2.6.0
fetch dl_fft 0.8.0
fetch cjson 1.7.19~2
