# Add bot to Discord Server

Use the following URL:

`https://discordapp.com/api/oauth2/authorize?client_id=600752105518792716&permissions=268568592&scope=bot`

# Build using Docker

First, create a Docker image called `swissrpg-bot` that contains the necessary build software:

`$ docker build --tag swissrpg-bot ./docker`

Then, run the compile script which will create a Docker container (or reuse an existing one) and build the project.

`$ ./docker/compile.sh`

On success, the compiled binary will be copied to the root of this repository.

# Install systemd service

Copy `bot.service` to `/etc/systemd/system/`. Then:

`$ systemctl start bot`

and to enable it permanently:

`$ systemctl enable bot`

NOTE: call `systemctl daemon-reload` after modifying service files

# Where to find logs (stdout, stderr)

`$ journalctl -u bot`

# Nginx

Copy/symlink `bot.conf` to `/etc/nginx/conf.d/` and remember to disable the default configuration that some distributions have in `/etc/nginx/sites-enabled/default`. Then restart nginx: `$ systemctl restart nginx`

# To link statically

## For OpenSSL

`$ sudo apt install libssl-dev`\
`$ cargo clean`\
`$ env OPENSSL_LIB_DIR=/usr/lib/x86_64-linux-gnu OPENSSL_INCLUDE_DIR=/usr/include OPENSSL_STATIC=yes cargo build`

## For libc

`$ rustup target add x86_64-unknown-linux-musl`\
`$ sudo apt install musl-tools`\
`$ cargo build --target x86_64-unknown-linux-musl`
