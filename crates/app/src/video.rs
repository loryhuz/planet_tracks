//! The menu's background footage: an MP4 loop (HEVC, 30 frames per second, filmed in the game
//! with `MARS_FILM` by `tools/video/menu_montage.sh`) decoded by the system on Apple platforms,
//! with AVFoundation's asset reader on a thread of its own, and handed over as BGRA frames. The
//! clips are files beside the game, not part of the executable (they are large and generated):
//! where none is found, or elsewhere than on Apple platforms, the menu keeps its night sky.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

/// Frames per second of the footage.
pub const FPS: f32 = 30.0;

/// One decoded image, BGRA, rows packed without padding.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    /// Seconds from the start of the loop.
    pub time: f32,
}

/// A clip decoded over and over; dropping it stops its thread.
pub struct VideoLoop {
    frames: Receiver<Frame>,
    spare: SyncSender<Vec<u8>>,
}

impl VideoLoop {
    /// Starts decoding the clip `name` (an MP4 file found by [`find`]) in a loop, or `None` where
    /// it is missing or nothing can decode it.
    #[allow(unused_variables)]
    pub fn open(name: &str) -> Option<Self> {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            let Some(path) = find(name) else {
                // Easy to miss in a package: the menu still works, over its night sky.
                eprintln!("menu film {name} not found: the menu shows the night sky (tools/video/menu_montage.sh, docs/release.md)");
                return None;
            };
            // Two frames ahead: the decoder waits for the menu instead of running away from it.
            let (frames_tx, frames) = sync_channel(2);
            let (spare, spare_rx) = sync_channel(4);
            std::thread::Builder::new()
                .name(format!("video {name}"))
                .spawn(move || apple::run(&path, &frames_tx, &spare_rx))
                .ok()?;
            Some(Self { frames, spare })
        }
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        None
    }

    /// The next frame, if one is decoded.
    pub fn next(&self) -> Option<Frame> {
        self.frames.try_recv().ok()
    }

    /// Gives a frame's pixels back for the decoder to fill again.
    pub fn recycle(&self, data: Vec<u8>) {
        let _ = self.spare.try_send(data);
    }
}

/// Where the clip `name` (`menu-wide.mp4`…) is: in `video/` beside the executable, in a macOS
/// bundle's `Resources/video/`, beside the executable itself (an iOS bundle), or in the source
/// tree's `assets/video/` for a game built and run from the repository.
pub fn find(name: &str) -> Option<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        dirs.push(exe_dir.join("video"));
        dirs.push(exe_dir.join("../Resources/video"));
        dirs.push(exe_dir);
    }
    dirs.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/video"));
    dirs.into_iter().map(|d| d.join(name)).find(|p| p.is_file())
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod apple {
    use std::path::Path;
    use std::sync::mpsc::{Receiver, SyncSender};

    use objc2::rc::autoreleasepool;
    use objc2::runtime::AnyObject;
    use objc2_av_foundation::{AVAssetReader, AVAssetReaderStatus, AVAssetReaderTrackOutput, AVMediaTypeVideo, AVURLAsset};
    use objc2_core_foundation::CFString;
    use objc2_core_video::{
        CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight, CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress,
        CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress, kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_32BGRA,
    };
    use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};

    use super::Frame;

    enum Pass {
        /// The clip played to its end: start again.
        Ended,
        /// The menu let the clip go.
        Stopped,
        /// The file could not be read.
        Failed,
    }

    pub fn run(path: &Path, frames: &SyncSender<Frame>, spare: &Receiver<Vec<u8>>) {
        loop {
            match autoreleasepool(|_| pass(path, frames, spare)) {
                Pass::Ended => {}
                Pass::Stopped | Pass::Failed => return,
            }
        }
    }

    /// Reads the clip once, from start to end.
    fn pass(path: &Path, frames: &SyncSender<Frame>, spare: &Receiver<Vec<u8>>) -> Pass {
        let Some(path) = path.to_str() else { return Pass::Failed };
        // SAFETY: plain calls into AVFoundation and CoreVideo with valid, retained objects; the
        // pixel buffer is read only between its lock and unlock.
        unsafe {
            let url = NSURL::fileURLWithPath(&NSString::from_str(path));
            let asset = AVURLAsset::URLAssetWithURL_options(&url, None);
            let Some(kind) = AVMediaTypeVideo else { return Pass::Failed };
            #[allow(deprecated)]
            let tracks = asset.tracksWithMediaType(kind);
            let Some(track) = tracks.firstObject() else { return Pass::Failed };
            let Ok(reader) = AVAssetReader::assetReaderWithAsset_error(&asset) else { return Pass::Failed };
            // CFString and NSString are the same object (toll-free bridged).
            let key: &NSString = &*(kCVPixelBufferPixelFormatTypeKey as *const CFString).cast::<NSString>();
            let format = NSNumber::new_u32(kCVPixelFormatType_32BGRA);
            let value: &AnyObject = &format;
            let settings = NSDictionary::<NSString, AnyObject>::from_slices(&[key], &[value]);
            let output = AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(&track, Some(&settings));
            output.setAlwaysCopiesSampleData(false);
            reader.addOutput(&output);
            if !reader.startReading() {
                return Pass::Failed;
            }
            let mut count = 0;
            loop {
                let frame = autoreleasepool(|_| {
                    let sample = output.copyNextSampleBuffer()?;
                    let image = sample.image_buffer()?;
                    let pts = sample.presentation_time_stamp();
                    let time = if pts.timescale > 0 { pts.value as f64 / pts.timescale as f64 } else { count as f64 / super::FPS as f64 };
                    if CVPixelBufferLockBaseAddress(&image, CVPixelBufferLockFlags::ReadOnly) != 0 {
                        return None;
                    }
                    let (w, h) = (CVPixelBufferGetWidth(&image), CVPixelBufferGetHeight(&image));
                    let row = CVPixelBufferGetBytesPerRow(&image);
                    let base = CVPixelBufferGetBaseAddress(&image) as *const u8;
                    let mut data = spare.try_recv().unwrap_or_default();
                    data.clear();
                    if !base.is_null() {
                        data.reserve(w * h * 4);
                        for y in 0..h {
                            data.extend_from_slice(std::slice::from_raw_parts(base.add(y * row), w * 4));
                        }
                    }
                    CVPixelBufferUnlockBaseAddress(&image, CVPixelBufferLockFlags::ReadOnly);
                    (data.len() == w * h * 4).then_some(Frame { width: w as u32, height: h as u32, data, time: time as f32 })
                });
                let Some(frame) = frame else { break };
                if frames.send(frame).is_err() {
                    reader.cancelReading();
                    return Pass::Stopped;
                }
                count += 1;
            }
            if count > 0 && reader.status() == AVAssetReaderStatus::Completed { Pass::Ended } else { Pass::Failed }
        }
    }
}
