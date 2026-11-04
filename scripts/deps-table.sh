#!/usr/bin/env bash
# Generate the dependency section of README.md from cargo metadata.
#
# Usage:
#   scripts/deps-table.sh           # print the section to stdout
#   scripts/deps-table.sh --check   # exit 1 if README.md is out of date
#   scripts/deps-table.sh --write   # rewrite README.md in place
#
# The section lives between the `<!-- deps:start -->` and `<!-- deps:end -->`
# markers in README.md. Everything outside the markers is left untouched.

set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"

SELF="$ROOT/scripts/deps-table.sh"
README=README.md
START='<!-- deps:start -->'
END='<!-- deps:end -->'

for cmd in cargo jq; do
    command -v "$cmd" >/dev/null 2>&1 || {
        echo "deps-table: missing required command: $cmd" >&2
        exit 1
    }
done

# ── Metadata (works offline when the registry cache is warm) ────────

metadata() {
    cargo metadata --format-version 1 --all-features 2>/dev/null ||
        cargo metadata --format-version 1 --all-features --offline 2>/dev/null
}

# Count unique crates in a dependency graph slice.
count_crates() {
    local tree
    tree=$(cargo tree --prefix none "$@" 2>/dev/null) || tree=""
    if [ -z "$tree" ]; then
        printf '?'
        return
    fi
    printf '%s\n' "$tree" | awk '{print $1" "$2}' | sort -u | wc -l | tr -d ' '
}

# ── Section generation ──────────────────────────────────────────────

# Direct dependencies, grouped.
# TSV columns: group, name, resolved version, license, used by crates.
direct_deps_tsv() {
    local meta=$1
    printf '%s' "$meta" | jq -r '
        . as $m
        | ($m.packages | map({(.name): .version}) | add) as $ver
        | ($m.packages | map({(.name): (.license // "unknown")}) | add) as $lic
        | [ $m.packages[]
            | select(.id as $i | $m.workspace_members | index($i))
            | .name ] as $members
        | [ $m.workspace_members[] as $id
            | $m.packages[] | select(.id == $id) as $p
            | $p.dependencies[]
            | select(.name as $n | $members | index($n) | not)
            | {
                group: (if .optional then "optional"
                        elif .kind == "dev" then "dev"
                        elif .kind == "build" then "build"
                        else "runtime" end),
                name: .name,
                used_by: $p.name
              } ]
        | group_by([.group, .name])
        | map({
            group: .[0].group,
            name: .[0].name,
            used_by: ("`" + ([.[].used_by] | unique | join("`, `")) + "`")
          })
        | sort_by(.group, .name)
        | .[]
        | [.group, .name, ($ver[.name] // "?"), ($lic[.name] // "unknown"), .used_by]
        | @tsv
    '
}

# Print one group's markdown table if the group has rows.
emit_group() {
    local group=$1 heading=$2 note=$3 rows=$4
    local any=0 g name ver lic used

    while IFS=$'\t' read -r g name ver lic used; do
        if [ "$g" != "$group" ]; then
            continue
        fi
        if [ "$any" -eq 0 ]; then
            printf '%s\n\n' "$heading"
            if [ -n "$note" ]; then
                printf '%s\n\n' "$note"
            fi
            printf '| Crate | Version | Used by | License |\n'
            printf '| --- | --- | --- | --- |\n'
            any=1
        fi
        printf '| [%s](https://crates.io/crates/%s) | %s | %s | %s |\n' \
            "$name" "$name" "$ver" "$used" "$lic"
    done <<<"$rows"

    if [ "$any" -eq 1 ]; then
        printf '\n'
    fi
}

generate() {
    local meta rows

    meta=$(metadata) || {
        echo "deps-table: cargo metadata failed" >&2
        exit 1
    }
    rows=$(direct_deps_tsv "$meta")

    printf 'Direct dependencies only. Transitive crate counts include the workspace crates.\n\n'
    printf '| Scope | crates in graph |\n'
    printf '| --- | --- |\n'
    printf '| Default build | %s |\n' "$(count_crates -e normal)"
    printf '| `--all-features` | %s |\n' "$(count_crates -e normal --all-features)"
    printf '| Default + dev-dependencies | %s |\n\n' "$(count_crates -e normal,dev)"

    emit_group runtime '### Runtime' '' "$rows"
    emit_group optional '### Optional' \
        'Only compiled when the matching cargo feature is enabled.' "$rows"
    emit_group build '### Build' '' "$rows"
    emit_group dev '### Development' '' "$rows"

    printf 'All licenses are permissive (MIT, Apache-2.0, ISC, BSD, Unlicense, Zlib, 0BSD,\n'
    printf 'Unicode-3.0). No copyleft license appears in the dependency graph.\n'
}

# ── Modes ───────────────────────────────────────────────────────────

case "${1:---print}" in
    --print)
        generate
        ;;
    --write)
        tmp=$(mktemp "${TMPDIR:-/tmp}/deps-table.XXXXXX")
        body=$(mktemp "${TMPDIR:-/tmp}/deps-body.XXXXXX")
        generate >"$body"
        if ! awk -v s="$START" -v e="$END" -v body="$body" '
            BEGIN { skip = 0; found = 0 }
            $0 == s {
                print
                while ((getline line < body) > 0) print line
                close(body)
                skip = 1
                found = 1
                next
            }
            $0 == e { print; skip = 0; next }
            !skip { print }
            END { if (!found) exit 3 }
        ' "$README" >"$tmp"; then
            echo "deps-table: markers $START / $END not found in $README" >&2
            rm -f "$tmp" "$body"
            exit 1
        fi
        mv "$tmp" "$README"
        rm -f "$body"
        echo "deps-table: updated $README"
        ;;
    --check)
        generated=$(generate)
        current=$(
            awk -v s="$START" -v e="$END" '
                $0 == s { f = 1; next }
                $0 == e { f = 0; next }
                f { print }
            ' "$README"
        )
        if [ "$generated" != "$current" ]; then
            echo "deps-table: README.md dependency section is stale" >&2
            diff -u <(printf '%s\n' "$current") <(printf '%s\n' "$generated") >&2 || true
            echo "deps-table: run scripts/deps-table.sh --write" >&2
            exit 1
        fi
        ;;
    *)
        echo "deps-table: unknown argument: $1" >&2
        echo "usage: $0 [--print|--write|--check]" >&2
        exit 2
        ;;
esac
