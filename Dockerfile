## Build Environment
FROM rust:1.65-slim-bullseye AS build-image
RUN apt-get update -y && apt-get install -y build-essential protobuf-compiler
# prefetch cargo index and cache it across code changes for faster build times
RUN cargo search lazy_static
ADD . /code
RUN cd /code && make



## Prod Environment
FROM rust:1.65-slim-bullseye
WORKDIR /home
COPY --from=build-image /code/target/release/anoma .
COPY --from=build-image /code/target/release/solver .
COPY --from=build-image /code/test/genesis.json .

EXPOSE 44668 9000
