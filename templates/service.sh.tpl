#!/system/bin/sh
MODDIR="$(dirname "$(readlink -f "$0")")"
export MODDIR
. "$MODDIR/utils.sh"

run() {
    until [ "$(getprop sys.boot_completed)" = "1" ]; do
        sleep 2
    done
    sleep 3

    [ -f "$MODDIR/disabled_by_action" ] && return 0

    if [ -f "$MODDIR/apps.list" ]; then
        while IFS=: read -r APP_ID PKG_NAME APP_MODE PATCHED_VER; do
            [ -z "$APP_ID" ] && continue
            [ "$APP_MODE" = "install" ] && continue

            APK_SRC="$MODDIR/apks/${APP_ID}.apk"
            [ ! -f "$APK_SRC" ] && APK_SRC="/data/adb/rvex/${APP_ID}.apk"
            if [ -f "$APK_SRC" ]; then
                mount_app_apk "$APP_ID" "$PKG_NAME" "$APK_SRC" "$PATCHED_VER" || :
            fi
        done < "$MODDIR/apps.list"
    fi
}

run &
