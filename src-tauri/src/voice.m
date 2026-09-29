// Reads AI Buddy's replies aloud with the Mac's default system voice
// (the voice chosen in Accessibility → Read & Speak).
#import <AVFoundation/AVFoundation.h>
#import <AppKit/AppKit.h>

static AVSpeechSynthesizer *gSynth;

void aibuddy_speak(const char *text) {
    NSString *s = text ? @(text) : @"";
    dispatch_async(dispatch_get_main_queue(), ^{
        if (!gSynth) gSynth = [AVSpeechSynthesizer new];
        [gSynth stopSpeakingAtBoundary:AVSpeechBoundaryImmediate];
        if (s.length) [gSynth speakUtterance:[AVSpeechUtterance speechUtteranceWithString:s]];
    });
}

void aibuddy_stop_speaking(void) {
    dispatch_async(dispatch_get_main_queue(), ^{
        [gSynth stopSpeakingAtBoundary:AVSpeechBoundaryImmediate];
    });
}

/// Short "talk now" sound when hold-to-talk starts listening, so people know
/// the mic is live before they speak.
void aibuddy_play_listen_cue(void) {
    dispatch_async(dispatch_get_main_queue(), ^{
        [[NSSound soundNamed:@"Pop"] play];
    });
}
