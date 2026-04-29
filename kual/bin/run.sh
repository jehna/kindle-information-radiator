#!/bin/sh
EXTENSION_DIR=$(cd "$(dirname "$0")/.." && pwd)
LOG="$EXTENSION_DIR/weather.log"

trap 'lipc-set-prop com.lab126.powerd preventScreenSaver 0 2>/dev/null' EXIT INT TERM

{
    echo "=== $(date) ==="
    echo "step 1: preventScreenSaver"
    lipc-set-prop com.lab126.powerd preventScreenSaver 1 2>&1 || echo "  failed"
    echo "step 2: run binary"
    "$EXTENSION_DIR/bin/weather"
    echo "step 3: exit $?"
} >>"$LOG" 2>&1
