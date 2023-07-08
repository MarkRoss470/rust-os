#!/bin/sh
cd $1/../kernel-builder
cargo run -- --run --debug=$1/.vscode/log.txt 
