#!/system/bin/sh
# ReVanceX Module Installer
# shellcheck disable=SC2034
SKIPUNZIP=1
export MODDIR="$MODPATH"

ui_print "****************************************"
ui_print "*           ReVanceX Bundle            *"
ui_print "*   Magisk / KernelSU Module Installer *"
ui_print "****************************************"

mkdir -p /data/adb/rvex
mkdir -p "$MODPATH"

if ! unzip -o "$ZIPFILE" -x 'META-INF/*' -d "$MODPATH" >/dev/null 2>&1; then
    abort "! Failed to extract module files from $ZIPFILE"
fi

if [ -f "$MODPATH/utils.sh" ]; then
    # shellcheck source=/dev/null
    . "$MODPATH/utils.sh"
fi

set_perm_recursive "$MODPATH" 0 0 0755 0644
[ -d "$MODPATH/webroot" ] && set_perm_recursive "$MODPATH/webroot" 0 0 0755 0644
[ -f "$MODPATH/service.sh" ] && set_perm "$MODPATH/service.sh" 0 0 0755
[ -f "$MODPATH/action.sh" ] && set_perm "$MODPATH/action.sh" 0 0 0755
[ -f "$MODPATH/uninstall.sh" ] && set_perm "$MODPATH/uninstall.sh" 0 0 0755
[ -f "$MODPATH/utils.sh" ] && set_perm "$MODPATH/utils.sh" 0 0 0755

find_first_apk() {
    for _ffa_p in "$@"; do
        if [ -f "$_ffa_p" ]; then
            echo "$_ffa_p"
            return 0
        fi
    done
    return 1
}

install_base_apk() {
    _ib_pkg="$1"
    _ib_apk="$2"
    _ib_res=$(pm install -r -d "$_ib_apk" 2>&1) && return 0

    case "$_ib_res" in
        *INSTALL_FAILED_UPDATE_INCOMPATIBLE*|*INSTALL_FAILED_SHARED_USER_INCOMPATIBLE*)
            ui_print "  ! Signature mismatch for $_ib_pkg."
            abort "! Refusing to uninstall $_ib_pkg (would wipe app data). Back up your data, uninstall $_ib_pkg manually, and re-flash."
            ;;
        *INSTALL_FAILED_VERSION_DOWNGRADE*)
            ui_print "  - Newer version of $_ib_pkg installed; attempting data-preserving reinstall (pm uninstall -k)..."
            pm uninstall -k "$_ib_pkg" >/dev/null 2>&1 || :
            if ! pm install -r -d "$_ib_apk" >/dev/null 2>&1; then
                abort "! Failed to reinstall $_ib_pkg without wiping app data. Back up your data, uninstall $_ib_pkg manually, and re-flash."
            fi
            ;;
        *)
            ui_print "  ! pm install failed for $_ib_pkg: $_ib_res"
            return 1
            ;;
    esac
}

if [ ! -f "$MODPATH/apps.list" ]; then
    for f in "$MODPATH"/apks/*.apk; do
        [ -f "$f" ] || continue
        name=$(basename "$f" .apk)
        echo "$name:$name:patch:" >> "$MODPATH/apps.list"
    done
fi

while IFS=: read -r APP_ID PKG_NAME APP_MODE PATCHED_VER; do
    [ -z "$APP_ID" ] && continue
    [ "$APP_ID" = "microg" ] && continue

    APK_SRC="$MODPATH/apks/${APP_ID}.apk"
    if [ ! -f "$APK_SRC" ]; then
        APK_SRC=$(find_first_apk "$MODPATH"/apks/"${APP_ID}"*.apk "$MODPATH"/system/app/"${APP_ID}"*/*.apk || :)
    fi

    if [ ! -f "$APK_SRC" ]; then
        ui_print "! APK for $APP_ID not found in module"
        continue
    fi

    ui_print "* Processing $APP_ID ($PKG_NAME)..."

    BASEPATH=$(get_basepath "$PKG_NAME" || :)

    if [ "$APP_MODE" != "install" ]; then
        if [ -z "$BASEPATH" ] || [ ! -f "$BASEPATH" ]; then
            STOCK_APK="$MODPATH/stock/${APP_ID}.apk"
            if [ -f "$STOCK_APK" ]; then
                ui_print "  - Stock app not detected in system, installing official stock base..."
                install_base_apk "$PKG_NAME" "$STOCK_APK" || :
                BASEPATH=$(get_basepath "$PKG_NAME" || :)
            fi
        fi

        if [ -z "$BASEPATH" ] || [ ! -f "$BASEPATH" ]; then
            ui_print "  - Stock app not detected in system, installing base..."
            install_base_apk "$PKG_NAME" "$APK_SRC" || :
            BASEPATH=$(get_basepath "$PKG_NAME" || :)
        fi

        if [ -n "$BASEPATH" ] && [ -f "$BASEPATH" ]; then
            _rc=0
            mount_app_apk "$APP_ID" "$PKG_NAME" "$APK_SRC" "$PATCHED_VER" || _rc=$?
            if [ "$_rc" -eq 0 ]; then
                ui_print "  - Replaced & mounted seamlessly over stock app"
            elif [ "$_rc" -eq 2 ]; then
                ui_print "  - Kept Play-Store-updated stock app (skipped older module overlay)"
            else
                ui_print "  ! Bind-mount failed for $APP_ID"
            fi
        else
            ui_print "  - Note: Installed as standalone fallback"
        fi
    else
        install_base_apk "$PKG_NAME" "$APK_SRC" || :
        ui_print "  - Standalone install complete."
    fi
done < "$MODPATH/apps.list"

ui_print "****************************************"
ui_print "* Installation complete! Enjoy!        *"
ui_print "****************************************"
