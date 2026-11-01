#!/usr/bin/env bash
# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
#
# Bitwig profiles for the conformance runs (doc/CONFORMANCE_AUTOMATION_PLAN.md): a copy of the
# Bitwig preferences (they hold the controllers and their ports) and the settings of the
# DrivenByMoss controllers.
#
#   bitwig_profile.sh save <name>     copy the current state into the profile (Bitwig must not run)
#   bitwig_profile.sh load <name>     copy the profile into place (Bitwig must not run)
#   bitwig_profile.sh list

set -euo pipefail

PROFILE_DIR="${DBM_PROFILE_DIR:-$HOME/.local/share/dbm-conformance/profiles}"
BITWIG_HOME="$HOME/.BitwigStudio"
VERSION="$(cat "$BITWIG_HOME/latest-launched-version.txt")"
PREFS="$BITWIG_HOME/prefs/$VERSION.prefs"
DBM_CONFIG="$HOME/.config/DrivenByMoss-Bitwig"

bitwig_running() {
    pgrep -f 'BitwigAudioEngin[e]|bitwig-studi[o]$' >/dev/null
}

require_stopped() {
    if bitwig_running; then
        echo "Bitwig is running, quit it first" >&2
        exit 1
    fi
}

case "${1:-}" in
    save)
        name="${2:?profile name}"
        require_stopped
        dir="$PROFILE_DIR/$name"
        mkdir -p "$dir/config"
        cp -p "$PREFS" "$dir/$VERSION.prefs"
        rm -f "$dir/config/"*.config
        cp -p "$DBM_CONFIG/"*.config "$dir/config/" 2>/dev/null || true
        echo "Saved $dir"
        ;;
    load)
        name="${2:?profile name}"
        require_stopped
        dir="$PROFILE_DIR/$name"
        [[ -f "$dir/$VERSION.prefs" ]] || { echo "No profile '$name' for Bitwig $VERSION in $PROFILE_DIR" >&2; exit 1; }
        cp -p "$dir/$VERSION.prefs" "$PREFS"
        mkdir -p "$DBM_CONFIG"
        cp -p "$dir/config/"*.config "$DBM_CONFIG/" 2>/dev/null || true
        echo "Loaded $dir"
        ;;
    list)
        ls -1 "$PROFILE_DIR" 2>/dev/null || true
        ;;
    *)
        sed -n '7,11p' "$0" | sed 's/^# \{0,1\}//'
        exit 2
        ;;
esac
