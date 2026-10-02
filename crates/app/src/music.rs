//! The music: tracks the app embeds as AAC in MP4 (assets/music/, made by tools/audio/music.py
//! from the Suno tracks in art/audio/music/), decoded in pure Rust (symphonia) so it plays on
//! every platform. A player decodes its tracks one after the other, in a loop, on a thread of its
//! own, and hands the audio thread short chunks already at the output's rate, so a three-minute
//! track never sits decoded in memory. When the mixer stops reading a player, its queue fills up
//! and the thread waits: the music pauses where it was and carries on from there.

use std::io::Cursor;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

/// The menu's theme, "Planet Tracks".
pub const MENU: &[&[u8]] = &[include_bytes!("../assets/music/theme.m4a")];
/// Raced to on Mars by day, in turn: "Red Frontier", "Dust Devil".
pub const MARS: &[&[u8]] = &[include_bytes!("../assets/music/red_frontier.m4a"), include_bytes!("../assets/music/dust_devil.m4a")];
/// Raced to on Mars by night (a map's `"time": "night"`): "Night Shift", the hypnotic night
/// track, in a loop.
pub const MARS_NIGHT: &[&[u8]] = &[include_bytes!("../assets/music/night_shift.m4a")];

/// Frames per chunk handed to the audio thread (about 21 ms).
const CHUNK: usize = 1024;
/// Chunks queued ahead of the mixer (about 0.7 s).
const AHEAD: usize = 32;

pub struct Music {
    rx: Receiver<Vec<f32>>,
    /// Played chunks go back to the decoding thread to be filled again, so the audio thread
    /// neither allocates nor frees.
    back: SyncSender<Vec<f32>>,
    /// Interleaved stereo.
    chunk: Vec<f32>,
    at: usize,
}

impl Music {
    /// Starts decoding `tracks` for an output at `rate` Hz.
    pub fn new(tracks: &'static [&'static [u8]], rate: f32) -> Self {
        let (tx, rx) = sync_channel(AHEAD);
        let (back, spare) = sync_channel(AHEAD + 2);
        let decoding = std::thread::Builder::new().name("music".into()).spawn(move || stream(tracks, rate, tx, spare));
        if let Err(e) = decoding {
            eprintln!("music: {e}");
        }
        Self { rx, back, chunk: Vec::new(), at: 0 }
    }

    /// The next (left, right) sample; silence while the decoding is behind.
    pub fn next(&mut self) -> (f32, f32) {
        if self.at >= self.chunk.len() {
            let Ok(chunk) = self.rx.try_recv() else { return (0.0, 0.0) };
            let played = std::mem::replace(&mut self.chunk, chunk);
            let _ = self.back.try_send(played);
            self.at = 0;
        }
        let s = (self.chunk[self.at], self.chunk[self.at + 1]);
        self.at += 2;
        s
    }
}

/// A track's reader and decoder: (format, decoder, track id, sample rate, channels).
type Opened = (Box<dyn FormatReader>, Box<dyn symphonia::core::codecs::audio::AudioDecoder>, u32, u32, usize);

fn open(bytes: &'static [u8]) -> Option<Opened> {
    let mss = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    let format = symphonia::default::get_probe()
        .probe(&Hint::new(), mss, FormatOptions::default(), MetadataOptions::default())
        .ok()?;
    let track = format.default_track(TrackType::Audio)?;
    let params = track.codec_params.as_ref()?.audio()?;
    let rate = params.sample_rate?;
    let channels = params.channels.as_ref().map_or(2, |c| c.count()).max(1);
    let decoder = symphonia::default::get_codecs().make_audio_decoder(params, &AudioDecoderOptions::default()).ok()?;
    let id = track.id;
    Some((format, decoder, id, rate, channels))
}

/// The decoding thread: plays `tracks` in a loop, resampled to `rate` (linear interpolation),
/// until the player is dropped.
fn stream(tracks: &'static [&'static [u8]], rate: f32, tx: SyncSender<Vec<f32>>, spare: Receiver<Vec<f32>>) {
    let mut out = Vec::with_capacity(CHUNK * 2);
    let mut pcm: Vec<f32> = Vec::new();
    // The output's position between the last two source frames, in source frames.
    let mut pos = 0.0f64;
    let mut prev = (0.0f32, 0.0f32);
    for &bytes in tracks.iter().cycle() {
        let Some((mut format, mut decoder, id, source_rate, channels)) = open(bytes) else {
            eprintln!("music: a track does not decode");
            return;
        };
        let step = source_rate as f64 / rate as f64;
        while let Ok(Some(packet)) = format.next_packet() {
            if packet.track_id != id {
                continue;
            }
            // A damaged packet is skipped.
            let Ok(buf) = decoder.decode(&packet) else { continue };
            buf.copy_to_vec_interleaved(&mut pcm);
            for frame in pcm.chunks_exact(channels) {
                let next = (frame[0], frame[channels.min(2) - 1]);
                while pos < 1.0 {
                    let t = pos as f32;
                    out.push(prev.0 + (next.0 - prev.0) * t);
                    out.push(prev.1 + (next.1 - prev.1) * t);
                    pos += step;
                    if out.len() == CHUNK * 2 {
                        let empty = match spare.try_recv() {
                            Ok(mut v) => {
                                v.clear();
                                v
                            }
                            Err(_) => Vec::with_capacity(CHUNK * 2),
                        };
                        if tx.send(std::mem::replace(&mut out, empty)).is_err() {
                            return;
                        }
                    }
                }
                pos -= 1.0;
                prev = next;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_track_decodes() {
        for &bytes in MENU.iter().chain(MARS).chain(MARS_NIGHT) {
            let (mut format, mut decoder, id, rate, channels) = open(bytes).expect("track opens");
            assert_eq!((rate, channels), (48_000, 2));
            let mut pcm: Vec<f32> = Vec::new();
            let mut frames = 0;
            while frames < 10 * 48_000 {
                let packet = format.next_packet().unwrap().expect("ten seconds of audio");
                if packet.track_id == id {
                    decoder.decode(&packet).unwrap().copy_to_vec_interleaved(&mut pcm);
                    assert!(pcm.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
                    frames += pcm.len() / channels;
                }
            }
        }
    }

    #[test]
    fn plays_at_the_output_rate() {
        let mut music = Music::new(MENU, 44_100.0);
        std::thread::sleep(std::time::Duration::from_millis(500));
        // Seconds 0..0.5 at 44.1 kHz: what is queued, all of it sound.
        let out: Vec<(f32, f32)> = (0..22_050).map(|_| music.next()).collect();
        let rms = (out.iter().map(|(l, r)| l * l + r * r).sum::<f32>() / (2 * out.len()) as f32).sqrt();
        assert!(rms > 0.003, "rms {rms}");
        assert!(out.iter().all(|(l, r)| l.abs() <= 1.0 && r.abs() <= 1.0));
    }
}
