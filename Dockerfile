FROM rust:1.41
RUN mkdir /build && cd /build
WORKDIR /build
RUN rustup component add rustfmt && rustup target add x86_64-unknown-linux-musl

COPY . .

RUN cargo build --release --target x86_64-unknown-linux-musl -p tgcd-server

FROM alpine:3.11.1

COPY --from=0 /build/target/x86_64-unknown-linux-musl/release/tgcd-server .

EXPOSE 8080

CMD ["./tgcd-server"]
