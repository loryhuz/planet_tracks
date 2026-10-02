// Films a session of the game as it is played, with its sound: the screen its window is on (its
// windows only, nothing of the other apps) at the screen's full resolution, and its own audio,
// through ScreenCaptureKit, into a .mov (HEVC at a high bitrate, up to 60 fps, a key frame every
// second so it cuts without re-encoding; AAC sound), until the game quits.
//
//   swiftc -O tools/video/record.swift -o /tmp/record
//   MARS_FULLSCREEN=1 ./target/release/mars-racer & /tmp/record OUT.mov $!
//
// `record --check` only asks for the screen: the app it runs from needs the Screen & System Audio
// Recording permission (System Settings, Privacy & Security), which macOS asks for the first time.

import AVFoundation
import Foundation
import ScreenCaptureKit
import VideoToolbox

/// HEVC bits per pixel and frame: about 140 Mbit/s for a 5K screen at 60 fps, 70 for 4K.
let BITS_PER_PIXEL = 0.16

/// Writes the stream's frames and sound as they come, on one queue.
final class Writer: NSObject, SCStreamOutput, SCStreamDelegate {
    let queue = DispatchQueue(label: "record")
    let writer: AVAssetWriter
    let video: AVAssetWriterInput
    let audio: AVAssetWriterInput
    var started = false
    var frames = 0
    var dropped = 0
    var first = CMTime.invalid
    var last = CMTime.invalid
    var failure: Error?

    init(url: URL, width: Int, height: Int) throws {
        writer = try AVAssetWriter(outputURL: url, fileType: .mov)
        video = AVAssetWriterInput(mediaType: .video, outputSettings: [
            AVVideoCodecKey: AVVideoCodecType.hevc,
            AVVideoWidthKey: width,
            AVVideoHeightKey: height,
            AVVideoColorPropertiesKey: [
                AVVideoColorPrimariesKey: AVVideoColorPrimaries_ITU_R_709_2,
                AVVideoTransferFunctionKey: AVVideoTransferFunction_ITU_R_709_2,
                AVVideoYCbCrMatrixKey: AVVideoYCbCrMatrix_ITU_R_709_2,
            ],
            AVVideoCompressionPropertiesKey: [
                AVVideoAverageBitRateKey: Int(Double(width * height) * 60 * BITS_PER_PIXEL),
                AVVideoExpectedSourceFrameRateKey: 60,
                AVVideoMaxKeyFrameIntervalKey: 60,
                AVVideoMaxKeyFrameIntervalDurationKey: 1.0,
                AVVideoProfileLevelKey: kVTProfileLevel_HEVC_Main_AutoLevel as String,
            ],
        ])
        video.expectsMediaDataInRealTime = true
        audio = AVAssetWriterInput(mediaType: .audio, outputSettings: [
            AVFormatIDKey: kAudioFormatMPEG4AAC,
            AVSampleRateKey: 48000,
            AVNumberOfChannelsKey: 2,
            AVEncoderBitRateKey: 256_000,
        ])
        audio.expectsMediaDataInRealTime = true
        writer.add(video)
        writer.add(audio)
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer sample: CMSampleBuffer, of type: SCStreamOutputType) {
        guard sample.isValid, failure == nil else { return }
        switch type {
        case .screen:
            // Only frames with new content carry an image (the others say nothing changed).
            guard let infos = CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
                  let raw = infos.first?[.status] as? Int, SCFrameStatus(rawValue: raw) == .complete
            else { return }
            if !started {
                guard writer.startWriting() else {
                    failure = writer.error
                    print("cannot write: \(writer.error?.localizedDescription ?? "?")")
                    return
                }
                writer.startSession(atSourceTime: sample.presentationTimeStamp)
                first = sample.presentationTimeStamp
                started = true
            }
            if video.isReadyForMoreMediaData && video.append(sample) {
                frames += 1
                last = sample.presentationTimeStamp
            } else {
                dropped += 1
            }
        case .audio:
            // Sound from before the first frame would start the file on a black screen.
            if started && audio.isReadyForMoreMediaData {
                audio.append(sample)
            }
        default:
            break
        }
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        queue.async { self.failure = error }
        print("capture stopped: \(error.localizedDescription)")
    }

    var seconds: Double {
        queue.sync { started && last.isValid ? CMTimeGetSeconds(last - first) : 0 }
    }

    func finish() async {
        guard queue.sync(execute: { started }) else { return }
        queue.sync {
            video.markAsFinished()
            audio.markAsFinished()
        }
        await writer.finishWriting()
        if let error = writer.error {
            print("finishing failed: \(error.localizedDescription)")
        }
    }
}

func alive(_ pid: pid_t) -> Bool {
    kill(pid, 0) == 0
}

func sleep(_ seconds: Double) async {
    try? await Task.sleep(nanoseconds: UInt64(seconds * 1e9))
}

/// Starts or stops the stream and waits until it has (the plain calls return at once).
func waitFor(_ call: (@escaping (Error?) -> Void) -> Void) async throws {
    try await withCheckedThrowingContinuation { (done: CheckedContinuation<Void, Error>) in
        call { error in
            if let error {
                done.resume(throwing: error)
            } else {
                done.resume()
            }
        }
    }
}

setvbuf(stdout, nil, _IOLBF, 0)
let args = CommandLine.arguments
if args.count == 2 && args[1] == "--check" {
    do {
        let content = try await SCShareableContent.excludingDesktopWindows(true, onScreenWindowsOnly: true)
        print("screen recording allowed (\(content.displays.count) displays)")
        exit(0)
    } catch {
        print("screen recording refused: \(error.localizedDescription)")
        exit(1)
    }
}
guard args.count == 3, let pid = pid_t(args[2]) else {
    print("usage: record OUT.mov GAME_PID | record --check")
    exit(2)
}
let out = URL(fileURLWithPath: args[1])
try? FileManager.default.removeItem(at: out)

// The game's window, once it is up and settled full screen.
var found: SCWindow?
for _ in 0..<60 {
    let content = try await SCShareableContent.excludingDesktopWindows(true, onScreenWindowsOnly: true)
    found = content.windows.first { $0.owningApplication?.processID == pid && $0.frame.width > 200 }
    if found != nil || !alive(pid) {
        break
    }
    await sleep(0.5)
}
guard let first = found, let app = first.owningApplication else {
    print("no window of the game (pid \(pid))")
    exit(1)
}
await sleep(2.0)
let content = try await SCShareableContent.excludingDesktopWindows(true, onScreenWindowsOnly: true)
let window = content.windows.first { $0.windowID == first.windowID } ?? first
let centre = CGPoint(x: window.frame.midX, y: window.frame.midY)
guard let display = content.displays.first(where: { $0.frame.contains(centre) }) ?? content.displays.first else {
    print("no display")
    exit(1)
}

let filter = SCContentFilter(display: display, including: [app], exceptingWindows: [])
let scale = CGFloat(filter.pointPixelScale)
let config = SCStreamConfiguration()
config.width = Int(display.frame.width * scale) & ~1
config.height = Int(display.frame.height * scale) & ~1
config.minimumFrameInterval = CMTime(value: 1, timescale: 60)
config.queueDepth = 8
config.showsCursor = false
config.pixelFormat = kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange
config.colorMatrix = CGDisplayStream.yCbCrMatrix_ITU_R_709_2
config.colorSpaceName = CGColorSpace.sRGB
config.capturesAudio = true
config.excludesCurrentProcessAudio = true
config.sampleRate = 48000
config.channelCount = 2

let writer = try Writer(url: out, width: config.width, height: config.height)
let stream = SCStream(filter: filter, configuration: config, delegate: writer)
try stream.addStreamOutput(writer, type: .screen, sampleHandlerQueue: writer.queue)
try stream.addStreamOutput(writer, type: .audio, sampleHandlerQueue: writer.queue)
try await waitFor { stream.startCapture(completionHandler: $0) }
let mbps = Int(Double(config.width * config.height) * 60 * BITS_PER_PIXEL / 1e6)
print("recording \(config.width) x \(config.height), up to 60 fps, \(mbps) Mbit/s, with the game's sound into \(out.path)")

var shown = 0
while alive(pid) && writer.queue.sync(execute: { writer.failure == nil }) {
    await sleep(0.5)
    let seconds = Int(writer.seconds)
    if seconds >= shown + 30 {
        shown = seconds - seconds % 30
        print("  \(shown) s")
    }
}
try? await waitFor { stream.stopCapture(completionHandler: $0) }
await writer.finish()
let (frames, dropped) = writer.queue.sync { (writer.frames, writer.dropped) }
print("done: \(Int(writer.seconds)) s, \(frames) frames (\(dropped) dropped) in \(out.path)")
