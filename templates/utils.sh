#!/system/bin/sh

get_app_version() {
    dumpsys package "$1" 2>/dev/null | sed -n 's/.*versionName=//p' | head -n 1 | tr -d '\r '
}

# Returns 0 if version $1 is strictly greater than version $2.
version_gt() {
    _vg_a="$1"
    _vg_b="$2"
    [ "$_vg_a" = "$_vg_b" ] && return 1
    while [ -n "$_vg_a" ] || [ -n "$_vg_b" ]; do
        _vg_seg_a="${_vg_a%%[.-]*}"
        _vg_seg_b="${_vg_b%%[.-]*}"
        if [ "$_vg_seg_a" = "$_vg_a" ]; then
            _vg_a=""
        else
            _vg_a="${_vg_a#*"$_vg_seg_a"}"
            _vg_a="${_vg_a#[.-]}"
        fi
        if [ "$_vg_seg_b" = "$_vg_b" ]; then
            _vg_b=""
        else
            _vg_b="${_vg_b#*"$_vg_seg_b"}"
            _vg_b="${_vg_b#[.-]}"
        fi
        _vg_na="${_vg_seg_a:-0}"
        _vg_nb="${_vg_seg_b:-0}"
        case "${_vg_na}:${_vg_nb}" in
            *[!0-9:]*)
                if [ "$_vg_na" != "$_vg_nb" ]; then
                    [ "$(printf '%s\n%s\n' "$_vg_na" "$_vg_nb" | sort | head -n 1)" = "$_vg_nb" ] && return 0
                    return 1
                fi
                ;;
            *)
                if [ "$_vg_na" -gt "$_vg_nb" ] 2>/dev/null; then
                    return 0
                elif [ "$_vg_na" -lt "$_vg_nb" ] 2>/dev/null; then
                    return 1
                fi
                ;;
        esac
    done
    return 1
}

get_basepath() {
    BASEPATH=$(pm path "$1" 2>/dev/null | head -n 1 | sed 's/^package://' | tr -d '\r')
    echo "$BASEPATH"
    [ -n "$BASEPATH" ] && [ -f "$BASEPATH" ]
}

umount_app() {
    _um_pkg="$1"
    su -M -c "grep -F '$_um_pkg' /proc/mounts" 2>/dev/null | while read -r _ _um_mp _; do
        if [ -n "$_um_mp" ]; then
            su -M -c "umount -l '$_um_mp'" 2>/dev/null || :
        fi
    done
    am force-stop "$_um_pkg" 2>/dev/null || :
}

mount_app_apk() {
    _ma_app_id="$1"
    _ma_pkg_name="$2"
    _ma_apk_src="$3"
    _ma_patched_ver="${4:-}"

    _ma_basepath=$(get_basepath "$_ma_pkg_name")
    if [ -z "$_ma_basepath" ] || [ ! -f "$_ma_basepath" ]; then
        return 1
    fi

    if [ -z "$_ma_patched_ver" ] && [ -n "${MODDIR:-}" ] && [ -f "$MODDIR/apps.list" ]; then
        while IFS=: read -r _ma_lid _ _ _ma_lver; do
            if [ "$_ma_lid" = "$_ma_app_id" ]; then
                _ma_patched_ver="$_ma_lver"
                break
            fi
        done < "$MODDIR/apps.list"
    fi

    _ma_stock_ver=$(get_app_version "$_ma_pkg_name")
    if [ -n "$_ma_stock_ver" ] && [ -n "$_ma_patched_ver" ]; then
        if version_gt "$_ma_stock_ver" "$_ma_patched_ver"; then
            _ma_msg="Skipping $_ma_app_id ($_ma_pkg_name): installed stock version ($_ma_stock_ver) is newer than module APK ($_ma_patched_ver)"
            echo "! $_ma_msg" >&2
            if command -v ui_print >/dev/null 2>&1; then
                ui_print "  ! $_ma_msg"
            fi
            if command -v log >/dev/null 2>&1; then
                log -t ReVanceX "$_ma_msg"
            fi
            return 2
        fi
    fi

    _ma_rvpath="/data/adb/rvex/${_ma_app_id}.apk"
    mkdir -p /data/adb/rvex
    if [ "$_ma_apk_src" != "$_ma_rvpath" ]; then
        cp -f "$_ma_apk_src" "$_ma_rvpath"
    fi
    chcon u:object_r:apk_data_file:s0 "$_ma_rvpath" 2>/dev/null || :
    chmod 0644 "$_ma_rvpath"

    umount_app "$_ma_pkg_name"
    if ! su -M -c "mount -o bind '$_ma_rvpath' '$_ma_basepath'" 2>/dev/null; then
        mount -o bind "$_ma_rvpath" "$_ma_basepath" 2>/dev/null || return 1
    fi
    am force-stop "$_ma_pkg_name" 2>/dev/null || :
    return 0
}
