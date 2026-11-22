#!/usr/bin/env bash
# The package for the ZX Spectrum Next itself: OUTDIR/lz80n with the
# compiler and the IDE (.nex), the files they read on the SD card (runtime,
# modules) and the example programs with their pictures, sounds and include
# files. Copy lz80n/ to the SD card, then: .cd /lz80n  .nexload ide.nex
#
#   scripts/zxnext-package.sh path/to/lz80nc OUTDIR
set -euo pipefail
lz80nc=$1
out=$2/lz80n
rm -rf "$out"
"$lz80nc" --export-selfhost "$out"
for f in lexer codegen assets asm ide; do test -s "$out/$f.nex"; done
# the compiler's own sources are not needed to run it
rm -f "$out"/{lexer,parser,parser_core,codegen,asm,asset_info,assets,ide}.lz80n
# the examples (in the folder itself: their include and asset paths are
# relative to it)
cp examples/*.lz80n "$out/"
cp -r examples/assets examples/lib "$out/"
ls "$out"
