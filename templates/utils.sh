#!/system/bin/sh

pmex() {
    OP=$(pm "$@" 2>&1 </dev/null)
    RET=$?
    echo "$OP"
    return $RET
}

get_app_version() {
    VERSION=$(dumpsys package "$1" 2>&1 | grep -m1 versionName=)
    VERSION="${VERSION#*=}"
    echo "$VERSION"
}

get_basepath() {
    BASEPATH=$(pm path "$1" 2>/dev/null | head -1 | sed 's/^package://' | tr -d '\r')
    echo "$BASEPATH"
    [ -n "$BASEPATH" ] && [ -f "$BASEPATH" ]
}

umount_app() {
    local pkg="$1"
    su -M -c "grep -F '$pkg' /proc/mounts" | while read -r line; do
        mp=$(echo "$line" | awk '{print $2}')
        su -M -c "umount -l '$mp'" 2>/dev/null || :
    done
    am force-stop "$pkg" 2>/dev/null || :
}

mount_app_apk() {
    local app_id="$1"
    local pkg_name="$2"
    local apk_src="$3"

    local basepath
    basepath=$(get_basepath "$pkg_name")
    if [ -z "$basepath" ] || [ ! -f "$basepath" ]; then
        return 1
    fi

    local rvpath="/data/adb/rvex/${app_id}.apk"
    mkdir -p /data/adb/rvex
    if [ "$apk_src" != "$rvpath" ]; then
        cp -f "$apk_src" "$rvpath"
    fi
    chcon u:object_r:apk_data_file:s0 "$rvpath" 2>/dev/null || :
    chmod 0644 "$rvpath"

    umount_app "$pkg_name"
    su -M -c "mount -o bind '$rvpath' '$basepath'" 2>/dev/null || mount -o bind "$rvpath" "$basepath" 2>/dev/null || :
    am force-stop "$pkg_name" 2>/dev/null || :
    return 0
}
