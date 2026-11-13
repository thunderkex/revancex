#!/system/bin/sh
MODDIR="$(dirname "$(readlink -f "$0")")"
export MODDIR
# shellcheck source=/dev/null
. "$MODDIR/utils.sh"

run() {
    until [ "$(getprop sys.boot_completed)" = "1" ]; do
        sleep 2
    done

    _wait=0
    while [ "$_wait" -lt 15 ]; do
        pm path android >/dev/null 2>&1 && break
        sleep 2
        _wait=$((_wait + 1))
    done
    sleep 2

    [ -f "$MODDIR/disabled_by_action" ] && return 0

    if [ -f "$MODDIR/apps.list" ]; then
        while IFS=: read -r APP_ID PKG_NAME APP_MODE PATCHED_VER; do
            [ -z "$APP_ID" ] && continue
            [ "$APP_MODE" = "install" ] && continue

            APK_SRC="$MODDIR/apks/${APP_ID}.apk"
            [ ! -f "$APK_SRC" ] && APK_SRC="/data/adb/rvex/${APP_ID}.apk"
            if [ -f "$APK_SRC" ]; then
                _try=0
                _rc=0
                while [ "$_try" -lt 3 ]; do
                    _rc=0
                    mount_app_apk "$APP_ID" "$PKG_NAME" "$APK_SRC" "$PATCHED_VER" || _rc=$?
                    if [ "$_rc" -eq 0 ] || [ "$_rc" -eq 2 ] || [ "$_rc" -eq 3 ]; then
                        break
                    fi
                    _try=$((_try + 1))
                    sleep 3
                done
                if command -v log >/dev/null 2>&1; then
                    log -t Revancex "boot mount $APP_ID ($PKG_NAME) rc=$_rc"
                fi
            fi
        done < "$MODDIR/apps.list"
    fi
}

run &
