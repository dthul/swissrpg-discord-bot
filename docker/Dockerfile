FROM ubuntu:18.04

RUN apt-get -qy update \
    && apt-get -qy install curl gcc git libssl-dev pkg-config

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
# RUN source $HOME/.cargo/env

# WORKDIR /tmp
# RUN git clone https://github.com/dthul/discord-bot.git \
#     && cd discord-bot \
#     && cargo build --release