#!/usr/bin/env bash

# Extensions considered "code"
declare -A exts=(
  ["rs"]=0
  ["js"]=0
  ["ts"]=0
  ["py"]=0
  ["c"]=0
  ["cpp"]=0
  ["h"]=0
  ["hpp"]=0
  ["java"]=0
  ["go"]=0
  ["lua"]=0
  ["sh"]=0
  ["html"]=0
  ["css"]=0
)

total=0

# List tracked files in git
while IFS= read -r file; do
  ext="${file##*.}"

  if [[ -n "${exts[$ext]+x}" && -f "$file" ]]; then
    lines=$(wc -l < "$file")
    exts[$ext]=$(( exts[$ext] + lines ))
    total=$(( total + lines ))
  fi
done < <(git ls-files)

echo "Lines of code by language:"
for ext in "${!exts[@]}"; do
  if [[ ${exts[$ext]} -gt 0 ]]; then
    printf "%-6s %10d\n" ".$ext" "${exts[$ext]}"
  fi
done | sort

echo "-------------------------"
echo "Total: $total"
