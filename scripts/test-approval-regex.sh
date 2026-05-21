#!/usr/bin/env bash
# Test harness for the regex in claude-check-approval.sh.
# Run via: bash scripts/test-approval-regex.sh

pattern='(^|[[:space:];&(|])git[[:space:]]+([^;&|()]*[[:space:]])?push([[:space:]]|$|;|&|\))'

G=git
P=push
pass=0
fail=0

test_match() {
  local cmd="$1"
  local expected="$2"
  if [[ "$cmd" =~ $pattern ]]; then
    actual="BLOCK"
  else
    actual="PASS"
  fi
  if [[ "$actual" == "$expected" ]]; then
    printf "  %-6s %-55s [OK]\n" "$actual" "$cmd"
    pass=$((pass + 1))
  else
    printf "  %-6s %-55s [WANT %s]\n" "$actual" "$cmd" "$expected"
    fail=$((fail + 1))
  fi
}

echo "Should BLOCK (legit ${G} ${P} variants):"
test_match "${G} ${P}" BLOCK
test_match "${G} ${P} origin HEAD:main" BLOCK
test_match "  ${G} ${P}" BLOCK
test_match "${G} -C /tmp/foo ${P}" BLOCK
test_match "${G} --git-dir=/tmp/foo ${P}" BLOCK
test_match "${G} -c user.email=foo ${P}" BLOCK
test_match "cd worktree && ${G} ${P}" BLOCK
test_match "(${G} ${P})" BLOCK
test_match "${G} ${P}; ls" BLOCK
test_match "ls && ${G} ${P} origin HEAD:main" BLOCK

echo ""
echo "Known-gap (architect: accept):"
test_match "/usr/bin/${G} ${P}" PASS

echo ""
echo "Should PASS (not a ${P}):"
test_match "ls" PASS
test_match "${G} status" PASS
test_match "${G} ${P}down" PASS
test_match "${G} --version" PASS

echo ""
echo "False positives — these PASS (quoted/data-only — acceptable):"
test_match "echo '${G} ${P}'" PASS
test_match "# ${G} ${P} (comment)" PASS

echo ""
echo "Result: ${pass} pass, ${fail} fail"
[[ $fail -eq 0 ]]
