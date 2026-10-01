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

Rust comes from Homebrew's rustup, which is keg-only. Prefix commands with:

    export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"

The disk is nearly full: use the single workspace `target/` directory, never a second one.
A git worktree building into it (`CARGO_TARGET_DIR`) shares the app crate's output and
fingerprint with every other checkout (cargo hashes it by its workspace-relative path), so a
build can look fresh and run another checkout's binary: remove the app's fingerprints before and
after building in a worktree (`find target/debug/.fingerprint -maxdepth 1 -name 'app-*' -exec rm -rf {} +`;
a bare `rm target/debug/.fingerprint/app-*` aborts in zsh when nothing matches).

## Layout

- `crates/track` — block kit, maps, triangle meshes with a surface per triangle.
- `crates/physics` — deterministic vehicle physics at a fixed 100 Hz tick, gameplay presets.
- `crates/app` — the macOS executable (`cargo run --bin mars-racer`): winit, wgpu (Metal), egui, gilrs.
- `tools/blender/build_buggy.py` — builds the buggy (`art/buggy/buggy.blend`, rigged suspension,
  reference views in `art/buggy/refs`) and exports `crates/app/assets/buggy.glb` and the tub's
  livery `buggy_livery.png`, which the app embeds: rerun `blender -b -P tools/blender/build_buggy.py`
  after changing the model. Part and empty names (`arm_lo.FL`, `mount_bot.FL`, …) are what
  `crates/app/src/car_model.rs` reads.
- Buggy references: `art/buggy/refs/concept/` holds the generated concept views;
  `tools/blender/warp_refs.py` fits them to the physics' wheelbase and track (blueprint scales in
  `tools/blender/blueprint.py`), and `tools/blender/compare.py` renders the model with the same
  orthographic cameras and scores the silhouettes against them (`-- OUT_DIR [view ...]`).
- `tools/textures/bake.py` — bakes the surface textures the app embeds
  (`crates/app/assets/textures/`, loaded by `crates/app/src/surfaces.rs` as two texture arrays)
  from the generated photos in `art/textures/src/`: one tile, seamless, delit, recoloured toward
  the palette, with a normal and height map, per material. Rerun
  `blender -b -P tools/textures/bake.py -- art/textures/src crates/app/assets/textures art/textures/preview`
  after changing a source or a setting; the layer order is `scene.wgsl`'s `L_*`.

Conventions: metres, y up, right-handed; yaw 0 faces +Z and a positive yaw turns left; a car
facing +Z has +X on its left. Wheel order: front-left, front-right, rear-left, rear-right.

Physics determinism: f32 only, `libm` instead of std float functions, no `mul_add`, no hash map
iteration, no time, randomness or threads inside the simulation. glam is built with
`scalar-math` and `libm` so results are bit-identical on arm64 and wasm32.
