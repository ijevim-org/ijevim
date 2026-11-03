#!/usr/bin/env bash
# Differential probe: compare ijevim's Vim keymap against real Vim.
#
# Usage: scripts/vim-diff.sh '<buffer text with \n>' '<keys>'
#   keys use Vim's <Esc>/<Enter>/<BS>/<Tab> notation.
# Prints both results plus MATCH/DIFF; exits non-zero on a difference.
set -uo pipefail

text=$1
keys=$2

root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# Translate the portable key notation into Vim's double-quoted string escapes.
vkeys=$keys
vkeys=${vkeys//<Esc>/\\<Esc>}
vkeys=${vkeys//<Enter>/\\<CR>}
vkeys=${vkeys//<BS>/\\<BS>}
vkeys=${vkeys//<Tab>/\\<Tab>}
vkeys=${vkeys//<Space>/ }

printf '%b' "$text" > "$tmp/buf"

# Ex mode leaves the cursor on the last line; start from the top like a real
# interactive session.
cat > "$tmp/probe.vim" <<EOF
call cursor(1, 1)
execute "normal! " . "$vkeys"
call writefile([string(line('.') - 1) . " " . string(col('.') - 1)], "$tmp/vpos")
silent write! $tmp/vout
qa!
EOF

vim -N -u NONE -i NONE -n -es -S "$tmp/probe.vim" "$tmp/buf" >/dev/null 2>&1

if [ ! -s "$tmp/vpos" ]; then
  echo "VIM FAILED"
  exit 2
fi

PROBE_TEXT="$text" PROBE_KEYS="$keys" PROBE_OUT="$tmp/iout" PROBE_POS="$tmp/ipos" \
  bash -c "cd '$root' && cargo test --test vim_probe probe -- --nocapture" >/dev/null 2>&1

if [ ! -s "$tmp/ipos" ]; then
  echo "IJEVIM FAILED"
  exit 2
fi

read -r vline vcol < "$tmp/vpos"
read -r iline icol < "$tmp/ipos"

# Vim writes a trailing newline; normalise it away before comparing.
vtrim=$(perl -0777 -pe 's/\n+\z//' < "$tmp/vout")
itrim=$(perl -0777 -pe 's/\n+\z//' < "$tmp/iout")

# ijevim uses 1-based lines, Vim 0-based.
iline=$((iline - 1))

status=MATCH
[ "$vtrim" = "$itrim" ] || status=DIFF
[ "$iline" = "$vline" ] || status=DIFF
[ "$icol" = "$vcol" ] || status=DIFF

if [ "$status" = MATCH ]; then
  printf 'MATCH  buf=%q line=%s col=%s\n' "$itrim" "$((vline + 1))" "$vcol"
  exit 0
fi

printf 'DIFF   vim: buf=%q line=%s col=%s\n' "$vtrim" "$((vline + 1))" "$vcol"
printf '       ije: buf=%q line=%s col=%s\n' "$itrim" "$((iline + 1))" "$icol"
exit 1
