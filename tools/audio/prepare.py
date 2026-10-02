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
    # Combustion engine, two layers the game crossfades and pitches to the revs. Loop on, 8 s,
    # prompt influence 50 %. The mid one runs at about 46 % of the high one's firing frequency
    # (108 Hz against 236 Hz), which `engine_sound.rs` relies on.
    "engine_mid": dict(
        loop=True, fade=0.15, rms=-16.0,
        prompt="Onboard recording of an off-road racing buggy driving at a steady medium speed: "
        "raspy four-cylinder engine held at constant RPM, throaty open exhaust and a light intake "
        "growl. Continuous and steady, no revving, no gear shifts, no tire noise, no wind.",
    ),
    "engine_high": dict(
        loop=True, fade=0.15, rms=-16.0,
        prompt="Onboard recording of an off-road racing buggy flat out at high speed: raspy "
        "four-cylinder engine screaming at constant high RPM near the redline, throaty open exhaust "
        "and a loud intake growl. Continuous and steady, no revving up or down, no gear shifts, no "
        "tire noise, no wind.",
    ),
    # Loop on, 30 s, prompt influence 40 %.
    "ambience_mars": dict(
        loop=True, fade=2.0, rms=-20.0, stereo=True,
        prompt="Desolate Martian desert ambience: thin cold wind over rocky plains, faint distant "
        "low rumble, soft hiss of blowing dust, calm and empty, no birds, no insects, no voices.",
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
