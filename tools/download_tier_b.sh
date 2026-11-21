#!/usr/bin/env bash
# download_tier_b.sh — повторное скачивание local-dev файлов Tier B
# (итерация 6, Phase 2.2 — лицензионная чистота).
#
# 60 GPL/неясных файлов в corpus/tier_b/local/ НЕ хранятся в git
# (public repo: Apache 2.0/ + GPL = лицензионный конфликт; .gitignore).
# После clone — этот скрипт восстанавливает их локально для тестов.
#
# Данные:
#   * corpus/manifest.toml [[tier_b_files]] tier="local-dev" → local путь
#     (куда) + полный sha256 (контроль);
#   * corpus/tier_b/PROVENANCE.md §1, таблица local-dev → «Путь в репо»
#     (upstream-путь в LinuxCNC; URL в manifest не используется: в нём вшито
#     локальное имя файла — генерационный артефакт, upstream даёт 404);
#   * upstream: raw.githubusercontent.com/LinuxCNC/linuxcnc/master/<repo-path>.
#
# Идемпотентно: существующий валидный файл (sha256 совпал) — skip.
# Использование: tools/download_tier_b.sh [корень-репо]
# Коды выхода: 0 — все файлы на месте и валидны; 1 — есть ошибки (список выше).

set -u
ROOT="${1:-$(cd "$(dirname "$0")/.." && pwd)}"
MANIFEST="$ROOT/corpus/manifest.toml"
PROVENANCE="$ROOT/corpus/tier_b/PROVENANCE.md"
DEST_BASE="$ROOT/corpus"
UPSTREAM_RAW="https://raw.githubusercontent.com/LinuxCNC/linuxcnc/master"

for f in "$MANIFEST" "$PROVENANCE"; do
    if [ ! -f "$f" ]; then
        echo "error: файл не найден: $f" >&2
        exit 1
    fi
done

# --- manifest: local-dev → local-путь<TAB>sha256 ----------------------------
manifest_records=$(awk '
    /^\[\[tier_b_files\]\]/ { flush(); inblock = 1; path = ""; sha = ""; tier = ""; next }
    /^\[\[/                { flush(); inblock = 0; next }
    inblock && /^path = /   { gsub(/^path = "|"$/, ""); path = $0; next }
    inblock && /^sha256 = / { gsub(/^sha256 = "|"$/, ""); sha = $0; next }
    inblock && /^tier = /   { gsub(/^tier = "|"$/, ""); tier = $0; next }
    function flush() {
        if (tier == "local-dev" && path != "")
            printf "%s\t%s\n", path, sha
    }
    END { flush() }
' "$MANIFEST")

# --- PROVENANCE: имя файла → upstream-путь в репо (таблица §1) --------------
declare -A REPO_PATH
while IFS= read -r line; do
    # | `lcnc_x.ngc` | `dir/file.ngc` | байты | `sha12` |
    name=$(printf '%s' "$line" | awk -F'|' '{ gsub(/[`[:space:]]/, "", $2); print $2 }')
    rpath=$(printf '%s' "$line" | awk -F'|' '{ gsub(/[`[:space:]]/, "", $3); print $3 }')
    [ -n "$name" ] && [ -n "$rpath" ] && REPO_PATH[$name]="$rpath"
done < <(grep -E '^\| `lcnc_' "$PROVENANCE")

if [ "${#REPO_PATH[@]}" -eq 0 ]; then
    echo "error: PROVENANCE не содержит таблицы local-dev (lcnc_*)" >&2
    exit 1
fi

sha_cmd() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
    elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | cut -d' ' -f1
    else return 127; fi
}

total=0; ok=0; skipped=0; failed=0
while IFS=$'\t' read -r rel sha; do
    [ -z "$rel" ] && continue
    total=$((total + 1))
    dest="$DEST_BASE/$rel"
    name=$(basename "$rel")

    # 1) уже есть и валиден → skip (идемпотентно)
    if [ -f "$dest" ]; then
        actual=$(sha_cmd "$dest")
        if [ "$actual" = "127" ]; then
            echo "skip (sha256sum/shasum недоступен, файл существует): $rel"
            skipped=$((skipped + 1)); continue
        fi
        if [ -n "$sha" ] && [ "$actual" != "$sha" ]; then
            echo "error: $rel: sha256 mismatch, перекачиваю (ожидалось $sha, есть $actual)" >&2
        else
            skipped=$((skipped + 1)); continue
        fi
    fi

    # 2) upstream-путь из PROVENANCE
    rpath="${REPO_PATH[$name]:-}"
    if [ -z "$rpath" ]; then
        echo "error: $rel: нет строки в PROVENANCE (имя '$name')" >&2
        failed=$((failed + 1)); continue
    fi
    raw="$UPSTREAM_RAW/$rpath"

    mkdir -p "$(dirname "$dest")"
    echo "download: $rel <- $raw"
    if ! curl -fsSL --retry 2 -o "$dest" "$raw"; then
        echo "error: $rel: скачивание не удалось: $raw" >&2
        failed=$((failed + 1)); continue
    fi

    # 3) контроль sha256
    if [ -n "$sha" ]; then
        actual=$(sha_cmd "$dest")
        if [ "$actual" != "$sha" ]; then
            echo "error: $rel: sha256 mismatch (ожидалось $sha, получено $actual)" >&2
            failed=$((failed + 1)); continue
        fi
    fi
    ok=$((ok + 1))
done <<EOF2
$manifest_records
EOF2

echo "итог: всего=$total скачано=$ok пропущено(валидны)=$skipped ошибок=$failed"
if [ "$failed" -ne 0 ]; then
    echo "ЧАСТИЧНО: часть local-dev файлов не восстановлена (см. error выше)." >&2
    exit 1
fi
echo "OK: все local-dev файлы Tier B на месте и валидны."
