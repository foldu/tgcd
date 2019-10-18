FROM rustlang/rust:nightly
RUN USER=root cargo new --bin tgcd
WORKDIR /tgcd

COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

RUN cargo build --release && rm src/*.rs

COPY . .

RUN cargo build --release

FROM rustlang/rust:nightly

COPY --from=0 /tgcd/target/release/tgcd .

EXPOSE 8000

CMD ["./tgcd"]
