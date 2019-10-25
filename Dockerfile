FROM rustlang/rust:nightly
RUN USER=root cargo new --bin tgcd
WORKDIR /tgcd

COPY ./Cargo.lock /Cargo.toml ./

RUN mkdir src/bin && mv src/main.rs src/bin/server.rs \
    &&  cargo build --release --no-default-features --features server\
    && rm src/bin/*.rs

COPY . .

RUN cargo build --release --no-default-features --features server

FROM rustlang/rust:nightly

COPY --from=0 /tgcd/target/release/tgcd .

EXPOSE 8080

CMD ["./tgcd"]
