#!/bin/bash
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
num_containers=$(docker container ls --all --quiet --filter name=swissrpg-compile-container | wc -l)
if [ $num_containers -lt 1 ]
then
    # Create the container if it doesn't exist yet
    docker create -it --volume "${SCRIPT_DIR}/..":/opt --name=swissrpg-compile-container swissrpg-bot
fi
# Start the container
docker start swissrpg-compile-container
# Run the build command inside of the container
# TODO: replace 'cp /tmp/target/release...' with Cargo's --out-dir option once it is stable
docker exec -it swissrpg-compile-container /bin/bash -lc 'cd /opt/app && cargo build --features "bottest" --release --target-dir /tmp/target && cp /tmp/target/release/swissrpg-app /opt/swissrpg-app-test'