#!/bin/sh
# Joshua+ installer for macOS and Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.sh | sh
#
# Downloads the latest release (or JOSHUA_VERSION=v0.5.0) from GitHub,
# verifies it against the release's SHA256SUMS and installs it:
#   macOS  → /Applications/Joshua+.app (or ~/Applications without admin rights)
#   Linux  → .deb via apt when available, otherwise an AppImage in ~/.local/bin
set -eu

REPO="fabandrade88/joshuaplus"
VERSION="${JOSHUA_VERSION:-}"

say() { printf '\033[1;33m▸\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m✗\033[0m %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "'$1' is required"; }

need curl
need uname

os=$(uname -s)
arch=$(uname -m)

case "$os" in
  Darwin)
    # A shell running under Rosetta reports x86_64 on Apple Silicon.
    if [ "$arch" = "arm64" ] || [ "$(sysctl -n hw.optional.arm64 2>/dev/null)" = "1" ]; then
      pattern='_aarch64\.dmg$'
    else
      die "Intel Macs are not supported by this beta yet (ONNX Runtime ships no Intel build); build from source instead"
    fi ;;
  Linux)
    [ "$arch" = "x86_64" ] || die "only x86_64 Linux builds are published for now (got $arch); build from source instead"
    if command -v apt-get >/dev/null 2>&1 && command -v dpkg >/dev/null 2>&1; then
      pattern='_amd64\.deb$'
    else
      pattern='_amd64\.AppImage$'
    fi ;;
  *) die "unsupported OS: $os (on Windows use install.ps1)" ;;
esac

# The release is found through github.com itself, not the REST API: the API
# allows 60 anonymous requests per hour per address, which shared networks
# (offices, schools, CI) run out of.
if [ -n "$VERSION" ]; then
  tag="$VERSION"
else
  say "Looking up the latest release…"
  latest=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest") ||
    die "could not reach GitHub"
  tag=${latest##*/tag/}
  [ "$tag" != "$latest" ] && [ -n "$tag" ] || die "no published release found"
fi
ver=${tag#v}
case "$pattern" in
  *dmg*) name="Joshua+_${ver}_aarch64.dmg" ;;
  *deb*) name="Joshua+_${ver}_amd64.deb" ;;
  *AppImage*) name="Joshua+_${ver}_amd64.AppImage" ;;
esac
base="https://github.com/$REPO/releases/download/$tag"
asset_url="$base/$(printf '%s' "$name" | sed 's/+/%2B/g')"
sums_url="$base/SHA256SUMS"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM
name=$(basename "$asset_url")
file="$tmp/pkg"

say "Downloading $(printf '%s' "$name" | sed 's/%2B/+/g') ($tag)"
curl -fL --proto '=https' --tlsv1.2 --progress-bar -o "$file" "$asset_url"
curl -fsSL --proto '=https' --tlsv1.2 -o "$tmp/SHA256SUMS" "$sums_url"

# Asset names may be URL-encoded in the download URL; match on the decoded name.
decoded=$(printf '%s' "$name" | sed 's/%2B/+/g; s/%20/ /g')
expected=$(grep -F "  $decoded" "$tmp/SHA256SUMS" | head -n 1 | cut -d' ' -f1 || true)
[ -n "$expected" ] || die "$decoded is not listed in SHA256SUMS"
if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$file" | cut -d' ' -f1)
else
  actual=$(shasum -a 256 "$file" | cut -d' ' -f1)
fi
[ "$actual" = "$expected" ] || die "checksum mismatch for $decoded (expected $expected, got $actual)"
say "Checksum verified"

case "$pattern" in
  *dmg*)
    mnt="$tmp/mnt"
    mkdir -p "$mnt"
    hdiutil attach -nobrowse -readonly -quiet -mountpoint "$mnt" "$file"
    app=$(find "$mnt" -maxdepth 1 -name '*.app' | head -n 1)
    [ -n "$app" ] || { hdiutil detach -quiet "$mnt"; die "no .app inside the disk image"; }
    dest="/Applications"
    [ -w "$dest" ] || { dest="$HOME/Applications"; mkdir -p "$dest"; }
    rm -rf "$dest/Joshua+.app"
    cp -R "$app" "$dest/"
    hdiutil detach -quiet "$mnt"
    # The beta is not notarised yet; the checksum above is what vouches for it.
    xattr -dr com.apple.quarantine "$dest/Joshua+.app" 2>/dev/null || true
    say "Installed to $dest/Joshua+.app"
    say "Start it from Launchpad or run: open \"$dest/Joshua+.app\"" ;;
  *deb*)
    cp "$file" "$tmp/$decoded"
    say "Installing with apt (sudo may ask for your password)"
    sudo apt-get install -y "$tmp/$decoded"
    say "Installed. Start Joshua+ from your applications menu." ;;
  *AppImage*)
    mkdir -p "$HOME/.local/bin"
    install -m 0755 "$file" "$HOME/.local/bin/joshua-plus"
    say "Installed to ~/.local/bin/joshua-plus (make sure ~/.local/bin is on your PATH)" ;;
esac
