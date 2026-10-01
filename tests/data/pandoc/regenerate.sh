#!/bin/sh
# Regenerate the pandoc fixtures (pandoc 3.x):
#   sh tests/data/pandoc/regenerate.sh
cd "$(dirname "$0")"
pandoc fixture.tex -t markdown -o default.md
pandoc fixture.tex -t gfm -o gfm.md
pandoc fixture2.tex -t markdown --columns=50 -o multiline.md  # headerless long-cell table
pandoc fixture3.tex -t markdown -o grid.md                   # cell with block content
