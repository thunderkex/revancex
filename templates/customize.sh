#!/system/bin/sh
# ReVanceX Module Installer
SKIPUNZIP=1
MODDIR="$MODPATH"

ui_print "****************************************"
ui_print "*           ReVanceX Bundle            *"
ui_print "*   Magisk / KernelSU Module Installer *"
ui_print "****************************************"

mkdir -p /data/adb/rvex
mkdir -p "$MODPATH"

unzip -o "$ZIPFILE" -x 'META-INF/*' -d "$MODPATH" >/dev/null 2>&1

[ -f "$MODPATH/utils.sh" ] && . "$MODPATH/utils.sh"

set_perm_recursive "$MODPATH" 0 0 0755 0644
[ -d "$MODPATH/webroot" ] && set_perm_recursive "$MODPATH/webroot" 0 0 0755 0644
[ -f "$MODPATH/service.sh" ] && set_perm "$MODPATH/service.sh" 0 0 0755
[ -f "$MODPATH/action.sh" ] && set_perm "$MODPATH/action.sh" 0 0 0755
[ -f "$MODPATH/uninstall.sh" ] && set_perm "$MODPATH/uninstall.sh" 0 0 0755
[ -f "$MODPATH/utils.sh" ] && set_perm "$MODPATH/utils.sh" 0 0 0755

if [ ! -f "$MODPATH/apps.list" ]; then
    for f in "$MODPATH"/apks/*.apk; do
        [ -f "$f" ] || continue
        name=$(basename "$f" .apk)
        echo "$name:$name:patch" >> "$MODPATH/apps.list"
    done
fi

while IFS=: read -r APP_ID PKG_NAME APP_MODE; do
    [ -z "$APP_ID" ] && continue
    [ "$APP_ID" = "microg" ] && continue

    APK_SRC="$MODPATH/apks/${APP_ID}.apk"
    [ ! -f "$APK_SRC" ] && APK_SRC=$(ls "$MODPATH"/apks/${APP_ID}*.apk 2>/dev/null | head -1)
    [ ! -f "$APK_SRC" ] && APK_SRC=$(ls "$MODPATH"/system/app/${APP_ID}*/*.apk 2>/dev/null | head -1)

    if [ ! -f "$APK_SRC" ]; then
        ui_print "! APK for $APP_ID not found in module"
        continue
    fi

    ui_print "* Processing $APP_ID ($PKG_NAME)..."

    BASEPATH=$(get_basepath "$PKG_NAME")

    if [ "$APP_MODE" != "install" ]; then
        if [ -z "$BASEPATH" ] || [ ! -f "$BASEPATH" ]; then
            STOCK_APK="$MODPATH/stock/${APP_ID}.apk"
            if [ -f "$STOCK_APK" ]; then
                ui_print "  - Stock app not detected in system, installing official stock base..."
                INSTALL_RES=$(pm install -r -d "$STOCK_APK" 2>&1)
                if echo "$INSTALL_RES" | grep -q -e "INSTALL_FAILED_UPDATE_INCOMPATIBLE" -e "INSTALL_FAILED_SHARED_USER_INCOMPATIBLE" -e "INSTALL_FAILED_VERSION_DOWNGRADE"; then
                    ui_print "  - Removing conflicting package signature..."
                    pm uninstall "$PKG_NAME" >/dev/null 2>&1 || :
                    pm install -r -d "$STOCK_APK" >/dev/null 2>&1 || :
                fi
                BASEPATH=$(get_basepath "$PKG_NAME")
            fi
        fi

        if [ -z "$BASEPATH" ] || [ ! -f "$BASEPATH" ]; then
            ui_print "  - Stock app not detected in system, installing base..."
            INSTALL_RES=$(pm install -r -d "$APK_SRC" 2>&1)
            if echo "$INSTALL_RES" | grep -q -e "INSTALL_FAILED_UPDATE_INCOMPATIBLE" -e "INSTALL_FAILED_SHARED_USER_INCOMPATIBLE" -e "INSTALL_FAILED_VERSION_DOWNGRADE"; then
                ui_print "  - Removing conflicting package signature..."
                pm uninstall "$PKG_NAME" >/dev/null 2>&1 || :
                pm install -r -d "$APK_SRC" >/dev/null 2>&1 || :
            fi
            BASEPATH=$(get_basepath "$PKG_NAME")
        fi

        if [ -n "$BASEPATH" ] && [ -f "$BASEPATH" ]; then
            mount_app_apk "$APP_ID" "$PKG_NAME" "$APK_SRC"
            ui_print "  - Replaced & mounted seamlessly over stock app"
        else
            ui_print "  - Note: Installed as standalone fallback"
        fi
    else
        pm install -r -d "$APK_SRC" >/dev/null 2>&1 || :
        ui_print "  - Standalone install complete."
    fi
done < "$MODPATH/apps.list"

ui_print "****************************************"
ui_print "* Installation complete! Enjoy!        *"
ui_print "****************************************"
