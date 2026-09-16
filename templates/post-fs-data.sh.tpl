#!/system/bin/sh
MODDIR=${0%/*}

# Mount overlay mounts or setup environment
if [ -d "$MODDIR/system/app" ]; then
    set_perm_recursive "$MODDIR/system/app" root root 0755 0644
fi
