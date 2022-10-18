#!/bin/bash
# To be run from the swissrpg-backupper Docker container
# Back up Redis database
# This is not the proper way of backing up the AOF files, but good enough for our use case I think
/usr/local/bin/aws s3 sync "${REDIS_DATA_ROOT}/appendonlydir" "s3://${S3_BUCKET_ID}"
/usr/local/bin/aws s3 cp "${REDIS_DATA_ROOT}/dump.rdb" "s3://${S3_BUCKET_ID}"
# Back up Postgres database
BACKUP_FILENAME="/tmp/postgresql_bot.pgdump"
PGPASSWORD="${PSQL_BOT_PW}" pg_dump -h swissrpg-postgres -U bot -Fc bot | ( umask u=rw,g=,o=; cat > "${BACKUP_FILENAME}"; )
/usr/local/bin/aws s3 cp "${BACKUP_FILENAME}" "s3://${S3_BUCKET_ID}"
rm "${BACKUP_FILENAME}"
# Back up Postgres dev database
BACKUP_FILENAME="/tmp/postgresql_bottest.pgdump"
PGPASSWORD="${PSQL_BOTTEST_PW}" pg_dump -h swissrpg-postgres -U bottest -Fc bottest | ( umask u=rw,g=,o=; cat > "${BACKUP_FILENAME}"; )
/usr/local/bin/aws s3 cp "${BACKUP_FILENAME}" "s3://${S3_BUCKET_ID}"
rm "${BACKUP_FILENAME}"
