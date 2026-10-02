"""Prepares the sounds the app embeds (crates/app/assets/audio/) from generated recordings.

The sources are ElevenLabs sound effects (48 kHz WAV) generated from the prompts in SOURCES, with
the magic wand (prompt rewriting) off; they live in art/audio/src/. Per sound this script:

1. mixes to mono, or keeps the stereo (`stereo`);
2. removes the DC offset; for a one-shot, keeps the part `trim` (start, end in seconds) with short
   fades at both ends;
3. for a loop, makes the end flow into the start: the last `fade` seconds are crossfaded (equal
   power) over the first ones and dropped, so the file repeats without a click;
4. sets its level: RMS to `rms` dBFS, lowered if a peak would pass -1 dBFS; or, for a one-shot
   (mostly a rise and a fall), its peak to `peak` dBFS;
5. writes 16-bit PCM WAV at the source's rate.

    /usr/bin/python3 tools/audio/prepare.py art/audio/src crates/app/assets/audio [name ...]

(plain Python with numpy; macOS's /usr/bin/python3 has it.)
"""

import sys
import wave
from pathlib import Path

import numpy as np

SOURCES = {
    # Loop on, 30 s, prompt influence 40 %.
    "ambience_mars": dict(
        loop=True, fade=2.0, rms=-20.0, stereo=True,
        prompt="Desolate Martian desert ambience: thin cold wind over rocky plains, faint distant "
        "low rumble, soft hiss of blowing dust, calm and empty, no birds, no insects, no voices.",
    ),
    # The buggy's electric drive, two loops (engine_sound.rs), loop on, 8 s, influence 60 %, made
    # through the API (tools/audio/elevenlabs.py, which holds the other prompts tried). The whine,
    # played faster with the motor's speed: take `ev_extreme_1`, steady, rich in harmonics over a
    # hum near 120 Hz (the plain high whines of a Formula E sounded like a vacuum cleaner).
    "drive_whine": dict(
        loop=True, fade=0.15, rms=-16.0,
        prompt="Onboard recording of an electric off-road racing car, like an Extreme E SUV, at a "
        "steady speed: a deep, powerful electric motor whine rich in harmonics like a spinning "
        "turbine, a straight-cut gear whine and the heavy mechanical hum of the drivetrain. "
        "Constant pitch, continuous and steady, no acceleration, no combustion engine, no wind.",
    ),
    # The drivetrain's body under it: take `ev_racing_4`, a steady low rumble (95 % of it under
    # 150 Hz) with a mechanical rattle.
    "drive_body": dict(
        loop=True, fade=0.15, rms=-16.0,
        prompt="Onboard recording of a powerful electric racing buggy at a steady speed: an "
        "aggressive electric motor whine like a turbine, a sharp straight-cut gear whine and a "
        "mechanical rattle of the drivetrain. Constant pitch, continuous and steady, no "
        "acceleration, no combustion engine, no wind.",
    ),
    # The tyres on dirt, two loops the game plays faster with speed (×0.7 to ×1.3) and levels by
    # the wheels on dirt; they replace a synthesized gravel whose isolated grains crackled. Made
    # through the API (tools/audio/elevenlabs.py, which holds the other prompts tried), sorted by
    # tools/audio/analyse.py: rolling, take `roll_gravel_2`, loop on, 10 s, influence 50 %.
    "roll_dirt": dict(
        loop=True, fade=0.15, rms=-20.0,
        prompt="Off-road racing buggy tyres rolling fast over a dirt track covered in fine gravel, "
        "recorded close to the wheels: a continuous dense rush of gravel and sand under the tyres "
        "over a low rumble, steady and even, no single stones, no engine, no skid, no wind.",
    ),
    # Sliding, take `slide_dirt_2`, loop on, 6 s, influence 50 %: a rough scrape and spray of
    # sand. Prompts naming a buggy drifting gave an engine; this one describes only the tyre.
    "slide_dirt": dict(
        loop=True, fade=0.15, rms=-20.0,
        prompt="Foley close-up of a big rubber tyre skidding sideways through loose dirt and sand: a "
        "continuous rough scraping rush of soil with a spray of sand and fine gravel, steady and "
        "even. Only the tyre and the ground, no engine, no motor, no wind.",
    ),
    # The tyres skidding on the road, at the grip limit or braking hard: take `skid_road_1`, loop
    # on, 5 s, influence 50 %, a steady screech around 1-2 kHz over the scrub of the tread. Asked
    # for a buggy's tyres squealing, every take had its engine in it.
    "skid_road": dict(
        loop=True, fade=0.15, rms=-20.0,
        prompt="Foley close-up of rubber tyres skidding on a smooth hard road under hard braking: a "
        "sustained rubbery screech with a rough scrub of the tread, steady intensity, continuous. "
        "Only the tyres and the road, no engine, no motor, no crash, no wind.",
    ),
    # Played once when the wheels touch a booster pad. Loop off, 2 s, prompt influence 50 %, take
    # 4 of the user's. The take opens on a click and a silence, cut away: the sound starts on the
    # whoosh building up (0.45 s) and ends once it has died out (1.6 s).
    "booster": dict(
        trim=(0.45, 1.6), fade_in=0.015, fade_out=0.12, peak=-3.0, stereo=True,
        prompt="Booster pad on a desert race track: a deep thump as the wheels hit a pad, a "
        "compressed-air blast, then a roaring rising whoosh of wind and sand streaming past a buggy "
        "at very high speed, fading out. No music, no voice.",
    ),
}


def read(path):
    with wave.open(str(path)) as w:
        assert w.getsampwidth() == 2, f"{path}: 16-bit PCM expected"
        x = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16)
        return x.astype(np.float64).reshape(-1, w.getnchannels()) / 32768.0, w.getframerate()


def write(path, x, rate):
    data = (np.clip(x, -1.0, 1.0) * 32767.0).round().astype("<i2")
    with wave.open(str(path), "wb") as w:
        w.setnchannels(x.shape[1])
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes(data.tobytes())


def prepare(src, dst, spec):
    x, rate = read(src)
    if not spec.get("stereo"):
        x = x.mean(axis=1, keepdims=True)
    x = x - x.mean(axis=0)
    if "trim" in spec:
        a, b = (int(t * rate) for t in spec["trim"])
        x = x[a:b]
        for n, ramp in ((int(spec["fade_in"] * rate), slice(None, None)), (int(spec["fade_out"] * rate), slice(None, None, -1))):
            t = (np.arange(n) + 0.5) / n
            env = np.ones(len(x))
            env[:n] = np.sin(t * np.pi / 2) ** 2
            x = x * env[ramp][:, None]
    if spec.get("loop"):
        n = int(spec["fade"] * rate)
        t = (np.arange(n) + 0.5) / n
        fade_in = np.sin(t * np.pi / 2)[:, None]
        fade_out = np.cos(t * np.pi / 2)[:, None]
        head = x[:n] * fade_in + x[-n:] * fade_out
        x = np.concatenate([head, x[n:-n]])
    if "peak" in spec:
        gain = 10 ** (spec["peak"] / 20) / np.abs(x).max()
    else:
        rms = np.sqrt((x**2).mean())
        gain = 10 ** (spec["rms"] / 20) / rms
        gain = min(gain, 10 ** (-1 / 20) / np.abs(x).max())
    x = x * gain
    write(dst, x, rate)
    level = 20 * np.log10(np.sqrt((x**2).mean()))
    print(f"{dst.name}: {len(x) / rate:.2f} s, {x.shape[1]} ch, RMS {level:.1f} dBFS, "
          f"peak {20 * np.log10(np.abs(x).max()):.1f} dBFS")


def main():
    src_dir, out_dir = Path(sys.argv[1]), Path(sys.argv[2])
    names = sys.argv[3:] or list(SOURCES)
    out_dir.mkdir(parents=True, exist_ok=True)
    for name in names:
        prepare(src_dir / f"{name}.wav", out_dir / f"{name}.wav", SOURCES[name])


if __name__ == "__main__":
    main()
