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
   | macOS (`Planet Tracks.app`) | `menu-wide.mp4`, `menu-tall.mp4` | `Contents/Resources/video/` |
   | iOS | `menu-tall.mp4` | the bundle's `video/` folder: `ios/build-rust.sh` copies it there (a Release build fails without it) |
   | Android | none yet | no decoder: the menu shows the night sky |

   On iOS, an iPad held in landscape takes the wide layout and plays `menu-wide.mp4`: ship a
   wide encode within the phone's budget (about 20 MB, two-pass like the tall one in
   `menu_montage.sh`), or accept the night sky there.

3. **Check the package away from the repository.** On the build machine, a missing file is
   hidden by the last place `video::find` looks, the source tree's `crates/app/assets/video/`.
   Run the packaged app with that folder moved aside (or on another machine): the title screen
   must show the film, and a missing clip prints `menu film menu-wide.mp4 not found` on the
   standard error.
