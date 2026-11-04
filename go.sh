#!/usr/bin/env bash
# Build and run the Space-Wars client on this machine.
#
# Usage:
#   ./go.sh                        # Release build, then open the launcher.
#   ./go.sh spacewars              # Release build, then start a scenario directly.
#   ./go.sh --list                 # List the scenarios the client knows.
#   ./go.sh clock --seed 7         # Client options pass through to engine-client.
#   ./go.sh --debug pizza          # Debug build instead of release.
#   ./go.sh -- --help              # Show engine-client's own options.
#   source <(./go.sh --completion) # Tab completion for scenarios and options.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PACKAGE="engine-client"
PROFILE="release"
SCENARIO=""
ACTION="run"
BUILD=true
FRESH_CONFIG=false
CARGO_EXTRA_ARGS=()
CLIENT_ARGS=()

# This script's own options, for tab completion.
SCRIPT_OPTIONS="-l --list -s --scenario -d --debug -p --profile -n --no-build -b --build-only
    --fresh --features --completion -h --help"

# engine-client options that the script forwards. Flags take no value; the
# value options consume the following word so a scenario name after them is
# never mistaken for their value.
CLIENT_FLAGS="--dev --debug-render --benchmark --benchmark-headless --fullscreen --windowed --kiosk --touch-test"
CLIENT_VALUE_OPTIONS="--seed --rom --debug-triangles --benchmark-seconds --benchmark-warmup-seconds
    --benchmark-width --benchmark-height --clock-benchmark-case --clock-benchmark-recipe
    --benchmark-report --pizza-benchmark-balls --pizza-benchmark-backend --pizza-benchmark-gravity
    --pizza-benchmark-workload --config-dir --renderer --raster-scale"

usage() {
    cat <<'USAGE'
Space-Wars local runner

Builds engine-client (release by default) and runs it on this machine.

Usage:
  ./go.sh [options] [scenario] [-- <engine-client options>]

A scenario may be abbreviated: a unique prefix or substring of its id is
enough, so "learned" starts spacewars-terrain-learned-flight. Without a
scenario the launcher opens.

Options:
  -l, --list            List the available scenarios and exit
  -s, --scenario <id>   Scenario to start directly (same as the positional form)
  -d, --debug           Build and run the dev profile instead of release
  -p, --profile <name>  Build and run a named cargo profile (release, dev, ci)
  -n, --no-build        Run the existing binary without invoking cargo
  -b, --build-only      Build, print the binary path, and exit without running
      --fresh           Run with a temporary, empty settings directory
      --features <list> Extra cargo features for the build
      --completion      Print bash completion for this script and exit
  -h, --help            Show this help

engine-client options such as --seed <n>, --dev, --fullscreen, --windowed,
--renderer <name>, --rom <path> and --config-dir <path> may be given anywhere;
they are forwarded. Anything after -- is forwarded untouched.

Examples:
  ./go.sh
  ./go.sh spacewars --seed 42
  ./go.sh --list
  ./go.sh --debug terrain-lab
  ./go.sh --fresh --windowed clock
  ./go.sh --no-build learned
  ./go.sh --rom data/roms/game.nes
  ./go.sh clock --benchmark-headless --benchmark-seconds 5
  RUST_LOG=debug ./go.sh pizza

Tab completion (scenario names, options and their values), either of:
  source <(./go.sh --completion)       # In ~/.bashrc; always current.
  ./go.sh --completion > ~/.local/share/bash-completion/completions/go.sh

Environment:
  RUST_LOG              Overrides the saved log level for this run.
  SPACEWARS_CONFIG_DIR  Settings directory; --fresh and --config-dir override it.
  CARGO_TARGET_DIR      Build output directory (default: <repo>/target).
USAGE
}

fail() {
    printf 'Error: %s\n' "$*" >&2
    exit 1
}

# True when $1 is one of the whitespace-separated words in $2.
word_in() {
    [[ " ${2//[$'\n']/ } " == *" $1 "* ]]
}

while (( $# > 0 )); do
    case "$1" in
        -l|--list)
            ACTION="list"
            shift
            ;;
        -s|--scenario)
            (( $# >= 2 )) || fail "Missing value for $1"
            SCENARIO="$2"
            shift 2
            ;;
        --scenario=*)
            SCENARIO="${1#*=}"
            shift
            ;;
        -d|--debug)
            PROFILE="dev"
            shift
            ;;
        -p|--profile)
            (( $# >= 2 )) || fail "Missing value for $1"
            PROFILE="$2"
            shift 2
            ;;
        --profile=*)
            PROFILE="${1#*=}"
            shift
            ;;
        -n|--no-build)
            BUILD=false
            shift
            ;;
        -b|--build-only)
            ACTION="build"
            shift
            ;;
        --fresh)
            FRESH_CONFIG=true
            shift
            ;;
        --features)
            (( $# >= 2 )) || fail "Missing value for $1"
            CARGO_EXTRA_ARGS+=(--features "$2")
            shift 2
            ;;
        --features=*)
            CARGO_EXTRA_ARGS+=(--features "${1#*=}")
            shift
            ;;
        --completion)
            ACTION="completion"
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        --)
            shift
            CLIENT_ARGS+=("$@")
            break
            ;;
        --*=*)
            CLIENT_ARGS+=("$1")
            shift
            ;;
        --*)
            if word_in "$1" "$CLIENT_FLAGS"; then
                CLIENT_ARGS+=("$1")
                shift
            elif word_in "$1" "$CLIENT_VALUE_OPTIONS"; then
                (( $# >= 2 )) || fail "Missing value for $1"
                CLIENT_ARGS+=("$1" "$2")
                shift 2
            else
                fail "Unknown option: $1 (put engine-client options after --)"
            fi
            ;;
        -*)
            fail "Unknown option: $1 (put engine-client options after --)"
            ;;
        *)
            [[ -z "$SCENARIO" ]] || fail "Scenario given twice: $SCENARIO and $1"
            SCENARIO="$1"
            shift
            ;;
    esac
done

# Cargo places the built-in dev and test profiles under target/debug and the
# bench profile under target/release; custom profiles use their own name.
case "$PROFILE" in
    dev|test) PROFILE_DIR="debug" ;;
    bench) PROFILE_DIR="release" ;;
    *) PROFILE_DIR="$PROFILE" ;;
esac
BINARY="${CARGO_TARGET_DIR:-$SCRIPT_DIR/target}/$PROFILE_DIR/$PACKAGE"

build() {
    [[ "$BUILD" == true ]] || return 0
    command -v cargo >/dev/null 2>&1 || fail "cargo is required"
    local started=$SECONDS
    (
        cd "$SCRIPT_DIR"
        cargo build --locked --profile "$PROFILE" -p "$PACKAGE" "${CARGO_EXTRA_ARGS[@]}"
    )
    printf 'Built %s (%s profile) in %ds.\n' "$BINARY" "$PROFILE" "$((SECONDS - started))" >&2
}

require_binary() {
    [[ -x "$BINARY" ]] || fail "No binary at $BINARY (build first, or drop --no-build)"
}

# engine-client has no list flag, but it names every registered scenario when
# it rejects an unknown one. That check runs before any window opens and before
# settings are written, and the probe uses its own empty settings directory.
list_scenarios() {
    local probe output
    probe="$(mktemp -d)"
    output="$(RUST_LOG=error "$BINARY" --scenario go-sh-list-probe --config-dir "$probe" 2>&1 || true)"
    rm -rf -- "$probe"
    if [[ "$output" != *"available scenarios:"* ]]; then
        printf '%s\n' "$output" >&2
        fail "Could not read the scenario list from $BINARY"
    fi
    output="${output##*available scenarios: }"
    printf '%s\n' "$output" | tr ',' '\n' | sed -e 's/^ *//' -e '/^$/d'
}

# Print the scenario id for a possibly abbreviated name. An exact id wins,
# then a unique prefix, then a unique substring.
resolve_scenario() {
    local requested="$1" name
    local -a all matches
    mapfile -t all < <(list_scenarios)
    (( ${#all[@]} )) || fail "No scenarios were listed by $BINARY"
    for name in "${all[@]}"; do
        if [[ "$name" == "$requested" ]]; then
            printf '%s\n' "$name"
            return 0
        fi
    done
    mapfile -t matches < <(printf '%s\n' "${all[@]}" | grep -- "^$requested" || true)
    if (( ${#matches[@]} == 0 )); then
        mapfile -t matches < <(printf '%s\n' "${all[@]}" | grep -F -- "$requested" || true)
    fi
    case ${#matches[@]} in
        1)
            printf '%s\n' "${matches[0]}"
            return 0
            ;;
        0)
            printf 'Unknown scenario "%s". Available scenarios:\n' "$requested" >&2
            printf '  %s\n' "${all[@]}" >&2
            ;;
        *)
            printf 'Scenario "%s" is ambiguous; it matches:\n' "$requested" >&2
            printf '  %s\n' "${matches[@]}" >&2
            ;;
    esac
    return 1
}

# Collapse a multi-line word list into one space-separated line.
one_line() {
    local words
    words="$(printf '%s' "$*" | tr -s '[:space:]' ' ')"
    words="${words# }"
    printf '%s\n' "${words% }"
}

# The cargo profiles this script can build: the built-in ones plus every
# profile declared in Cargo.toml.
cargo_profiles() {
    local names="release dev" name
    while IFS= read -r name; do
        [[ " $names " == *" $name "* ]] || names+=" $name"
    done < <(sed -n 's/^\[profile\.\([A-Za-z0-9_-]*\)\].*/\1/p' "$SCRIPT_DIR/Cargo.toml")
    printf '%s\n' "$names"
}

# The feature names declared by the client crate.
client_features() {
    awk '
        /^\[features\]/ { active = 1; next }
        /^\[/ { active = 0 }
        active && /^[A-Za-z0-9_-]+ *=/ { sub(/ *=.*/, ""); printf "%s%s", (count++ ? " " : ""), $0 }
        END { print "" }
    ' "$SCRIPT_DIR/crates/$PACKAGE/Cargo.toml"
}

# Print a bash completion function for this script. Scenario names and the
# values of engine-client options are read from the built client at completion
# time, so they follow rebuilds; the option lists are fixed when generated.
print_completion() {
    cat <<'COMPLETION'
# Bash completion for go.sh, the Space-Wars local runner. Install with one of:
#   source <(./go.sh --completion)
#   ./go.sh --completion > ~/.local/share/bash-completion/completions/go.sh
_go_sh_complete() {
COMPLETION
    printf "    local script_options='%s'\n" "$(one_line "$SCRIPT_OPTIONS")"
    printf "    local client_options='%s'\n" "$(one_line "$CLIENT_FLAGS" "$CLIENT_VALUE_OPTIONS")"
    printf "    local client_value_options='%s'\n" "$(one_line "$CLIENT_VALUE_OPTIONS")"
    printf "    local profiles='%s'\n" "$(cargo_profiles)"
    printf "    local features='%s'\n" "$(client_features)"
    cat <<'COMPLETION'
    local cur="${COMP_WORDS[COMP_CWORD]}" prev="${COMP_WORDS[COMP_CWORD-1]}"
    local cmd="${COMP_WORDS[0]}" word values i
    local after_separator=false has_scenario=false takes_value
    COMPREPLY=()

    # Bash splits --option=value at the "=", so look past it.
    if [[ "$cur" == "=" ]]; then
        cur=""
    elif [[ "$prev" == "=" ]] && (( COMP_CWORD >= 2 )); then
        prev="${COMP_WORDS[COMP_CWORD-2]}"
    fi

    # Work out whether a scenario is already on the line and whether the
    # cursor sits after the -- separator, skipping the values of options.
    for ((i = 1; i < COMP_CWORD; i++)); do
        word="${COMP_WORDS[i]}"
        takes_value=false
        case "$word" in
            --) after_separator=true; break ;;
            =) continue ;;
            -s|--scenario) has_scenario=true; takes_value=true ;;
            -p|--profile|--features) takes_value=true ;;
            -*) [[ " $client_value_options " == *" $word "* ]] && takes_value=true ;;
            *) has_scenario=true ;;
        esac
        if [[ "$takes_value" == true ]]; then
            [[ "${COMP_WORDS[i+1]-}" == "=" ]] && ((i++))
            ((i++))
        fi
    done

    case "$prev" in
        -s|--scenario)
            mapfile -t COMPREPLY < <(compgen -W "$("$cmd" --list --no-build 2>/dev/null)" -- "$cur")
            return
            ;;
        -p|--profile)
            mapfile -t COMPREPLY < <(compgen -W "$profiles" -- "$cur")
            return
            ;;
        --features)
            mapfile -t COMPREPLY < <(compgen -W "$features" -- "$cur")
            return
            ;;
        --rom|--config-dir|--benchmark-report|*-output|*-device|*-font-*)
            compopt -o filenames 2>/dev/null
            mapfile -t COMPREPLY < <(compgen -f -- "$cur")
            return
            ;;
        --*)
            # engine-client's help says whether an option takes a value and
            # lists the values it accepts; a flag falls through.
            if values="$("$cmd" --no-build -- --help 2>/dev/null | awk -v option="$prev" '
                found && $1 ~ /^-/ { exit }
                $1 == option { found = 1; takes_value = ($2 ~ /^</) }
                found && match($0, /\[possible values: [^]]*\]/) {
                    values = substr($0, RSTART + 18, RLENGTH - 19)
                    gsub(/,/, "", values)
                    print values
                }
                END { exit !takes_value }
            ')"; then
                mapfile -t COMPREPLY < <(compgen -W "$values" -- "$cur")
                return
            fi
            ;;
    esac

    if [[ "$cur" == -* ]]; then
        if [[ "$after_separator" == true ]]; then
            values="$("$cmd" --no-build -- --help 2>/dev/null | grep -oE -- '--[a-z0-9-]+' | sort -u)"
        else
            values="$script_options $client_options"
        fi
        mapfile -t COMPREPLY < <(compgen -W "$values" -- "$cur")
    elif [[ "$after_separator" == false && "$has_scenario" == false ]]; then
        mapfile -t COMPREPLY < <(compgen -W "$("$cmd" --list --no-build 2>/dev/null)" -- "$cur")
    fi
}
# A compspec on the bare name also serves ./go.sh and absolute paths.
complete -F _go_sh_complete go.sh
COMPLETION
}

if [[ "$ACTION" == completion ]]; then
    print_completion
    exit 0
fi

build

case "$ACTION" in
    build)
        require_binary
        printf '%s\n' "$BINARY"
        exit 0
        ;;
    list)
        require_binary
        list_scenarios
        exit 0
        ;;
esac

require_binary

if [[ -n "$SCENARIO" ]]; then
    SCENARIO="$(resolve_scenario "$SCENARIO")" || exit 1
    CLIENT_ARGS=(--scenario "$SCENARIO" "${CLIENT_ARGS[@]}")
fi

if [[ "$FRESH_CONFIG" == true ]]; then
    FRESH_DIR="$(mktemp -d -t spacewars-fresh.XXXXXX)"
    # A signal that ends the client (Ctrl-C, timeout) must still remove the
    # directory, so the signal traps exit and let the EXIT trap clean up.
    trap 'rm -rf -- "$FRESH_DIR"' EXIT
    trap 'exit 129' HUP
    trap 'exit 130' INT
    trap 'exit 143' TERM
    CLIENT_ARGS+=(--config-dir "$FRESH_DIR")
    printf 'Using a temporary settings directory: %s\n' "$FRESH_DIR" >&2
fi

printf 'Running: %s' "$BINARY" >&2
if (( ${#CLIENT_ARGS[@]} )); then
    printf ' %q' "${CLIENT_ARGS[@]}" >&2
fi
printf '\n' >&2

# Relative paths such as --rom data/roms/game.nes resolve from the caller's
# directory, so the script does not change directory before running.
"$BINARY" "${CLIENT_ARGS[@]}"
