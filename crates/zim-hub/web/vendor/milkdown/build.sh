#!/usr/bin/env bash
# Rebuild the vendored Crepe bundle: static/milkdown/{milkdown.js,milkdown.css,*.woff2}.
# Needs node + npm; the output is committed so trunk/cargo builds don't.
set -euo pipefail
cd "$(dirname "$0")"
npm ci --no-audit --no-fund
out=../../../static/milkdown
rm -rf "$out"
npx esbuild entry.js --bundle --minify --format=esm --target=es2022 \
  --loader:.woff2=file --loader:.woff=file --loader:.ttf=file \
  --asset-names='[name]-[hash]' --outdir="$out" --entry-names=milkdown \
  --legal-comments=none
# Keep only woff2 (every browser the SPA targets): drop the woff/ttf fallbacks.
rm -f "$out"/*.woff "$out"/*.ttf
perl -pi -e 's/,url\("[^"]+"\) format\("(?:woff|truetype)"\)//g' "$out/milkdown.css"
du -sh "$out"/*
