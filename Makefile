all: build

build:
	@cargo build --release

test:
	@cargo test --all

fmt:
	@cargo +nightly fmt --all

clean:
	@cargo clean
