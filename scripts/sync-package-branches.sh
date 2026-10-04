#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
[ "${1:-}" = "" ] || [ "$1" = "--push" ] || { echo "usage: $0 [--push]" >&2; exit 2; }
if [ -n "$(git -C "$root" status --porcelain --untracked-files=no)" ]; then
  echo "Commit main branch changes before syncing package branches." >&2
  exit 2
fi
stage=$(mktemp -d)
cleanup() {
  for package in tester spar-log spar-args spar-web spar-tcp spar-http-server; do
    if [ -d "$stage/$package" ]; then
      git -C "$root" worktree remove --force "$stage/$package" >/dev/null 2>&1 || true
    fi
  done
  rm -rf "$stage"
}
trap cleanup EXIT
for package in tester spar-log spar-args spar-web spar-tcp spar-http-server; do
  branch=$package
  checkout="$stage/$package"
  if git -C "$root" show-ref --verify --quiet "refs/heads/$branch"; then
    git -C "$root" worktree add --quiet "$checkout" "$branch"
    git -C "$checkout" rm -qr --ignore-unmatch .
  else
    git -C "$root" worktree add --quiet --detach "$checkout" HEAD
    git -C "$checkout" switch --quiet --orphan "$branch"
  fi
  git -C "$checkout" clean -qfdx
  tar -C "$root/packages/$package" --exclude=target --exclude=dist --exclude=spar.package.lock.spar --exclude='*.key' -cf - . | tar -C "$checkout" -xf -
  cp "$root/LICENSE" "$checkout/LICENSE"
  cat >> "$checkout/.gitignore" <<'IGNORE'
/target/
/dist/
/spar.package.lock.spar
*.key
IGNORE
  if [ "$package" = spar-tcp ]; then
    mkdir -p "$checkout/sdk"
    for sdk in spar-native-sys spar-native-macros spar-native; do
      mkdir -p "$checkout/sdk/$sdk"
      tar -C "$root/sdk/$sdk" --exclude=target --exclude=Cargo.lock --exclude='*.key' -cf - . | tar -C "$checkout/sdk/$sdk" -xf -
    done
    sed -i 's@../../sdk/spar-native@sdk/spar-native@' "$checkout/Cargo.toml"
  fi
  if [ -f "$checkout/spar.package.spar" ]; then
    sed -i -E 's@path:\.\./(tester|spar-log|spar-args|spar-web|spar-tcp|spar-http-server)@github:oraclevs/spar-libraries#\1@g' "$checkout/spar.package.spar"
  fi
  git -C "$checkout" add -A
  if ! git -C "$checkout" diff --cached --quiet; then
    git -C "$checkout" commit --quiet -m "chore: sync $package package snapshot"
  fi
  if [ "${1:-}" = "--push" ]; then
    git -C "$checkout" push --quiet -u origin "$branch"
  fi
  echo "$branch: $(git -C "$checkout" rev-parse --short HEAD)"
done
