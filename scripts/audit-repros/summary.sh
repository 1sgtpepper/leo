#!/usr/bin/env bash
set -u

result_file="${1:?result file required}"

echo "## Audit repro result"
echo
if [ -f "$result_file" ]; then
  sed -n '/^RESULT /p' "$result_file"
else
  echo "No result file was produced."
fi
