#!/bin/sh
. ./secrets.sh
export BOT_ENV=test
./target/debug/swissrpg-app
