#!/bin/sh
#
#       user/desktop/assets/session.sh
#       Start Xorg and the SPUI desktop shell for the default seat
#
#       2026/10/30 By JiTianYu391
#       Copyright (C) 2026 ViudiraTech.
#

export DISPLAY=:0
export HOME=/root
export PATH=/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin

/usr/bin/Xorg :0 -logfile /var/log/xorg.log -novtswitch -nolisten tcp -noreset >/var/log/xorg-stderr.log 2>&1 &
i=0
while [ ! -S /tmp/.X11-unix/X0 ] && [ "$i" -lt 20 ]; do
    sleep 1
    i=$((i + 1))
done
if [ ! -S /tmp/.X11-unix/X0 ]; then
    echo "xorg did not create /tmp/.X11-unix/X0" | tee -a /var/log/spui.log >/dev/ttyS0
    wait
fi
# The socket appears before Xorg has finished input/DRM initialization, but the
# server only answers the connection handshake from its dispatch loop, which
# starts after that.  SPUI's X::connect() retries until the handshake succeeds,
# so no fixed delay is needed here.
exec /usr/bin/spui shell >/dev/ttyS0 2>&1
