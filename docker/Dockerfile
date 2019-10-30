FROM ubuntu:18.04

RUN apt-get -qy update \
    && apt-get -qy install curl gcc git libssl-dev pkg-config

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal --default-toolchain beta