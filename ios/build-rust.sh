#!/bin/zsh
# Xcode's build phase: builds the game (crates/app, binary mars-racer) for the iPhone or the
# simulator and puts it in the app bundle as its executable; Xcode then signs the bundle.
# Debug builds use cargo's dev profile (optimised, see the workspace's Cargo.toml), Release its
# release profile.
set -euo pipefail

case "${PLATFORM_NAME:?run from Xcode}" in
    iphoneos) triple=aarch64-apple-ios ;;
    iphonesimulator) triple=aarch64-apple-ios-sim ;;
    *) echo "error: unsupported platform $PLATFORM_NAME" >&2; exit 1 ;;
esac
if [[ "${CONFIGURATION:-Debug}" == Release ]]; then
    profile=(--release); dir=release
else
    profile=(); dir=debug
fi

# The workspace's single target/ directory, in the main checkout, also for git worktrees.
repo=$(git -C "$SRCROOT" rev-parse --show-toplevel)
common=$(cd "$repo" && cd "$(git rev-parse --git-common-dir)" && pwd)
target="${common:h}/target"
# A worktree shares the app crate's fingerprint with every other checkout: remove it so cargo
# builds this checkout's sources (see CLAUDE.md).
forget() {
    if [[ "$(git -C "$repo" rev-parse --git-dir)" != "$(git -C "$repo" rev-parse --git-common-dir)" ]]; then
        find "$target/$triple/$dir/.fingerprint" -maxdepth 1 -name 'app-*' -exec rm -rf {} + 2>/dev/null || true
    fi
}

# Cargo runs outside Xcode's environment: its SDKROOT, compiler flags and deployment settings
# are for the app, not for the build scripts cargo compiles for the Mac.
vars=(HOME="$HOME" USER="$USER" TERM=dumb
    PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin"
    IPHONEOS_DEPLOYMENT_TARGET="$IPHONEOS_DEPLOYMENT_TARGET" CARGO_TARGET_DIR="$target")
# The Xcode this build runs in, when several are installed.
[[ -n "${DEVELOPER_DIR:-}" ]] && vars+=(DEVELOPER_DIR="$DEVELOPER_DIR")
forget
env -i "${vars[@]}" cargo build --manifest-path "$repo/Cargo.toml" --bin mars-racer --target "$triple" "${profile[@]}"
forget

mkdir -p "$TARGET_BUILD_DIR/$EXECUTABLE_FOLDER_PATH"
cp "$target/$triple/$dir/mars-racer" "$TARGET_BUILD_DIR/$EXECUTABLE_PATH"

# The menu's film: a generated file kept out of git (tools/video/menu_montage.sh, docs/release.md),
# copied into the bundle where the game looks for it. A release cannot go without it.
film="$repo/crates/app/assets/video/menu-tall.mp4"
if [[ -f "$film" ]]; then
    mkdir -p "$TARGET_BUILD_DIR/$CONTENTS_FOLDER_PATH/video"
    cp "$film" "$TARGET_BUILD_DIR/$CONTENTS_FOLDER_PATH/video/"
elif [[ "${CONFIGURATION:-Debug}" == Release ]]; then
    echo "error: $film is missing: make it with tools/video/menu_montage.sh (docs/release.md)" >&2
    exit 1
else
    echo "warning: $film is missing: the menu shows the night sky (tools/video/menu_montage.sh)" >&2
fi
