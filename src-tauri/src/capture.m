// Audio capture: system audio via ScreenCaptureKit + microphone via AVAudioEngine.
// Both fire the same AiBuddyAudioCallback so Rust sees a single interleaved stream.
// Exposes a plain C interface so Rust can call it via extern "C".

#import <ScreenCaptureKit/ScreenCaptureKit.h>
#import <CoreMedia/CoreMedia.h>
#import <AVFoundation/AVFoundation.h>
#include <stddef.h>

typedef void (*AiBuddyAudioCallback)(const float* samples, size_t count, void* ctx);

// ── ScreenCaptureKit session (system audio output) ─────────────────────────

API_AVAILABLE(macos(13.0))
@interface AiBuddyCaptureSession : NSObject <SCStreamOutput, SCStreamDelegate>
- (void)startWithCallback:(AiBuddyAudioCallback)cb context:(void*)ctx;
- (void)stop;
@end

API_AVAILABLE(macos(13.0))
@implementation AiBuddyCaptureSession {
    AiBuddyAudioCallback _cb;
    void*                _ctx;
    SCStream*            _stream;
    volatile BOOL        _active;
}

- (void)startWithCallback:(AiBuddyAudioCallback)cb context:(void*)ctx {
    _cb     = cb;
    _ctx    = ctx;
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
            cfg.sampleRate    = 16000;
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
    if (type != SCStreamOutputTypeAudio || !_active || !_cb) return;

    CMBlockBufferRef block = CMSampleBufferGetDataBuffer(buf);
    if (!block) return;

    size_t totalLen  = 0;
    char*  dataPtr   = NULL;
    OSStatus st = CMBlockBufferGetDataPointer(block, 0, NULL, &totalLen, &dataPtr);
    if (st != 0 || !dataPtr || totalLen < sizeof(float)) return;

    size_t count = totalLen / sizeof(float);
    _cb((const float*)dataPtr, count, _ctx);
}

// SCStreamDelegate
- (void)stream:(SCStream*)stream didStopWithError:(NSError*)error {
    _active = NO;
}

@end

// ── AVAudioEngine session (microphone input) ───────────────────────────────

@interface AiBuddyMicSession : NSObject
- (void)startWithCallback:(AiBuddyAudioCallback)cb context:(void*)ctx;
- (void)stop;
@end

@implementation AiBuddyMicSession {
    AiBuddyAudioCallback _cb;
    void*                _ctx;
    AVAudioEngine*       _engine;
    AVAudioConverter*    _converter;
    volatile BOOL        _active;
}

- (void)startWithCallback:(AiBuddyAudioCallback)cb context:(void*)ctx {
    _cb     = cb;
    _ctx    = ctx;
    _active = YES;
    // AVAudioEngine must be set up and started on the main thread.
    dispatch_async(dispatch_get_main_queue(), ^{ [self _setupEngine]; });
}

- (void)_setupEngine {
    if (!_active) return;

    _engine = [[AVAudioEngine alloc] init];

    // Access inputNode first — AVAudioEngine nodes are lazy; prepare requires at least one.
    AVAudioInputNode* inputNode = _engine.inputNode;

    // Now prepare: the engine's graph has inputNode and can initialize correctly.
    [_engine prepare];
    AVAudioFormat* hwFmt = [inputNode outputFormatForBus:0];

    if (!hwFmt || hwFmt.sampleRate == 0) {
        NSLog(@"[AiBuddy] Mic: could not get hardware audio format — mic capture disabled");
        _active = NO;
        return;
    }
    NSLog(@"[AiBuddy] Mic: hardware format = %.0f Hz, %u ch", hwFmt.sampleRate, (unsigned)hwFmt.channelCount);

    // Whisper wants float32, 16 kHz, mono.
    AVAudioFormat* targetFmt = [[AVAudioFormat alloc]
        initWithCommonFormat:AVAudioPCMFormatFloat32
        sampleRate:16000
        channels:1
        interleaved:NO];

    _converter = [[AVAudioConverter alloc] initFromFormat:hwFmt toFormat:targetFmt];
    if (!_converter) {
        NSLog(@"[AiBuddy] Mic: AVAudioConverter init failed — mic capture disabled");
        _active = NO;
        return;
    }

    double ratio = targetFmt.sampleRate / hwFmt.sampleRate;

    [inputNode
        installTapOnBus:0
        bufferSize:4096
        format:hwFmt
        block:^(AVAudioPCMBuffer* inBuf, AVAudioTime* __unused when) {
            if (!self->_active || !self->_cb) return;

            // Log RMS every ~2 s so we can verify audio is arriving and at what level.
            static NSTimeInterval nextLog = 0;
            NSTimeInterval now = [NSDate timeIntervalSinceReferenceDate];
            if (now >= nextLog && inBuf.floatChannelData && inBuf.frameLength > 0) {
                nextLog = now + 2.0;
                const float* s = inBuf.floatChannelData[0];
                float sum = 0;
                for (AVAudioFrameCount i = 0; i < inBuf.frameLength; i++) sum += s[i] * s[i];
                NSLog(@"[AiBuddy] Mic RMS = %.5f (frames=%u)", sqrtf(sum / inBuf.frameLength), inBuf.frameLength);
            }

            AVAudioFrameCount outCap = (AVAudioFrameCount)(inBuf.frameLength * ratio) + 2;
            AVAudioPCMBuffer* outBuf = [[AVAudioPCMBuffer alloc]
                initWithPCMFormat:targetFmt
                frameCapacity:outCap];
            if (!outBuf) return;

            __block BOOL consumed = NO;
            AVAudioConverterInputBlock inputBlock =
                ^AVAudioBuffer*(AVAudioPacketCount __unused n,
                                AVAudioConverterInputStatus* status) {
                    if (consumed) {
                        *status = AVAudioConverterInputStatus_NoDataNow;
                        return nil;
                    }
                    consumed = YES;
                    *status = AVAudioConverterInputStatus_HaveData;
                    return inBuf;
                };

            NSError* err = nil;
            [self->_converter convertToBuffer:outBuf
                                        error:&err
                           withInputFromBlock:inputBlock];

            if (!err && outBuf.frameLength > 0 && outBuf.floatChannelData) {
                self->_cb(outBuf.floatChannelData[0], outBuf.frameLength, self->_ctx);
            }
        }];

    NSError* startErr = nil;
    [_engine startAndReturnError:&startErr];
    if (startErr) {
        NSLog(@"[AiBuddy] Mic: AVAudioEngine start failed: %@", startErr.localizedDescription);
        _active = NO;
    } else {
        NSLog(@"[AiBuddy] Mic: started successfully");
    }
}

- (void)stop {
    _active = NO;
    // Engine teardown must also happen on the main thread (same as setup).
    dispatch_async(dispatch_get_main_queue(), ^{
        if (self->_engine) {
            [self->_engine.inputNode removeTapOnBus:0];
            [self->_engine stop];
            self->_engine = nil;
            self->_converter = nil;
        }
    });
}

@end

// ── Global sessions (one of each at a time) ────────────────────────────────

static id gSCSession  = nil;
static id gMicSession = nil;

// Returns 1 if capture started (macOS 13.0+), 0 if unavailable.
int aibuddy_start_capture(AiBuddyAudioCallback cb, void* ctx) {
    if (@available(macOS 13.0, *)) {
        // Stop any existing sessions first.
        if (gSCSession)  { [(AiBuddyCaptureSession*)gSCSession stop]; }
        if (gMicSession) { [(AiBuddyMicSession*)gMicSession stop]; }

        gSCSession = [[AiBuddyCaptureSession alloc] init];
        [(AiBuddyCaptureSession*)gSCSession startWithCallback:cb context:ctx];

        gMicSession = [[AiBuddyMicSession alloc] init];
        [(AiBuddyMicSession*)gMicSession startWithCallback:cb context:ctx];

        return 1;
    }
    return 0;
}

void aibuddy_stop_capture(void) {
    if (@available(macOS 13.0, *)) {
        if (gSCSession)  { [(AiBuddyCaptureSession*)gSCSession stop];  gSCSession  = nil; }
        if (gMicSession) { [(AiBuddyMicSession*)gMicSession stop];     gMicSession = nil; }
    }
}
