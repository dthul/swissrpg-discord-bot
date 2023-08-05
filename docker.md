# Install Docker on Host

As described here: https://docs.docker.com/engine/install/ubuntu/

That is:

    sudo apt-get update

    sudo apt-get install \
      ca-certificates \
      curl \
      gnupg \
      lsb-release

    sudo mkdir -p /etc/apt/keyrings

    curl -fsSL https://download.docker.com/linux/ubuntu/gpg | sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg

    echo \
      "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/ubuntu \
      $(lsb_release -cs) stable" | sudo tee /etc/apt/sources.list.d/docker.list > /dev/null

    sudo apt-get update

    sudo apt-get install docker-ce docker-ce-cli containerd.io docker-compose-plugin

# Build Docker Images on Build Machine

    docker build --platform linux/amd64 -f swissrpg-app.Dockerfile -t swissrpg-app .
    docker save swissrpg-app | gzip | pv | ssh root@167.235.157.111 docker load

    docker build --platform linux/amd64 -f swissrpg-app-test.Dockerfile -t swissrpg-app-test .
    docker save swissrpg-app-test | gzip | pv | ssh root@167.235.157.111 docker load

    docker build --platform linux/amd64 -f swissrpg-backupper.Dockerfile -t swissrpg-backupper .
    docker save swissrpg-backupper | gzip | pv | ssh root@167.235.157.111 docker load

# Update the running Docker container

Stop and remove the existing container, then check below for the `run` command that will recreate the container.

    docker stop swissrpg-app
    docker rm swissrpg-app

# Create User Accounts on Host

    useradd -M --user-group bot
    useradd -M --user-group bottest
    useradd -M --user-group postgres
    useradd -M --user-group redis
    useradd -M --user-group caddy

# Other Host Preparation

Increase a kernel value for Caddy:

    sysctl -w net.core.rmem_max=2500000

Copy configuration files to server:

    scp Caddyfile create_secret_env_files.py secrets.csv root@167.235.157.111:~
    scp -r .aws root@167.235.157.111:~

Run `create_secret_env_files.py` to create the different `secrets-...sh` files.

# Create Docker Network, Volumes and Containers on Host

Create Docker resources:

    docker network create swissrpg-net
    docker volume create swissrpg-postgres-data
    docker volume create swissrpg-caddy-data
    docker volume create swissrpg-redis-data

Adjust permissions of the volumes:

    docker run -it --rm \
      -v swissrpg-postgres-data:/data \
      bash \
      chown "$(id -u postgres):$(id -g postgres)" /data

    docker run -it --rm \
      -v swissrpg-caddy-data:/data \
      bash \
      chown "$(id -u caddy):$(id -g caddy)" /data

    docker run -it --rm \
      -v swissrpg-redis-data:/data \
      bash \
      chown "$(id -u redis):$(id -g redis)" /data

These next two commands are only necessary if the database cluster has not been initialized yet:

    docker run -it --rm \
      --name swissrpg-postgres-init \
      -v swissrpg-postgres-data:/var/lib/postgresql/data \
      --env-file secrets-postgres.sh \
      postgres:14

(stop this container once the initialization is done).

    docker run -it --rm \
      --name swissrpg-postgres-init \
      -v swissrpg-postgres-data:/var/lib/postgresql/data \
      bash \
      chown -R "$(id -u postgres):$(id -g postgres)" /var/lib/postgresql/data

Start the database container:

    docker run -d \
      --name swissrpg-postgres \
      --user "$(id -u postgres):$(id -g postgres)" \
      --network swissrpg-net \
      --network-alias swissrpg-postgres \
      --shm-size 256MB \
      --stop-timeout 120 \
      -v swissrpg-postgres-data:/var/lib/postgresql/data \
      -v /etc/passwd:/etc/passwd:ro \
      -p 127.0.0.1:5432:5432 \
      --restart unless-stopped \
      postgres:14

Start the Redis container:

    docker run -d \
      --name swissrpg-redis \
      --user "$(id -u redis):$(id -g redis)" \
      --network swissrpg-net \
      --network-alias swissrpg-redis \
      -v swissrpg-redis-data:/data \
      -v /etc/passwd:/etc/passwd:ro \
      --restart unless-stopped \
      redis:7 \
      redis-server \
      --save 900 1 \
      --save 300 10 \
      --save 60 10000 \
      --stop-writes-on-bgsave-error yes \
      --rdbchecksum yes \
      --appendonly yes \
      --appendfsync everysec \
      --aof-use-rdb-preamble yes \
      --loglevel warning

Start the reverse proxy container:

    docker run -d \
      --name swissrpg-caddy \
      --user "$(id -u caddy):$(id -g caddy)" \
      --network swissrpg-net \
      --network-alias swissrpg-caddy \
      -v $PWD/Caddyfile:/etc/caddy/Caddyfile:ro \
      -v swissrpg-caddy-data:/data \
      -v /etc/passwd:/etc/passwd:ro \
      -p 80:80 \
      -p 443:443 \
      --restart unless-stopped \
      caddy:2

Start the app container:

    docker run -d \
      --name swissrpg-app \
      --user "$(id -u bot):$(id -g bot)" \
      --network swissrpg-net \
      --network-alias swissrpg-app \
      -v /etc/passwd:/etc/passwd:ro \
      --env-file secrets-prod.sh \
      -e "BOT_ENV=prod" \
      --restart unless-stopped \
      swissrpg-app

Optionally start the test app container:

    docker run -d \
      --name swissrpg-app-test \
      --user "$(id -u bottest):$(id -g bottest)" \
      --network swissrpg-net \
      --network-alias swissrpg-app-test \
      -v /etc/passwd:/etc/passwd:ro \
      --env-file secrets-test.sh \
      -e "BOT_ENV=test" \
      --restart unless-stopped \
      swissrpg-app-test

# Backups

To manually run it one-off:

    docker run -it --rm \
      --name swissrpg-backupper \
      --user "$(id -u redis):$(id -g redis)" \
      --network swissrpg-net \
      -v /root/.aws:/.aws:ro \
      -v swissrpg-redis-data:/var/lib/redis:ro \
      -v /etc/passwd:/etc/passwd:ro \
      -e "AWS_CONFIG_FILE=/.aws/config" \
      -e "AWS_SHARED_CREDENTIALS_FILE=/.aws/credentials" \
      -e "REDIS_DATA_ROOT=/var/lib/redis" \
      --env-file /root/secrets-backup.sh \
      swissrpg-backupper

# Miscellaneous

## Connecting to redis from the docker host

    docker run -it --network swissrpg-net --rm redis:7 redis-cli -h swissrpg-redis
