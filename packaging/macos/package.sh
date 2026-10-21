#!/usr/bin/env bash
# Build, sign and (optionally) notarize the macOS release artifacts:
#
#   $DIST/figcraft-<version>-macos-<arch>.dmg          FigCraft.app on a drag-to-Applications DMG
#   $DIST/figcraft-cli-<version>-macos-<arch>.zip      the headless CLI (when apps/figcraft-cli exists)
#
# Both carry the Spine hosts (figcraft-spine-host-4.2/-4.3, docs/spine.md) when the spine-runtimes
# checkouts are there (SPINE_RUNTIMES_DIR, default ../spine-runtimes; `cargo xtask spine-host
# --fetch` makes them); without them Spine 4.2 files won't open in the build (a warning here).
#
# Usage: packaging/macos/package.sh [--arch universal|aarch64|x86_64] [--skip-build]
#
# Signing (env):
#   MACOS_SIGN_IDENTITY   codesign identity (name or SHA-1). Default "-" = ad-hoc (local testing;
#                         Gatekeeper will reject the result on other Macs). In CI, import-cert.sh sets it.
#   MACOS_KEYCHAIN        keychain holding the identity (optional)
# Notarization (env; all three needed, and a real identity):
#   APPLE_ID, APPLE_PASSWORD (app-specific password), APPLE_TEAM_ID
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/macos"

ARCH=universal
SKIP_BUILD=0
while [ $# -gt 0 ]; do
  case "$1" in
    --arch) ARCH="$2"; shift 2 ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    -h | --help) sed -n '2,20p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$ARCH" in
  universal) TARGETS=(aarch64-apple-darwin x86_64-apple-darwin); OSX_ARCHS="arm64;x86_64" ;;
  aarch64) TARGETS=(aarch64-apple-darwin); OSX_ARCHS="arm64" ;;
  x86_64) TARGETS=(x86_64-apple-darwin); OSX_ARCHS="x86_64" ;;
  *) echo "unknown --arch $ARCH" >&2; exit 2 ;;
esac

# Keep in sync with LSMinimumSystemVersion in Info.plist.in.
export MACOSX_DEPLOYMENT_TARGET=11.0
IDENTITY="${MACOS_SIGN_IDENTITY:--}"
SHORT_VERSION="${VERSION%%-*}"
WORK="$CARGO_TARGET_DIR/macos-package"
APP="$WORK/FigCraft.app"
DMG="$DIST/figcraft-$VERSION-macos-$ARCH.dmg"
CLI_ZIP="$DIST/figcraft-cli-$VERSION-macos-$ARCH.zip"

NOTARIZE=0
if [ "$IDENTITY" = "-" ]; then
  warn "macOS: ad-hoc signing (no MACOS_SIGN_IDENTITY); artifacts are not notarized"
elif [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_PASSWORD:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ]; then
  NOTARIZE=1
else
  warn "macOS: APPLE_ID / APPLE_PASSWORD / APPLE_TEAM_ID incomplete; signed but not notarized"
fi

echo "==> FigCraft $VERSION for macOS ($ARCH; ${BINS[*]}), identity: $IDENTITY, notarize: $NOTARIZE"

# ---- build -------------------------------------------------------------------------------------
if [ "$SKIP_BUILD" = 0 ]; then
  args=()
  for t in "${TARGETS[@]}"; do args+=(--target "$t"); done
  (cd "$ROOT" && cargo build --release --locked "${CARGO_PKGS[@]}" "${args[@]}")
fi

rm -rf "$WORK"
mkdir -p "$WORK/bin"
for bin in "${BINS[@]}"; do
  inputs=()
  for t in "${TARGETS[@]}"; do inputs+=("$CARGO_TARGET_DIR/$t/release/$bin"); done
  lipo -create -output "$WORK/bin/$bin" "${inputs[@]}"
  lipo -info "$WORK/bin/$bin"
done

# ---- Spine hosts -------------------------------------------------------------------------------
# The official spine-cpp in processes of their own (tools/spine-host), built for the same
# architectures and deployment target as the app. They and the Spine Runtimes License they are
# redistributed under ship beside the executables.
SPINE_HOSTS="$WORK/spine-host"
SPINE_RUNTIMES="${SPINE_RUNTIMES_DIR:-$ROOT/../spine-runtimes}"
HOST_BINS=()
if [ -d "$SPINE_RUNTIMES" ]; then
  (cd "$ROOT" && CMAKE_OSX_ARCHITECTURES="$OSX_ARCHS" cargo xtask spine-host --out "$SPINE_HOSTS" --tag "macos-$ARCH")
  for h in "$SPINE_HOSTS"/figcraft-spine-host-*; do
    [ -f "$h" ] || continue
    lipo -info "$h"
    HOST_BINS+=("$(basename "$h")")
  done
else
  warn "macOS: no spine-runtimes in $SPINE_RUNTIMES: packaged without the Spine hosts (Spine 4.2 files won't open)"
fi

# Copy the Spine hosts and their licence into folder $1.
copy_spine_hosts() {
  local h
  for h in ${HOST_BINS[@]+"${HOST_BINS[@]}"}; do cp "$SPINE_HOSTS/$h" "$1/"; done
  if [ ${#HOST_BINS[@]} -gt 0 ]; then cp "$SPINE_HOSTS/LICENSE-spine-runtimes.txt" "$2/"; fi
}

# ---- signing helpers ---------------------------------------------------------------------------
sign() {
  # Hardened runtime + secure timestamp for a real identity; ad-hoc can't be timestamped.
  local ts=(--timestamp)
  [ "$IDENTITY" = "-" ] && ts=(--timestamp=none)
  local kc=()
  [ -n "${MACOS_KEYCHAIN:-}" ] && kc=(--keychain "$MACOS_KEYCHAIN")
  codesign --force --sign "$IDENTITY" ${kc[@]+"${kc[@]}"} "${ts[@]}" "$@"
}

notarize() {
  local file="$1" out id status
  echo "==> notarizing $(basename "$file") (this can take a few minutes)"
  out="$(xcrun notarytool submit "$file" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" \
    --team-id "$APPLE_TEAM_ID" --wait --timeout 1h --output-format json)" || true
  echo "$out"
  id="$(printf '%s' "$out" | plutil -extract id raw -o - - 2>/dev/null || true)"
  status="$(printf '%s' "$out" | plutil -extract status raw -o - - 2>/dev/null || true)"
  if [ "$status" != "Accepted" ]; then
    if [ -n "$id" ]; then
      xcrun notarytool log "$id" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" || true
    fi
    echo "error: notarization of $(basename "$file") failed (status: ${status:-unknown})" >&2
    exit 1
  fi
}

# ---- icon --------------------------------------------------------------------------------------
# The .icns is generated here from the committed PNGs (assets/app-icon/figcraft-<size>.png, made by
# `cargo xtask icon`); it is never committed.
ICONS="$ROOT/assets/app-icon"
ICONSET="$WORK/FigCraft.iconset"
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  cp "$ICONS/figcraft-$s.png" "$ICONSET/icon_${s}x${s}.png"
  cp "$ICONS/figcraft-$((s * 2)).png" "$ICONSET/icon_${s}x${s}@2x.png"
done
iconutil -c icns -o "$WORK/FigCraft.icns" "$ICONSET"

# ---- FigCraft.app ------------------------------------------------------------------------------
echo "==> assembling $APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
# Executable and icon carry the display name (CFBundleExecutable / CFBundleIconFile).
cp "$WORK/bin/figcraft" "$APP/Contents/MacOS/FigCraft"
cp "$WORK/FigCraft.icns" "$APP/Contents/Resources/FigCraft.icns"
# The Spine hosts beside the executable, where the app looks for them.
mkdir -p "$APP/Contents/Resources/Licenses"
copy_spine_hosts "$APP/Contents/MacOS" "$APP/Contents/Resources/Licenses"
# Licences of the embedded craft-fonts fonts (only when built with CRAFT_FONTS_DIR).
if [ -n "${CRAFT_FONTS_DIR:-}" ]; then
  mkdir -p "$APP/Contents/Resources/Licenses"
  copy_font_licences "$APP/Contents/Resources/Licenses"
fi
sed -e "s/@VERSION@/$VERSION/g" -e "s/@SHORT_VERSION@/$SHORT_VERSION/g" \
  -e "s/@BUILD_SHA@/${FIGCRAFT_BUILD_SHA:-unknown}/g" \
  "$HERE/Info.plist.in" >"$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist"
printf 'APPL????' >"$APP/Contents/PkgInfo"

# Sign inside-out: nested code first, then the bundle itself (no --deep on the final signature).
# Nested code: the Spine hosts (helper executables, hardened runtime, no entitlements) and the main
# executable.
for h in ${HOST_BINS[@]+"${HOST_BINS[@]}"}; do
  sign --options runtime --identifier "ai.storyteller.figcraft.${h#figcraft-}" "$APP/Contents/MacOS/$h"
done
sign --options runtime --entitlements "$HERE/entitlements.plist" "$APP/Contents/MacOS/FigCraft"
sign --options runtime --entitlements "$HERE/entitlements.plist" "$APP"
codesign --verify --strict --deep --verbose=2 "$APP"

if [ "$NOTARIZE" = 1 ]; then
  ditto -c -k --keepParent "$APP" "$WORK/FigCraft-notarize.zip"
  notarize "$WORK/FigCraft-notarize.zip"
  retry xcrun stapler staple "$APP"
  retry xcrun stapler validate "$APP"
  spctl --assess --type execute -vvv "$APP"
fi

# ---- DMG ---------------------------------------------------------------------------------------
echo "==> building $DMG"
STAGE="$WORK/dmg"
mkdir -p "$STAGE"
ditto "$APP" "$STAGE/FigCraft.app"
ln -s /Applications "$STAGE/Applications"
rm -f "$DMG" "$WORK/raw.dmg"
# makehybrid + convert builds the image without attaching a device, unlike `create -srcfolder`,
# which is flaky on CI runners ("Resource busy") and hangs in sandboxed sessions.
hdiutil makehybrid -hfs -hfs-volume-name "FigCraft" -hfs-openfolder "$STAGE" -o "$WORK/raw.dmg" "$STAGE"
hdiutil convert "$WORK/raw.dmg" -format UDZO -imagekey zlib-level=9 -o "$DMG"
rm -f "$WORK/raw.dmg"
sign "$DMG"
codesign --verify --strict --verbose=2 "$DMG"
if [ "$NOTARIZE" = 1 ]; then
  notarize "$DMG"
  retry xcrun stapler staple "$DMG"
  retry xcrun stapler validate "$DMG"
  spctl --assess --type open --context context:primary-signature -vvv "$DMG"
fi

# ---- CLI ---------------------------------------------------------------------------------------
if [ "$HAS_CLI" = 1 ]; then
  echo "==> building $CLI_ZIP"
  CLI_DIR="$WORK/figcraft-cli-$VERSION-macos-$ARCH"
  mkdir -p "$CLI_DIR"
  cp "$WORK/bin/figcraft-cli" "$CLI_DIR/"
  copy_docs "$CLI_DIR"
  copy_spine_hosts "$CLI_DIR" "$CLI_DIR"
  for h in ${HOST_BINS[@]+"${HOST_BINS[@]}"}; do
    sign --options runtime --identifier "ai.storyteller.figcraft.${h#figcraft-}" "$CLI_DIR/$h"
  done
  # Same Developer ID and hardened runtime as the app, with an explicit reverse-DNS identifier
  # (codesign would otherwise use the bare file name).
  sign --options runtime --identifier ai.storyteller.figcraft-cli "$CLI_DIR/figcraft-cli"
  codesign --verify --strict --verbose=2 "$CLI_DIR/figcraft-cli"
  rm -f "$CLI_ZIP"
  ditto -c -k --keepParent "$CLI_DIR" "$CLI_ZIP"
  # notarytool accepts a zip of the bare binary. A bare Mach-O can't carry a stapled ticket, so
  # Gatekeeper looks the notarization up online when a quarantined copy first runs.
  if [ "$NOTARIZE" = 1 ]; then notarize "$CLI_ZIP"; fi
  "$WORK/bin/figcraft-cli" --version
else
  warn "apps/figcraft-cli is not in this workspace: no CLI zip"
  rm -f "$CLI_ZIP"
fi

"$WORK/bin/figcraft" --version
echo "==> done (check the artifacts as shipped with packaging/macos/verify.sh --arch $ARCH)"
ls -lh "$DIST"/figcraft*-"$VERSION"-macos-"$ARCH".*
