## Build Environment
FROM rust:1.66-slim-bullseye AS build
RUN apt-get update -y && apt-get install -y build-essential protobuf-compiler && \
    rustup target add wasm32-unknown-unknown
ADD . /code
WORKDIR /code
RUN cargo build --package anoma-devnode --release
RUN cargo build --package anoma-solver-sdk --example pgqf-solver --release
RUN cargo build --package anoma-client-sdk --example pgqf-client --release
RUN cargo build --package anoma-predicates-sdk --example pgqf --target wasm32-unknown-unknown --release
RUN cargo build --package anoma-predicates-sdk --example token --target wasm32-unknown-unknown --release
RUN cargo build --package stdpred --target wasm32-unknown-unknown --release

FROM rust:1.66-slim-bullseye
WORKDIR /home
COPY --from=build /code/target/release/anoma-devnode .
COPY --from=build /code/target/release/examples/pgqf-solver .
COPY --from=build /code/target/release/examples/pgqf-client .
COPY --from=build /code/target/wasm32-unknown-unknown/release/stdpred.wasm .
COPY --from=build /code/target/wasm32-unknown-unknown/release/examples/pgqf.wasm .
COPY --from=build /code/target/wasm32-unknown-unknown/release/examples/token.wasm .