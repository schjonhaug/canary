#!/bin/sh
# Renders og.html to the 1200x630 Open Graph and X share images in public/images/: opengraph-image.png for the
# home page and og/<page>.png for each page in og.html's PAGES. Needs Google Chrome.
set -eu
dir=$(cd "$(dirname "$0")" && pwd)
out="$dir/../../public/images"
render() {
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new --disable-gpu --hide-scrollbars \
    --force-device-scale-factor=1 --allow-file-access-from-files --window-size=1200,630 \
    --virtual-time-budget=2000 --screenshot="$1" "file://$dir/og.html$2" 2>/dev/null
}
render "$out/opengraph-image.png" ""
mkdir -p "$out/og"
for page in cloud private donations; do
  render "$out/og/$page.png" "?page=$page"
done
