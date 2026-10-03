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
a bare `rm target/debug/.fingerprint/app-*` aborts in zsh when nothing matches). The same holds
for `track-*` and `physics-*` when the worktree changes them: an app build there can otherwise
link the main checkout's `track` (missing a new block or surface).

## Layout

- `crates/track` — block kit, maps, triangle meshes with a surface per triangle. Maps are
  `crates/track/maps/*.json`, written from a `.chain` file (blocks, landforms, props) by
  `cargo run -p track --example chain -- crates/track/maps/noctis.chain`; a new one is added to
  `BUILTIN_MAPS`. A map's `planet` (`planet ice` in a `.chain`, Mars when left out) picks its car
  (`physics::car_for`), its look (scene.wgsl's `frame.misc.w`) and its planet in the menu:
  `noctis_neige.chain`, the ice planet's prototype, is Noctis's layout where the road decks are
  bare ice, the dirt packed snow and the ground deep powder (flat colours, no storm or weather).
  The ice planet has blocks of its own (`docs/blocks-ice.md`, its shape sheet `docs/blocks-ice/`
  drawn by the same `shapes` example): progressive turns for drifting (`curveN`, `curvebermN`),
  S-bends (`sbendN`), snakes (`snakeN`) and a 16 m snow track (variant `snow`); any turn can
  climb or descend as it turns (`curve3_left_down2`, `uberm2_right_up1`; `kit::climb_fits`
  refuses those that would hump). `sulcus.chain` is its first circuit built from them, up and
  down a mesa and a plateau. `cargo run -p physics --release --example lap -- Noctis [road dirt]` times the
  test autopilot over a map (with the two numbers it brakes for the bends). The menu's series:
  easy circuits stay short (30 s), the hard series' (`series()` in `crates/app/src/menu/catalog.rs`,
  Marineris first) run 45 s to a minute, a roller coaster on scaffolding with boosters. At
  300 km/h, under gravity 40, a crest of radius under v²/40 m/s² launches the car: drops and
  climbs taken fast are 5 to 8 cells long (`marineris.chain` explains its choices).
  Roads follow `art/roads/brief.md` ("camp roads": a laminated tarp deck, sandbag or bumper
  edges per block variant, raised slabs on trusses and piers of red plastic tubes,
  `crates/track/src/stilts.rs`); `docs/blocks.md` is the reference of every block and variant,
  kept up to date with the code (Mars's kit; the ice planet's brief is `docs/blocks-ice.md`, its
  own tab of the shared page). Its shape sheet (`docs/blocks/*.svg`) is drawn from the
  geometry by `cargo run -p track --release --example shapes`: rerun it when a shape changes.
  The `booster` variant paints arrows on any road block (a `Surface::Booster` deck: the physics'
  `boost_accel`/`boost_time` push along the path).
  `time night` in a `.chain` (`"time": "night"` in the map) races it by night (Olympus): a low
  moon and stars (`Lighting` in `gfx.rs`), colours greyed toward blue out of the light, and the
  buggy's headlights lighting the road, with a shadow map of their own so raised roads and crests
  cut the beam (`headlight` in `scene.wgsl`). `MARS_TIME=day|night` forces it on every map.
  The colony's scenery (supply caches, observation posts, field and base camps, comms masts, a
  landing zone with its mine, the colony with its giant domes, tower and rocket) is
  `crates/track/src/camp.rs`, placed by a map's `structures` list (`structure kind x z yaw` in a
  `.chain`) and levelling the terrain under it; its art direction is `art/scenery/brief.md`.
- `crates/physics` — deterministic vehicle physics at a fixed 100 Hz tick, gameplay presets:
  `profiles()` is every planet's car (Mars's `combo`; the ice planet's `neige`, skis in front and
  rear drive, the brake swinging the rear out in a turn, the line catching up with the nose after
  a drift: `cargo run -p physics --release --example skis` measures both), `presets()` Mars's
  alone, which the tests drive on the Mars maps. Per surface, `response` slows a slide (the drift
  car's ice), `slide_cost` makes it plough (snow), `sink` lets the wheels ride in snow over gentle
  ruts (`WheelState::sink`, drawn buried) and `trail` keeps their tracks.
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
  buggy accelerates by itself, the bottom strip brakes, the screen's halves steer, and three
  round buttons in the top right corner go back to the last checkpoint, restart, and open the
  settings, a sheet that pauses the race and slides the scene up so the car shows above it; on a
  computer Escape or the pad's Start opens the same settings as a card in the middle of the
  screen (`MARS_HUD_SETTINGS=seconds` opens them in a self-test; a hidden run draws the HUD only
  on its screenshot frames, so they open on one and need another shot 0.3 s later). Everything
  for debugging (FPS, profile, telemetry, tuning panel in `ui.rs`) shows only with Tab, the
  settings' debug switch, or `MARS_DEBUG_PANEL=1`.
- `crates/app/src/weather.rs` — the weather, render only (the wind never pushes the car): a
  `Climate` per planet that every map gets (Mars: a breeze blowing from the sandstorm, light sand
  drifting in the air, a gust every few seconds; the ice planet: light snow falling and swaying
  on a wind that turns to a new direction every few seconds, starting out blowing at the car,
  and now and then a thin, icy cloud of blown snow), the gusts being placed on the track's route
  ahead of the car, often to cross the road as it gets there. Drawn by `shaders/weather.wgsl`:
  grains (or snowflakes) as short streaks in a box of air that follows the camera, gust clouds as
  puffs of animated smoke that thin out near the camera, a light veil when the camera is inside one.
  `MARS_WEATHER=off|gusty` stills the air or brings a gust every second (checks); a hidden
  screenshot run needs `MARS_BENCH` for the weather to advance between its shots.
- `tools/blender/build_buggy.py` — builds the buggy "B" (`art/buggy/buggy.blend`, rigged
  suspension, the registered plans as image empties) and exports `crates/app/assets/buggy.glb` and
  its livery atlas `buggy_livery.png`, which the app embeds: rerun
  `blender -b -P tools/blender/build_buggy.py` after changing the model (`-- --out DIR` writes
  elsewhere, for checks). Part and empty names (`arm_lo.FL`, `mount_bot.FL`, …) are what
  `crates/app/src/car_model.rs` reads. The body panels are in `tools/blender/buggy_body.py`, the
  livery (vector shapes, glass, grilles, seams) in `tools/blender/buggy_livery.py`, mesh helpers and
  materials in `tools/blender/meshkit.py`.
- `tools/blender/build_skicar.py` — builds the ice planet's car, an open single-seater on skis
  (`art/ice_car/skicar.blend`; the validated concepts in `art/ice_car/concepts/monoplace/`, the
  generated plans in `art/ice_car/views/`) and exports `crates/app/assets/skicar.glb` and
  `skicar_livery.png`: rerun `blender -b -P tools/blender/build_skicar.py` after changing it.
  Same part names and rig as the buggy's; its front corners' `wheel` is the ski, which steers
  without spinning and tips on its pivot to lie on the ground (`game.rs`). The skis are drawn
  at ±0.65 m (`car_model::SKI_X`), closer together than the physics' contacts (its single
  `track_width`, 1.8 m), and their marks follow them. Its rear tyres take on what they run on
  (`Look::coat`: a frost on ice, snow on the snow tracks, kept in the air), drawn by scene.wgsl
  on the tread, its studs and chains, which it dulls. Its body is `tools/blender/skicar_body.py`,
  its livery `tools/blender/skicar_livery.py`. A preset with `front_skis` drives it, every other
  the buggy; the bound livery texture follows the car.
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
  lettering (one more layer) is drawn by `/usr/bin/python3 tools/textures/signs.py`, the
  booster arrow (another) by `/usr/bin/python3 tools/textures/booster.py` from a Higgsfield
  picture, redrawn as exact polygons.
- `tools/audio/elevenlabs.py` — generates takes of the car's sounds with ElevenLabs' API (prompts
  in its `PROMPTS`, the key in the macOS keychain, service `elevenlabs-api`) into `art/audio/takes/`
  (not in git); `tools/audio/analyse.py` measures them (crackle, steadiness, loop seam, an engine
  left in) and draws their spectrograms, to sort them before listening. `tools/audio/prepare.py`
  turns the chosen ones, copied to `art/audio/src/` (prompts and settings in its `SOURCES`), into
  the seamless, levelled loops the app embeds (`crates/app/assets/audio/`):
  `/usr/bin/python3 tools/audio/prepare.py art/audio/src crates/app/assets/audio`; the booster's
  whoosh is a one-shot cut from its take (`trim`), played by `audio.rs` when a pad is touched.
  The buggy is electric: its drive (`engine_sound.rs`) is a motor whine played faster with the
  motor's speed over a drivetrain rumble, never a combustion engine; the tyres on dirt roll and
  slide on two more loops, and skid on the road on a third. Braking hard at speed skids the
  braking tyres (`CarState::skid`, render and sound only): marks, dust and that sound, as in a
  drift. `MARS_SOUNDS=DIR` loads `DIR/<file>` in place of an embedded sound (to
  try takes in the game without rebuilding); `MARS_ENGINE_DEMO=out.wav cargo run --bin mars-racer`
  renders the car's sound over a scripted lap, `MARS_AUDIO_WAV=out.wav` over 30 s of autopilot.
- The music — Suno tracks in `art/audio/music/` (art direction and prompts in its `prompts.md`:
  one sound for the game, one colour per planet, Mars's being desert-rock guitar) —
  `tools/audio/music.py` levels them to -16 LUFS, ends them cleanly and encodes them as AAC
  (`crates/app/assets/music/`): `/usr/bin/python3 tools/audio/music.py art/audio/music crates/app/assets/music`.
  `crates/app/src/music.rs` embeds them and streams them (symphonia, on a thread); `audio.rs`
  plays the theme in the menu (its only background: the menu has no wind of its own) and the
  planet's day tracks in turn in a race, about as loud as the car, fading between the two; a
  race by night plays "Night Shift" alone, in a loop (`MARS_NIGHT`).

Conventions: metres, y up, right-handed; yaw 0 faces +Z and a positive yaw turns left; a car
facing +Z has +X on its left. Wheel order: front-left, front-right, rear-left, rear-right.

Physics determinism: f32 only, `libm` instead of std float functions, no `mul_add`, no hash map
iteration, no time, randomness or threads inside the simulation. glam is built with
`scalar-math` and `libm` so results are bit-identical on arm64 and wasm32.
