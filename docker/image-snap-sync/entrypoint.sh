#!/usr/bin/env sh
set -e

# Do updates
apt-get update
apt-get -y upgrade
apt-get clean
rm -rf /var/lib/apt/lists/*

# Drop privileges
exec gosu runner:runner "$@"
