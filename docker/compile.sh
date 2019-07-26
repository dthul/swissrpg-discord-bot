#!/bin/bash
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
num_cont=$(docker container ls -f name=swissrpg-compile-container -q | wc -l)
if [ $num_cont -lt 1 ]
then
    # Create the container if it doesn't exist yet
    docker create -it -v "${DIR}/..":/opt --name=swissrpg-compile-container swissrpg-bot
fi
# Start the container
docker start swissrpg-compile-container
# Run the build command inside of the container
# TODO: replace 'cp /tmp/target/release...' with Cargo's --out-dir option once it is stable
docker exec -it swissrpg-compile-container /bin/bash -lc 'cd /opt && cargo build --release --target-dir /tmp/target && cp /tmp/target/release/swissrpg-discord-bot /opt/'