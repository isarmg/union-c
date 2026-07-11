#!/bin/sh
set -eu

if ! getent group unionc-agent >/dev/null 2>&1; then
  groupadd --system unionc-agent
fi
if ! getent passwd unionc-agent >/dev/null 2>&1; then
  useradd --system --gid unionc-agent --home-dir /var/lib/unionc-agent \
    --shell /usr/sbin/nologin unionc-agent
fi
install -d -m 0700 -o unionc-agent -g unionc-agent /var/lib/unionc-agent
if [ -f /etc/unionc-agent/config.json ]; then
  chown root:unionc-agent /etc/unionc-agent/config.json
  chmod 0640 /etc/unionc-agent/config.json
fi
systemctl daemon-reload >/dev/null 2>&1 || true
