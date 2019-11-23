:: Start the container
docker start swissrpg-compile-container
:: Run the build command inside of the container
:: TODO: replace 'cp /tmp/target/release...' with Cargo's --out-dir option once it is stable
docker exec -it swissrpg-compile-container /bin/bash -lc "cd /opt/app && cargo build --features \"bottest\" --target-dir /tmp/target && cp /tmp/target/debug/swissrpg-app /opt/swissrpg-app-test"