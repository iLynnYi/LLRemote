#import <Foundation/Foundation.h>
#import <ScreenCaptureKit/ScreenCaptureKit.h>
#import <CoreVideo/CoreVideo.h>
#include <stdint.h>
#include <string.h>

// C boundary: 0 = frame, 1 = pending, 2 = unavailable, 3 = permission, 4 = invalid.
@interface VncScreenCapture : NSObject <SCStreamOutput, SCStreamDelegate> {
    NSLock *_lock;
    dispatch_group_t _starting;
    dispatch_queue_t _outputQueue;
    SCStream *_stream;
    CVPixelBufferRef _latest;
    uint64_t _sequence;
    uint32_t _width, _height;
    int _status;
    BOOL _stopped;
}
- (instancetype)initWithDisplay:(uint32_t)display width:(uint32_t)width height:(uint32_t)height interval:(uint32_t)interval;
- (int)copyFrame:(uint8_t *)bytes capacity:(size_t)capacity sequence:(uint64_t *)sequence;
- (void)stop;
@end

@implementation VncScreenCapture
- (void)recordError:(NSError *)error {
    [_lock lock];
    if (!_stopped) {
        _status = [error.domain isEqualToString:SCStreamErrorDomain] && error.code == SCStreamErrorUserDeclined ? 3 : 2;
    }
    [_lock unlock];
}

- (instancetype)initWithDisplay:(uint32_t)displayID width:(uint32_t)width height:(uint32_t)height interval:(uint32_t)interval {
    self = [super init];
    if (!self) return nil;
    _lock = [NSLock new];
    _starting = dispatch_group_create();
    _outputQueue = dispatch_queue_create("vnc.screen.capture", DISPATCH_QUEUE_SERIAL);
    _width = width;
    _height = height;
    _status = 1;
    dispatch_group_enter(_starting);
    [SCShareableContent getShareableContentExcludingDesktopWindows:NO onScreenWindowsOnly:YES completionHandler:^(SCShareableContent *content, NSError *error) {
        @autoreleasepool {
            if (error) {
                [self recordError:error];
                dispatch_group_leave(self->_starting);
                return;
            }
            [self->_lock lock];
            BOOL stopped = self->_stopped;
            [self->_lock unlock];
            SCDisplay *selected = nil;
            for (SCDisplay *display in content.displays) {
                if (display.displayID == displayID) { selected = display; break; }
            }
            if (stopped || !selected) {
                if (!stopped) [self recordError:nil];
                dispatch_group_leave(self->_starting);
                return;
            }
            SCStreamConfiguration *configuration = [SCStreamConfiguration new];
            configuration.width = width;
            configuration.height = height;
            configuration.scalesToFit = YES;
            configuration.preservesAspectRatio = NO;
            configuration.destinationRect = CGRectMake(0, 0, width, height);
            configuration.pixelFormat = kCVPixelFormatType_32BGRA;
            configuration.colorSpaceName = kCGColorSpaceSRGB;
            configuration.minimumFrameInterval = CMTimeMake(interval, 1000);
            configuration.queueDepth = 3;
            configuration.showsCursor = YES;
            SCContentFilter *filter = [[SCContentFilter alloc] initWithDisplay:selected excludingWindows:@[]];
            self->_stream = [[SCStream alloc] initWithFilter:filter configuration:configuration delegate:self];
            NSError *outputError = nil;
            if (![self->_stream addStreamOutput:self type:SCStreamOutputTypeScreen sampleHandlerQueue:self->_outputQueue error:&outputError]) {
                [self recordError:outputError];
                dispatch_group_leave(self->_starting);
                return;
            }
            [self->_stream startCaptureWithCompletionHandler:^(NSError *startError) {
                if (startError) [self recordError:startError];
                dispatch_group_leave(self->_starting);
            }];
        }
    }];
    return self;
}

- (void)stream:(SCStream *)stream didOutputSampleBuffer:(CMSampleBufferRef)sampleBuffer ofType:(SCStreamOutputType)type {
    if (type != SCStreamOutputTypeScreen || !CMSampleBufferIsValid(sampleBuffer)) return;
    NSArray *attachments = (__bridge NSArray *)CMSampleBufferGetSampleAttachmentsArray(sampleBuffer, NO);
    NSNumber *status = attachments.firstObject[SCStreamFrameInfoStatus];
    if (!status || status.integerValue != SCFrameStatusComplete) return;
    CVPixelBufferRef pixels = CMSampleBufferGetImageBuffer(sampleBuffer);
    if (!pixels) return;
    [_lock lock];
    if (!_stopped) {
        CVPixelBufferRetain(pixels);
        if (_latest) CVPixelBufferRelease(_latest);
        _latest = pixels;
        if (_sequence == UINT64_MAX) _status = 2;
        else { ++_sequence; _status = 0; }
    }
    [_lock unlock];
}

- (void)stream:(SCStream *)stream didStopWithError:(NSError *)error {
    [self recordError:error];
}

- (int)copyFrame:(uint8_t *)bytes capacity:(size_t)capacity sequence:(uint64_t *)sequence {
    [_lock lock];
    int status = _status;
    uint64_t next = _sequence;
    CVPixelBufferRef pixels = status == 0 && next != *sequence ? _latest : NULL;
    if (pixels) CVPixelBufferRetain(pixels);
    [_lock unlock];
    if (!pixels) return status == 0 ? 1 : status;
    // Probe before Rust allocates an output buffer; probing never consumes a frame.
    if (!bytes) { CVPixelBufferRelease(pixels); return 0; }
    int result = 4;
    size_t rowBytes = (size_t)_width * 4;
    if (capacity >= rowBytes * _height && CVPixelBufferGetWidth(pixels) == _width && CVPixelBufferGetHeight(pixels) == _height && CVPixelBufferGetPixelFormatType(pixels) == kCVPixelFormatType_32BGRA && CVPixelBufferLockBaseAddress(pixels, kCVPixelBufferLock_ReadOnly) == kCVReturnSuccess) {
        const uint8_t *base = CVPixelBufferGetBaseAddress(pixels);
        size_t stride = CVPixelBufferGetBytesPerRow(pixels);
        if (base && stride >= rowBytes && stride <= SIZE_MAX / _height && CVPixelBufferGetDataSize(pixels) >= stride * (_height - 1) + rowBytes) {
            for (size_t row = 0; row < _height; ++row) memcpy(bytes + row * rowBytes, base + row * stride, rowBytes);
            *sequence = next;
            result = 0;
        }
        CVPixelBufferUnlockBaseAddress(pixels, kCVPixelBufferLock_ReadOnly);
    }
    CVPixelBufferRelease(pixels);
    return result;
}

- (void)stop {
    [_lock lock];
    _stopped = YES;
    if (_latest) { CVPixelBufferRelease(_latest); _latest = NULL; }
    [_lock unlock];
    // Startup and stop completions retain their native owner. Rust calls this
    // only on its capture worker; the UI thread remains available for framework work.
    dispatch_group_wait(_starting, DISPATCH_TIME_FOREVER);
    if (_stream) {
        [_stream removeStreamOutput:self type:SCStreamOutputTypeScreen error:nil];
        dispatch_semaphore_t stopped = dispatch_semaphore_create(0);
        [_stream stopCaptureWithCompletionHandler:^(NSError *error) { dispatch_semaphore_signal(stopped); }];
        dispatch_semaphore_wait(stopped, DISPATCH_TIME_FOREVER);
        dispatch_sync(_outputQueue, ^{});
        _stream = nil;
    }
}
@end

void *vnc_screen_capture_start(uint32_t display, uint32_t width, uint32_t height, uint32_t interval) {
    @autoreleasepool {
        if (!width || !height || (uint64_t)width * height > (128 * 1024 * 1024) / 4) return NULL;
        return (__bridge_retained void *)[[VncScreenCapture alloc] initWithDisplay:display width:width height:height interval:interval];
    }
}

int vnc_screen_capture_copy(void *handle, uint8_t *bytes, size_t capacity, uint64_t *sequence) {
    @autoreleasepool {
        return [(__bridge VncScreenCapture *)handle copyFrame:bytes capacity:capacity sequence:sequence];
    }
}

void vnc_screen_capture_stop(void *handle) {
    @autoreleasepool {
        VncScreenCapture *capture = (__bridge_transfer VncScreenCapture *)handle;
        [capture stop];
    }
}
