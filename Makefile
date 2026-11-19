PREFIX ?= $(HOME)/.local
DESTDIR ?=
CARGO_TARGET_DIR ?= target
export PREFIX DESTDIR CARGO_TARGET_DIR
launcher = io.github.endqwerty.EbookManager.desktop

.PHONY: build install

build:
	cargo build --release --locked --workspace --bins

install: build
	python3 apps/desktop/data/launcher.py "$$PREFIX" > "$$CARGO_TARGET_DIR/release/$(launcher)"
	install -Dm755 "$$CARGO_TARGET_DIR/release/ebook-reader-helper" "$$DESTDIR$$PREFIX/bin/ebook-reader-helper"
	install -Dm755 "$$CARGO_TARGET_DIR/release/ebook-manager" "$$DESTDIR$$PREFIX/bin/ebook-manager"
	install -Dm644 "$$CARGO_TARGET_DIR/release/$(launcher)" "$$DESTDIR$$PREFIX/share/applications/$(launcher)"
