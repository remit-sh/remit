#!/usr/bin/env sh
# Toggle the `dev_bypass` setting in ~/.remit/config.toml.
# Preserves any other fields/sections in the file.

set -eu

CONFIG="${HOME}/.remit/config.toml"
mkdir -p "$(dirname "$CONFIG")"

# No file or no dev_bypass field — append as true.
if [ ! -f "$CONFIG" ] || ! grep -qE '^[[:space:]]*dev_bypass[[:space:]]*=' "$CONFIG"; then
    {
        [ -f "$CONFIG" ] && cat "$CONFIG"
        printf 'dev_bypass = true\n'
    } > "$CONFIG.tmp"
    mv "$CONFIG.tmp" "$CONFIG"
    echo "dev_bypass = true"
    exit 0
fi

# Flip in place. Match `true`/`false` after `dev_bypass =`, preserve the rest.
current=$(grep -m1 -E '^[[:space:]]*dev_bypass[[:space:]]*=' "$CONFIG" \
    | sed -E 's/^[[:space:]]*dev_bypass[[:space:]]*=[[:space:]]*([^[:space:]#]+).*/\1/')

case "$current" in
    true)  new="false" ;;
    false) new="true"  ;;
    *)
        echo "toggle-dev-bypass: unexpected value '$current' in $CONFIG" >&2
        exit 1
        ;;
esac

sed -E "s/^([[:space:]]*dev_bypass[[:space:]]*=[[:space:]]*)(true|false)/\1${new}/" "$CONFIG" > "$CONFIG.tmp"
mv "$CONFIG.tmp" "$CONFIG"

echo "dev_bypass = ${new}"
