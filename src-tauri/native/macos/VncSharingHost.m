#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>
#import <dispatch/dispatch.h>
#import <Security/Security.h>
#import <CoreServices/CoreServices.h>
#include <string.h>

// A development bundle launched by launchd may not have been seen by Finder.
// Register only its known URL before an explicit, bundle-scoped TCC reset.
bool vnc_sharing_host_register_bundle(const char *path) {
    @autoreleasepool {
        NSString *name = [[NSFileManager defaultManager] stringWithFileSystemRepresentation:path length:strlen(path)];
        return name != nil && LSRegisterURL((__bridge CFURLRef)[NSURL fileURLWithPath:name], true) == noErr;
    }
}

// Read once in the host before accepting IPC. Dynamic validation rejects a
// running image whose on-disk signature was replaced underneath it.
bool vnc_sharing_host_identity(unsigned char *identifier, size_t identifierSize,
                               unsigned char *hash, size_t hashSize) {
    @autoreleasepool {
        SecCodeRef code = NULL;
        CFDictionaryRef information = NULL;
        if (SecCodeCopySelf(kSecCSDefaultFlags, &code) != errSecSuccess) { return false; }
        OSStatus valid = SecCodeCheckValidity(code, kSecCSDefaultFlags, NULL);
        OSStatus result = valid == errSecSuccess
            ? SecCodeCopySigningInformation((SecStaticCodeRef)code, kSecCSDefaultFlags, &information)
            : valid;
        CFRelease(code);
        if (result != errSecSuccess || information == NULL) { return false; }
        NSDictionary *info = CFBridgingRelease(information);
        NSString *name = info[(__bridge NSString *)kSecCodeInfoIdentifier];
        NSData *digest = info[(__bridge NSString *)kSecCodeInfoUnique];
        NSString *bundleID = [NSBundle mainBundle].bundleIdentifier;
        if (![name isKindOfClass:[NSString class]] || ![digest isKindOfClass:[NSData class]] ||
            ![name isEqualToString:bundleID] || digest.length == 0 ||
            strlen(name.UTF8String) >= identifierSize || digest.length * 2 + 1 > hashSize) { return false; }
        strcpy((char *)identifier, name.UTF8String);
        const unsigned char *bytes = digest.bytes;
        const char *hex = "0123456789abcdef";
        for (NSUInteger i = 0; i < digest.length; i++) {
            hash[i * 2] = hex[bytes[i] >> 4];
            hash[i * 2 + 1] = hex[bytes[i] & 15];
        }
        hash[digest.length * 2] = 0;
        return true;
    }
}

// The capturing process requests its own trust; AppKit prompts run on its main queue.
bool vnc_sharing_host_request_accessibility(void) {
    __block bool trusted = false;
    void (^request)(void) = ^{
        NSDictionary *options = @{(__bridge NSString *)kAXTrustedCheckOptionPrompt: @YES};
        trusted = AXIsProcessTrustedWithOptions((__bridge CFDictionaryRef)options);
    };
    if ([NSThread isMainThread]) { request(); }
    else { dispatch_sync(dispatch_get_main_queue(), request); }
    return trusted;
}

void vnc_sharing_host_run_loop(void) {
    @autoreleasepool {
        NSApplication *application = [NSApplication sharedApplication];
        [application setActivationPolicy:NSApplicationActivationPolicyProhibited];
        [application run];
    }
}

void vnc_sharing_host_stop_loop(void) {
    dispatch_async(dispatch_get_main_queue(), ^{
        [NSApp stop:nil];
        NSEvent *wake = [NSEvent otherEventWithType:NSEventTypeApplicationDefined
            location:NSZeroPoint modifierFlags:0 timestamp:0 windowNumber:0
            context:nil subtype:0 data1:0 data2:0];
        [NSApp postEvent:wake atStart:NO];
    });
}
