"""Shared console logging for the Python helper scripts.

One event per line, consistent `[HH:MM:SS] <marker> message` format, ASCII
fallback when NO_COLOR is set or stdout is not a TTY (GitHub Actions renders the
unicode markers, so it counts as capable), identical consecutive lines collapsed
to "(xN)", `::warning::` / `::error::` annotations on Actions, secrets masked.

Detail (`debug`) is shown only with VERBOSE=1 / RUNNER_DEBUG=1.
"""

import os
import sys
import time

_CI = bool(os.environ.get("GITHUB_ACTIONS"))
_RICH = not os.environ.get("NO_COLOR") and (sys.stdout.isatty() or _CI)
_VERBOSE = os.environ.get("VERBOSE", "0") not in ("", "0") or os.environ.get("RUNNER_DEBUG") == "1"
_SECRET_ENVS = ("TG_TOKEN", "TG_CHAT", "GITHUB_TOKEN")

_MARKERS = {
    "ok": ("\u2714", "[OK]"),
    "fail": ("\u2716", "[FAIL]"),
    "warn": ("\u26a0", "[WARN]"),
    "skip": ("\u21b7", "[SKIP]"),
    "info": ("\u25cf", "[INFO]"),
}

_last = {"line": None, "count": 0, "ts": ""}
_warned = set()


def mask(text):
    """Replace any secret value from the environment with ***."""
    for key in _SECRET_ENVS:
        val = os.environ.get(key, "")
        if len(val) >= 6:
            text = text.replace(val, "***")
    return text


def _flush():
    if _last["line"] is not None:
        suffix = f"  (x{_last['count']})" if _last["count"] > 1 else ""
        print(f"[{_last['ts']}] {_last['line']}{suffix}", flush=True)
        _last["line"], _last["count"] = None, 0


def _emit(kind, msg):
    marker = _MARKERS[kind][0 if _RICH else 1]
    line = mask(f"{marker:<1} {msg}" if _RICH else f"{marker:<6} {msg}")
    if line == _last["line"]:
        _last["count"] += 1
        return
    _flush()
    _last.update(line=line, count=1, ts=time.strftime("%H:%M:%S"))


def ok(msg):
    _emit("ok", msg)


def info(msg):
    _emit("info", msg)


def skip(msg):
    _emit("skip", msg)


def debug(msg):
    if _VERBOSE:
        _emit("info", msg)


def warn(msg):
    """Warn once per distinct message; shows up in Actions Annotations."""
    msg = mask(msg)
    if msg in _warned:
        return
    _warned.add(msg)
    _emit("warn", msg)
    if _CI:
        _flush()
        print(f"::warning::{msg}", flush=True)


def fail(msg):
    msg = mask(msg)
    _emit("fail", msg)
    if _CI:
        _flush()
        print(f"::error::{msg}", flush=True)


def flush():
    _flush()


import atexit  # noqa: E402

atexit.register(_flush)
