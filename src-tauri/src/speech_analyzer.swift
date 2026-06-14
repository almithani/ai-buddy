// SpeechAnalyzer/SpeechTranscriber engine (macOS 26+).
// Exposes a plain C interface consumed by capture.m's runtime dispatch;
// pre-26 systems fall back to the ObjC AiBuddySpeechLane path.
//
// Design notes:
// - Two independent pipelines (mic = source 0, system audio = source 1), each
//   its own SpeechAnalyzer + SpeechTranscriber. Both transcribers are
//   configured identically so they share the backing engine/model.
// - The analyzer does NOT convert audio; we convert every incoming buffer to
//   bestAvailableAudioFormat via AVAudioConverter (rebuilt on format change).
// - Results: volatile (is_final=false) then finalized (is_final=true) — maps
//   1:1 onto the existing AiBuddySpeechCallback semantics.

import AVFoundation
import CoreMedia
import Foundation
import Speech

// source: 0 = mic ("me"), 1 = system audio ("them"), -2 = non-fatal warning
// start/end are audio-relative seconds (-1 when unknown).
public typealias AiBuddySpeechCallbackSwift =
    @convention(c) (Int32, UnsafePointer<CChar>?, Bool, Double, Double, UnsafeMutableRawPointer?) -> Void

// ── Lane ────────────────────────────────────────────────────────────────────

@available(macOS 26.0, *)
final class AnalyzerLane: @unchecked Sendable {
    let source: Int32
    private let cb: AiBuddySpeechCallbackSwift
    private let ctx: UnsafeMutableRawPointer?

    private let transcriber: SpeechTranscriber
    private let analyzer: SpeechAnalyzer
    private let analyzerFormat: AVAudioFormat
    private let continuation: AsyncStream<AnalyzerInput>.Continuation
    private var resultsTask: Task<Void, Never>?

    private let convLock = NSLock()
    private var converter: AVAudioConverter?
    private var lastInputFormat: AVAudioFormat?

    // Optional recording of the converted stream (for post-meeting diarization).
    private let fileLock = NSLock()
    private var audioFile: AVAudioFile?

    init?(source: Int32, locale: Locale, recordPath: String?,
          cb: @escaping AiBuddySpeechCallbackSwift, ctx: UnsafeMutableRawPointer?) async {
        self.source = source
        self.cb = cb
        self.ctx = ctx

        let transcriber = SpeechTranscriber(
            locale: locale,
            transcriptionOptions: [],
            reportingOptions: [.volatileResults],
            attributeOptions: [.audioTimeRange]
        )
        self.transcriber = transcriber
        self.analyzer = SpeechAnalyzer(modules: [transcriber])

        guard let fmt = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [transcriber]) else {
            NSLog("[AiBuddy] SA lane %d: no compatible audio format", source)
            return nil
        }
        self.analyzerFormat = fmt

        // Record the converted (analyzer-format) stream to WAV for diarization.
        if let recordPath {
            do {
                self.audioFile = try AVAudioFile(
                    forWriting: URL(fileURLWithPath: recordPath),
                    settings: fmt.settings,
                    commonFormat: fmt.commonFormat,
                    interleaved: fmt.isInterleaved)
                NSLog("[AiBuddy] SA lane %d: recording to %@", source, recordPath)
            } catch {
                NSLog("[AiBuddy] SA lane %d: recording open failed: %@",
                      source, error.localizedDescription)
            }
        }

        let (stream, continuation) = AsyncStream<AnalyzerInput>.makeStream()
        self.continuation = continuation

        do {
            try await analyzer.prepareToAnalyze(in: fmt)
            try await analyzer.start(inputSequence: stream)
        } catch {
            NSLog("[AiBuddy] SA lane %d: start failed: %@", source, error.localizedDescription)
            return nil
        }

        NSLog("[AiBuddy] SA lane %d: started (%.0f Hz, %u ch)",
              source, fmt.sampleRate, fmt.channelCount)

        resultsTask = Task { [weak self] in
            do {
                for try await result in transcriber.results {
                    guard let self else { return }
                    let text = String(result.text.characters)
                        .trimmingCharacters(in: .whitespacesAndNewlines)
                    if !text.isEmpty {
                        let range = result.range
                        let start = range.start.isNumeric ? range.start.seconds : -1.0
                        let end = range.end.isNumeric ? range.end.seconds : -1.0
                        text.withCString { cstr in
                            self.cb(self.source, cstr, result.isFinal, start, end, self.ctx)
                        }
                    }
                }
            } catch {
                guard let self else { return }
                NSLog("[AiBuddy] SA lane %d: results error: %@",
                      self.source, error.localizedDescription)
                let msg = "Speech analysis error on \(self.source == 0 ? "microphone" : "system audio"): \(error.localizedDescription)"
                msg.withCString { cstr in
                    self.cb(-2, cstr, true, -1.0, -1.0, self.ctx)
                }
            }
        }
    }

    /// Convert to the analyzer's format and feed. Called synchronously from
    /// the audio threads — conversion copies the data, so the caller's buffer
    /// is never referenced after return.
    func append(_ buf: AVAudioPCMBuffer) {
        guard let converted = convert(buf) else { return }
        if audioFile != nil {
            fileLock.lock()
            try? audioFile?.write(from: converted)
            fileLock.unlock()
        }
        continuation.yield(AnalyzerInput(buffer: converted))
    }

    func append(_ sbuf: CMSampleBuffer) {
        guard let pcm = Self.pcmBuffer(from: sbuf) else { return }
        append(pcm)
    }

    private func convert(_ buf: AVAudioPCMBuffer) -> AVAudioPCMBuffer? {
        if buf.format == analyzerFormat {
            // Same format — still copy, since the caller's buffer is transient.
            return Self.copy(buf)
        }

        convLock.lock()
        defer { convLock.unlock() }

        if converter == nil || lastInputFormat != buf.format {
            converter = AVAudioConverter(from: buf.format, to: analyzerFormat)
            lastInputFormat = buf.format
            NSLog("[AiBuddy] SA lane %d: converter %@ -> %.0f Hz", source,
                  buf.format.description, analyzerFormat.sampleRate)
        }
        guard let converter else { return nil }

        let ratio = analyzerFormat.sampleRate / buf.format.sampleRate
        let capacity = AVAudioFrameCount(Double(buf.frameLength) * ratio) + 16
        guard let out = AVAudioPCMBuffer(pcmFormat: analyzerFormat, frameCapacity: capacity) else {
            return nil
        }

        var consumed = false
        var err: NSError?
        converter.convert(to: out, error: &err) { _, status in
            if consumed {
                status.pointee = .noDataNow
                return nil
            }
            consumed = true
            status.pointee = .haveData
            return buf
        }
        if let err {
            NSLog("[AiBuddy] SA lane %d: convert failed: %@", source, err.localizedDescription)
            return nil
        }
        return out.frameLength > 0 ? out : nil
    }

    private static func copy(_ buf: AVAudioPCMBuffer) -> AVAudioPCMBuffer? {
        guard let out = AVAudioPCMBuffer(pcmFormat: buf.format, frameCapacity: buf.frameLength)
        else { return nil }
        out.frameLength = buf.frameLength
        let src = buf.audioBufferList
        let dst = out.mutableAudioBufferList
        for i in 0..<Int(src.pointee.mNumberBuffers) {
            let s = UnsafeMutableAudioBufferListPointer(UnsafeMutablePointer(mutating: src))[i]
            var d = UnsafeMutableAudioBufferListPointer(dst)[i]
            if let sd = s.mData, let dd = d.mData {
                memcpy(dd, sd, Int(min(s.mDataByteSize, d.mDataByteSize)))
                d.mDataByteSize = min(s.mDataByteSize, d.mDataByteSize)
            }
        }
        return out
    }

    /// Wrap a ScreenCaptureKit CMSampleBuffer's audio as an AVAudioPCMBuffer
    /// (no copy — `convert` copies before this call returns).
    private static func pcmBuffer(from sbuf: CMSampleBuffer) -> AVAudioPCMBuffer? {
        guard let desc = CMSampleBufferGetFormatDescription(sbuf),
              let asbd = CMAudioFormatDescriptionGetStreamBasicDescription(desc),
              let fmt = AVAudioFormat(streamDescription: asbd)
        else { return nil }

        let frames = AVAudioFrameCount(CMSampleBufferGetNumSamples(sbuf))
        guard frames > 0 else { return nil }

        guard let pcm = AVAudioPCMBuffer(pcmFormat: fmt, frameCapacity: frames) else { return nil }
        pcm.frameLength = frames
        let status = CMSampleBufferCopyPCMDataIntoAudioBufferList(
            sbuf, at: 0, frameCount: Int32(frames),
            into: pcm.mutableAudioBufferList)
        return status == noErr ? pcm : nil
    }

    func stop() {
        continuation.finish()
        // Close the recording — releasing the AVAudioFile flushes the WAV header.
        fileLock.lock()
        audioFile = nil
        fileLock.unlock()
        let analyzer = self.analyzer
        let source = self.source
        Task {
            do {
                try await analyzer.finalizeAndFinishThroughEndOfInput()
            } catch {
                NSLog("[AiBuddy] SA lane %d: finalize failed: %@", source, error.localizedDescription)
            }
        }
    }
}

// ── Engine singleton ────────────────────────────────────────────────────────

@available(macOS 26.0, *)
enum SAEngine {
    nonisolated(unsafe) static var lanes: [Int32: AnalyzerLane] = [:]
    nonisolated(unsafe) static let lock = NSLock()

    static func resolveLocale() async -> Locale? {
        let installed = await Set(SpeechTranscriber.installedLocales.map {
            $0.identifier(.bcp47)
        })
        if let l = await SpeechTranscriber.supportedLocale(equivalentTo: .current),
           installed.contains(l.identifier(.bcp47)) {
            return l
        }
        let en = Locale(identifier: "en_US")
        if let l = await SpeechTranscriber.supportedLocale(equivalentTo: en),
           installed.contains(l.identifier(.bcp47)) {
            return l
        }
        return nil
    }

    /// Locale the assets flow should target (supported, regardless of installed).
    static func targetLocale() async -> Locale? {
        if let l = await SpeechTranscriber.supportedLocale(equivalentTo: .current) { return l }
        return await SpeechTranscriber.supportedLocale(equivalentTo: Locale(identifier: "en_US"))
    }
}

/// Bridge an async operation into the synchronous C world.
private func blockingAsync<T>(_ op: @escaping @Sendable () async -> T) -> T {
    let sem = DispatchSemaphore(value: 0)
    nonisolated(unsafe) var result: T?
    Task.detached {
        result = await op()
        sem.signal()
    }
    sem.wait()
    return result!
}

// ── C entry points ──────────────────────────────────────────────────────────

/// 1 if the SpeechAnalyzer path can be used right now (macOS 26+, assets installed).
@_cdecl("aibuddy_sa_available")
public func aibuddy_sa_available() -> Int32 {
    guard #available(macOS 26.0, *) else { return 0 }
    return blockingAsync { await SAEngine.resolveLocale() != nil } ? 1 : 0
}

/// 0 unsupported, 1 installed, 2 download required, -1 pre-macOS-26.
@_cdecl("aibuddy_sa_assets_status")
public func aibuddy_sa_assets_status() -> Int32 {
    guard #available(macOS 26.0, *) else { return -1 }
    return blockingAsync {
        if await SAEngine.resolveLocale() != nil { return Int32(1) }
        if await SAEngine.targetLocale() != nil { return Int32(2) }
        return Int32(0)
    }
}

@_cdecl("aibuddy_sa_assets_install")
public func aibuddy_sa_assets_install(
    _ progressCb: @convention(c) (Double, UnsafeMutableRawPointer?) -> Void,
    _ doneCb: @convention(c) (Int32, UnsafeMutableRawPointer?) -> Void,
    _ ctx: UnsafeMutableRawPointer?
) {
    guard #available(macOS 26.0, *) else {
        doneCb(-1, ctx)
        return
    }
    nonisolated(unsafe) let uctx = ctx
    Task.detached {
        guard let locale = await SAEngine.targetLocale() else {
            doneCb(-2, uctx)
            return
        }
        let transcriber = SpeechTranscriber(
            locale: locale,
            transcriptionOptions: [],
            reportingOptions: [.volatileResults],
            attributeOptions: []
        )
        do {
            if let request = try await AssetInventory.assetInstallationRequest(
                supporting: [transcriber]) {
                let progress = request.progress
                let poller = Task.detached {
                    while !Task.isCancelled {
                        progressCb(progress.fractionCompleted * 100.0, uctx)
                        try? await Task.sleep(nanoseconds: 300_000_000)
                    }
                }
                try await request.downloadAndInstall()
                poller.cancel()
            }
            progressCb(100.0, uctx)
            doneCb(0, uctx)
        } catch {
            NSLog("[AiBuddy] SA asset install failed: %@", error.localizedDescription)
            doneCb(-3, uctx)
        }
    }
}

/// 0 ok, -1 unavailable, -3 assets/locale unavailable.
/// recordWavPath (nullable): records the Them stream (source 1) for diarization.
@_cdecl("aibuddy_sa_start")
public func aibuddy_sa_start(
    _ cb: AiBuddySpeechCallbackSwift,
    _ ctx: UnsafeMutableRawPointer?,
    _ recordWavPath: UnsafePointer<CChar>?
) -> Int32 {
    guard #available(macOS 26.0, *) else { return -1 }
    nonisolated(unsafe) let uctx = ctx
    let recordPath = recordWavPath.map { String(cString: $0) }
    return blockingAsync {
        guard let locale = await SAEngine.resolveLocale() else { return Int32(-3) }

        SAEngine.lock.lock()
        let old = SAEngine.lanes
        SAEngine.lanes = [:]
        SAEngine.lock.unlock()
        for (_, lane) in old { lane.stop() }

        guard let mic = await AnalyzerLane(source: 0, locale: locale, recordPath: nil,
                                           cb: cb, ctx: uctx),
              let sys = await AnalyzerLane(source: 1, locale: locale, recordPath: recordPath,
                                           cb: cb, ctx: uctx)
        else {
            return Int32(-3)
        }
        SAEngine.lock.lock()
        SAEngine.lanes = [0: mic, 1: sys]
        SAEngine.lock.unlock()
        return Int32(0)
    }
}

@_cdecl("aibuddy_sa_append_pcm")
public func aibuddy_sa_append_pcm(_ source: Int32, _ buf: UnsafeMutableRawPointer) {
    guard #available(macOS 26.0, *) else { return }
    let pcm = Unmanaged<AVAudioPCMBuffer>.fromOpaque(buf).takeUnretainedValue()
    SAEngine.lock.lock()
    let lane = SAEngine.lanes[source]
    SAEngine.lock.unlock()
    lane?.append(pcm)
}

@_cdecl("aibuddy_sa_append_sample")
public func aibuddy_sa_append_sample(_ source: Int32, _ sbuf: UnsafeMutableRawPointer) {
    guard #available(macOS 26.0, *) else { return }
    let sample = Unmanaged<CMSampleBuffer>.fromOpaque(sbuf).takeUnretainedValue()
    SAEngine.lock.lock()
    let lane = SAEngine.lanes[source]
    SAEngine.lock.unlock()
    lane?.append(sample)
}

@_cdecl("aibuddy_sa_stop")
public func aibuddy_sa_stop() {
    guard #available(macOS 26.0, *) else { return }
    SAEngine.lock.lock()
    let lanes = SAEngine.lanes
    SAEngine.lanes = [:]
    SAEngine.lock.unlock()
    for (_, lane) in lanes { lane.stop() }
}
