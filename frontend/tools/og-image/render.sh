#!/bin/sh
# Renders og.html to public/images/opengraph-image.png (1200x630, the Open Graph and X share image). Needs Google Chrome.
set -eu
dir=$(cd "$(dirname "$0")" && pwd)
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new --disable-gpu --hide-scrollbars \
  --force-device-scale-factor=1 --allow-file-access-from-files --window-size=1200,630 \
  --screenshot="$dir/../../public/images/opengraph-image.png" "file://$dir/og.html" 2>/dev/null
