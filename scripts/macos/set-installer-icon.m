#import <AppKit/AppKit.h>
#include <stdio.h>

int main(int argc, const char *argv[]) {
    @autoreleasepool {
        if (argc < 3) {
            fprintf(stderr, "Usage: set-installer-icon <icon.icns> <installer.dmg> [...]\n");
            return 1;
        }
        NSImage *icon = [[NSImage alloc] initWithContentsOfFile:[NSString stringWithUTF8String:argv[1]]];
        if (icon == nil) {
            fprintf(stderr, "Cannot load installer icon: %s\n", argv[1]);
            return 1;
        }
        for (int i = 2; i < argc; i++) {
            NSString *path = [NSString stringWithUTF8String:argv[i]];
            if (![[NSWorkspace sharedWorkspace] setIcon:icon forFile:path options:0]) {
                fprintf(stderr, "Cannot set installer icon: %s\n", argv[i]);
                return 1;
            }
        }
    }
    return 0;
}
