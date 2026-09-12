export RUSTUP_TOOLCHAIN := "nightly"

build:
    cd programs/yield-tranche-market && quasar build

test:
    cd programs/yield-tranche-market && quasar test
