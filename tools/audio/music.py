"""Prepares the music the app embeds (crates/app/assets/music/) from the Suno tracks in
art/audio/music/ (their art direction and prompts: art/audio/music/prompts.md).

Per track this script:

1. decodes it with ffmpeg to 48 kHz stereo;
2. cuts it at `end` seconds if given, drops what follows the end (silence or a reverb tail
   quieter than -50 dBFS), fades the last `fade_out` seconds out and the first 10 ms in, so the
   track runs into the next one, or back to its start, without a click;
3. sets its loudness to -16 LUFS (EBU R128, measured by ffmpeg) with one gain, lowered if a peak
   would pass -1 dBFS: every track leaves here at the same loudness, and the game sets the music's
   level against the menu's wind and the car (`audio.rs`);
4. encodes it as AAC at 192 kbit/s in MP4 (macOS's AudioToolbox encoder, through ffmpeg).

    /usr/bin/python3 tools/audio/music.py art/audio/music crates/app/assets/music [name ...]

(plain Python with numpy, and Homebrew's ffmpeg.)
"""

import re
import subprocess
import sys
from pathlib import Path

import numpy as np

RATE = 48_000
LUFS = -16.0

TRACKS = {
    # "Planet Tracks", the menu's theme (Suno, 2 October 2026): ends on a hard stop and its reverb.
    "theme": dict(fade_out=0.5),
    # "Red Frontier", Mars's anthem, the first of the tracks raced to on Mars: fades to -31 dBFS,
    # then stops.
    "red_frontier": dict(fade_out=1.0),
    # "Dust Devil", the hardest-driving Martian track (~154 BPM): ends on a decaying tail.
    "dust_devil": dict(fade_out=0.5),
    # "Night Shift", the hypnotic night track (~128 BPM): Suno ran it to 8:00 and cut it off in
    # the middle of a groove that repeats from 4:15 on; it ends fading out over the quiet bars
    # before that groove.
    "night_shift": dict(end=254.5, fade_out=7.5),
}


def decode(path):
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", str(path), "-f", "f32le", "-ac", "2", "-ar", str(RATE), "-"],
        check=True, capture_output=True,
    ).stdout
    return np.frombuffer(raw, dtype="<f4").astype(np.float64).reshape(-1, 2)


def loudness(x):
    """Integrated loudness in LUFS."""
    out = subprocess.run(
        ["ffmpeg", "-nostats", "-f", "f32le", "-ar", str(RATE), "-ac", "2", "-i", "-",
         "-af", "ebur128", "-f", "null", "-"],
        input=x.astype("<f4").tobytes(), check=True, capture_output=True,
    ).stderr.decode()
    return float(re.findall(r"I:\s+(-?[\d.]+) LUFS", out)[-1])


def encode(x, path):
    subprocess.run(
        ["ffmpeg", "-v", "error", "-y", "-f", "f32le", "-ar", str(RATE), "-ac", "2", "-i", "-",
         "-c:a", "aac_at", "-b:a", "192k", "-map_metadata", "-1", str(path)],
        input=x.astype("<f4").tobytes(), check=True,
    )


def prepare(src, dst, spec):
    x = decode(src)
    if "end" in spec:
        x = x[: int(spec["end"] * RATE)]
    # The end: the last 50 ms window louder than -50 dBFS.
    win = RATE // 20
    n = len(x) // win
    rms = np.sqrt((x[: n * win] ** 2).mean(axis=1).reshape(n, win).mean(axis=1))
    last = np.nonzero(rms > 10 ** (-50 / 20))[0][-1]
    x = x[: (last + 1) * win]
    for n, ramp in ((int(0.01 * RATE), slice(None)), (int(spec["fade_out"] * RATE), slice(None, None, -1))):
        t = (np.arange(n) + 0.5) / n
        env = np.ones(len(x))
        env[:n] = np.sin(t * np.pi / 2) ** 2
        x = x * env[ramp][:, None]
    gain = 10 ** ((LUFS - loudness(x)) / 20)
    gain = min(gain, 10 ** (-1 / 20) / np.abs(x).max())
    x = x * gain
    encode(x, dst)
    print(f"{dst.name}: {len(x) / RATE:.1f} s, {loudness(decode(dst)):.1f} LUFS, "
          f"peak {20 * np.log10(np.abs(x).max()):.1f} dBFS, {dst.stat().st_size / 1e6:.1f} MB")


def main():
    src_dir, out_dir = Path(sys.argv[1]), Path(sys.argv[2])
    names = sys.argv[3:] or list(TRACKS)
    out_dir.mkdir(parents=True, exist_ok=True)
    for name in names:
        prepare(src_dir / f"{name}.m4a", out_dir / f"{name}.m4a", TRACKS[name])


if __name__ == "__main__":
    main()
