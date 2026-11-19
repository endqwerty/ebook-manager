#!/bin/sh
# Run a command on a private X display and a private D-Bus session.
#
# The bus has an empty service directory, and the variables that name the
# real desktop are cleared, so nothing here starts the desktop's own portals,
# gvfs or dconf. Those services crash when a short-lived bus closes under
# them, and users must never see a crash.
#
# The display's scale is set to 1, so a window asked to be 900 wide is 900
# wide whatever the desktop this runs from is set to.
#
#   apps/desktop/tests/private-session.sh cargo nextest run -p ebook-manager --test view
set -eu
dir=$(mktemp -d)
trap 'rm -r "$dir"' EXIT
cat > "$dir/bus.conf" <<CONF
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
  <servicedir>$dir/no-services</servicedir>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
CONF
env -u XDG_CURRENT_DESKTOP -u HYPRLAND_INSTANCE_SIGNATURE -u WAYLAND_DISPLAY -u DBUS_SESSION_BUS_ADDRESS \
  GIO_USE_VFS=local GSETTINGS_BACKEND=memory GDK_BACKEND=x11 NO_AT_BRIDGE=1 GDK_SCALE=1 GDK_DPI_SCALE=1 \
  dbus-run-session --config-file="$dir/bus.conf" -- xvfb-run -a -s "-screen 0 1280x900x24" "$@"
