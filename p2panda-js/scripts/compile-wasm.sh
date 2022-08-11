#!/bin/bash

# Exit with error when any command fails
set -e

# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
# User Arguments
# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

# Run in 'development' or 'production' mode, check for NODE_ENV environment
# variable to set mode
node_env="${NODE_ENV:-development}"
MODE="${1:-$node_env}"

# Path to Rust project with Cargo.toml file
RUST_DIR="${2:-../p2panda-rs}"

# Path to temporary folder where compiled files are stored
TMP_DIR="${3:-./wasm}"

# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

# Names of the compilation targets
NODE_PROJECT=node
WEB_PROJECT=web

# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

echo "Compile '$RUST_DIR' and optimize WebAssembly in '$MODE' mode"

# Check if Rust project exists
if [[ ! -f $RUST_DIR/Cargo.toml ]]
then
    echo "△ Could not find Rust project in '$RUST_DIR'"
    exit 1
fi

# Check if tools are missing
ensure_installed () {
  if ! command -v $1 &> /dev/null
  then
      echo "△ '$1' needs to be installed first"
      exit 1
  fi
}

ensure_installed cargo
ensure_installed wasm-bindgen

# Finds and returns a .wasm file in a folder
find_wasm_file () {
  ls $1/*.wasm
}

# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

echo "◆ Compile WebAssembly"

if [[ $MODE == "development" ]]
then
    cargo --quiet build \
        --target=wasm32-unknown-unknown \
        --manifest-path $RUST_DIR/Cargo.toml
elif [[ $MODE == "production" ]]
then
    cargo --quiet build \
        --target=wasm32-unknown-unknown \
        --release \
        --manifest-path $RUST_DIR/Cargo.toml
else
    echo "△ Mode needs to be 'production' or 'development'"
    exit 1
fi

# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

RELEASE_WASM=$(find_wasm_file "$RUST_DIR/target/wasm32-unknown-unknown/release")

echo "◇ Adjust WebAssembly for 'node' target"
wasm-bindgen \
    --out-dir=$RUST_DIR/$NODE_PROJECT \
    --out-name=index \
    --target=nodejs \
    $RELEASE_WASM

echo "◇ Adjust WebAssembly for 'web' target"
wasm-bindgen \
    --out-dir=$RUST_DIR/$WEB_PROJECT \
    --out-name=index \
    --target=web \
    --omit-default-module-path \
    $RELEASE_WASM

# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

if [[ $MODE == "production" ]]
then
    ensure_installed wasm-opt

    echo "◌ Optimize 'node' target for speed"

    INPUT_WASM=$(find_wasm_file "$RUST_DIR/$NODE_PROJECT/$RUST_PROJECT")
    OUTPUT_WASM=$TMP_DIR/optimized-node.wasm
    wasm-opt -O -o $OUTPUT_WASM $INPUT_WASM
    mv $OUTPUT_WASM $INPUT_WASM

    echo "◌ Optimize 'web' target for size"

    INPUT_WASM=$(find_wasm_file "$RUST_DIR/$WEB_PROJECT/$RUST_PROJECT")
    OUTPUT_WASM=$TMP_DIR/optimized-web.wasm
    input_filesize=$(wc -c < $INPUT_WASM)
    wasm-opt -Os -o $OUTPUT_WASM $INPUT_WASM
    output_filesize=$(wc -c < $OUTPUT_WASM)
    mv $OUTPUT_WASM $INPUT_WASM
    echo "▷ file size before: $input_filesize / after: $output_filesize bytes"
else
    echo "◌ Skip optimizations in 'development' mode"
fi

# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

# Move compiled files over
rm -rf $TMP_DIR
mkdir -p $TMP_DIR
mv $RUST_DIR/$NODE_PROJECT $TMP_DIR
mv $RUST_DIR/$WEB_PROJECT $TMP_DIR
