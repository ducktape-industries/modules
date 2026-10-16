#!/bin/sh
# One view's (or notifier's) release gate over the artifact cargo built:
# optimize (keeping the pre-wasm-opt bytes beside it), check the bare-WASM
# ABI, print the size.
#   tools/view-gate.sh <view> <artifact> [notifier]
set -eu
view=$1
artifact=$2
kind=${3:-view}
here=$(dirname "$0")
WASM_OPT="${WASM_OPT:-wasm-opt}" "$here/optimize-view.sh" "$artifact"
python3 "$here/check-view-abi.py" --"$kind" "$artifact"
echo "$view  $(wc -c < "$artifact")"
