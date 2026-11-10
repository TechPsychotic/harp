#!/bin/sh
# Puts Harp's Android release signing key into the GitHub repo's Actions secrets,
# so tagged releases are signed with the same key as your local builds.
# Run it yourself once (the key never leaves your machine except into the
# repo's encrypted secrets):   scripts/release-secrets.sh
set -eu
D="$HOME/.local/share/harp-signing"
REPO="${1:-TechPsychotic/harp}"
[ -f "$D/harp-release.jks" ] && [ -f "$D/key.properties" ] || { echo "no signing key in $D"; exit 1; }
PW=$(grep '^storePassword=' "$D/key.properties" | cut -d= -f2-)
ALIAS=$(grep '^keyAlias=' "$D/key.properties" | cut -d= -f2-)
base64 -w0 "$D/harp-release.jks" | gh secret set HARP_KEYSTORE_B64 --repo "$REPO"
printf '%s' "$PW" | gh secret set HARP_KEYSTORE_PASSWORD --repo "$REPO"
printf '%s' "$PW" | gh secret set HARP_KEY_PASSWORD --repo "$REPO"
printf '%s' "$ALIAS" | gh secret set HARP_KEY_ALIAS --repo "$REPO"
gh secret list --repo "$REPO"
echo "Done. Keep a copy of $D somewhere safe: without it, future updates can't install over this version."
