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

    /// Like stop(), but waits (up to `timeout` seconds) until the final results
    /// for everything already fed have been delivered — hold-to-talk needs the
    /// last words before it sends. Returns false if it gave up at the timeout.
    @discardableResult
    func finish(timeout: Double) async -> Bool {
        continuation.finish()
        let analyzer = self.analyzer
        let results = self.resultsTask
        return await withTaskGroup(of: Bool.self) { group in
            group.addTask {
                try? await analyzer.finalizeAndFinishThroughEndOfInput()
                await results?.value
                return true
            }
            group.addTask {
                try? await Task.sleep(nanoseconds: UInt64(timeout * 1_000_000_000))
                return false
            }
            let completed = await group.next() ?? false
            group.cancelAll()
            return completed
        }
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

// ── Hold-to-talk dictation ──────────────────────────────────────────────────
// Separate from the meeting lanes (own AVAudioEngine + lane, source 2) so
// holding ⌥Space works even while a meeting is being transcribed.
//
// Latency matters: people start talking the moment they press. Measured on
// macOS 26.6: creating a lane takes ~55–140 ms and starting the mic 150–770 ms,
// so a spare lane is prepared in advance (prewarm) and the mic is started on
// key-down rather than after the hold is confirmed (voice.rs).

@available(macOS 26.0, *)
enum Dictation {
    nonisolated(unsafe) static var engine: AVAudioEngine?
    nonisolated(unsafe) static var lane: AnalyzerLane?
    /// A ready-to-use lane (no mic attached), so starting doesn't wait for setup.
    nonisolated(unsafe) static var spare: AnalyzerLane?
    nonisolated(unsafe) static let lock = NSLock()
    // Diagnostics for the current session (logged at stop).
    nonisolated(unsafe) static var buffers = 0
    nonisolated(unsafe) static var peak: Float = 0
    nonisolated(unsafe) static var configChanged = false
    nonisolated(unsafe) static var observer: NSObjectProtocol?

    static func makeSpare(cb: AiBuddySpeechCallbackSwift, ctx: UnsafeMutableRawPointer?) async {
        guard let locale = await SAEngine.resolveLocale(),
              let lane = await AnalyzerLane(source: 2, locale: locale, recordPath: nil, cb: cb, ctx: ctx)
        else { return }
        lock.lock()
        let old = spare
        spare = lane
        lock.unlock()
        old?.stop()
    }
}

/// 1 when microphone access is already granted (so starting the mic won't
/// show a permission prompt).
@_cdecl("aibuddy_mic_authorized")
public func aibuddy_mic_authorized() -> Int32 {
    AVCaptureDevice.authorizationStatus(for: .audio) == .authorized ? 1 : 0
}

/// Prepares a spare dictation lane in the background (no mic). Call at launch.
@_cdecl("aibuddy_dictation_prewarm")
public func aibuddy_dictation_prewarm(_ cb: AiBuddySpeechCallbackSwift, _ ctx: UnsafeMutableRawPointer?) {
    guard #available(macOS 26.0, *) else { return }
    nonisolated(unsafe) let uctx = ctx
    Task.detached { await Dictation.makeSpare(cb: cb, ctx: uctx) }
}

/// 0 listening, -1 pre-macOS-26, -3 speech assets/locale unavailable,
/// -4 microphone unavailable or permission denied. Call off the main thread.
@_cdecl("aibuddy_dictation_start")
public func aibuddy_dictation_start(
    _ cb: AiBuddySpeechCallbackSwift,
    _ ctx: UnsafeMutableRawPointer?
) -> Int32 {
    guard #available(macOS 26.0, *) else { return -1 }
    switch AVCaptureDevice.authorizationStatus(for: .audio) {
    case .denied, .restricted: return -4
    default: break
    }
    nonisolated(unsafe) let uctx = ctx
    return blockingAsync {
        Dictation.lock.lock()
        var lane = Dictation.spare
        Dictation.spare = nil
        Dictation.lock.unlock()
        if lane == nil {
            guard let locale = await SAEngine.resolveLocale() else { return Int32(-3) }
            lane = await AnalyzerLane(source: 2, locale: locale, recordPath: nil, cb: cb, ctx: uctx)
        }
        guard let lane else { return Int32(-3) }

        let engine = AVAudioEngine()
        let input = engine.inputNode
        guard input.outputFormat(forBus: 0).sampleRate > 0 else {
            lane.stop()
            return Int32(-4)
        }
        Dictation.buffers = 0
        Dictation.peak = 0
        Dictation.configChanged = false
        // format: nil = whatever the device delivers right now. A fixed format
        // goes stale when a Bluetooth headset switches into call mode as its mic
        // opens, and the tap then stops delivering audio.
        let tap: AVAudioNodeTapBlock = { buf, _ in
            Dictation.buffers += 1
            if let d = buf.floatChannelData?[0] {
                for i in 0..<Int(buf.frameLength) { Dictation.peak = max(Dictation.peak, abs(d[i])) }
            }
            lane.append(buf)
        }
        input.installTap(onBus: 0, bufferSize: 1024, format: nil, block: tap)
        // A device change (e.g. that headset switch) stops the engine; rewire the
        // tap to the new format and restart so the recording carries on.
        Dictation.observer = NotificationCenter.default.addObserver(
            forName: .AVAudioEngineConfigurationChange, object: engine, queue: nil
        ) { _ in
            Dictation.configChanged = true
            Dictation.lock.lock()
            let active = Dictation.engine === engine
            Dictation.lock.unlock()
            guard active else { return }
            input.removeTap(onBus: 0)
            input.installTap(onBus: 0, bufferSize: 1024, format: nil, block: tap)
            engine.prepare()
            do {
                try engine.start()
                NSLog("[AiBuddy] dictation: audio device changed — restarted with %@", input.outputFormat(forBus: 0).description)
            } catch {
                NSLog("[AiBuddy] dictation: audio device changed — restart failed: %@", error.localizedDescription)
            }
        }
        // Register the session before starting: the headset's device change
        // arrives ~50 ms after start and must find this engine as the active one.
        Dictation.lock.lock()
        Dictation.engine = engine
        Dictation.lane = lane
        Dictation.lock.unlock()
        do {
            engine.prepare()
            try engine.start()
        } catch {
            NSLog("[AiBuddy] dictation: mic start failed: %@", error.localizedDescription)
            Dictation.lock.lock()
            Dictation.engine = nil
            Dictation.lane = nil
            Dictation.lock.unlock()
            if let observer = Dictation.observer {
                NotificationCenter.default.removeObserver(observer)
                Dictation.observer = nil
            }
            input.removeTap(onBus: 0)
            lane.stop()
            return Int32(-4)
        }
        NSLog("[AiBuddy] dictation: mic started — %@", input.outputFormat(forBus: 0).description)
        return Int32(0)
    }
}

/// Stops the mic, waits (≤4 s) for the final words, then calls `done(ctx)`.
/// Also prepares the next spare lane so the next hold starts instantly.
@_cdecl("aibuddy_dictation_stop")
public func aibuddy_dictation_stop(
    _ done: @convention(c) (UnsafeMutableRawPointer?) -> Void,
    _ ctx: UnsafeMutableRawPointer?,
    _ cb: AiBuddySpeechCallbackSwift
) {
    guard #available(macOS 26.0, *) else {
        done(ctx)
        return
    }
    Dictation.lock.lock()
    let engine = Dictation.engine
    let lane = Dictation.lane
    Dictation.engine = nil
    Dictation.lane = nil
    Dictation.lock.unlock()

    NSLog("[AiBuddy] dictation: stopping — %d buffers, peak level %.3f, engine running=%d, config changed=%d",
          Dictation.buffers, Dictation.peak, (engine?.isRunning ?? false) ? 1 : 0, Dictation.configChanged ? 1 : 0)
    if let observer = Dictation.observer {
        NotificationCenter.default.removeObserver(observer)
        Dictation.observer = nil
    }
    engine?.inputNode.removeTap(onBus: 0)
    engine?.stop()
    nonisolated(unsafe) let uctx = ctx
    nonisolated(unsafe) let udone = done
    Task.detached {
        if let lane, await !lane.finish(timeout: 4.0) {
            NSLog("[AiBuddy] dictation: finish timed out after 4 s — sending what was heard so far")
        }
        udone(uctx)
        await Dictation.makeSpare(cb: cb, ctx: nil)
    }
}
