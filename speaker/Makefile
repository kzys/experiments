# What CI runs. The firmware is the whole project and needs espup's Xtensa
# toolchain and ESP-SR (see README), so only its formatting is checked.
.PHONY: all fmt-check fmt

all: fmt-check

fmt-check:
	cargo fmt --check

fmt:
	cargo fmt
