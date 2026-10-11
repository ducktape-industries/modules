#!/bin/sh
# Publishes this commit's programs, describe modules and views as the GitHub release
# wasm-<commit>: the bytes a founder downloads (qa's kit) instead of building them.
#   tools/publish-wasm.sh <artifact>...   (run by `make wasm-publish`)
set -eu
fail() { echo "wasm-publish: $*" >&2; exit 1; }
host=$(rustc -vV | sed -n 's/^host: //p')
[ "$host" = x86_64-unknown-linux-gnu ] || fail "the reference host is x86_64-unknown-linux-gnu, this is $host"
[ -z "$(git status --porcelain --untracked-files=no)" ] || fail "the checkout has uncommitted changes"
commit=$(git rev-parse HEAD)
[ -n "$(git branch --remotes --contains "$commit")" ] || fail "$commit is on no branch of origin"
tag="wasm-$commit"
! gh release view "$tag" >/dev/null 2>&1 || fail "$tag is published already"
"${MAKE:-make}" wasm-modules
out="$BUILD_TARGET/publish"
rm -rf "$out" && mkdir -p "$out"
(cd "$RELEASE" && sha256sum "$@" > "$out/SHA256SUMS" && tar czf "$out/modules-wasm.tar.gz" "$@" -C "$out" SHA256SUMS)
gh release create "$tag" "$out/modules-wasm.tar.gz" --target "$commit" --latest=false \
  --title "wasm $(git rev-parse --short HEAD)" \
  --notes "Every program, describe module and view at $commit, built by \`make wasm-publish\` on x86_64-unknown-linux-gnu (wasm-opt 132). SHA256SUMS is inside the archive."
echo "published $tag"
