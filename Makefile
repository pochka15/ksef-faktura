# ksef: invoices as PDF + FA(3) XML, sent to KSeF only on request.
# `make help` lists targets. Pass CLI arguments with ARGS, e.g. `make run ARGS="token --env test"`.

ARGS ?=

.PHONY: help build run test lint fmt install golden preview dist web-setup web web-dev web-fmt

help:
	@echo "make run [ARGS=...]  run the app (no ARGS: the browser page + terminal menu)"
	@echo "make test            unit tests, incl. the FA(3) schema check and a golden XML (no network)"
	@echo "make golden          rewrite tests/fixtures/golden-fa3*.xml after an intended XML change; review the diff!"
	@echo "make preview         render sample PDFs (pl, en) into ./target/preview"
	@echo "make lint            clippy + format check"
	@echo "make fmt             format the code"
	@echo "make build           release build"
	@echo "make install         install the 'ksef' binary into ~/.cargo/bin"
	@echo "make dist            zip the binary + INSTALL_PROMPT.md + tutorials into target/dist to share (universal if the Intel target is installed)"
	@echo "make web-setup       install the pinned Svelte/Vite packages (Node 18+, pnpm); only to change web/"
	@echo "make web             build web/ into src/assets/index.html (embedded in the binary; commit it)"
	@echo "make web-dev         Vite dev server; run 'ksef --no-open' too and open Vite's URL with the same ?t=..."
	@echo "make web-fmt         prettier on web/"

build:
	cargo build --release

run:
	cargo run -q -- $(ARGS)

test:
	cargo test

golden:
	UPDATE_GOLDEN=1 cargo test -q golden
	git diff --stat -- tests/fixtures/ || true

preview:
	mkdir -p target/preview
	KSEF_PDF_PREVIEW=target/preview cargo test -q -- renders_both_languages foreign_invoices_show
	@echo "open target/preview/sample-pl.pdf"

lint:
	cargo clippy --all-targets -- -D warnings
	cargo fmt --check

fmt:
	cargo fmt

install:
	cargo install --path .

# Universal (Apple Silicon + Intel) when `rustup target add x86_64-apple-darwin` was done once; else Apple Silicon only.
# Paths compiled into panic messages would show this machine's home directory; map it to `~`.
dist: export RUSTFLAGS = --remap-path-prefix=$(HOME)=~
dist:
	cargo build --release
	rm -rf target/dist && mkdir -p target/dist/ksef
	@if rustup target list --installed | grep -q x86_64-apple-darwin; then \
		cargo build --release --target x86_64-apple-darwin && \
		lipo -create -output target/dist/ksef/ksef target/release/ksef target/x86_64-apple-darwin/release/ksef && \
		echo universal > target/dist/arch; \
	else \
		cp target/release/ksef target/dist/ksef/ksef && echo apple-silicon > target/dist/arch; \
	fi
	cp INSTALL_PROMPT.md README.md LICENSE target/dist/ksef/
	cp -R tutorials examples target/dist/ksef/
	cd target/dist && zip -qr "ksef-macos-$$(cat arch).zip" ksef && rm arch
	@ls -lh target/dist/*.zip

web-setup:
	cd web && pnpm install --frozen-lockfile

web:
	cd web && pnpm build

web-dev:
	cd web && pnpm dev

web-fmt:
	cd web && pnpm exec prettier --write 'src/**/*.{js,svelte}' style.css index.html vite.config.js
