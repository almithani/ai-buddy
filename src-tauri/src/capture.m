// On-device transcription: system audio via ScreenCaptureKit + microphone via
// AVAudioEngine, each feeding its own SFSpeechRecognizer "lane".
// Exposes a plain C interface so Rust can call it via extern "C".

#import <ScreenCaptureKit/ScreenCaptureKit.h>
#import <CoreMedia/CoreMedia.h>
#import <AVFoundation/AVFoundation.h>
#import <Speech/Speech.h>
#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>

// source: 0 = mic ("me"), 1 = system audio ("them"), -1 = session-level error (text = message)
typedef void (*AiBuddySpeechCallback)(int32_t source, const char* text, bool is_final, void* ctx);

// ── Speech lane: one recognizer per audio source ───────────────────────────

API_AVAILABLE(macos(13.0))
@interface AiBuddySpeechLane : NSObject
- (instancetype)initWithSource:(int32_t)source
                      callback:(AiBuddySpeechCallback)cb
                       context:(void*)ctx;
- (BOOL)start;
- (void)appendPCMBuffer:(AVAudioPCMBuffer*)buf;
- (void)appendSampleBuffer:(CMSampleBufferRef)buf;
- (void)stop;
@end

// Atomic so audio-thread appends can read it while _q rotates it.
API_AVAILABLE(macos(13.0))
@interface AiBuddySpeechLane ()
@property (atomic, strong) SFSpeechAudioBufferRecognitionRequest* request;
@end

API_AVAILABLE(macos(13.0))
@implementation AiBuddySpeechLane {
    int32_t                  _source;
    AiBuddySpeechCallback    _cb;
    void*                    _ctx;
    SFSpeechRecognizer*      _recognizer;
    SFSpeechRecognitionTask* _task;
    SFSpeechRecognitionTask* _suppressFinalFrom; // task whose partial we already flushed
    NSString*                _lastPartial;
    dispatch_queue_t         _q;
    dispatch_source_t        _rotateTimer;
    dispatch_source_t        _gateTimer;
    volatile BOOL            _active;
    volatile NSTimeInterval  _lastEnergyTime;   // written from audio threads, read on _q
    int                      _consecutiveErrors;
    NSTimeInterval           _firstErrorTime;
}

static const NSTimeInterval kRotateInterval  = 50.0;  // restart request mid-monologue before Apple's ~1 min guidance
static const float          kGateThreshold   = 0.008; // RMS above this counts as sound on the lane
static const NSTimeInterval kGateHold        = 2.0;   // keep listening this long after the last sound

- (instancetype)initWithSource:(int32_t)source
                      callback:(AiBuddySpeechCallback)cb
                       context:(void*)ctx {
    if ((self = [super init])) {
        _source = source;
        _cb     = cb;
        _ctx    = ctx;
        _q = dispatch_queue_create("com.aibuddy.speechlane", DISPATCH_QUEUE_SERIAL);
    }
    return self;
}

- (BOOL)start {
    _recognizer = [[SFSpeechRecognizer alloc] initWithLocale:[NSLocale currentLocale]];
    if (!_recognizer || !_recognizer.supportsOnDeviceRecognition) {
        _recognizer = [[SFSpeechRecognizer alloc]
            initWithLocale:[NSLocale localeWithLocaleIdentifier:@"en_US"]];
    }
    if (!_recognizer || !_recognizer.supportsOnDeviceRecognition) {
        return NO;
    }
    _recognizer.defaultTaskHint = SFSpeechRecognitionTaskHintDictation;
    _active = YES;
    // No task yet — recognition starts when the energy gate opens (sound on the lane).
    return YES;
}

// ── Energy gate ─────────────────────────────────────────────────────────────
// A lane only runs a recognition task while there is sound on it. A silent
// lane (e.g. system audio with no meeting playing) runs no task at all,
// avoiding the endless "no speech detected" → restart churn that destabilises
// the shared recognition service for both lanes.

// Called on the audio threads.
- (void)_noteEnergy:(float)rms {
    if (rms < kGateThreshold) return;
    _lastEnergyTime = [NSDate timeIntervalSinceReferenceDate];
    if (self.request == nil) {
        dispatch_async(_q, ^{ [self _gateOpen]; });
    }
}

// Must be called on _q.
- (void)_gateOpen {
    if (!_active || self.request != nil) return;
    NSLog(@"[AiBuddy] Lane %d: gate open — starting recognition", _source);
    [self _beginTask];
    [self _armRotateTimer];
    [self _armGateTimer];
}

// Must be called on _q. Deliver the accumulated partial as a final NOW —
// the recognizer's own post-endAudio final is unreliable (can arrive empty
// or truncated), so we flush deterministically and suppress the real one.
- (void)_flushPartialAsFinal {
    if (_lastPartial.length > 0) {
        [self _deliverText:_lastPartial final:YES];
        _lastPartial = nil;
    }
}

// Must be called on _q.
- (void)_gateClose {
    if (self.request == nil) return;
    NSLog(@"[AiBuddy] Lane %d: gate closed — ending recognition", _source);
    SFSpeechAudioBufferRecognitionRequest* req = self.request;
    self.request = nil;            // appends stop; idle until next sound
    _suppressFinalFrom = _task;
    [self _flushPartialAsFinal];
    [req endAudio];
    [self _cancelTimers];
}

- (void)_armGateTimer {
    if (_gateTimer) { dispatch_source_cancel(_gateTimer); _gateTimer = nil; }
    if (!_active) return;
    _gateTimer = dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER, 0, 0, _q);
    dispatch_source_set_timer(_gateTimer,
        dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.5 * NSEC_PER_SEC)),
        (uint64_t)(0.5 * NSEC_PER_SEC), (int64_t)(0.1 * NSEC_PER_SEC));
    __weak AiBuddySpeechLane* weakSelf = self;
    dispatch_source_set_event_handler(_gateTimer, ^{
        AiBuddySpeechLane* self = weakSelf;
        if (!self) return;
        NSTimeInterval now = [NSDate timeIntervalSinceReferenceDate];
        if (now - self->_lastEnergyTime > kGateHold) {
            [self _gateClose];
        }
    });
    dispatch_resume(_gateTimer);
}

// Must be called on _q.
- (void)_cancelTimers {
    if (_rotateTimer) { dispatch_source_cancel(_rotateTimer); _rotateTimer = nil; }
    if (_gateTimer)   { dispatch_source_cancel(_gateTimer);   _gateTimer   = nil; }
}

// Must be called on _q.
- (void)_beginTask {
    if (!_active) return;

    SFSpeechAudioBufferRecognitionRequest* req =
        [[SFSpeechAudioBufferRecognitionRequest alloc] init];
    req.shouldReportPartialResults = YES;
    req.requiresOnDeviceRecognition = YES;
    req.addsPunctuation = YES;
    self.request = req;

    __block SFSpeechRecognitionTask* thisTask = nil;
    __weak AiBuddySpeechLane* weakSelf = self;
    thisTask = [_recognizer
        recognitionTaskWithRequest:req
                     resultHandler:^(SFSpeechRecognitionResult* result, NSError* error) {
            AiBuddySpeechLane* self = weakSelf;
            if (!self) return;
            dispatch_async(self->_q, ^{
                // Finals from a request we already flushed ourselves
                // (gate close / rotation / stop) would be duplicates — drop them.
                BOOL suppressed = (thisTask == self->_suppressFinalFrom);
                if (suppressed && result && result.isFinal) {
                    self->_suppressFinalFrom = nil;
                }

                // Callbacks from a task that has been rotated out.
                if (thisTask != self->_task) {
                    if (result && result.isFinal && !suppressed) {
                        [self _deliverText:result.bestTranscription.formattedString final:YES];
                    }
                    return;
                }
                if (result) {
                    NSString* text = result.bestTranscription.formattedString;
                    if (result.isFinal) {
                        if (!suppressed) {
                            [self _deliverText:text final:YES];
                        }
                        self->_lastPartial = nil;
                        self->_consecutiveErrors = 0;
                        if (self.request != nil) {
                            // Gate still open — keep listening with a fresh request.
                            [self _beginTask];
                            [self _armRotateTimer];
                        }
                    } else if (!suppressed) {
                        self->_lastPartial = text;
                        [self _deliverText:text final:NO];
                    }
                } else if (error && self->_active) {
                    [self _handleError:error];
                }
            });
        }];
    _task = thisTask;
}

// Must be called on _q.
- (void)_deliverText:(NSString*)text final:(BOOL)final {
    if (!_cb) return;
    NSString* trimmed = [text stringByTrimmingCharactersInSet:
        [NSCharacterSet whitespaceAndNewlineCharacterSet]];
    if (trimmed.length == 0) return;
    _cb(_source, trimmed.UTF8String, final, _ctx);
}

// Must be called on _q.
- (void)_handleError:(NSError*)error {
    // Don't lose in-flight text: promote the last partial to a final.
    if (_lastPartial.length > 0) {
        [self _deliverText:_lastPartial final:YES];
        _lastPartial = nil;
    }

    NSLog(@"[AiBuddy] Lane %d error: %@ code=%ld (%@)",
          _source, error.domain, (long)error.code, error.localizedDescription);

    // Benign conditions, not failures: 1110 = no speech detected, 203 = retry,
    // 216/301 = request cancelled. Go idle; the energy gate restarts
    // recognition when there is sound again.
    if ([error.domain isEqualToString:@"kAFAssistantErrorDomain"] &&
        (error.code == 1110 || error.code == 203 || error.code == 216 || error.code == 301)) {
        self.request = nil;
        [self _cancelTimers];
        return;
    }

    NSTimeInterval now = [NSDate timeIntervalSinceReferenceDate];
    if (_consecutiveErrors == 0 || now - _firstErrorTime > 10.0) {
        _firstErrorTime = now;
        _consecutiveErrors = 0;
    }
    _consecutiveErrors++;

    if (_consecutiveErrors > 5) {
        NSLog(@"[AiBuddy] Speech lane %d giving up: %@", _source, error.localizedDescription);
        _active = NO;
        self.request = nil;
        [self _cancelTimers];
        if (_cb) {
            NSString* msg = [NSString stringWithFormat:
                @"Speech recognition failed repeatedly: %@", error.localizedDescription];
            _cb(-1, msg.UTF8String, true, _ctx);
        }
        return;
    }

    // Real error but not fatal yet — go idle and let the gate retry on sound.
    self.request = nil;
    [self _cancelTimers];
}

- (void)_armRotateTimer {
    if (_rotateTimer) {
        dispatch_source_cancel(_rotateTimer);
        _rotateTimer = nil;
    }
    if (!_active) return;
    _rotateTimer = dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER, 0, 0, _q);
    dispatch_source_set_timer(_rotateTimer,
        dispatch_time(DISPATCH_TIME_NOW, (int64_t)(kRotateInterval * NSEC_PER_SEC)),
        DISPATCH_TIME_FOREVER, (int64_t)(1 * NSEC_PER_SEC));
    __weak AiBuddySpeechLane* weakSelf = self;
    dispatch_source_set_event_handler(_rotateTimer, ^{ [weakSelf _rotate]; });
    dispatch_resume(_rotateTimer);
}

// Timer handler, runs on _q. Swap in a fresh request so long monologues don't
// hit Apple's per-request limits; the old task flushes its final through the
// normal result handler.
- (void)_rotate {
    if (!_active || self.request == nil) return;
    SFSpeechAudioBufferRecognitionRequest* oldReq = self.request;
    _suppressFinalFrom = _task;
    [self _flushPartialAsFinal];
    [self _beginTask];          // swaps request and _task; appends now land in the new request
    [oldReq endAudio];
    [self _armRotateTimer];
}

// Called synchronously on the audio threads. appendAudio* copy the data
// internally and are thread-safe; appending in-place avoids holding tap
// buffers past their block scope (AVAudioEngine recycles that memory).
- (void)appendPCMBuffer:(AVAudioPCMBuffer*)buf {
    if (!_active) return;
    if (buf.floatChannelData && buf.frameLength > 0) {
        const float* s = buf.floatChannelData[0];
        float sum = 0;
        for (AVAudioFrameCount i = 0; i < buf.frameLength; i++) sum += s[i] * s[i];
        [self _noteEnergy:sqrtf(sum / buf.frameLength)];
    }
    [self.request appendAudioPCMBuffer:buf];
}

- (void)appendSampleBuffer:(CMSampleBufferRef)buf {
    if (!_active || !CMSampleBufferDataIsReady(buf)) return;
    CMBlockBufferRef block = CMSampleBufferGetDataBuffer(buf);
    if (block) {
        size_t len = 0;
        char* p = NULL;
        // SCK delivers float32 LPCM per our stream configuration.
        if (CMBlockBufferGetDataPointer(block, 0, NULL, &len, &p) == 0 && p && len >= sizeof(float)) {
            const float* s = (const float*)p;
            size_t n = len / sizeof(float);
            float sum = 0;
            for (size_t i = 0; i < n; i++) sum += s[i] * s[i];
            [self _noteEnergy:sqrtf(sum / n)];
        }
    }
    [self.request appendAudioSampleBuffer:buf];
}

- (void)stop {
    _active = NO;
    dispatch_async(_q, ^{
        [self _cancelTimers];
        SFSpeechAudioBufferRecognitionRequest* req = self.request;
        self.request = nil;
        self->_suppressFinalFrom = self->_task;
        [self _flushPartialAsFinal];
        [req endAudio];
        // The result handler only holds a WEAK reference to the lane; capture
        // self strongly here so the lane survives long enough to clean up the
        // in-flight task, then cancel it.
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(3 * NSEC_PER_SEC)),
                       self->_q, ^{ [self->_task cancel]; });
    });
}

@end

// ── ScreenCaptureKit session (system audio output → "them" lane) ───────────

API_AVAILABLE(macos(13.0))
@interface AiBuddyCaptureSession : NSObject <SCStreamOutput, SCStreamDelegate>
- (void)startWithLane:(AiBuddySpeechLane*)lane;
- (void)stop;
@end

API_AVAILABLE(macos(13.0))
@implementation AiBuddyCaptureSession {
    AiBuddySpeechLane* _lane;
    SCStream*          _stream;
    volatile BOOL      _active;
}

- (void)startWithLane:(AiBuddySpeechLane*)lane {
    _lane   = lane;
    _active = YES;

    [SCShareableContent
        getShareableContentExcludingDesktopWindows:NO
        onScreenWindowsOnly:NO
        completionHandler:^(SCShareableContent* _Nullable content, NSError* _Nullable err) {
            if (err || !content || content.displays.count == 0) {
                self->_active = NO;
                return;
            }

            SCContentFilter* filter = [[SCContentFilter alloc]
                initWithDisplay:content.displays[0]
                excludingApplications:@[]
                exceptingWindows:@[]];

            SCStreamConfiguration* cfg = [[SCStreamConfiguration alloc] init];
            cfg.capturesAudio = YES;
            cfg.sampleRate    = 48000;
            cfg.channelCount  = 1;
            // Minimise video overhead — we discard screen frames entirely.
            cfg.width  = 2;
            cfg.height = 2;

            self->_stream = [[SCStream alloc]
                initWithFilter:filter
                configuration:cfg
                delegate:self];

            NSError* addErr = nil;
            [self->_stream
                addStreamOutput:self
                type:SCStreamOutputTypeAudio
                sampleHandlerQueue:dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0)
                error:&addErr];

            if (addErr) { self->_active = NO; return; }

            [self->_stream startCaptureWithCompletionHandler:^(NSError* startErr) {
                if (startErr) { self->_active = NO; }
            }];
        }];
}

- (void)stop {
    _active = NO;
    SCStream* s = _stream;
    _stream = nil;
    if (s) {
        [s stopCaptureWithCompletionHandler:^(NSError* __unused e) {}];
    }
}

// SCStreamOutput
- (void)stream:(SCStream*)stream
    didOutputSampleBuffer:(CMSampleBufferRef)buf
                   ofType:(SCStreamOutputType)type {
    if (type != SCStreamOutputTypeAudio || !_active) return;
    [_lane appendSampleBuffer:buf];
}

// SCStreamDelegate
- (void)stream:(SCStream*)stream didStopWithError:(NSError*)error {
    _active = NO;
}

@end

// ── AVAudioEngine session (microphone input → "me" lane) ───────────────────

API_AVAILABLE(macos(13.0))
@interface AiBuddyMicSession : NSObject
- (void)startWithLane:(AiBuddySpeechLane*)lane;
- (void)stop;
@end

API_AVAILABLE(macos(13.0))
@implementation AiBuddyMicSession {
    AiBuddySpeechLane* _lane;
    AVAudioEngine*     _engine;
    id                 _configObserver;
    volatile BOOL      _active;
}

- (void)startWithLane:(AiBuddySpeechLane*)lane {
    _lane   = lane;
    _active = YES;
    // AVAudioEngine must be set up and started on the main thread.
    dispatch_async(dispatch_get_main_queue(), ^{ [self _setupEngine]; });
}

// Must run on the main thread.
- (void)_setupEngine {
    if (!_active) return;

    [self _teardownEngine];

    _engine = [[AVAudioEngine alloc] init];

    // Access inputNode first — AVAudioEngine nodes are lazy; prepare requires at least one.
    AVAudioInputNode* inputNode = _engine.inputNode;
    [_engine prepare];

    AVAudioFormat* hwFmt = [inputNode outputFormatForBus:0];
    if (!hwFmt || hwFmt.sampleRate == 0) {
        NSLog(@"[AiBuddy] Mic: could not get hardware audio format — mic capture disabled");
        _active = NO;
        return;
    }
    NSLog(@"[AiBuddy] Mic: tap installed, hw format = %.0f Hz, %u ch",
          hwFmt.sampleRate, (unsigned)hwFmt.channelCount);

    [inputNode
        installTapOnBus:0
        bufferSize:4096
        format:hwFmt
        block:^(AVAudioPCMBuffer* inBuf, AVAudioTime* __unused when) {
            if (!self->_active) return;

            // Diagnostic: log input level every ~2 s so silent-mic issues
            // (e.g. another app's voice processing muting us) are visible.
            static NSTimeInterval nextLog = 0;
            NSTimeInterval now = [NSDate timeIntervalSinceReferenceDate];
            if (now >= nextLog && inBuf.floatChannelData && inBuf.frameLength > 0) {
                nextLog = now + 2.0;
                const float* s = inBuf.floatChannelData[0];
                float sum = 0;
                for (AVAudioFrameCount i = 0; i < inBuf.frameLength; i++) sum += s[i] * s[i];
                NSLog(@"[AiBuddy] Mic RMS = %.5f", sqrtf(sum / inBuf.frameLength));
            }

            [self->_lane appendPCMBuffer:inBuf];
        }];

    // Voice-chat apps (Discord/Zoom) toggling echo cancellation change the
    // input device configuration, which silently kills a running tap — rebuild.
    __weak AiBuddyMicSession* weakSelf = self;
    _configObserver = [[NSNotificationCenter defaultCenter]
        addObserverForName:AVAudioEngineConfigurationChangeNotification
                    object:_engine
                     queue:[NSOperationQueue mainQueue]
                usingBlock:^(NSNotification* __unused note) {
            AiBuddyMicSession* self = weakSelf;
            if (!self || !self->_active) return;
            NSLog(@"[AiBuddy] Mic: audio configuration changed — rebuilding engine");
            [self _setupEngine];
        }];

    NSError* startErr = nil;
    [_engine startAndReturnError:&startErr];
    if (startErr) {
        NSLog(@"[AiBuddy] Mic: AVAudioEngine start failed: %@", startErr.localizedDescription);
        _active = NO;
    }
}

// Must run on the main thread.
- (void)_teardownEngine {
    if (_configObserver) {
        [[NSNotificationCenter defaultCenter] removeObserver:_configObserver];
        _configObserver = nil;
    }
    if (_engine) {
        [_engine.inputNode removeTapOnBus:0];
        [_engine stop];
        _engine = nil;
    }
}

- (void)stop {
    _active = NO;
    // Engine teardown must also happen on the main thread (same as setup).
    dispatch_async(dispatch_get_main_queue(), ^{ [self _teardownEngine]; });
}

@end

// ── Global sessions (one of each at a time) ────────────────────────────────

static id gSCSession  = nil;
static id gMicSession = nil;
static id gMicLane    = nil;
static id gSystemLane = nil;

static void aibuddy_teardown(void) API_AVAILABLE(macos(13.0)) {
    if (gSCSession)  { [(AiBuddyCaptureSession*)gSCSession stop];  gSCSession  = nil; }
    if (gMicSession) { [(AiBuddyMicSession*)gMicSession stop];     gMicSession = nil; }
    if (gMicLane)    { [(AiBuddySpeechLane*)gMicLane stop];        gMicLane    = nil; }
    if (gSystemLane) { [(AiBuddySpeechLane*)gSystemLane stop];     gSystemLane = nil; }
}

// Raw SFSpeechRecognizerAuthorizationStatus: 0 notDetermined, 1 denied, 2 restricted, 3 authorized
int32_t aibuddy_speech_auth_status(void) {
    return (int32_t)[SFSpeechRecognizer authorizationStatus];
}

void aibuddy_speech_request_auth(void (*cb)(int32_t status, void* ctx), void* ctx) {
    [SFSpeechRecognizer requestAuthorization:^(SFSpeechRecognizerAuthorizationStatus status) {
        cb((int32_t)status, ctx);
    }];
}

// 0 = started; -1 = macOS < 13; -2 = not authorized; -3 = on-device recognition unavailable
int32_t aibuddy_speech_start(AiBuddySpeechCallback cb, void* ctx) {
    if (@available(macOS 13.0, *)) {
        if ([SFSpeechRecognizer authorizationStatus] != SFSpeechRecognizerAuthorizationStatusAuthorized) {
            return -2;
        }
        aibuddy_teardown();

        AiBuddySpeechLane* micLane = [[AiBuddySpeechLane alloc]
            initWithSource:0 callback:cb context:ctx];
        AiBuddySpeechLane* sysLane = [[AiBuddySpeechLane alloc]
            initWithSource:1 callback:cb context:ctx];

        if (![micLane start] || ![sysLane start]) {
            [micLane stop];
            [sysLane stop];
            return -3;
        }
        gMicLane    = micLane;
        gSystemLane = sysLane;

        gSCSession = [[AiBuddyCaptureSession alloc] init];
        [(AiBuddyCaptureSession*)gSCSession startWithLane:sysLane];

        gMicSession = [[AiBuddyMicSession alloc] init];
        [(AiBuddyMicSession*)gMicSession startWithLane:micLane];

        return 0;
    }
    return -1;
}

void aibuddy_speech_stop(void) {
    if (@available(macOS 13.0, *)) {
        aibuddy_teardown();
    }
}
