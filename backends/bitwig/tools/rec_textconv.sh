#!/bin/sh
# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
#
# A readable form of a protocol recording (*.rec) for git diff: without the time column, the
# tick lines and the time a configuration file was saved. The file itself is not changed.
#
#   git config diff.dbmrec.textconv backends/bitwig/tools/rec_textconv.sh
#   git config diff.dbmrec.cachetextconv true

cut -f2- "$1" | grep -v -E '^tick(	|$)' | sed 's/#\\n#[^\\]*\\n/#\\n/'
