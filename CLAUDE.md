# mars-racer (codename)

A Trackmania-like racing game with its own world and car: a buggy racing on Mars, on the paved
roads of modern installations and on Martian dirt. Native first: Rust, macOS for the playable
demo, then iOS and Android from the same crates.

## Clean-room rule

This game must not derive from TrackMania's code or data. Never read, copy or port anything from
`~/Documents/Trackmania-Original` (TMNF-C in `physics/tmnf`, captured data in `data/tmnf`, `sim/`,
`web/src/physics`, `tools/memory`, `tools/physics`). Driving behaviour is designed here; the only
reference taken from the real game is its observed behaviour, summarised as numbers in
`docs/feel-targets/`.

## Toolchain

Rust comes from Homebrew's rustup (with the `aarch64-apple-ios` and `aarch64-apple-ios-sim`
targets), which is keg-only. Prefix commands with:

    export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"

The disk is nearly full: use the single workspace `target/` directory, never a second one.
A git worktree building into it (`CARGO_TARGET_DIR`) shares the app crate's output and
fingerprint with every other checkout (cargo hashes it by its workspace-relative path), so a
build can look fresh and run another checkout's binary: remove the app's fingerprints before and
after building in a worktree (`find target/debug/.fingerprint -maxdepth 1 -name 'app-*' -exec rm -rf {} +`;
a bare `rm target/debug/.fingerprint/app-*` aborts in zsh when nothing matches).

## Layout

- `crates/track` — block kit, maps, triangle meshes with a surface per triangle. Maps are
  `crates/track/maps/*.json`, written from a `.chain` file (blocks, landforms, props) by
  `cargo run -p track --example chain -- crates/track/maps/noctis.chain`; a new one is added to
  `BUILTIN_MAPS`. `cargo run -p physics --release --example lap -- Noctis [road dirt]` times the
  test autopilot over a map (with the two numbers it brakes for the bends).
  Roads follow `art/roads/brief.md` ("camp roads": a laminated tarp deck, sandbag or bumper
  edges per block variant, raised slabs on trusses and piers of red plastic tubes,
  `crates/track/src/stilts.rs`); `docs/blocks.md` is the reference of every block and variant,
  kept up to date with the code.
- `crates/physics` — deterministic vehicle physics at a fixed 100 Hz tick, gameplay presets.
- `crates/app` — the macOS and iOS executable (`cargo run --bin mars-racer`): winit, wgpu (Metal), egui, gilrs.
- `ios/` — the iOS app: `PlanetTracks.xcodeproj` (open it in Xcode and Run, on an iPhone or a
  simulator), whose only build phase, `ios/build-rust.sh`, builds `mars-racer` with cargo for
  `aarch64-apple-ios` or `aarch64-apple-ios-sim` (into the main checkout's `target/`, with the
  worktree fingerprint clean-up below) and makes it the app's executable; Xcode then signs it.
  From the command line: `xcodebuild -project ios/PlanetTracks.xcodeproj -scheme PlanetTracks
  -sdk iphonesimulator -derivedDataPath DIR build`. On iOS the window is the whole screen
  (`gfx::window_pixels`; winit's inner size is the safe area), drawn at 2× rather than 3×, put
  in the scene `Info.plist` declares (iOS 27 stops apps without one; winit 0.30 makes none, so
  `crates/app/src/ios.rs` attaches its window; the edges deferring system gestures, the hidden
  status bar and home indicator are asked again once it is there, or iOS holds back taps along
  the edges), and the session is saved in the app's Documents folder. The race's touch controls
  are the HUD's (`hud.rs`, from egui's touch events). egui clips painting to the safe area
  unless a painter's clip rect is set to `viewport_rect()`; `MARS_SAFE_AREA=59,0,34,0` gives a
  macOS self-test an iPhone's insets. Self-test variables reach the app as
  `SIMCTL_CHILD_MARS_MAP=Jezero xcrun simctl launch booted com.marsracer.planettracks`.
- `crates/app/src/menu/` — the game's menu, "Planet Tracks" (title with the tagline, one planet at
  a time, then the planet's circuits as tiles; no Solo/Multi choice, multiplayer will get one
  entry before the planets), in a wide layout (1280 × 720 design space) and a tall phone one
  (390 × 844), fitted to the window with egui's zoom. Its background is `menu_gfx.rs` with
  `shaders/menu.wgsl`: a film of the game in a loop (`crates/app/assets/video/menu-wide.mp4`,
  1920 × 1080, and `menu-tall.mp4`, 1080 × 2338, HEVC, decoded by AVFoundation in `video.rs` on
  macOS and iOS; elsewhere, or without the files, the night sky stays), the static and
  silhouette of a planet still to come. The loops are large generated files, kept out of git and
  found on disk (beside the executable, in the bundle's resources, or in the source tree): make
  them, and refilm them after changing the circuits' look, with `tools/video/menu_montage.sh`
  (its cuts are `CUTS` in `menu/mod.rs`; `--encode` re-encodes without refilming). A release
  must bundle them: `docs/release.md`. Its sounds are `ui_sound.rs`, its typefaces (Saira and Saira Italic, Saira
  Stencil One, Martian Mono, from Google Fonts under the OFL) `crates/app/assets/fonts/`.
  Self-tests: `MARS_MENU=title|planets|solo` opens it on a screen, `MARS_MENU_NAV=1.5:right,3:confirm`
  moves through it, `MARS_WINDOW=390x844` shows the phone layout; add `MARS_BENCH=0,<seconds>`
  to a screenshot run so every frame renders (otherwise a hidden window only draws the shot,
  before the film's first frame); race self-tests (`MARS_MAP`, `MARS_AUTODRIVE`…) skip it.
- `crates/app/src/hud.rs` — the race HUD, light like Trackmania's (map name, record and time to
  beat, chrono with the checkpoint gaps above it, speed in a ring coloured by the gear), plus the
  countdown and the finish card in the menu's style; wide and tall like the menu. On phones the
  buggy accelerates by itself, the bottom strip brakes, the screen's halves steer, and a settings
  button pauses the race and slides the scene up so the car shows above its sheet; on a
  computer Escape or the pad's Start opens the same sheet (`MARS_HUD_SETTINGS=seconds` opens it
  in a self-test; a hidden run draws the HUD only on its screenshot frames, so the sheet opens
  on one and needs another shot 0.3 s later). Everything for
  debugging (FPS, profile, telemetry, tuning panel in `ui.rs`) shows only with Tab, or
  `MARS_DEBUG_PANEL=1`.
- `tools/blender/build_buggy.py` — builds the buggy "B" (`art/buggy/buggy.blend`, rigged
  suspension, the registered plans as image empties) and exports `crates/app/assets/buggy.glb` and
  its livery atlas `buggy_livery.png`, which the app embeds: rerun
  `blender -b -P tools/blender/build_buggy.py` after changing the model (`-- --out DIR` writes
  elsewhere, for checks). Part and empty names (`arm_lo.FL`, `mount_bot.FL`, …) are what
  `crates/app/src/car_model.rs` reads. The body panels are in `tools/blender/buggy_body.py`, the
  livery (vector shapes, glass, grilles, seams) in `tools/blender/buggy_livery.py`, mesh helpers and
  materials in `tools/blender/meshkit.py`.
- Buggy plans: `art/buggy/v2/views/` holds the generated plans (`art/buggy/v2/3q_B_compact.jpg` is
  the validated look); `tools/blender/warp_refs.py` registers them at the physics' wheelbase and
  track into `art/buggy/v2/registered/` (scales in `tools/blender/blueprint.py`);
  `tools/blender/trace_livery.py` (plain Python with numpy, scipy, scikit-image, Pillow) traces
  their paint into clean polygons (`art/buggy/v2/livery/`); `tools/blender/compare.py` renders the
  model with cameras matching the plans and the 3/4 view and scores the silhouettes
  (`-- OUT_DIR [view ...]`).
- `tools/textures/bake.py` — bakes the surface textures the app embeds
  (`crates/app/assets/textures/`, loaded by `crates/app/src/surfaces.rs` as two texture arrays)
  from the generated photos in `art/textures/src/`: one tile, seamless, delit, recoloured toward
  the palette, with a normal and height map, per material. Rerun
  `blender -b -P tools/textures/bake.py -- art/textures/src crates/app/assets/textures art/textures/preview`
  after changing a source or a setting; the layer order is `scene.wgsl`'s `L_*`. The gates'
  lettering (one more layer) is drawn by `/usr/bin/python3 tools/textures/signs.py`.
- `tools/audio/prepare.py` — turns the ElevenLabs sounds in `art/audio/src/` (prompts and settings
  in its `SOURCES`) into the seamless, levelled loops the app embeds (`crates/app/assets/audio/`):
  `/usr/bin/python3 tools/audio/prepare.py art/audio/src crates/app/assets/audio`.
  `MARS_ENGINE_DEMO=out.wav cargo run --bin mars-racer` renders the car's sound over a scripted lap.

Conventions: metres, y up, right-handed; yaw 0 faces +Z and a positive yaw turns left; a car
facing +Z has +X on its left. Wheel order: front-left, front-right, rear-left, rear-right.

Physics determinism: f32 only, `libm` instead of std float functions, no `mul_add`, no hash map
iteration, no time, randomness or threads inside the simulation. glam is built with
`scalar-math` and `libm` so results are bit-identical on arm64 and wasm32.
