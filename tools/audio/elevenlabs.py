"""Generates takes of the game's sounds with ElevenLabs Sound Effects, through its API.

Each take is written as art/audio/takes/<name>_<n>.wav (48 kHz stereo, 16-bit; the folder is not
kept in git), numbered after those already there: `analyse.py` measures them, the chosen one is
copied to art/audio/src/ for prepare.py, whose SOURCES says which take each sound is. The API key is read from the macOS keychain, never printed:

    security add-generic-password -a "$USER" -s elevenlabs-api -w

    /usr/bin/python3 tools/audio/elevenlabs.py NAME [NAME ...] [--takes N]

(plain Python, standard library only.)
"""

import json
import subprocess
import sys
import urllib.error
import urllib.request
import wave
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

RATE = 48_000
OUT = Path(__file__).resolve().parents[2] / "art/audio/takes"

# name: prompt, duration in seconds, prompt influence, loop. The prompts name the vehicle and where
# the sound is heard from ("onboard", "close-up at the wheels"): without that ElevenLabs makes a
# machine or a drone. The magic wand (prompt rewriting) has no equivalent here: the text is sent
# as it is.
PROMPTS = {
    # Tyres on dirt. The synthesized stand-in (isolated grains of noise ringing at 1.8 and 4.2 kHz)
    # crackled: these ask for a dull rumble and a wash of sand, no single stones.
    "roll_dirt": (
        "Close-up recording at the wheels of an off-road racing buggy driving fast on a packed dirt "
        "track: a deep, soft rumble of knobby tyres on hard earth with a steady rushing wash of fine "
        "sand. Smooth, even and continuous, low and muffled, no crackling, no clicks, no engine, no "
        "wind.",
        10.0, 0.5, True,
    ),
    "roll_gravel": (
        "Off-road racing buggy tyres rolling fast over a dirt track covered in fine gravel, recorded "
        "close to the wheels: a continuous dense rush of gravel and sand under the tyres over a low "
        "rumble, steady and even, no single stones, no engine, no skid, no wind.",
        10.0, 0.5, True,
    ),
    # Tyres sliding on dirt. Named after a buggy drifting, every take had an engine in it (its
    # harmonics, 105-170 Hz, wavering): these describe only the tyre and the ground, as foley.
    "slide_dirt": (
        "Foley close-up of a big rubber tyre skidding sideways through loose dirt and sand: a "
        "continuous rough scraping rush of soil with a spray of sand and fine gravel, steady and "
        "even. Only the tyre and the ground, no engine, no motor, no wind.",
        6.0, 0.5, True,
    ),
    "slide_sand": (
        "Continuous spray of sand and fine gravel blasted sideways by a sliding tyre, recorded "
        "close: a dense rushing hiss of sand over a rough scrape of soil, steady and even, no "
        "engine, no motor, no wind.",
        6.0, 0.5, True,
    ),
    # The buggy is electric (a combustion engine, tried first, sounded like a lawnmower): its drive,
    # steady, which the game plays faster with the speed. A rustic electric UTV (the game's first
    # sound was a recording of one) rattled and wavered; a Formula E's whine, steady, sounded like a
    # vacuum cleaner, save one rich in harmonics; the racing buggy gave a low rumble (the drive's
    # body, `ev_racing_4`).
    "ev_utv": (
        "Onboard recording of an electric off-road buggy, an electric UTV, driving at a steady "
        "medium speed on a dirt road: the high whine of its electric motor and the gear whine of its "
        "transmission over a light mechanical rattle of the drivetrain and chassis. Constant speed, "
        "continuous and steady, no acceleration, no combustion engine, no wind.",
        8.0, 0.6, True,
    ),
    "ev_formula": (
        "Onboard recording of an electric race car like a Formula E car at a steady high speed: a "
        "loud, high-pitched electric motor whine with a sharp gearbox whine, like a jet turbine. "
        "Constant pitch, continuous and steady, no acceleration, no combustion engine, no tyre "
        "noise, no wind.",
        8.0, 0.6, True,
    ),
    "ev_racing": (
        "Onboard recording of a powerful electric racing buggy at a steady speed: an aggressive "
        "electric motor whine like a turbine, a sharp straight-cut gear whine and a mechanical "
        "rattle of the drivetrain. Constant pitch, continuous and steady, no acceleration, no "
        "combustion engine, no wind.",
        8.0, 0.6, True,
    ),
    # Lower, around that rich whine: the drive's whine is `ev_extreme_1`. Asked deeper still, the
    # takes are only a hum.
    "ev_extreme": (
        "Onboard recording of an electric off-road racing car, like an Extreme E SUV, at a steady "
        "speed: a deep, powerful electric motor whine rich in harmonics like a spinning turbine, a "
        "straight-cut gear whine and the heavy mechanical hum of the drivetrain. Constant pitch, "
        "continuous and steady, no acceleration, no combustion engine, no wind.",
        8.0, 0.6, True,
    ),
    "ev_deep": (
        "Onboard recording of a heavy electric racing buggy at a steady medium speed: a deep "
        "electric motor hum with a rich, low turbine-like whine and a growling gear whine, recorded "
        "close to the motor. Constant pitch, continuous and steady, no acceleration, no "
        "high-pitched whistle, no combustion engine, no wind.",
        8.0, 0.6, True,
    ),
    # The tyres skidding on the road. Named after a buggy, the takes had its engine in them (a
    # wavering 250 Hz); as foley, without a car, they are clean.
    "squeal_road": (
        "Tyres of a racing buggy sliding through a fast corner on smooth asphalt: a sustained tyre "
        "squeal with a slight natural wobble, steady intensity, continuous, recorded close, no "
        "engine, no crash.",
        5.0, 0.5, True,
    ),
    "skid_road": (
        "Foley close-up of rubber tyres skidding on a smooth hard road under hard braking: a "
        "sustained rubbery screech with a rough scrub of the tread, steady intensity, continuous. "
        "Only the tyres and the road, no engine, no motor, no crash, no wind.",
        5.0, 0.5, True,
    ),
    # Still to make.
    "roll_road": (
        "Close-up recording at the wheels of an off-road racing buggy driving fast on a smooth hard "
        "road: steady low tyre roar with a soft hiss, even and continuous, no engine, no squeal, no "
        "wind.",
        10.0, 0.5, True,
    ),
    # The road edges are sandbags and bumpers.
    "impact_light": (
        "Off-road buggy bumping into a low wall of sandbags: a short dull thud of heavy sand with a "
        "light plastic knock of the body panels, close, dry, no engine.",
        0.6, 0.4, False,
    ),
    "impact_medium": (
        "Off-road buggy hitting a wall of sandbags at speed: a heavy deep thud, a crack of composite "
        "body panels and a short rattle of the suspension, close, dry, no engine.",
        1.0, 0.4, False,
    ),
    "impact_heavy": (
        "Off-road buggy slamming into a barrier at high speed: a loud deep boom, cracking composite "
        "panels, a metallic rattle of the suspension, then sand and small debris falling, dry, no "
        "engine.",
        1.5, 0.4, False,
    ),
    "land": (
        "Off-road buggy landing hard after a big jump: a deep thump of the suspension bottoming out, "
        "the tyres slapping down, a short rattle of the chassis, close, dry, no engine.",
        1.0, 0.4, False,
    ),
    # A gust of the storm the car drives through (weather.rs).
    "gust": (
        "A strong gust of a Martian sandstorm sweeping past: a rush of wind and blowing sand that "
        "swells and fades away in three seconds, no voice, no music.",
        3.0, 0.4, False,
    ),
}


def api_key():
    out = subprocess.run(["security", "find-generic-password", "-s", "elevenlabs-api", "-w"],
                         capture_output=True, text=True)
    if out.returncode != 0:
        sys.exit("no ElevenLabs key in the keychain (service elevenlabs-api)")
    return out.stdout.strip()


def generate(key, name, n):
    text, seconds, influence, loop = PROMPTS[name]
    body = json.dumps(dict(text=text, duration_seconds=seconds, prompt_influence=influence,
                           loop=loop, model_id="eleven_text_to_sound_v2")).encode()
    req = urllib.request.Request(
        f"https://api.elevenlabs.io/v1/sound-generation?output_format=pcm_{RATE}", data=body,
        headers={"xi-api-key": key, "Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=180) as r:
            pcm = r.read()
    except urllib.error.HTTPError as e:
        return f"{name}_{n}: HTTP {e.code} {e.read().decode(errors='replace')[:300]}"
    path = OUT / f"{name}_{n}.wav"
    with wave.open(str(path), "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(pcm[: len(pcm) // 4 * 4])
    return f"{path.name}: {len(pcm) / 4 / RATE:.2f} s"


def main():
    args = sys.argv[1:]
    takes = 4
    if "--takes" in args:
        i = args.index("--takes")
        takes = int(args[i + 1])
        del args[i : i + 2]
    unknown = [a for a in args if a not in PROMPTS]
    if not args or unknown:
        sys.exit(f"names: {', '.join(PROMPTS)}" + (f" (unknown: {', '.join(unknown)})" if unknown else ""))
    OUT.mkdir(parents=True, exist_ok=True)
    key = api_key()
    # New takes are numbered after the ones already there.
    first = {name: 1 + max((int(p.stem.rsplit("_", 1)[1]) for p in OUT.glob(f"{name}_*.wav")), default=0)
             for name in args}
    jobs = [(name, n) for name in args for n in range(first[name], first[name] + takes)]
    with ThreadPoolExecutor(max_workers=2) as pool:
        for line in pool.map(lambda j: generate(key, *j), jobs):
            print(line, flush=True)


if __name__ == "__main__":
    main()
