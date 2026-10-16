# Container image of the `bleedthrough` command-line tool.
# The image must carry NOTICE (checked by `cargo xtask check-builtins`).

FROM rust:1.99 AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p bleedthrough-cli

FROM debian:stable-slim
COPY --from=build /src/target/release/bleedthrough /usr/local/bin/bleedthrough
COPY LICENSE-MIT LICENSE-APACHE NOTICE /usr/share/doc/bleedthrough/
ENTRYPOINT ["bleedthrough"]
