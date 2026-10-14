# What CI runs. Cargo tracks what needs rebuilding, so every target is phony.
# The firmware is left out: it needs espup's Xtensa toolchain (see README).
.PHONY: all test

all: test

test:
	cargo test --locked
