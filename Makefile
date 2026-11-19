PREFIX ?= $(HOME)/.local
DESTDIR ?=
CARGO_TARGET_DIR ?= target
export CARGO_TARGET_DIR

.PHONY: build install

build:
	cargo build --release --locked --workspace --bins

install: build
	install -Dm755 "$(CARGO_TARGET_DIR)/release/ebook-reader-helper" "$(DESTDIR)$(PREFIX)/bin/ebook-reader-helper"
	install -Dm755 "$(CARGO_TARGET_DIR)/release/ebook-manager" "$(DESTDIR)$(PREFIX)/bin/ebook-manager"
	install -Dm644 apps/desktop/data/io.github.endqwerty.EbookManager.desktop "$(DESTDIR)$(PREFIX)/share/applications/io.github.endqwerty.EbookManager.desktop"
