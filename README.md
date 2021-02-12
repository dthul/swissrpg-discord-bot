# Add bot to Discord Server

Use the following URL:

`https://discordapp.com/api/oauth2/authorize?client_id=600752105518792716&permissions=834873616&scope=bot`

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

# Build using Docker

First, create a Docker image called `swissrpg-bot` that contains the necessary build software:

`$ docker build --tag swissrpg-bot ./docker`

For Windows, additionally create the container (done automatically on Linux and Mac):
`$ docker create -it --volume ${pwd}:/opt --name=swissrpg-compile-container swissrpg-bot`

Then, run the compile script which will create a Docker container (or reuse an existing one) and build the project.

`$ ./docker/compile.sh`

On success, the compiled binary will be copied to the root of this repository.

# Install systemd service

Copy/symlink `bot.service` to `/etc/systemd/system/`. Then:

`$ systemctl start bot`

and to enable it permanently:

`$ systemctl enable bot`

NOTE: call `systemctl daemon-reload` after modifying service files

# Where to find logs (stdout, stderr)

`$ journalctl -e -u bot`

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

# Sudo

The bot's `stop` command needs access to `systemctl`. In order to grant this access,
enter the following text into file `/etc/sudoers.d/100-discord-bot` using the
`visudo` command (`visudo -f /etc/sudoers.d/100-discord-bot`):

```
# Bot systemctl commands
Cmnd_Alias BOT_SYSTEMD = /bin/systemctl start bot, /bin/systemctl stop bot, /bin/systemctl restart bot, /bin/systemctl kill bot
bot ALL=(ALL) NOPASSWD: BOT_SYSTEMD
```

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