FROM --platform=$BUILDPLATFORM rust:1.64.0-slim-buster AS chef
RUN rustup target add x86_64-unknown-linux-gnu
RUN cargo install cargo-chef
RUN apt-get update && \
    apt-get install -y gcc-x86-64-linux-gnu && \
    rm -rf /var/lib/apt/lists/*
# set correct linker
ENV RUSTFLAGS='-C linker=x86_64-linux-gnu-gcc'
WORKDIR /usr/src/swissrpg-app

FROM chef AS planner
COPY app ./app
COPY command_macro ./command_macro
COPY lib ./lib
COPY ui ./ui
COPY Cargo.lock Cargo.toml ./
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /usr/src/swissrpg-app/recipe.json recipe.json
# COPY .cargo ./.cargo
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --target x86_64-unknown-linux-gnu --recipe-path recipe.json
# Build application
COPY app ./app
COPY command_macro ./command_macro
COPY lib ./lib
COPY ui ./ui
COPY .env Cargo.lock Cargo.toml ./
RUN cargo build --release --target x86_64-unknown-linux-gnu --bin swissrpg-app

FROM debian:buster-slim AS runtime
WORKDIR /usr/src/swissrpg-app
RUN apt update && apt install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/swissrpg-app/target/x86_64-unknown-linux-gnu/release/swissrpg-app /usr/local/bin/swissrpg-app
RUN chmod a=rx /usr/local/bin/swissrpg-app
COPY --chown=bot ui/src/web/html/static /usr/local/share/swissrpg-app/www
RUN find /usr/local/share/swissrpg-app/www -type d -exec chmod a=rx {} \;
RUN find /usr/local/share/swissrpg-app/www -type f -exec chmod a=r {} \;
EXPOSE 3000
CMD ["/usr/local/bin/swissrpg-app"]
