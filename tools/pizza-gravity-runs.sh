#!/usr/bin/env bash
# Paired point-mass versus bounded-interior Pizza gravity runs.
# See docs/pizza-bounded-gravity.md for the build commands and summary step.
#
# Usage: pizza-gravity-runs.sh BASELINE_LAB CANDIDATE_LAB BASELINE_DIAG CANDIDATE_DIAG OUT_DIR
#
# The diagnostic labs are built with `--features ccd-diagnostics`. Uninstrumented
# timing runs are pinned to PIZZA_CPU (default 6) when taskset is available.
set -euo pipefail

if [[ $# -ne 5 ]]; then
  echo "usage: $0 BASELINE_LAB CANDIDATE_LAB BASELINE_DIAG CANDIDATE_DIAG OUT_DIR" >&2
  exit 2
fi
declare -A lab=([baseline]=$1 [candidate]=$2)
declare -A diag=([baseline]=$3 [candidate]=$4)
out=$5
mkdir -p "$out"
pin=()
if command -v taskset > /dev/null; then
  pin=(taskset -c "${PIZZA_CPU:-6}")
fi

# Deterministic work: instrumented CCD counts with the field On, plain runs Off.
for seed in $(seq 1 20); do
  for side in baseline candidate; do
    "${diag[$side]}" 500 500 1200 "$seed" 1024 768 on ordinary \
      > "$out/ordinary-on-s$seed-$side-diag.json"
    "${lab[$side]}" 500 500 1200 "$seed" 1024 768 off ordinary \
      > "$out/ordinary-off-s$seed-$side.json"
  done
done

# Host-dependent timing: uninstrumented runs in alternating order. Each seed
# evolves differently, so the cross-seed mean is the expected update cost.
for seed in $(seq 1 20); do
  order=(baseline candidate)
  if ((seed % 2 == 0)); then
    order=(candidate baseline)
  fi
  for side in "${order[@]}"; do
    "${pin[@]}" "${lab[$side]}" 500 500 1200 "$seed" 1024 768 on ordinary \
      > "$out/ordinary-on-s$seed-$side.json"
  done
done
for rep in 1 2 3 4 5; do
  order=(baseline candidate)
  if ((rep % 2 == 0)); then
    order=(candidate baseline)
  fi
  for side in "${order[@]}"; do
    "${pin[@]}" "${lab[$side]}" 500 500 1200 7 1024 768 on ordinary \
      > "$out/ordinary-on-s7-$side-r$rep.json"
    for n in 500 1000 2500 5000; do
      "${pin[@]}" "${lab[$side]}" "$n" "$n" 240 7 1024 768 off mixed \
        > "$out/mixed-$n-$side-r$rep.json"
    done
  done
done
