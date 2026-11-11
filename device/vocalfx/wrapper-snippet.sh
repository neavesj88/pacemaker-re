# Pacemaker Revival: optional LD_PRELOAD libraries, inserted into the /bin/pmplayer wrapper script
# (/bin/sh = busybox ash on the device) just before the final `exec /pmds/bin/pmplayer.real "$@"`.
#
# A library is preloaded only if its file is in the image AND the kill switch file is absent, so
#   - an image without the library behaves like stock,
#   - creating  /mnt/public/.Pacemaker/revival-no-preload  (empty file, from the PC over USB) turns every
#     preloaded library off at the next boot without reflashing.
# If ld.so cannot load a listed library it prints "cannot be preloaded: ignored" and carries on (glibc 2.4).
# Add further libraries (for example libfavourite.so) to the list; the result is a colon-separated LD_PRELOAD.
pre=
if [ ! -e /mnt/public/.Pacemaker/revival-no-preload ]; then
  for lib in libvocalfx.so; do
    if [ -f "/pmds/bin/$lib" ]; then pre="$pre${pre:+:}/pmds/bin/$lib"; fi
  done
fi
if [ -n "$pre" ]; then export LD_PRELOAD="$pre"; fi
