#!/usr/bin/env bash
# Builds a release that holds only the Milky Way: the sources of the `stars` and
# `galaxy_tracers` scale domains, without the researched structures (most of them are of the
# cosmic web) and without the satellite galaxies of `mcconnachie-2012`. The masers of Reid et
# al. 2019 are kept as objects without their arm memberships, as Univ Ingest builds a release
# that takes none of the researched structures.
#
# The release has its own state and build output under `.cache/univ-ingest/milky-way/`, so its
# ids never touch the real state's, and it shares the download cache with every other build.
#
# Usage: scripts/mw-build.sh [--publish] [univ-ingest build options, such as --offline]
#
#   --publish   Then publish the release to its own local D1 and R2, for
#               `UNIV_LOCAL_STATE=.cache/univ-ingest/milky-way/wrangler cargo xtask explore`.
#
# Environment: MW_DIR names another home for the state, build and local store;
# UNIV_INGEST_CACHE names another download cache.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

sources=(
  # Stars
  hipparcos-2007
  hipparcos-1997
  gaia-dr3-bright
  gaia-dr3-hipparcos2
  bailer-jones-2021
  gcns
  iau-star-names
  researched-star-distances
  edenhofer-2024-dust
  # The Galaxy's tracers
  zari-2021-luminous-hot-stars
  hunt-reffert-2023
  reid-2019-masers
  skowron-2025-cepheids
  li-2023-rr-lyrae
  baumgardt-vasiliev-2021
)

publish=false
build_args=()
for arg in "$@"; do
  case "$arg" in
    --publish) publish=true ;;
    *) build_args+=("$arg") ;;
  esac
done

dir="${MW_DIR:-.cache/univ-ingest/milky-way}"
state="$dir/state"
out="$dir/build"
store="$dir/wrangler"

source_args=()
for s in "${sources[@]}"; do
  source_args+=(--source "$s")
done

cargo build --release --quiet --package univ-ingest
ingest="$root/target/release/univ-ingest"

echo "Building a Milky Way release of ${#sources[@]} sources into $out" >&2
"$ingest" build "${source_args[@]}" --state "$state" --out "$out" ${build_args[@]+"${build_args[@]}"}

latest="$(find "$out/releases" -mindepth 1 -maxdepth 1 -type d -name '[0-9]*' -exec basename {} \; \
  | sort -n | tail -n 1)"
echo "Built release $latest in $out/releases/$latest" >&2

if [[ "$publish" == true ]]; then
  "$ingest" publish "$latest" --state "$state" --out "$out" --persist-to "$store"
  echo "Published release $latest to $store; explore it with:" >&2
  echo "  UNIV_LOCAL_STATE=$store cargo xtask explore" >&2
fi
