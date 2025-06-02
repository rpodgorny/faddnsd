FROM docker.io/library/rust:1.87-slim as builder
WORKDIR /usr/src/app
COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/
RUN cargo build --release

FROM docker.io/library/ubuntu:noble
ENV DEBIAN_FRONTEND noninteractive
RUN apt-get update && apt-get install -y bind9 dnsutils && apt-get clean && rm -rf /var/lib/apt/lists/
WORKDIR /usr/src/app
COPY --from=builder /usr/src/app/target/release/faddnsd ./
CMD ["./faddnsd"]
