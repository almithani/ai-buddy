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

    _engine = [[AVAudioEngine alloc] init];
    AVAudioInputNode* inputNode = _engine.inputNode;

    // Hardware's native format (typically float32 at 44100 or 48000 Hz, stereo).
    AVAudioFormat* hwFmt = [inputNode outputFormatForBus:0];

    // Whisper wants float32, 16 kHz, mono.
    AVAudioFormat* targetFmt = [[AVAudioFormat alloc]
        initWithCommonFormat:AVAudioPCMFormatFloat32
        sampleRate:16000
        channels:1
        interleaved:NO];

    _converter = [[AVAudioConverter alloc] initFromFormat:hwFmt toFormat:targetFmt];

    [inputNode
        installTapOnBus:0
        bufferSize:4096
        format:hwFmt
        block:^(AVAudioPCMBuffer* inBuf, AVAudioTime* __unused when) {
            if (!self->_active || !self->_cb || !self->_converter) return;

            // Calculate output frame capacity with a small margin.
            double ratio = targetFmt.sampleRate / hwFmt.sampleRate;
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
        _active = NO;
    }
}

- (void)stop {
    _active = NO;
    if (_engine) {
        [_engine.inputNode removeTapOnBus:0];
        [_engine stop];
        _engine = nil;
        _converter = nil;
    }
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
