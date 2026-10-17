#!/bin/sh
# One view's (or notifier's) gate over the artifact cargo built: optimize
# (keeping the pre-wasm-opt bytes beside it), check the bare-WASM ABI, print
# the size. A fast build (`WASM_PROFILE=fast`, the Makefile's) is not
# optimized: its bytes never ship.
#   tools/view-gate.sh <view> <artifact> [notifier]
set -eu
view=$1
artifact=$2
kind=${3:-view}
here=$(dirname "$0")
if [ "${WASM_PROFILE:-release}" = release ]; then
    WASM_OPT="${WASM_OPT:-wasm-opt}" "$here/optimize-view.sh" "$artifact"
fi
python3 "$here/check-view-abi.py" --"$kind" "$artifact"
echo "$view  $(wc -c < "$artifact")"
