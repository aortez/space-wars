#!/usr/bin/env bash
# Collect independent, fixed-workload Clock runs. Never writes kiosk settings.
set -euo pipefail

bench_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
bench_client="$bench_root/target/release/engine-client"
bench_output="$bench_root/target/clock-benchmarks"
bench_repeats=3
bench_seconds=15
bench_warmup=2
bench_width=1024
bench_height=768
bench_scales=1,2
bench_cases=idle,falling,color-cycle,meltdown,duck,marquee,digit-slide
bench_renderer=raster
bench_recipe=clock-wave
bench_seed=7

bench_environment() {
    date -u '+collected_utc=%Y-%m-%dT%H:%M:%SZ'
    for bench_path in /sys/class/thermal/thermal_zone0/temp \
        /sys/class/thermal/thermal_zone0/type \
        /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor \
        /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq; do
        if [[ -r "$bench_path" ]]; then
            printf '%s=' "$bench_path"
            cat -- "$bench_path"
        fi
    done
}

usage() {
    printf '%s\n' 'Usage: ./benchmark-clock.sh [options]' \
        '  --client PATH     Built engine-client (default: target/release/engine-client)' \
        '  --output DIR      Parent for a new, unique report directory' \
        '  --repeats N       Independent process runs (default: 3)' \
        '  --seconds N       Simulated seconds per run (default: 15)' \
        '  --warmup N        Warm-up seconds, excluded from report (default: 2)' \
        '  --width N         Logical viewport width (default: 1024)' \
        '  --height N        Logical viewport height (default: 768)' \
        '  --scales LIST     Comma-separated raster scales (default: 1,2)' \
        '  --cases LIST      Comma-separated case IDs (default: all seven)' \
        '  --renderer NAME   raster or vector (default: raster)' \
        '  --recipe NAME     Fixed Marquee recipe (default: clock-wave)' \
        '  --seed N          Fixed seed (default: 7)' \
        'Reports are CPU-only: they exclude Slint drawing, display upload and vsync.'
}
while (( $# )); do
    case "$1" in
        --help|-h) usage; exit 0 ;;
        --client|--output|--repeats|--seconds|--warmup|--width|--height|--scales|--cases|--renderer|--recipe|--seed)
            if (( $# < 2 )); then printf 'Missing value for %s\n' "$1" >&2; exit 2; fi
            case "$1" in
                --client) bench_client=$2 ;; --output) bench_output=$2 ;;
                --repeats) bench_repeats=$2 ;; --seconds) bench_seconds=$2 ;;
                --warmup) bench_warmup=$2 ;; --width) bench_width=$2 ;;
                --height) bench_height=$2 ;; --scales) bench_scales=$2 ;;
                --cases) bench_cases=$2 ;; --renderer) bench_renderer=$2 ;;
                --recipe) bench_recipe=$2 ;; --seed) bench_seed=$2 ;;
            esac
            shift 2 ;;
        *) printf 'Unknown option: %s\n' "$1" >&2; exit 2 ;;
    esac
done
[[ -x "$bench_client" ]] || { printf 'Client not executable: %s\n' "$bench_client" >&2; exit 2; }
[[ "$bench_repeats" =~ ^[1-9][0-9]?$ ]] || { printf 'Repeats must be 1–99\n' >&2; exit 2; }
[[ "$bench_renderer" == raster || "$bench_renderer" == vector ]] || {
    printf 'Renderer must be raster or vector\n' >&2; exit 2;
}
[[ "$bench_scales" != ,* && "$bench_scales" != *, && "$bench_scales" != *,,* \
    && "$bench_cases" != ,* && "$bench_cases" != *, && "$bench_cases" != *,,* ]] || {
    printf 'Scale and case lists must not contain empty entries\n' >&2; exit 2;
}
IFS=, read -r -a bench_scale_list <<< "$bench_scales"
IFS=, read -r -a bench_case_list <<< "$bench_cases"
(( ${#bench_scale_list[@]} && ${#bench_case_list[@]} )) || exit 2
declare -A bench_seen_scales=() bench_seen_cases=()
for bench_scale in "${bench_scale_list[@]}"; do
    if ! [[ "$bench_scale" =~ ^[0-9]+([.][0-9]+)?$ ]] \
        || ! awk -v scale="$bench_scale" 'BEGIN { exit !(scale >= 0.1 && scale <= 3) }'; then
        printf 'Raster scale must be between 0.1 and 3: %s\n' "$bench_scale" >&2; exit 2
    fi
    if [[ -n "${bench_seen_scales[$bench_scale]:-}" ]]; then
        printf 'Duplicate scale: %s\n' "$bench_scale" >&2; exit 2
    fi
    bench_seen_scales[$bench_scale]=1
done
for bench_case in "${bench_case_list[@]}"; do
    case "$bench_case" in
        idle|falling|color-cycle|meltdown|duck|marquee|digit-slide) ;;
        *) printf 'Unknown case: %s\n' "$bench_case" >&2; exit 2 ;;
    esac
    if [[ -n "${bench_seen_cases[$bench_case]:-}" ]]; then
        printf 'Duplicate case: %s\n' "$bench_case" >&2; exit 2
    fi
    bench_seen_cases[$bench_case]=1
done

mkdir -p -- "$bench_output"
bench_run=$(mktemp -d "$bench_output/run.XXXXXXXX")
printf 'Reports: %s\n' "$bench_run"
{
    bench_environment
    uname -a
    sha256sum -- "$bench_client"
    printf 'renderer=%s viewport=%sx%s scales=%s cases=%s recipe=%s seed=%s seconds=%s warmup=%s repeats=%s\n' \
        "$bench_renderer" "$bench_width" "$bench_height" "$bench_scales" "$bench_cases" \
        "$bench_recipe" "$bench_seed" "$bench_seconds" "$bench_warmup" "$bench_repeats"
} > "$bench_run/metadata.txt"

for bench_case in "${bench_case_list[@]}"; do
    for bench_scale in "${bench_scale_list[@]}"; do
        for ((bench_repeat=1; bench_repeat<=bench_repeats; bench_repeat++)); do
            bench_name="$bench_case-$bench_renderer-scale$bench_scale-repeat$bench_repeat"
            printf 'Running %s\n' "$bench_name"
            bench_environment > "$bench_run/$bench_name.environment-before.txt"
            if ! "$bench_client" --scenario clock --benchmark-headless --seed "$bench_seed" \
                --config-dir "$bench_run/unused-settings" --renderer "$bench_renderer" \
                --raster-scale "$bench_scale" --clock-benchmark-case "$bench_case" \
                --clock-benchmark-recipe "$bench_recipe" \
                --benchmark-width "$bench_width" --benchmark-height "$bench_height" \
                --benchmark-seconds "$bench_seconds" --benchmark-warmup-seconds "$bench_warmup" \
                > "$bench_run/$bench_name.csv" 2> "$bench_run/$bench_name.log"; then
                printf 'Benchmark failed; partial reports retained in %s\n' "$bench_run" >&2
                cat -- "$bench_run/$bench_name.log" >&2
                exit 1
            fi
            bench_environment > "$bench_run/$bench_name.environment-after.txt"
            awk -F, '
                NR == 1 { for (i=1;i<=NF;i++) column[$i]=i; next }
                { n=$(column["frames"]); count+=n;
                  step+=n*$(column["avg_step_ms"]); scene+=n*$(column["avg_render_ms"]);
                  prepare+=n*$(column["avg_present_ms"]); total+=n*$(column["avg_total_ms"]);
                  p=$(column["p95_total_ms"]); if (p>worst) worst=p }
                END { if (count) printf "  mean ms/frame: step=%.3f scene=%.3f prepare=%.3f total=%.3f; worst row p95=%.3f\n", step/count,scene/count,prepare/count,total/count,worst }
            ' "$bench_run/$bench_name.csv"
        done
    done
done
printf 'Complete: %s\n' "$bench_run"
