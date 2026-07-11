#!/bin/sh
set -eu
systemctl disable --now unionc-agent.service >/dev/null 2>&1 || true

