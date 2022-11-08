all: build

build:
	@cargo build --release

test:
	@cargo test

fmt:
	@cargo +nightly fmt --all

clean:
	@cargo clean
