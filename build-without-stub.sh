#!/bin/sh

set -eu

workspace_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
client_dir="$workspace_dir/target/client/rust/yield_tranche_market-client"

mkdir -p "$client_dir/src"

cat > "$client_dir/Cargo.toml" << 'EOF'
[package]
name = "yield_tranche_market-client"
version = "0.1.0"
edition = "2021"
EOF

echo '// stub' > "$client_dir/src/lib.rs"

cd "$workspace_dir/programs/yield-tranche-market"
quasar build
