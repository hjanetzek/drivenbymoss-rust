#!/usr/bin/env bash
# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
#
# Unattended conformance runs (doc/CONFORMANCE_AUTOMATION_PLAN.md): for each setup load its Bitwig
# profile, start Bitwig, run all scenario files of tests/conformance/<setup>/, copy the replays,
# kill Bitwig. The user's own Bitwig state is restored at the end (also on abort).
#
#   conformance_all.sh [--no-replays] [setup ...]     (all setups with a profile if none given)
#
# Environment: BITWIG (the binary), DBM_PROFILE_DIR, JAVA_PORT, RUST_PORT.

set -euo pipefail

TOOLS="$(cd "$(dirname "$0")" && pwd)"
RUST="$(cd "$TOOLS/../../.." && pwd)"
PROFILE_DIR="${DBM_PROFILE_DIR:-$HOME/.local/share/dbm-conformance/profiles}"
BITWIG="${BITWIG:-$HOME/.local/opt/bitwig-studio-jdk25/bitwig-studio}"
JAVA_PORT="${JAVA_PORT:-VirMIDI 5-2}"
RUST_PORT="${RUST_PORT:-VirMIDI 5-1}"
BITWIG_HOME="$HOME/.BitwigStudio"
VERSION="$(cat "$BITWIG_HOME/latest-launched-version.txt")"
OUT="$RUST/target/conformance"
NIHOST="$RUST/target/ci/ni_host/dbm-nihost"

copy_replays=1
if [[ "${1:-}" == "--no-replays" ]]; then
    copy_replays=0
    shift
fi
setups=("$@")
if [[ ${#setups[@]} -eq 0 ]]; then
    for dir in "$PROFILE_DIR"/*/; do
        name="$(basename "$dir")"
        [[ -d "$RUST/tests/conformance/$name" ]] && setups+=("$name")
    done
fi
[[ ${#setups[@]} -gt 0 ]] || { echo "No setups (profiles in $PROFILE_DIR)" >&2; exit 1; }

bitwig_running() {
    pgrep -f 'BitwigAudioEngin[e]|bitwig-studi[o]$' >/dev/null
}

if bitwig_running; then
    echo "Bitwig is running, quit it first" >&2
    exit 1
fi

mkdir -p "$OUT"
# The library which the Rust extension loads in dev mode
(cd "$RUST" && cargo build -q)
backup="$(mktemp -d "${TMPDIR:-/tmp}/dbm-conformance-backup.XXXXXX")"
nihost_was_running=0
bitwig_pid=""

# The user's state: the preferences and the DrivenByMoss settings (unsaved projects are not kept,
# Bitwig on this computer is only for development and tests)
cp -p "$BITWIG_HOME/prefs/$VERSION.prefs" "$backup/"
cp -rp "$HOME/.config/DrivenByMoss-Bitwig" "$backup/config" 2>/dev/null || true

stop_bitwig() {
    [[ -n "$bitwig_pid" ]] || return 0
    kill -TERM -- "-$bitwig_pid" 2>/dev/null || true
    for _ in $(seq 10); do
        bitwig_running || break
        sleep 1
    done
    kill -KILL -- "-$bitwig_pid" 2>/dev/null || true
    pkill -KILL -f 'BitwigAudioEngin[e]|bitwig-studi[o]$' 2>/dev/null || true
    bitwig_pid=""
}

# A killed Bitwig leaves its projects for a restore (the dialog "Recover Projects"), their undo
# history and view settings, and its lock files
clean_projects() {
    rm -rf "$BITWIG_HOME/temp-projects" "$BITWIG_HOME/undo-history" "$BITWIG_HOME/view-settings"
    rm -f "$BITWIG_HOME/lock" "$BITWIG_HOME/server-port"
}

# True if the scenario file had no difference and no failed expectation in the run log (its
# sections "--- <file>: java on ...", "--- <file>: rust on ...", "--- <file>: compare")
file_ok() {
    awk -v file="$1" '
        /^--- / { inside = index($0, "--- " file ":") == 1 }
        inside && /^(DIFF|MISSING|FAIL) / { bad = 1 }
        END { exit bad }' "$2"
}

restore() {
    stop_bitwig
    cp -p "$backup/$VERSION.prefs" "$BITWIG_HOME/prefs/"
    if [[ -d "$backup/config" ]]; then
        cp -p "$backup/config/"*.config "$HOME/.config/DrivenByMoss-Bitwig/" 2>/dev/null || true
    fi
    clean_projects
    if [[ $nihost_was_running -eq 1 && -x "$NIHOST" ]]; then
        setsid nohup "$NIHOST" --virmidi >"$OUT/nihost.log" 2>&1 </dev/null &
    fi
    rm -rf "$backup"
}
trap restore EXIT

# dbm-nihost connects to the same snd-virmidi devices as the runner
if pgrep -x dbm-nihost >/dev/null; then
    nihost_was_running=1
    pkill -x dbm-nihost
fi

summary=()
for setup in "${setups[@]}"; do
    echo "=== $setup"
    if [[ ! -f "$PROFILE_DIR/$setup/$VERSION.prefs" ]]; then
        summary+=("$setup: no profile")
        continue
    fi
    "$TOOLS/bitwig_profile.sh" load "$setup"
    clean_projects

    log="$OUT/bitwig-$setup.log"
    DBM_OSC_CONFORMANCE=1 DBM_DEV_LIBRARY="$RUST/target/debug/libdrivenbymoss.so" \
        setsid "$BITWIG" >"$log" 2>&1 </dev/null &
    bitwig_pid=$!

    if ! uv run --quiet --directory "$TOOLS" conformance_runner.py wait --timeout 180 --port "$JAVA_PORT" --port "$RUST_PORT"; then
        summary+=("$setup: Bitwig not ready (see $log)")
        stop_bitwig
        continue
    fi
    # The controllers start a moment after the OSC controller
    sleep 10

    start="$(date +%s)"
    scenarios=("$RUST/tests/conformance/$setup/"*.toml)
    runlog="$OUT/run-$setup.log"
    if uv run --quiet --directory "$TOOLS" conformance_runner.py pair "${scenarios[@]}" \
        --java-port "$JAVA_PORT" --rust-port "$RUST_PORT" --fresh-project 8 --restart 15 --out "$OUT" 2>&1 | tee "$runlog"; then
        result="expectations ok"
    else
        result="expectations failed"
    fi
    same=$(grep -c '^same' "$runlog" || true)
    different=$(grep -c '^DIFF\|^MISSING' "$runlog" || true)

    # The replays of a scenario file only from a clean run of it, and only if they replay
    copied=()
    skipped=()
    if [[ $copy_replays -eq 1 ]]; then
        replay="$RUST/tests/conformance/$setup/replay"
        mkdir -p "$replay"
        for scenario in "${scenarios[@]}"; do
            file="$(basename "$scenario" .toml)"
            if ! file_ok "$file" "$runlog"; then
                skipped+=("$file (differences)")
                continue
            fi
            for rec in "$OUT/recordings/"*"-$file-rust.rec"; do
                [[ -f "$rec" && $(stat -c %Y "$rec") -ge $start ]] || continue
                run="$(basename "$rec" -rust.rec)"
                files=("$replay/$run-rust.rec" "$replay/$run-java.leds" "$replay/$run-java.known")
                cp "$rec" "$replay/"
                cp "$OUT/$run-java.leds" "$OUT/$run-java.known" "$replay/"
                if (cd "$RUST" && DBM_REPLAY="$replay/$run-rust.rec" cargo test -q --test conformance_replay >/dev/null 2>&1); then
                    copied+=("$file")
                else
                    for f in "${files[@]}"; do
                        git -C "$RUST" checkout -q -- "$f" 2>/dev/null || rm -f "$f"
                    done
                    skipped+=("$file (does not replay)")
                fi
            done
        done
    fi

    summary+=("$setup: $same same, $different different, $result; replays copied: ${copied[*]:-none}; not copied: ${skipped[*]:-none}")
    stop_bitwig
done

echo "=== Replays"
if (cd "$RUST" && cargo test -q --test conformance_replay); then
    summary+=("replays: ok")
else
    summary+=("replays: failed")
fi

echo "=== Summary"
printf '%s\n' "${summary[@]}"
