#!/usr/bin/env bash
# ==============================================================================
# Digest Article-to-Audio-Lesson Development Runner
#
# Orchestrates everything needed to work on Digest locally:
#   1. Kokoro neural TTS Docker daemon (port 3000), for speech synthesis
#   2. The Tauri v2 desktop app in dev mode (Vite on 1420 + Rust core)
#
# On exit -- whether you press Ctrl-C, the window closes, or the script fails --
# every process this script started is terminated and the Kokoro container is
# stopped, so no orphan holds a port and no container keeps its ~2.3 GiB resident.
#
# Playback of already-generated lessons does NOT need Kokoro. It is only required
# when you generate new audio, so `--no-kokoro` is a valid way to work.
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Resolve the repository root by walking up to pnpm-workspace.yaml, so this
# script keeps working if it is invoked through the root symlink or from a
# subdirectory.
REPO_ROOT="$SCRIPT_DIR"
while [ "$REPO_ROOT" != "/" ] && [ ! -f "$REPO_ROOT/pnpm-workspace.yaml" ]; do
    REPO_ROOT="$(dirname "$REPO_ROOT")"
done
if [ ! -f "$REPO_ROOT/pnpm-workspace.yaml" ]; then
    echo "Error: could not locate repository root (pnpm-workspace.yaml) from $SCRIPT_DIR" >&2
    exit 1
fi

KOKORO_IMAGE="ghcr.io/lucasjinreal/kokoros:main"
KOKORO_CONTAINER="digest-kokoro"
KOKORO_PORT=3000
KOKORO_URL="http://127.0.0.1:${KOKORO_PORT}/v1/audio/speech"
KOKORO_VOICE="af_sky"
VITE_PORT=1420
KOKORO_STARTUP_TIMEOUT=180
SHUTDOWN_GRACE=10

KEEP_KOKORO=0
USE_KOKORO=1
SKIP_CHECKS=0
PASSTHROUGH_ARGS=()
APP_PID=""
APP_PGID=""
APP_LOG=""
TAIL_PID=""
CLEANED_UP=0
KOKORO_STARTED_HERE=0

# ------------------------------------------------------------------------------
# Usage & Help
# ------------------------------------------------------------------------------
show_help() {
    cat << 'EOF'
Usage: ./run_demo.sh [OPTIONS] [-- <extra tauri args>]

Starts Kokoro, then runs the Digest desktop app in Tauri dev mode. Everything
the script starts is torn down when you exit.

Options:
  --no-kokoro          Do not start Kokoro. Playback of existing lessons still
                       works; generating new audio will fail.
  --keep-kokoro        Leave the Kokoro container running after exit. Useful
                       across repeated runs to skip the model warm-up.
  --port <PORT>        Host port for Kokoro (default: 3000).
  --voice <VOICE>      Kokoro voice (default: af_sky).
                       Options: af_sky, af_sarah, af_bella, bf_emma, am_adam.
  --timeout <SECS>     How long to wait for Kokoro to answer (default: 180).
  --skip-checks        Skip pre-flight dependency and port checks.
  -h, --help           Show this message and exit.

Examples:
  ./run_demo.sh                              # normal development session
  ./run_demo.sh --keep-kokoro                # reuse the warm container
  ./run_demo.sh --no-kokoro                  # just browse existing lessons
  ./run_demo.sh -- --no-watch                # pass flags through to tauri dev

Controls:
  Ctrl-C, or closing the app window, shuts everything down and stops Kokoro.
  Nothing is left listening on 3000 or 1420.

Notes:
  Run this in the foreground. If you background it with `&`, the shell sets
  SIGINT to ignored before the script starts, and POSIX does not allow a script
  to trap a signal that arrived ignored. Ctrl-C will then not reach the cleanup
  handler. Stop a backgrounded run with SIGTERM instead:
    ./run_demo.sh &            # then, to stop it:
    kill -TERM %1
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -h|--help)
            show_help
            exit 0
            ;;
        --no-kokoro)
            USE_KOKORO=0
            shift
            ;;
        --keep-kokoro)
            KEEP_KOKORO=1
            shift
            ;;
        --port)
            KOKORO_PORT="${2:?--port requires a value}"
            KOKORO_URL="http://127.0.0.1:${KOKORO_PORT}/v1/audio/speech"
            shift 2
            ;;
        --voice)
            KOKORO_VOICE="${2:?--voice requires a value}"
            shift 2
            ;;
        --timeout)
            KOKORO_STARTUP_TIMEOUT="${2:?--timeout requires a value}"
            shift 2
            ;;
        --skip-checks)
            SKIP_CHECKS=1
            shift
            ;;
        --)
            shift
            PASSTHROUGH_ARGS+=("$@")
            break
            ;;
        *)
            PASSTHROUGH_ARGS+=("$1")
            shift
            ;;
    esac
done

# ------------------------------------------------------------------------------
# Helpers
# ------------------------------------------------------------------------------
log()  { printf '%s\n' "$*"; }
step() { printf '\n[%s] %s\n' "$1" "$2"; }

# The process group this script itself runs in. Cleanup must never signal it,
# or the script kills its own session mid-handler.
SELF_PGID="$(ps -o pgid= -p $$ 2>/dev/null | tr -d ' ')"

port_owner() {
    # Prints the PID holding a TCP port, or nothing. Falls back through ss, lsof
    # and fuser because not every box ships all three.
    local port="$1"
    if command -v ss >/dev/null 2>&1; then
        ss -lptnH "sport = :${port}" 2>/dev/null \
            | grep -oP 'pid=\K[0-9]+' | head -1 && return 0
    fi
    if command -v lsof >/dev/null 2>&1; then
        lsof -ti ":${port}" -sTCP:LISTEN 2>/dev/null | head -1 && return 0
    fi
    if command -v fuser >/dev/null 2>&1; then
        fuser "${port}/tcp" 2>/dev/null | tr -s ' ' '\n' | grep -E '^[0-9]+$' | head -1 && return 0
    fi
    return 0
}

group_alive() {
    [ -n "$APP_PGID" ] && kill -0 -- "-${APP_PGID}" 2>/dev/null
}

signal_app() {
    local sig="$1"
    # Refuse to signal our own process group under any circumstances.
    if [ -n "$APP_PGID" ] && [ "$APP_PGID" != "$SELF_PGID" ]; then
        kill "-${sig}" -- "-${APP_PGID}" 2>/dev/null || true
    elif [ -n "$APP_PID" ]; then
        kill "-${sig}" "$APP_PID" 2>/dev/null || true
    fi
}

# ------------------------------------------------------------------------------
# Graceful Cleanup Handler
# ------------------------------------------------------------------------------
cleanup() {
    local exit_code=$?
    if [ "$CLEANED_UP" -eq 1 ]; then
        return
    fi
    CLEANED_UP=1

    log ""
    log "========================================================"
    log " Digest: Graceful Shutdown"
    log "========================================================"

    # 1. Stop the whole Tauri process tree. `pnpm tauri dev` spawns the Tauri
    #    CLI, which spawns cargo, which spawns the digest binary, which spawns
    #    WebKit helper processes. Signalling only the top PID orphans the rest,
    #    so we signal the entire process group and escalate to SIGKILL.
    if group_alive; then
        log "[1/4] Stopping Tauri dev tree (process group $APP_PGID)..."
        signal_app TERM
        for _ in $(seq 1 $((SHUTDOWN_GRACE * 2))); do
            group_alive || break
            sleep 0.5
        done
        if group_alive; then
            log "      Still alive after ${SHUTDOWN_GRACE}s, sending SIGKILL..."
            signal_app KILL
            sleep 0.5
        fi
        log "      Tauri dev tree terminated."
    elif [ -n "$APP_PID" ] && kill -0 "$APP_PID" 2>/dev/null; then
        log "[1/4] Stopping Tauri dev (PID $APP_PID)..."
        signal_app TERM
        sleep 1
        signal_app KILL
        log "      Tauri dev terminated."
    else
        log "[1/4] Tauri dev already exited."
    fi

    # Stop the log mirror before reporting, so its output cannot interleave.
    if [ -n "$TAIL_PID" ] && kill -0 "$TAIL_PID" 2>/dev/null; then
        kill -TERM "$TAIL_PID" 2>/dev/null || true
    fi

    # 2. Report anything that outlived the group, so an orphan is visible rather
    #    than silently holding a port.
    local leftover
    leftover="$(port_owner "$VITE_PORT")"
    if [ -n "$leftover" ]; then
        log "      Warning: PID $leftover still holds port $VITE_PORT. Stop it with:"
        log "        kill $leftover"
    else
        log "      Port $VITE_PORT is free."
    fi

    # 3. Stop the Kokoro container unless asked to keep it.
    if [ "$USE_KOKORO" -eq 1 ] && [ "$KEEP_KOKORO" -eq 0 ]; then
        if [ "$KOKORO_STARTED_HERE" -eq 1 ]; then
            log "[2/4] Stopping Kokoro TTS container ($KOKORO_CONTAINER)..."
            docker stop -t 5 "$KOKORO_CONTAINER" >/dev/null 2>&1 \
                && log "      Kokoro container stopped." \
                || log "      Container already stopped."
        else
            log "[2/4] Leaving Kokoro container alone (this script did not start it)."
        fi
    elif [ "$KEEP_KOKORO" -eq 1 ]; then
        log "[2/4] Keeping Kokoro container running (--keep-kokoro)."
    else
        log "[2/4] No Kokoro container to stop (--no-kokoro)."
    fi

    # 4. Release the build log.
    if [ -n "$APP_LOG" ] && [ -f "$APP_LOG" ]; then
        log "[3/4] Full dev output kept at $APP_LOG"
    else
        log "[3/4] Nothing to clean."
    fi

    log "========================================================"
    log " Shutdown complete."
    log "========================================================"
    exit "$exit_code"
}

trap cleanup EXIT INT TERM HUP

# ------------------------------------------------------------------------------
# Pre-Flight Environment Checks
# ------------------------------------------------------------------------------
log "========================================================"
log " Digest: Article-to-Audio-Lesson Development Runner"
log "========================================================"
log " Repository: $REPO_ROOT"
log " Script PID: $$ (stop with: kill -TERM $$)"

if [ "$SKIP_CHECKS" -eq 0 ]; then
    step "1/4" "Checking prerequisites"

    MISSING=0
    for tool in node pnpm cargo; do
        if command -v "$tool" >/dev/null 2>&1; then
            log "  ok   $tool ($("$tool" --version 2>/dev/null | head -1))"
        else
            log "  MISS $tool is not on PATH"
            MISSING=1
        fi
    done
    if [ "$MISSING" -eq 1 ]; then
        echo "" >&2
        echo "Error: install the missing tools, then re-run. Rust via https://rustup.rs," >&2
        echo "Node 20+ and pnpm via corepack: corepack enable pnpm" >&2
        exit 1
    fi

    # Frontend dependencies. Installing here is friendlier than a bare ENOENT.
    if [ ! -d "$REPO_ROOT/node_modules" ]; then
        log "  installing frontend dependencies (pnpm install)..."
        (cd "$REPO_ROOT" && pnpm install)
    fi

    # Tauri needs a display. Without one, `tauri dev` builds then dies silently.
    if [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
        echo "" >&2
        echo "Error: no display detected (DISPLAY and WAYLAND_DISPLAY are both unset)." >&2
        echo "       tauri dev builds the Rust core but cannot open a window." >&2
        echo "       Run it inside a graphical session, or use Xvfb:" >&2
        echo "         Xvfb :99 -screen 0 1920x1080x24 &" >&2
        echo "         DISPLAY=:99 ./run_demo.sh" >&2
        exit 1
    fi

    # Vite runs with strictPort, so a stale listener is a hard failure.
    VITE_OWNER="$(port_owner "$VITE_PORT")"
    if [ -n "$VITE_OWNER" ]; then
        echo "" >&2
        echo "Error: port $VITE_PORT is already held by PID $VITE_OWNER." >&2
        echo "       Vite is configured with strictPort, so tauri dev will not start." >&2
        echo "       If that is a leftover from a previous run:" >&2
        echo "         kill $VITE_OWNER" >&2
        exit 1
    fi

    DOCKER_AVAILABLE=0
    if [ "$USE_KOKORO" -eq 1 ]; then
        if ! command -v docker >/dev/null 2>&1; then
            log "  warn 'docker' not found. Run with --no-kokoro to skip synthesis."
            USE_KOKORO=0
        elif ! docker info >/dev/null 2>&1; then
            log "  warn Docker daemon is not running. Cannot start Kokoro."
            USE_KOKORO=0
        else
            DOCKER_AVAILABLE=1
            log "  ok   docker"
        fi
    fi
    log "  prerequisites checked."
else
    step "1/4" "Skipping prerequisite checks (--skip-checks)"
    DOCKER_AVAILABLE=0
    command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1 && DOCKER_AVAILABLE=1
fi

# ------------------------------------------------------------------------------
# Kokoro Neural TTS Service Startup
# ------------------------------------------------------------------------------
is_kokoro_responsive() {
    # Digest asks for Ogg Opus, so probe with the same response format the app
    # will use. A HEAD/GET would not exercise the model.
    local code
    code=$(curl -s -o /dev/null -w "%{http_code}" -m 10 -X POST "$KOKORO_URL" \
        -H "Content-Type: application/json" \
        -d "{\"model\":\"tts-1\",\"input\":\"ready check\",\"voice\":\"${KOKORO_VOICE}\",\"response_format\":\"opus\"}" \
        2>/dev/null || echo "000")
    [ "$code" = "200" ]
}

if [ "$USE_KOKORO" -eq 1 ] && [ "$DOCKER_AVAILABLE" -eq 1 ]; then
    step "2/4" "Starting Kokoro neural TTS on port $KOKORO_PORT"

    if is_kokoro_responsive; then
        log "  Kokoro is already answering on port $KOKORO_PORT; leaving it alone."
    else
        STATE="$(docker inspect -f '{{.State.Running}}' "$KOKORO_CONTAINER" 2>/dev/null || echo "absent")"
        case "$STATE" in
            true)
                log "  Container exists and is running; waiting for readiness."
                ;;
            false)
                log "  Starting existing container $KOKORO_CONTAINER..."
                docker start "$KOKORO_CONTAINER" >/dev/null
                KOKORO_STARTED_HERE=1
                ;;
            *)
                log "  Launching $KOKORO_IMAGE..."
                # A stale container on the same name would block this.
                docker rm -f "$KOKORO_CONTAINER" >/dev/null 2>&1 || true
                docker run -d \
                    --name "$KOKORO_CONTAINER" \
                    -p "${KOKORO_PORT}:3000" \
                    --restart no \
                    "$KOKORO_IMAGE" \
                    openai >/dev/null
                KOKORO_STARTED_HERE=1
                ;;
        esac

        # The first request loads the model, so allow far more than a plain
        # health check would need.
        log -n "  Waiting for the Kokoro endpoint"
        READY=0
        DEADLINE=$((SECONDS + KOKORO_STARTUP_TIMEOUT))
        while [ "$SECONDS" -lt "$DEADLINE" ]; do
            if is_kokoro_responsive; then
                READY=1
                break
            fi
            log -n "."
            sleep 2
        done
        log ""

        if [ "$READY" -eq 1 ]; then
            log "  Kokoro is ready (voice ${KOKORO_VOICE}, Ogg Opus)."
        else
            log "  Warning: Kokoro did not answer within ${KOKORO_STARTUP_TIMEOUT}s."
            log "  New audio generation will fail. Existing lessons still play."
            log "  Container logs: docker logs $KOKORO_CONTAINER"
        fi
    fi
else
    step "2/4" "Skipping Kokoro (--no-kokoro or Docker unavailable)"
    log "  Existing lessons will play. Generating new audio will fail."
fi

# ------------------------------------------------------------------------------
# Launch the Tauri Desktop App
# ------------------------------------------------------------------------------
step "3/4" "Building and launching the Digest desktop app"

cd "$REPO_ROOT"
export DIGEST_KOKORO_URL="http://127.0.0.1:${KOKORO_PORT}"

APP_LOG="$(mktemp -t digest-dev-XXXXXX.log)"

# setsid puts the app in its own process group. Without this, Ctrl-C reaches the
# tree only by luck and the WebKit helpers routinely survive; with it, cleanup
# can signal the whole group deterministically.
#
# Output goes straight to the log rather than through a `| tee` pipeline, because
# `$!` in a pipeline is the PID of the *last* stage. Piping here would make the
# script track tee, resolve the wrong process group, and ultimately signal its
# own session. A background `tail -F` mirrors the log to the terminal instead.
if command -v setsid >/dev/null 2>&1; then
    setsid pnpm tauri dev "${PASSTHROUGH_ARGS[@]+"${PASSTHROUGH_ARGS[@]}"}" >"$APP_LOG" 2>&1 &
else
    log "  warn 'setsid' unavailable; falling back to plain background launch."
    pnpm tauri dev "${PASSTHROUGH_ARGS[@]+"${PASSTHROUGH_ARGS[@]}"}" >"$APP_LOG" 2>&1 &
fi
APP_PID=$!

tail -n 20 -F "$APP_LOG" &
TAIL_PID=$!

# setsid execs rather than forks when it is not already a group leader, which is
# the case for a background job, so APP_PID is normally the new group leader.
# Resolve the group from the PID and sanity-check it against our own.
sleep 1
APP_PGID="$(ps -o pgid= -p "$APP_PID" 2>/dev/null | tr -d ' ')"
if [ -z "$APP_PGID" ] || [ "$APP_PGID" = "$SELF_PGID" ]; then
    if [ -n "$APP_PGID" ] && [ "$APP_PGID" = "$SELF_PGID" ]; then
        log "  warn app shares this script's process group; falling back to PID-only teardown."
    else
        APP_PGID="$(ps -o pgid= --ppid "$APP_PID" 2>/dev/null | tr -d ' ' | head -1)"
    fi
fi
if [ "$APP_PGID" = "$SELF_PGID" ]; then
    APP_PGID=""
fi
log "  Tauri dev running (pid ${APP_PID}, process group ${APP_PGID:-pid-only})."
log "  Digest is configured to use Kokoro at $DIGEST_KOKORO_URL"

# ------------------------------------------------------------------------------
# Wait for the App, Interrupting Cleanly on Ctrl-C
# ------------------------------------------------------------------------------
step "4/4" "Running"
log "  Close the window, or press Ctrl-C here, to shut everything down."

set +e
wait "$APP_PID"
APP_EXIT=$?
set -e

log ""
log "Digest exited with status $APP_EXIT."
exit "$APP_EXIT"
