# What CI runs. Cargo tracks what needs rebuilding, so every target is phony.
# The firmware is left out of the build: it needs espup's Xtensa toolchain
# (see README). rustfmt doesn't, so its formatting is checked too.
.PHONY: all test fmt-check fmt

all: fmt-check test

test:
	cargo test --locked

fmt-check:
	cargo fmt --check
	cd firmware && cargo fmt --check

fmt:
	cargo fmt
	cd firmware && cargo fmt
