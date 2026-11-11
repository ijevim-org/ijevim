#!/usr/bin/env bash
# Differential probe: compare ijevim's Emacs keymap against real Emacs.
#
# Usage: scripts/emacs-diff.sh '<buffer text with \n>' '<keys>'
#   keys use <C-x>, <M-x>, <C-M-x>, <Esc>, <Enter>, <BS>, <Tab>, <Space>
#   and <Delete> notation.
# Prints both results plus MATCH/DIFF; exits non-zero on a difference.
#
# Requires emacs (GNU Emacs) on PATH. The comparison currently uses
# `current-column`, which counts display columns, so keep the corpus free of
# tabs and wide characters.
set -uo pipefail

text=$1
keys=$2

if ! command -v emacs >/dev/null 2>&1; then
  echo "EMACS NOT FOUND"
  exit 2
fi

root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

printf '%b' "$text" > "$tmp/buf"

# Translate the portable key notation into `kbd` notation.
# The combined-modifier form must be rewritten before the single ones.
ekeys=$(printf '%s' "$keys" | sed -E \
  -e 's/<C-M-([^>]+)>/C-M-\1/g' \
  -e 's/<C-([^>]+)>/C-\1/g' \
  -e 's/<M-([^>]+)>/M-\1/g' \
  -e 's/<Esc>/ESC/g' \
  -e 's/<Enter>/RET/g' \
  -e 's/<BS>/DEL/g' \
  -e 's/<Tab>/TAB/g' \
  -e 's/<Space>/SPC/g' \
  -e 's/<Delete>/<delete>/g')
# Escape the characters that would terminate the Elisp string early.
ekeys=${ekeys//\\/\\\\}
ekeys=${ekeys//\"/\\\"}

cat > "$tmp/probe.el" <<EOF
(setq coding-system-for-write 'utf-8-unix)
(with-current-buffer (find-file-noselect "$tmp/buf")
  (goto-char (point-min))
  (execute-kbd-macro (kbd "$ekeys"))
  (write-region (point-min) (point-max) "$tmp/eout" nil 'silent)
  (with-temp-file "$tmp/epos"
    (insert (format "%d %d" (line-number-at-pos)
                    (max 0 (1- (current-column)))))))
EOF

emacs -Q --batch -l "$tmp/probe.el" >/dev/null 2>&1

if [ ! -s "$tmp/epos" ]; then
  echo "EMACS FAILED"
  exit 2
fi

PROBE_TEXT="$text" PROBE_KEYS="$keys" PROBE_OUT="$tmp/iout" PROBE_POS="$tmp/ipos" \
  bash -c "cd '$root' && cargo test --test emacs_probe probe -- --nocapture" >/dev/null 2>&1

if [ ! -s "$tmp/ipos" ]; then
  echo "IJEVIM FAILED"
  exit 2
fi

read -r eline ecol < "$tmp/epos"
read -r iline icol < "$tmp/ipos"

# Both editors write a trailing newline for a text buffer; normalise before
# comparing so the last line's separator does not decide the result.
etrim=$(perl -0777 -pe 's/\n+\z//' < "$tmp/eout")
itrim=$(perl -0777 -pe 's/\n+\z//' < "$tmp/iout")

status=MATCH
[ "$etrim" = "$itrim" ] || status=DIFF
[ "$iline" = "$eline" ] || status=DIFF
[ "$icol" = "$ecol" ] || status=DIFF

if [ "$status" = MATCH ]; then
  printf 'MATCH  buf=%q line=%s col=%s\n' "$itrim" "$iline" "$icol"
  exit 0
fi

printf 'DIFF   emacs: buf=%q line=%s col=%s\n' "$etrim" "$eline" "$ecol"
printf '       ije:   buf=%q line=%s col=%s\n' "$itrim" "$iline" "$icol"
exit 1
