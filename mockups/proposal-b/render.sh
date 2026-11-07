#!/usr/bin/env bash
# Render every screen of proposal B to shots/ with headless Firefox. Nothing opens on
# screen: MOZ_HEADLESS draws offscreen. (Chromium aborts headless on this machine.)
# Usage: ./render.sh               every shot
#        ./render.sh overview vm   only shots whose name starts with these
set -euo pipefail
cd "$(dirname "$0")"
mkdir -p shots
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/ff"

# name|query|width|height
shots=(
  "overview|screen=overview|1280|800"
  "overview-800|screen=overview|800|800"
  "overview-light|screen=overview&theme=light|1280|800"
  "overview-full|screen=overview|1280|1180"
  "vm-display|screen=vm-display|1280|800"
  "vm-display-light|screen=vm-display&theme=light|1280|800"
  "vm-hardware|screen=vm-hardware|1280|800"
  "vm-fullscreen|screen=vm-fullscreen|1280|800"
  "lxc-terminal|screen=lxc-terminal|1280|800"
  "docker-logs|screen=docker-logs|1280|800"
  "stack-compose|screen=stack-compose|1280|800"
  "create|screen=create|1280|800"
  "host|screen=host|1280|800"
  "host-800|screen=host|800|800"
)

want=("$@")
for entry in "${shots[@]}"; do
  IFS='|' read -r name query w h <<<"$entry"
  if ((${#want[@]})); then
    hit=0; for p in "${want[@]}"; do if [[ $name == "$p"* ]]; then hit=1; fi; done
    ((hit)) || continue
  fi
  MOZ_HEADLESS=1 firefox --headless --no-remote --profile "$tmp/ff" --window-size="$w,$h" \
    --screenshot="$PWD/shots/$name.png" "file://$PWD/index.html?$query" >/dev/null 2>&1
  echo "rendered $name ($w x $h)"
done
