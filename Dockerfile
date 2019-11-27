FROM rust:1.39
RUN USER=root cargo new --bin tgcd
WORKDIR /tgcd
RUN rustup component add rustfmt

COPY ./Cargo.lock /Cargo.toml ./

RUN mkdir src/bin && mv src/main.rs src/bin/server.rs \
    &&  cargo build --release --no-default-features --features server\
    && rm src/bin/*.rs

COPY . .

RUN cargo build --release --no-default-features --features server

FROM rust:1.39

COPY --from=0 /tgcd/target/release/tgcd .

EXPOSE 8080

CMD ["./tgcd"]
