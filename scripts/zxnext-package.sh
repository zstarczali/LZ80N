#!/usr/bin/env bash
# The package for the ZX Spectrum Next itself: OUTDIR/lz80n with the
# compiler and the IDE (.nex), lib/ (the runtime, the modules, the manuals)
# and examples/ (the example programs with their pictures, sounds and
# include files). Copy lz80n/ to the SD card, then: .cd /lz80n  .nexload ide.nex
#
#   scripts/zxnext-package.sh path/to/lz80nc OUTDIR
set -euo pipefail
lz80nc=$1
out=$2/lz80n
rm -rf "$out"
"$lz80nc" --export-selfhost "$out"
for f in lexer codegen assets asm ide; do test -s "$out/$f.nex"; done
# the compiler's own sources are not needed to run it
rm -rf "$out/src"
# the examples, with their pictures, sounds and include files (their paths
# are relative to the program's folder)
mkdir -p "$out/examples"
cp examples/*.lz80n "$out/examples/"
cp -r examples/assets examples/lib "$out/examples/"
ls "$out"
