# On new and old server

Make sure that the correct version of the postgres client is installed (pg_dump needs to match the target database version, and the current database version must not be newer).

    sudo apt install curl ca-certificates gnupg

    curl https://www.postgresql.org/media/keys/ACCC4CF8.asc | gpg --dearmor | sudo tee /etc/apt/trusted.gpg.d/apt.postgresql.org.gpg >/dev/null

    sudo sh -c 'echo "deb http://apt.postgresql.org/pub/repos/apt $(lsb_release -cs)-pgdg main" > /etc/apt/sources.list.d/pgdg.list'

    sudo apt update

    sudo apt install postgresql-client-14

# On old server

Back up the database:

    /usr/lib/postgresql/14/bin/pg_dumpall -h localhost -U postgres -f swissrpg-dump.out

# On new server

Load the database content (assumes a fresh cluster):

    /usr/lib/postgresql/14/bin/psql -h localhost -U postgres -f swissrpg-dump.out postgres

# Redis

Copy dump.rdb and appendonly files from old to new server (detailed instructions missing).
