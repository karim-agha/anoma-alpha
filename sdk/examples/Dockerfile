## Build Environment
FROM rust:1.66-slim-bullseye AS build
RUN apt-get update -y && apt-get install -y build-essential protobuf-compiler && \
    rustup target add wasm32-unknown-unknown
ADD . /code
WORKDIR /code
RUN cargo build --package stdpred --target wasm32-unknown-unknown --release && \
    cargo build --package anoma-predicates-sdk --example token --target wasm32-unknown-unknown --release && \
    cargo build --package anoma-devnode --release && \
    cargo build --release --package pgqf-sdk-example --features="nodes" && \
    cargo build --release --package pgqf-sdk-example --lib --target wasm32-unknown-unknown

FROM rust:1.66-slim-bullseye
WORKDIR /home
COPY --from=build /code/target/release/anoma-devnode .
COPY --from=build /code/target/release/pgqf-solver .
COPY --from=build /code/target/release/pgqf-client .
COPY --from=build /code/target/wasm32-unknown-unknown/release/stdpred.wasm .
COPY --from=build /code/target/wasm32-unknown-unknown/release/pgqf_predicates.wasm .
COPY --from=build /code/target/wasm32-unknown-unknown/release/examples/token.wasm .