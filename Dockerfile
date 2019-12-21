FROM rust:1.40
RUN USER=root cargo new --bin tgcd
WORKDIR /tgcd
RUN rustup component add rustfmt && rustup target add x86_64-unknown-linux-musl

COPY ./Cargo.lock /Cargo.toml ./

RUN mkdir src/bin && mv src/main.rs src/bin/server.rs \
    &&  cargo build --release --no-default-features --features server --target x86_64-unknown-linux-musl \
    && rm src/bin/*.rs

COPY . .

RUN cargo build --release --no-default-features --features server --target x86_64-unknown-linux-musl

FROM alpine:3.11.0

COPY --from=0 /tgcd/target/x86_64-unknown-linux-musl/release/tgcd .

EXPOSE 8080

CMD ["./tgcd"]
