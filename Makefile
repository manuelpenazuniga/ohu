.PHONY: default setup build build-wasm test lint deploy

default:
	@just --list 2>/dev/null || echo "Install just: https://github.com/casey/just"

setup:
	rustup target add wasm32-unknown-unknown
	rustup target add wasm32-unknown-unknown --toolchain nightly-2026-01-01
	cargo install cargo-odra --version 0.1.7 --locked
	pnpm install

build:
	cd contracts && cargo build --release
	pnpm install --frozen-lockfile
	pnpm run build

build-wasm:
	cd contracts && cargo odra build

test:
	cd contracts && cargo odra test
	pnpm install --frozen-lockfile
	pnpm run test

lint:
	cd contracts && cargo clippy --all-targets --all-features -- -D warnings
	pnpm install --frozen-lockfile
	pnpm run typecheck

deploy:
	bash infra/scripts/deploy.sh
