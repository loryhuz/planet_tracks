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

## Layout

- `crates/track` — block kit, maps, triangle meshes with a surface per triangle.
- `crates/physics` — deterministic vehicle physics at a fixed 100 Hz tick, gameplay presets.
- `crates/app` — the macOS executable (`cargo run --bin mars-racer`): winit, wgpu (Metal), egui, gilrs.

Conventions: metres, y up, right-handed; yaw 0 faces +Z and a positive yaw turns left; a car
facing +Z has +X on its left. Wheel order: front-left, front-right, rear-left, rear-right.

Physics determinism: f32 only, `libm` instead of std float functions, no `mul_add`, no hash map
iteration, no time, randomness or threads inside the simulation. glam is built with
`scalar-math` and `libm` so results are bit-identical on arm64 and wasm32.
