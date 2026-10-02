#!/bin/zsh
# Films the menu's background footage in the game and edits it into the two loops the menu plays:
# crates/app/assets/video/menu-wide.mp4 (16:9, 1920 × 1080, at most about 100 MB) and
# menu-tall.mp4 (a phone held upright, 1080 × 2338 with the game's own portrait framing, at most
# about 20 MB), in HEVC. They are generated files, kept out of git (`.gitignore`): the game finds
# them on disk (`video::find`) and keeps its night sky without them. Eight shots of the autopilot
# on the four circuits, chase and orbit cameras, cut one after the other; the menu flashes on
# each cut (`CUTS` in crates/app/src/menu/mod.rs, keep both in step).
#
# Run from anywhere after `cargo build --release --bin mars-racer`; needs ffmpeg with libx265.
# The game runs with its window hidden and silent (MARS_FILM, see crates/app/src/debug.rs); about
# fifteen minutes. `--encode` only re-encodes the loops from the last filming's shots.
set -eu
cd "$(dirname "$0")/../.."
BIN=target/release/mars-racer
WORK="${TMPDIR:-/tmp}/planet-tracks-montage"
OUT=crates/app/assets/video
FILM=1
[[ ${1:-} == --encode ]] && FILM=0

# name | map | orbit camera (yaw°, distance, height; empty for the chase camera) | from (s) | length (s)
SHOTS=(
  "ares-side|Ares Vallis|90,9,1.4|13.0|2.6"
  "noctis-chase|Noctis||9.0|2.2"
  "jezero-front|Jezero|35,7,1.6|22.0|2.4"
  "noctis-rear|Noctis|150,7,2.2|12.0|2.2"
  "ares-chase|Ares Vallis||4.0|2.2"
  "noctis-dust|Noctis|150,7,2.2|18.5|2.2"
  "olympus-chase|Olympus||18.0|2.2"
  "jezero-start|Jezero|35,7,1.6|6.0|2.4"
)

mkdir -p "$OUT"
for format in wide tall; do
  # The window's logical size: the game renders at twice it.
  if [[ $format == wide ]]; then window=960x540; else window=540x1169; fi
  list="${WORK:?}/$format.txt"
  mkdir -p "${WORK:?}/$format"
  : > "$list"
  for shot in $SHOTS; do
    IFS='|' read -r name map orbit from secs <<< "$shot"
    frames="${WORK:?}/$format/$name"
    if (( FILM )); then
      rm -rf "${frames:?}"
      env MARS_NO_AUDIO=1 MARS_NO_HUD=1 MARS_AUTODRIVE=1 MARS_MAP="$map" MARS_WINDOW=$window \
        MARS_FILM="$frames" MARS_FILM_FROM=$from MARS_FILM_SECONDS=$secs ${orbit:+MARS_ORBIT=$orbit} \
        "$BIN" > /dev/null 2>&1
      count=$(( ${secs/./} * 3 ))
      # Each shot kept nearly lossless until the final encode.
      ffmpeg -loglevel error -y -framerate 30 -i "$frames/frame-%05d.bmp" -frames:v $count \
        -c:v libx264 -preset veryfast -crf 6 -pix_fmt yuv420p "$frames.mp4"
      rm -rf "${frames:?}"
      echo "$format $name: $count frames"
    fi
    echo "file '$frames.mp4'" >> "$list"
  done
  # The computer's loop at a near-transparent quality; the phone's in two passes at a fixed rate
  # that lands about 18 MB, tuned to keep the ground's grain rather than smooth it away.
  psy="aq-mode=3:psy-rd=2.0:psy-rdoq=3.0:log-level=error"
  encode=(ffmpeg -loglevel error -y -f concat -safe 0 -i "$list" -an -c:v libx265 -preset slow -tag:v hvc1 -pix_fmt yuv420p -movflags +faststart)
  if [[ $format == wide ]]; then
    $encode -crf 18 -x265-params log-level=error "$OUT/menu-$format.mp4"
  else
    $encode -b:v 8300k -x265-params "pass=1:stats=${WORK:?}/x265.log:$psy" -f mp4 /dev/null
    $encode -b:v 8300k -x265-params "pass=2:stats=${WORK:?}/x265.log:$psy" "$OUT/menu-$format.mp4"
  fi
  echo "$OUT/menu-$format.mp4: $(du -h "$OUT/menu-$format.mp4" | cut -f1)"
done
