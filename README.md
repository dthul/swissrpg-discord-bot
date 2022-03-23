# Add bot to Discord Server

Use the following URL:

prod: `https://discordapp.com/api/oauth2/authorize?client_id=600752105518792716&permissions=834873616&scope=bot`
test: `https://discordapp.com/api/oauth2/authorize?client_id=643523617702936596&permissions=834873616&scope=bot`

## Permissions

The bot needs the following permissions:
* Manage Roles
* Manage Channels
* Manage Webhooks
* Read Message History
* Send Messages
* Manage Messages
* Mention @everyone
* Mute Members
* Move Members
* Deafen Members
* Priority Speaker

Permission Calculator: https://discordapi.com/permissions.html#834873616

# Authorize Slash Commands

prod: `https://discordapp.com/api/oauth2/authorize?client_id=600752105518792716&scope=applications.commands`
test: `https://discordapp.com/api/oauth2/authorize?client_id=643523617702936596&scope=applications.commands`

# Build using Docker

First, create a Docker image called `swissrpg-bot` that contains the necessary build software:

`$ docker build --tag swissrpg-bot ./docker`

For Windows, additionally create the container (done automatically on Linux and Mac):
`$ docker create -it --volume ${pwd}:/opt --network=host --name=swissrpg-compile-container swissrpg-bot`

Then, run the compile script which will create a Docker container (or reuse an existing one) and build the project.

`$ ./docker/compile.sh`

On success, the compiled binary will be copied to the root of this repository.

## Database connection

Depending on the settings a database connection might be required to build. An SSH tunnel to the database can be established as follows:

`$ ssh -N -L 5432:localhost:5432 <username>@bot.swissrpg.ch`

# Install systemd service

Copy/symlink `bot.service` to `/etc/systemd/system/`. Then:

`$ systemctl start bot`

and to enable it permanently:

`$ systemctl enable bot`

NOTE: call `systemctl daemon-reload` after modifying service files

# Where to find logs (stdout, stderr)

`$ journalctl -e -u bot`

# Certbot (Let's Encrypt)

Instructions missing

# Nginx

Copy/symlink `bot.conf` to `/etc/nginx/conf.d/` and remember to disable the default configuration that some distributions have in `/etc/nginx/sites-enabled/default`. Then restart nginx: `$ systemctl restart nginx`

# Redis

Configuration file can be found in `/etc/redis/redis.conf`. By default backups will be found at `/var/lib/redis/dump.rdb` and `/var/lib/redis/appendonly.aof`.
Important settings:

```
# General settings
bind 127.0.0.1 ::1
protected-mode yes

# RDB settings
save 900 1
save 300 10
save 60 10000
stop-writes-on-bgsave-error yes
rdbchecksum yes

# AOF setings
appendonly yes
appendfsync everysec
aof-use-rdb-preamble yes
```

# Postgres

Install according to the instructions on the website. Under Ubuntu, a new `postgres` user will be created and the database cluster will be initialized in `/var/lib/postgresql/13/main` (or appropriate other major version number).

Create Postgres database users ("roles" in Postgres speak) for the Unix users which need a database (`bot` and `bottest` probably):

`$ sudo -u postgres createuser --interactive` (x2)
```
Enter name of role to add: bot
Shall the new role be a superuser? (y/n) n
Shall the new role be allowed to create databases? (y/n) n
Shall the new role be allowed to create more new roles? (y/n) n
```

Create databases for the `bot` and `bottest` users:

`$ sudo -u postgres createdb bot --owner bot`
`$ sudo -u postgres createdb bottest --owner bottest`

Revoke public connection rights to the newly created databases:

`$ sudo -u postgres psql`
`# REVOKE connect ON DATABASE bot FROM PUBLIC;`
`# REVOKE connect ON DATABASE bottest FROM PUBLIC;`

## Port forwarding

`$ ssh -N -L 5432:localhost:5432 daniel@bot.swissrpg.ch`

## Optional: create a superuser with password login for database managing tools

`$ sudo -u postgres createuser --interactive`
```
Enter name of role to add: daniel
Shall the new role be a superuser? (y/n) y
```

Set a password

`$ sudo -u postgres psql`
`# ALTER USER daniel ENCRYPTED PASSWORD '...'`

## Optional: create a read-only user for sqlx

`$ sudo -u postgres psql bottest`
`# CREATE USER sqlx WITH PASSWORD '...';`
`# GRANT CONNECT ON DATABASE bottest TO sqlx;`
`# GRANT USAGE ON SCHEMA public TO sqlx;` (requires to be connected to the right database)
`# GRANT SELECT ON ALL TABLES IN SCHEMA public TO sqlx;`
`# ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT ON TABLES TO sqlx;`

## Optional: create a user for tusker

`$ sudo -u postgres psql`
`# CREATE USER tusker WITH PASSWORD '...' CREATEDB;`

# Sudo

The bot's `stop` command needs access to `systemctl`. In order to grant this access,
enter the following text into file `/etc/sudoers.d/100-discord-bot` using the
`visudo` command (`visudo -f /etc/sudoers.d/100-discord-bot`):

```
# Bot systemctl commands
Cmnd_Alias BOT_SYSTEMD = /bin/systemctl start bot, /bin/systemctl stop bot, /bin/systemctl restart bot, /bin/systemctl kill bot
bot ALL=(ALL) NOPASSWD: BOT_SYSTEMD
```

# Metabase

`$ sudo useradd --system --create-home --shell /bin/bash metabase`
`$ sudo apt-get install openjdk-11-jre` (or later version)
`$ sudo -i -u metabase`
`$ wget https://downloads.metabase.com/v0.37.8/metabase.jar` (or later version)

## Optional: create a read-only Postgres user for metabase

`$ sudo -u postgres psql bottest`
`# CREATE USER metabase WITH PASSWORD '...';`
`# GRANT CONNECT ON DATABASE bottest TO metabase;`
`# GRANT USAGE ON SCHEMA public TO metabase;` (requires to be connected to the right database)
`# GRANT SELECT ON ALL TABLES IN SCHEMA public TO metabase;` (requires to be connected to the right database)
`# ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT ON TABLES TO metabase;` (requires to be connected to the right database)

# Backups to Amazon S3

Install the AWS CLI tools.

Switch to the user account that should do the backups and add the AWS IAM user access key:

```
$ aws configure
> AWS Access Key ID [None]: ...
> AWS Secret Access Key [None]: ...
> Default region name [None]: eu-central-1
> Default output format [None]: json
```

Check the access to S3 works, for example by listing the bucket contents:

`$ aws s3 ls s3://bucket-name`

Give the user account that should do the backups (e.g. `daniel`) read access to the database files:

`$ setfacl -m u:daniel:r-x /var/lib/redis`
`$ setfacl -d -m u:daniel:r /var/lib/redis`
`$ setfacl -m u:daniel:r /var/lib/redis/appendonly.aof`
`$ setfacl -m u:daniel:r /var/lib/redis/dump.rdb`

Copy/symlink `bot-backup.service` and `bot-backup.timer` to `/etc/systemd/system/`. Then:

`$ systemctl start bot-backup.timer`

and to enable it permanently:

`$ systemctl enable bot-backup.timer`

NOTE: call `systemctl daemon-reload` after modifying service files

To check the scheduled timers, use:

`$ systemctl list-timers`

# Running the bot on a development machine

Set up tunnels to the databases:

`$ ssh -N -L 5432:localhost:5432 daniel@bot.swissrpg.ch`
`$ ssh -N -L 6379:localhost:6379 daniel@bot.swissrpg.ch`

Set up a reverse tunnel to bind to the server's port (stop the server's bot first):

`$ ssh -N -R 3001:localhost:3001 daniel@bot.swissrpg.ch`

Start the test bot (check that `secrets.sh` contains the right values):

`$ (source secrets.sh; BOT_ENV=test ./swissrpg-app-test)`