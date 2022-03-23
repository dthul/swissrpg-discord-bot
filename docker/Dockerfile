FROM ubuntu:18.04

RUN apt-get -qy update \
    && apt-get -qy install clang curl gcc git libssl-dev lld pkg-config \
    && curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*