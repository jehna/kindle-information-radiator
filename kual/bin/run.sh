#!/bin/sh
EXTENSION_DIR=$(cd "$(dirname "$0")/.." && pwd)
LOG="$EXTENSION_DIR/weather.log"

restore() {
    echo "restore: preventScreenSaver off, resume volumd, start framework"
    lipc-set-prop com.lab126.powerd preventScreenSaver 0 2>/dev/null || true
    killall -CONT volumd 2>/dev/null || true
    cd / && start lab126_gui 2>/dev/null || true
}
trap restore EXIT INT TERM

{
    echo "=== $(date) ==="

    echo "step 1: preventScreenSaver"
    lipc-set-prop com.lab126.powerd preventScreenSaver 1 2>&1 || echo "  failed"

    echo "step 2: stop framework"
    # Ignore the SIGTERM the framework may broadcast on its way down.
    trap "" TERM
    stop lab126_gui 2>/dev/null || true
    usleep 1250000 2>/dev/null || sleep 2
    # Re-arm the restore trap.
    trap restore TERM

    echo "step 3: pause volumd (block USBMS)"
    killall -STOP volumd 2>/dev/null || true

    echo "step 4: run binary"
    "$EXTENSION_DIR/bin/weather"
    echo "step 5: binary exited $?"
} >>"$LOG" 2>&1
