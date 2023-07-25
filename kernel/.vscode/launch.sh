#!/bin/sh
cd $1/../kernel-builder
# Hardcoded path to nightly cargo rather than just 'cargo'
# To prevent the default cargo binary from syncing nightly version every run.
# This speeds up launches significantly and also allows running offline or on a bad connection.
~/.rustup/toolchains/nightly-2023-06-20-x86_64-unknown-linux-gnu/bin/cargo run -- --run --debug=$1/.vscode/log.txt 
