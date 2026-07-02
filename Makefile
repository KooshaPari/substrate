# Makefile — slim release + packaging for `substrate`
# Wraps: cargo build --release -p driver-cli

BIN := target/release/substrate
BIN_WIN := target/release/substrate.exe
JOBS ?= $(shell sysctl -n hw.ncpu 2>/dev/null || nproc)
VERSION ?= $(shell awk -F'"' '/^version/ {print $$2; exit}' Cargo.toml)
DIST := dist/v$(VERSION)

.PHONY: build release slim install uninstall clean dist cross-linux cross-windows cross-macos-all dist-all sha verify

build:
	cargo build -p driver-cli --jobs $(JOBS)

# Slim release: strip symbols, opt-level 3, LTO, single codegen unit
release:
	cargo build --release -p driver-cli --jobs $(JOBS)

slim: release
	@if [ -f "$(BIN)" ]; then \
		strip $(BIN); \
		echo "==> $(BIN) $$(du -h $(BIN) | cut -f1)"; \
	fi
	@if [ -f "$(BIN_WIN)" ]; then \
		echo "==> $(BIN_WIN) $$(du -h $(BIN_WIN) | cut -f1)"; \
	fi

install: slim
	INSTALL_DIR=$${INSTALL_DIR:-/usr/local/bin} ./scripts/install.sh

uninstall:
	rm -f $${INSTALL_DIR:-/usr/local/bin}/substrate

dist: slim
	@mkdir -p $(DIST)
	@if [ -f "$(BIN)" ]; then \
		tar -C target/release -czf $(DIST)/substrate-$$(uname -s | tr A-Z a-z)-$$(uname -m).tar.gz substrate; \
		echo "==> $(DIST)/substrate-$$(uname -s | tr A-Z a-z)-$$(uname -m).tar.gz"; \
	fi

cross-linux:
	cargo build --release -p driver-cli --target x86_64-unknown-linux-musl

cross-windows:
	cargo build --release -p driver-cli --target x86_64-pc-windows-msvc

cross-macos-all:
	cargo build --release -p driver-cli --target aarch64-apple-darwin
	cargo build --release -p driver-cli --target x86_64-apple-darwin

dist-all: dist cross-linux cross-windows cross-macos-all
	@mkdir -p $(DIST)
	cp target/x86_64-unknown-linux-musl/release/substrate $(DIST)/substrate-linux-x86_64
	cp target/x86_64-pc-windows-msvc/release/substrate.exe $(DIST)/substrate-windows-x86_64.exe
	cp target/aarch64-apple-darwin/release/substrate $(DIST)/substrate-macos-arm64
	cp target/x86_64-apple-darwin/release/substrate $(DIST)/substrate-macos-x86_64
	cd $(DIST) && for f in substrate-linux-x86_64 substrate-windows-x86_64.exe substrate-macos-arm64 substrate-macos-x86_64; do \
		if [ -f $$f ]; then \
			tar -czf $$f.tar.gz $$f && \
			shasum -a 256 $$f.tar.gz > $$f.tar.gz.sha256 && \
			echo "==> $$f.tar.gz"; \
		fi; \
	done

sha:
	@cd $(DIST) 2>/dev/null && shasum -a 256 *.tar.gz *.zip 2>/dev/null || echo "no dist artifacts"

verify:
	./target/release/substrate --version
	./target/release/substrate --help > /dev/null

clean:
	cargo clean
	rm -rf dist/