# Source from speaker/: build environment for oxivgl on the ESP32-S3.
# Needs espup's export file, and pypng + lz4 for LVGL's image tool.
. "${ESP_EXPORT:-$HOME/export-esp.sh}"
INC=$(dirname "$(find ~/.rustup/toolchains/esp/xtensa-esp-elf -path '*xtensa-esp-elf/xtensa-esp-elf/include/inttypes.h' | head -1)")
export BINDGEN_EXTRA_CLANG_ARGS="-isystem $INC"  # ESP clang lacks libc headers
