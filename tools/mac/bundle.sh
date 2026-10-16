#!/bin/zsh
# Builds Planet Tracks' macOS apps (release profile) and packages them as .app bundles:
# "Planet Tracks.app" (binary mars-racer) and "PT Éditeur.app" (the track editor, mars-editor),
# each with its Info.plist, its icon (art/icons/, drawn by tools/mac/icon.py) and an ad hoc
# signature.
#
#     tools/mac/bundle.sh [--game] [--editor] [--out DIR] [--force] [--release]
#                         [--src DIR] [--editor-src DIR]
#
#   --game, --editor  the apps to make (both when neither is given)
#   --out DIR         where the apps go (~/Desktop by default)
#   --force           replace the apps already there (only bundles this script made)
#   --release         put the menu's films in the game's bundle (docs/release.md); without it
#                     the game finds them in the source tree it was built from, on this Mac only
#   --src DIR         the checkout to build from (the one this script is in by default)
#   --editor-src DIR  the checkout to build the editor from (--src by default): the editor is
#                     on its own branch until it is merged (.claude/worktrees/editor)
#
# Every build goes into the main checkout's target/ directory, shared by all the worktrees.
set -euo pipefail

export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"

here=${0:A:h:h:h}
icons="$here/art/icons"
out="$HOME/Desktop"
src="$here"
editor_src=""
force=0
release=0
apps=()
while (( $# )); do
    case "$1" in
        --game) apps+=(game) ;;
        --editor) apps+=(editor) ;;
        --out) out="${2:?--out needs a directory}"; shift ;;
        --src) src="${2:?--src needs a checkout}"; shift ;;
        --editor-src) editor_src="${2:?--editor-src needs a checkout}"; shift ;;
        --force) force=1 ;;
        --release) release=1 ;;
        -h|--help) awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0"; exit 0 ;;
        *) echo "error: unknown option $1 (--help lists them)" >&2; exit 1 ;;
    esac
    shift
done
(( ${#apps} )) || apps=(game editor)
apps=(${(u)apps})

die() { echo "error: $*" >&2; exit 1; }
toplevel() { git -C "$1" rev-parse --show-toplevel 2>/dev/null || die "$1 is not a checkout of the repository"; }
src=$(toplevel "$src")
editor_src=$(toplevel "${editor_src:-$src}")

# Per app: bundle name, binary, bundle id, icon, source checkout.
typeset -A name bin id icon from
name=(game "Planet Tracks" editor "PT Éditeur")
bin=(game mars-racer editor mars-editor)
id=(game com.marsracer.planettracks.mac editor com.marsracer.planettracks.editor.mac)
icon=(game game.png editor editor.png)
from=(game "$src" editor "$editor_src")

# The menu's films: generated, kept out of git, so a worktree has none; the main checkout does.
films=(menu-wide.mp4 menu-tall.mp4)
film_dir() {
    local common dir
    common=$(git -C "$1" rev-parse --path-format=absolute --git-common-dir)
    for dir in "$1/crates/app/assets/video" "${common:h}/crates/app/assets/video"; do
        [[ -f "$dir/$films[1]" && -f "$dir/$films[2]" ]] && { echo "$dir"; return 0; }
    done
    return 1
}

# Everything is checked before the builds, which take a while.
mkdir -p "$out"
out=${out:A}
for app in $apps; do
    if ! grep -q "^name = \"$bin[$app]\"" "$from[$app]/crates/app/Cargo.toml"; then
        [[ $app == editor ]] && die "$from[$app] has no binary $bin[$app]: pass --editor-src with a checkout of the editor's branch (.claude/worktrees/editor), or --game alone"
        die "$from[$app] has no binary $bin[$app]"
    fi
    [[ -f "$icons/$icon[$app]" ]] || die "$icons/$icon[$app] is missing: draw it with /usr/bin/python3 tools/mac/icon.py"
    dest="$out/$name[$app].app"
    if [[ -e "$dest" ]]; then
        (( force )) || die "$dest already exists: pass --force to replace it"
        # Never replace anything but this script's own bundle.
        have=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$dest/Contents/Info.plist" 2>/dev/null || true)
        [[ "$have" == "$id[$app]" ]] || die "$dest is not a bundle of $id[$app]: not replacing it"
    fi
done
if (( ${apps[(Ie)game]} )); then
    if (( release )); then
        film_dir "$src" >/dev/null || die "the menu's films are missing: make them with tools/video/menu_montage.sh (docs/release.md)"
    elif [[ ! -f "$src/crates/app/assets/video/$films[1]" ]]; then
        echo "warning: no film in $src/crates/app/assets/video: the menu will show the night sky (--release puts the films in the bundle)" >&2
    fi
fi

# A clone on APFS (it takes no space until either copy changes: the disk is nearly full), a
# copy elsewhere.
copy() { cp -c "$1" "$2" 2>/dev/null || cp "$1" "$2"; }

# Every checkout shares the fingerprints of app, track and physics in target/ (CLAUDE.md): forget
# them so that the binary is built from the checkout named, whichever built last.
forget() {
    find "$1/release/.fingerprint" -maxdepth 1 \( -name 'app-*' -o -name 'track-*' -o -name 'physics-*' \) \
        -exec rm -rf {} + 2>/dev/null || true
}

stage=$(mktemp -d "$out/.bundle.XXXXXX")
trap 'rm -rf "$stage"' EXIT

for app in $apps; do
    checkout=$from[$app]
    common=$(git -C "$checkout" rev-parse --path-format=absolute --git-common-dir)
    target="${common:h}/target"
    echo "== $name[$app]: $bin[$app] from $checkout"

    forget "$target"
    CARGO_TARGET_DIR="$target" cargo build --release --manifest-path "$checkout/Cargo.toml" --bin "$bin[$app]" \
        || { forget "$target"; exit 1; }
    forget "$target"

    bundle="$stage/$name[$app].app"
    mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
    copy "$target/release/$bin[$app]" "$bundle/Contents/MacOS/$bin[$app]"

    version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' "$checkout/Cargo.toml")
    build=$(git -C "$checkout" rev-list --count HEAD)
    cat > "$bundle/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
	<key>CFBundleName</key><string>$name[$app]</string>
	<key>CFBundleDisplayName</key><string>$name[$app]</string>
	<key>CFBundleIdentifier</key><string>$id[$app]</string>
	<key>CFBundleExecutable</key><string>$bin[$app]</string>
	<key>CFBundleIconFile</key><string>AppIcon</string>
	<key>CFBundlePackageType</key><string>APPL</string>
	<key>CFBundleShortVersionString</key><string>$version</string>
	<key>CFBundleVersion</key><string>$build</string>
	<key>LSMinimumSystemVersion</key><string>13.0</string>
	<key>NSHighResolutionCapable</key><true/>
	<key>LSApplicationCategoryType</key><string>public.app-category.racing-games</string>
</dict>
</plist>
EOF
    plutil -lint -s "$bundle/Contents/Info.plist"

    # The icon: every size macOS asks for, from the 1024 px source.
    iconset="$stage/$app.iconset"
    mkdir -p "$iconset"
    for size in 16 32 128 256 512; do
        sips -s format png -z $size $size "$icons/$icon[$app]" --out "$iconset/icon_${size}x${size}.png" >/dev/null
        sips -s format png -z $((size * 2)) $((size * 2)) "$icons/$icon[$app]" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
    done
    iconutil -c icns "$iconset" -o "$bundle/Contents/Resources/AppIcon.icns"

    # A release carries the menu's films where video::find looks first (docs/release.md).
    if [[ $app == game ]] && (( release )); then
        dir=$(film_dir "$src")
        mkdir -p "$bundle/Contents/Resources/video"
        for film in $films; do copy "$dir/$film" "$bundle/Contents/Resources/video/$film"; done
    fi

    codesign --force --sign - "$bundle"
    codesign --verify --strict "$bundle"
done

# The old apps are replaced only once every new one is ready.
for app in $apps; do
    dest="$out/$name[$app].app"
    rm -rf "$dest"
    mv "$stage/$name[$app].app" "$dest"
    echo "$dest"
done
