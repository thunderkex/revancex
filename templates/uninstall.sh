#!/system/bin/sh
MODDIR=${0%/*}

if [ -f "$MODDIR/apps.list" ]; then
    while IFS=: read -r APP_ID PKG_NAME APP_MODE; do
        [ -z "$PKG_NAME" ] && continue
        su -M -c "grep -F '$PKG_NAME' /proc/mounts" | while read -r line; do
            mp=$(echo "$line" | awk '{print $2}')
            su -M -c "umount -l '$mp'" 2>/dev/null || :
        done
        rm -f "/data/adb/rvex/${APP_ID}.apk"
    done < "$MODDIR/apps.list"
fi

rmdir /data/adb/rvex 2>/dev/null || :
