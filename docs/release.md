# Release checklist

## Bundle the menu's films

The menu plays a film of the game behind its screens: `menu-wide.mp4` (1920 × 1080, about
160 MB) and `menu-tall.mp4` (1080 × 2338, about 19 MB), both HEVC. They are generated files,
kept out of git (`.gitignore`: too large for GitHub, and every refilm would add their weight to
the history). The executable does not embed them: `video::find` (`crates/app/src/video.rs`)
looks for them on disk, and without them the menu quietly falls back to its night sky. A release
can therefore ship without them and still look like it works, so before every release:

1. **Refilm them with the release build**, after the last change to the circuits' look:

       cargo build --release --bin mars-racer
       tools/video/menu_montage.sh

   They land in `crates/app/assets/video/` (about fifteen minutes; `--encode` re-encodes from
   the last filming without refilming).

2. **Put them in the package**, where `video::find` looks:

   | Platform | Files | Where |
   |---|---|---|
   | macOS (`Planet Tracks.app`) | `menu-wide.mp4`, `menu-tall.mp4` | `Contents/Resources/video/`: `tools/mac/bundle.sh --release` copies them there (below) |
   | iOS | `menu-tall.mp4` | the bundle's `video/` folder: `ios/build-rust.sh` copies it there (a Release build fails without it) |
   | Android | none yet | no decoder: the menu shows the night sky |

   On iOS, an iPad held in landscape takes the wide layout and plays `menu-wide.mp4`: ship a
   wide encode within the phone's budget (about 20 MB, two-pass like the tall one in
   `menu_montage.sh`), or accept the night sky there.

3. **Check the package away from the repository.** On the build machine, a missing file is
   hidden by the last place `video::find` looks, the source tree's `crates/app/assets/video/`.
   Run the packaged app with that folder moved aside (or on another machine): the title screen
   must show the film, and a missing clip prints `menu film menu-wide.mp4 not found` on the
   standard error. On macOS, building the package from a worktree (`--src`) does the same
   without moving anything: a worktree has no films, the script takes them from the main
   checkout, and the binary only looks in its own source tree. Then, hidden and muted:

       cd / && MARS_NO_AUDIO=1 MARS_MENU=title MARS_SHOTS=/tmp/check MARS_SHOT_TIMES=5 \
           MARS_BENCH=0,8 MARS_EXIT_AFTER=8.5 "DIR/Planet Tracks.app/Contents/MacOS/mars-racer"

   (`MARS_BENCH` alone counts as a race self-test and skips the menu: `MARS_MENU=title` keeps it.)

## Package the macOS app

    tools/mac/bundle.sh --game --release --out DIR

builds `mars-racer` with the release profile and makes `DIR/Planet Tracks.app`: the binary in
`Contents/MacOS/`, an `Info.plist` (`com.marsracer.planettracks.mac`, the workspace's version,
the commit count as the build number), `AppIcon.icns` from `art/icons/game.png`, the films in
`Contents/Resources/video/`, and an ad hoc signature. Without `--release` the films stay out:
that is the Desktop app of the machine that builds it, which plays them from its source tree.

It is not yet a package for other Macs:

- **The signature is ad hoc.** Another Mac's Gatekeeper refuses it once downloaded: it needs a
  Developer ID Application signature with the hardened runtime, notarized (`xcrun notarytool`)
  and stapled.
- **The session is saved in the source tree** (`session::path`: `tuning/session.json` beside
  the sources the binary was built from). Elsewhere that folder does not exist and the records
  are not kept: on macOS it belongs in `~/Library/Application Support/`.
- **It is arm64 only** (cargo's host target): an Intel Mac needs `x86_64-apple-darwin` too,
  joined with `lipo`.
