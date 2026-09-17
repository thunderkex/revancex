#!/system/bin/sh
MODDIR="$(dirname "$(readlink -f "$0")")"
export MODDIR
. "$MODDIR/utils.sh"

echo ""
DFILE="$MODDIR/disabled_by_action"

if [ -f "$DFILE" ]; then
    rm -f "$DFILE"
    if [ -f "$MODDIR/apps.list" ]; then
        while IFS=: read -r APP_ID PKG_NAME APP_MODE; do
            [ -z "$APP_ID" ] && continue
            [ "$APP_MODE" = "install" ] && continue
            mount_app_apk "$APP_ID" "$PKG_NAME" "$MODDIR/apks/${APP_ID}.apk" || :
        done < "$MODDIR/apps.list"
    fi
    echo "* Apps mounted and enabled!"
else
    touch "$DFILE"
    if [ -f "$MODDIR/apps.list" ]; then
        while IFS=: read -r APP_ID PKG_NAME APP_MODE; do
            [ -z "$PKG_NAME" ] && continue
            umount_app "$PKG_NAME"
        done < "$MODDIR/apps.list"
    fi
    echo "* Apps unmounted and disabled!"
fi
