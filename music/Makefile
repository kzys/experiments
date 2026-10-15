# Cross-linking is configured in .cargo/config.toml.
HOST   ?= root@192.168.8.154
TARGET := aarch64-unknown-linux-gnu
BIN    := target/$(TARGET)/release/music

.PHONY: all install install-config clean

all:
	cargo build --release --target $(TARGET)

install: all
	-ssh $(HOST) 'kill $$(pidof music) 2>/dev/null; sleep 1'
	scp $(BIN) $(HOST):/storage/.local/bin/music

# rocknix/Music.sh is the EmulationStation Ports entry (shows up as "Music").
install-config:
	scp rocknix/Music.sh $(HOST):/storage/roms/ports/Music.sh
	ssh $(HOST) 'chmod +x /storage/roms/ports/Music.sh'

clean:
	cargo clean
