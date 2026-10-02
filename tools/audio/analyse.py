"""Measures generated takes (art/audio/takes/, see elevenlabs.py) to sort them before listening.

Per take, one line of numbers and a spectrogram (art/audio/takes/<name>.png, log frequency 30 Hz to
16 kHz, time across):

- level: RMS and peak in dBFS, and the share of 50 ms frames under -50 dBFS (silences);
- bands: the share of the energy below 150 Hz, 150-600, 600-2k, 2k-6k and above 6 kHz;
- crackle: clicks per second in the 2-10 kHz band (a 1 ms peak at more than 5 times the band's
  local RMS) and that band's kurtosis (3 for an even hiss, far more for isolated grains): the
  synthesized gravel the dirt recordings replaced, which crackled, is measured as `synth_gravel`
  for comparison;
- steadiness: the spread (standard deviation, dB) of the RMS over 200 ms frames;
- seam: for a loop, the RMS step (dB) from its last 50 ms to its first 50 ms, and the jump at the
  join relative to the typical step between samples;
- tone: how far the strongest peak of the 40-1000 Hz spectrum stands above its neighbourhood (dB),
  and where: an engine or a hum left in a take of tyres shows as a high value.

    /usr/bin/python3 tools/audio/analyse.py PATTERN [PATTERN ...]   (e.g. 'roll_dirt_*')

(plain Python with numpy and Pillow; macOS's /usr/bin/python3 has them.)
"""

import sys
import wave
from pathlib import Path

import numpy as np
from PIL import Image

from elevenlabs import PROMPTS

TAKES = Path(__file__).resolve().parents[2] / "art/audio/takes"


def read(path):
    with wave.open(str(path)) as w:
        x = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2").astype(np.float64) / 32768.0
        return x.reshape(-1, w.getnchannels()).mean(axis=1), w.getframerate()


def band(x, rate, lo, hi):
    """Zero-phase band-pass through the FFT."""
    f = np.fft.rfftfreq(len(x), 1 / rate)
    s = np.fft.rfft(x)
    s[(f < lo) | (f > hi)] = 0
    return np.fft.irfft(s, len(x))


def frames(x, n):
    m = len(x) // n
    return x[: m * n].reshape(m, n)


def db(v):
    return 20 * np.log10(np.maximum(v, 1e-9))


def measure(x, rate, loop):
    rms = np.sqrt((x**2).mean())
    f50 = np.sqrt((frames(x, rate // 20) ** 2).mean(axis=1))
    out = dict(rms=db(rms), peak=db(np.abs(x).max()), silent=(db(f50) < -50).mean())

    spec = np.abs(np.fft.rfft(x)) ** 2
    freq = np.fft.rfftfreq(len(x), 1 / rate)
    edges = [0, 150, 600, 2000, 6000, rate / 2]
    out["bands"] = [spec[(freq >= a) & (freq < b)].sum() / spec.sum() for a, b in zip(edges, edges[1:])]

    hf = band(x, rate, 2000, 10000)
    ms = rate // 1000
    peaks = np.abs(frames(hf, ms)).max(axis=1)
    local = np.sqrt(np.convolve(hf**2, np.ones(rate // 20) / (rate // 20), mode="same"))
    local = frames(local, ms).mean(axis=1)
    out["clicks"] = (peaks > 5 * local).sum() / (len(x) / rate)
    out["kurt"] = ((hf - hf.mean()) ** 4).mean() / hf.var() ** 2

    f200 = db(np.sqrt((frames(x, rate // 5) ** 2).mean(axis=1)))
    out["spread"] = f200.std()

    if loop:
        n = rate // 20
        out["seam_db"] = db(np.sqrt((x[:n] ** 2).mean())) - db(np.sqrt((x[-n:] ** 2).mean()))
        step = np.abs(np.diff(x))
        out["seam_jump"] = abs(x[0] - x[-1]) / np.median(step)

    # Averaged spectrum (8192-sample windows, about 6 Hz apart): a peak against the median of the
    # 30 bins around it. Noise stays near 1.5 dB.
    n = 8192
    win = np.hanning(n)
    p = np.mean([np.abs(np.fft.rfft(x[i : i + n] * win)) ** 2 for i in range(0, len(x) - n, n // 2)], axis=0)
    f = np.fft.rfftfreq(n, 1 / rate)
    s = 10 * np.log10(p + 1e-20)
    idx = np.where((f >= 40) & (f <= 1000))[0]
    rise = np.array([s[i] - np.median(s[max(0, i - 15) : i + 16]) for i in idx])
    out["tone"], out["tone_hz"] = rise.max(), f[idx[rise.argmax()]]
    return out


def spectrogram(x, rate, path):
    n, hop = 4096, rate // 100
    win = np.hanning(n)
    cols = [np.abs(np.fft.rfft(x[i : i + n] * win)) for i in range(0, len(x) - n, hop)]
    s = db(np.array(cols).T + 1e-9)
    freq = np.fft.rfftfreq(n, 1 / rate)
    rows = np.geomspace(30, 16000, 300)[::-1]
    img = np.array([np.interp(rows, freq, c) for c in s.T]).T
    img = np.clip((img - (img.max() - 80)) / 80, 0, 1)
    Image.fromarray((img * 255).astype(np.uint8)).resize((min(1200, img.shape[1]), 300)).save(path)


def synth_gravel(rate=48_000, seconds=6.0, speed=40.0, scrub=0.0):
    """The gravel audio.rs synthesized before the dirt recordings, at a steady speed (m/s)."""
    rng = np.random.default_rng(1)
    n = int(rate * seconds)
    density = 300 + 2700 * min(speed / 60, 1) * (1 + scrub)
    a = rng.uniform(-1, 1, n)
    grain = np.where(rng.uniform(0, 1, n) < density / rate, a * np.abs(a), 0.0)

    def svf(x, freq, q):
        f = 2 * np.sin(np.pi * freq / rate)
        low = bp = 0.0
        y = np.empty_like(x)
        for i, v in enumerate(x):
            high = v - low - bp / q
            bp += f * high
            low += f * bp
            y[i] = bp
        return y

    noise = rng.uniform(-1, 1, n)
    rumble = np.empty(n)
    c = 1 - np.exp(-2 * np.pi * 250 / rate)
    y = 0.0
    for i, v in enumerate(noise):
        y += c * (v - y)
        rumble[i] = y
    return (svf(grain, 1800, 2.0) + 0.35 * svf(grain, 4200, 2.5) + 0.5 * rumble) * 0.4


def line(name, m):
    b = " ".join(f"{100 * v:4.0f}" for v in m["bands"])
    seam = f"  seam {m['seam_db']:+5.1f} dB x{m['seam_jump']:4.1f}" if "seam_db" in m else ""
    return (f"{name:16} rms {m['rms']:6.1f} peak {m['peak']:5.1f} silent {100 * m['silent']:3.0f}%  "
            f"bands% {b}  clicks/s {m['clicks']:5.1f} kurt {m['kurt']:5.1f}  spread {m['spread']:4.1f} dB  "
            f"tone {m['tone']:4.1f} dB at {m['tone_hz']:4.0f} Hz{seam}")


def main():
    print(f"{'':16} {'':33}  bands%  <150 -600  -2k  -6k  >6k")
    x = synth_gravel()
    print(line("synth_gravel", measure(x, 48_000, False)))
    spectrogram(x, 48_000, TAKES / "synth_gravel.png")
    for pattern in sys.argv[1:]:
        for path in sorted(TAKES.glob(f"{pattern}.wav")):
            x, rate = read(path)
            loop = PROMPTS.get(path.stem.rsplit("_", 1)[0], (None, None, None, True))[3]
            print(line(path.stem, measure(x, rate, loop)))
            spectrogram(x, rate, path.with_suffix(".png"))


if __name__ == "__main__":
    main()
