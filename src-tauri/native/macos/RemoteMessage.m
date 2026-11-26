#import <AppKit/AppKit.h>
#import <CoreGraphics/CoreGraphics.h>
#import <dispatch/dispatch.h>

static NSColor *MessageColor(unsigned int hex) {
    return [NSColor colorWithSRGBRed:((hex >> 16) & 255) / 255.0
        green:((hex >> 8) & 255) / 255.0 blue:(hex & 255) / 255.0 alpha:1];
}

// Keep native button semantics and accessibility while matching the app's theme.
@interface LLRemoteMessageButton : NSButton
@property(nonatomic) BOOL primary;
@end
@implementation LLRemoteMessageButton
- (void)drawRect:(NSRect)dirtyRect {
    (void)dirtyRect;
    BOOL pressed = self.cell.highlighted;
    NSColor *fill = self.primary ? MessageColor(pressed ? 0x176f5b : 0x1f8a70)
        : MessageColor(pressed ? 0xf6f8fa : 0xffffff);
    BOOL focused = self.window.firstResponder == self;
    NSBezierPath *shape = [NSBezierPath bezierPathWithRoundedRect:NSInsetRect(self.bounds, 1, 1) xRadius:6 yRadius:6];
    [fill setFill]; [shape fill];
    [(focused || self.primary ? MessageColor(0x176f5b) : MessageColor(0xd7dee7)) setStroke];
    shape.lineWidth = focused ? 2 : 1; [shape stroke];
    NSDictionary *attributes = @{
        NSFontAttributeName: [NSFont systemFontOfSize:14 weight:NSFontWeightSemibold],
        NSForegroundColorAttributeName: self.primary ? [NSColor whiteColor] : MessageColor(0x1f2933)
    };
    NSSize size = [self.title sizeWithAttributes:attributes];
    [self.title drawAtPoint:NSMakePoint((NSWidth(self.bounds) - size.width) / 2,
        (NSHeight(self.bounds) - size.height) / 2) withAttributes:attributes];
}
@end

@interface LLRemoteMessageWindow : NSObject <NSWindowDelegate>
@property(nonatomic, strong) NSWindow *window;
@property(nonatomic, strong) NSMutableString *text;
@property(nonatomic, strong) NSTextView *body;
@property(nonatomic, strong) NSScrollView *scroll;
@property(nonatomic, strong) NSButton *clipboardButton;
@property(nonatomic, strong) NSLayoutConstraint *bodyHeight;
@property(nonatomic) NSUInteger messageCount;
@property(nonatomic) NSSize referenceSize;
@property(nonatomic) NSTimeInterval referenceTime;
- (void)appendEntries:(NSArray<NSString *> *)entries;
@end

// Main-queue ownership serializes append versus close and retains the one inbox.
static LLRemoteMessageWindow *messageWindow;
static NSObject *messageLock;
static NSMutableArray<NSString *> *pendingMessages;
static NSUInteger retainedBytes;
static BOOL deliveryScheduled;
static void DeliverPendingMessages(void);

static void FocusMessageWindow(LLRemoteMessageWindow *controller) {
    NSWindow *window = controller.window;
    if (window.miniaturized) { [window deminiaturize:nil]; }
    [NSApp activateIgnoringOtherApps:YES];
    [window orderFrontRegardless];
    [window makeKeyAndOrderFront:nil];
    [window makeFirstResponder:controller.body];
    // Activation may complete on the next main-loop turn.
    dispatch_async(dispatch_get_main_queue(), ^{
        if (messageWindow == controller && window.visible && !window.keyWindow) {
            [NSApp activateIgnoringOtherApps:YES];
            [window makeKeyAndOrderFront:nil];
            [window makeFirstResponder:controller.body];
        }
    });
}

static NSSize MessageReferenceSize(NSWindow *inbox) {
    for (NSWindow *candidate in NSApp.windows) {
        if (candidate != inbox && candidate.visible && [candidate.title isEqualToString:@"LL Remote"]) {
            return candidate.frame.size;
        }
    }
    // The background sharing host is a separate process from the main GUI.
    NSArray *windows = CFBridgingRelease(CGWindowListCopyWindowInfo(
        kCGWindowListOptionAll | kCGWindowListExcludeDesktopElements, kCGNullWindowID));
    for (NSDictionary *info in windows) {
        pid_t pid = [info[(__bridge NSString *)kCGWindowOwnerPID] intValue];
        if (pid == [NSProcessInfo processInfo].processIdentifier) { continue; }
        NSString *title = info[(__bridge NSString *)kCGWindowName];
        if (![title isEqualToString:@"LL Remote"] && ![title isEqualToString:@"LLRemote"]) { continue; }
        NSRunningApplication *app = [NSRunningApplication runningApplicationWithProcessIdentifier:pid];
        NSString *owner = info[(__bridge NSString *)kCGWindowOwnerName];
        if (app.bundleIdentifier.length > 0) {
            if (![app.bundleIdentifier isEqualToString:@"com.iLynnYi.LLRemote"]) { continue; }
        } else if (![owner isEqualToString:@"LL Remote"] && ![owner isEqualToString:@"LLRemote"]) { continue; }
        CGRect bounds;
        NSDictionary *value = info[(__bridge NSString *)kCGWindowBounds];
        if (value != nil && CGRectMakeWithDictionaryRepresentation((__bridge CFDictionaryRef)value, &bounds) &&
            bounds.size.width > 0 && bounds.size.height > 0) { return bounds.size; }
    }
    return NSMakeSize(1280, 768);
}

@implementation LLRemoteMessageWindow
- (void)appendEntries:(NSArray<NSString *> *)entries {
    NSMutableParagraphStyle *paragraph = [NSMutableParagraphStyle new];
    paragraph.lineSpacing = 3;
    NSDictionary *attributes = @{NSFontAttributeName:[NSFont systemFontOfSize:14],
        NSForegroundColorAttributeName:MessageColor(0x1f2933), NSParagraphStyleAttributeName:paragraph};
    NSMutableAttributedString *batch = [[NSMutableAttributedString alloc] initWithString:@""];
    for (NSString *entry in entries) {
        NSString *separator = self.text.length > 0 ? @"\n\n" : @"";
        [batch appendAttributedString:[[NSAttributedString alloc] initWithString:separator attributes:attributes]];
        NSMutableAttributedString *message = [[NSMutableAttributedString alloc] initWithString:entry attributes:attributes];
        NSRange timeSeparator = [entry rangeOfString:@"  "];
        if (timeSeparator.location != NSNotFound) {
            [message addAttributes:@{NSFontAttributeName:[NSFont monospacedDigitSystemFontOfSize:11 weight:NSFontWeightRegular],
                NSForegroundColorAttributeName:MessageColor(0x8a97a3)} range:NSMakeRange(0, timeSeparator.location)];
        }
        [batch appendAttributedString:message];
        [self.text appendString:separator];
        [self.text appendString:entry];
        self.messageCount += 1;
    }
    [self.body.textStorage beginEditing];
    [self.body.textStorage appendAttributedString:batch];
    [self.body.textStorage endEditing];
    self.clipboardButton.title = self.messageCount > 1 ? @"复制全部" : @"复制";
    self.clipboardButton.needsDisplay = YES;
    NSRect visible = (self.window.screen ?: [NSScreen mainScreen]).visibleFrame;
    NSTimeInterval now = [NSDate timeIntervalSinceReferenceDate];
    if (self.referenceTime == 0 || now - self.referenceTime > 1) {
        self.referenceSize = MessageReferenceSize(self.window);
        self.referenceTime = now;
    }
    NSSize reference = self.referenceSize;
    CGFloat maxWidth = MAX(1, floor(MIN(reference.width / 2, NSWidth(visible) - 24)));
    CGFloat maxHeight = MAX(1, floor(MIN(reference.height / 2, NSHeight(visible) - 24)));
    NSRect decoration = [self.window frameRectForContentRect:NSZeroRect];
    CGFloat maxClientWidth = MAX(1, maxWidth - NSWidth(decoration));
    CGFloat maxBodyHeight = MAX(1, maxHeight - NSHeight(decoration) - 140);
    CGFloat width = MIN(MAX(420, NSWidth(self.window.contentView.frame)), maxClientWidth);
    CGFloat measured = 0;
    while (YES) {
        measured = ceil([self.body.textStorage boundingRectWithSize:NSMakeSize(MAX(1, width - 64), CGFLOAT_MAX)
            options:NSStringDrawingUsesLineFragmentOrigin | NSStringDrawingUsesFontLeading].size.height);
        if (measured <= MIN(160, maxBodyHeight - 24) || width >= maxClientWidth) { break; }
        width = MIN(width + 80, maxClientWidth);
    }
    BOOL overflow = measured + 24 > maxBodyHeight;
    self.scroll.hasVerticalScroller = overflow;
    self.scroll.autohidesScrollers = NO;
    self.scroll.scrollerStyle = NSScrollerStyleLegacy;
    CGFloat scrollbarWidth = overflow ? [NSScroller scrollerWidthForControlSize:NSControlSizeRegular scrollerStyle:NSScrollerStyleLegacy] : 0;
    CGFloat textWidth = MAX(1, width - 64 - scrollbarWidth);
    measured = ceil([self.body.textStorage boundingRectWithSize:NSMakeSize(textWidth, CGFLOAT_MAX)
        options:NSStringDrawingUsesLineFragmentOrigin | NSStringDrawingUsesFontLeading].size.height);
    CGFloat height = MIN(maxBodyHeight, MAX(MIN(44, maxBodyHeight), measured + 24));
    self.bodyHeight.constant = height;
    NSRect previous = self.window.frame;
    NSRect frame = [self.window frameRectForContentRect:NSMakeRect(0, 0, width, height + 140)];
    frame.size.width = MIN(frame.size.width, maxWidth);
    frame.size.height = MIN(frame.size.height, maxHeight);
    frame.origin = NSMakePoint(previous.origin.x, NSMaxY(previous) - NSHeight(frame));
    frame.origin.x = MAX(NSMinX(visible), MIN(frame.origin.x, NSMaxX(visible) - NSWidth(frame)));
    frame.origin.y = MAX(NSMinY(visible), MIN(frame.origin.y, NSMaxY(visible) - NSHeight(frame)));
    [self.window setFrame:frame display:YES];
    [self.window.contentView layoutSubtreeIfNeeded];
    NSSize documentSize = NSMakeSize(self.scroll.contentSize.width, MAX(height, measured + 24));
    self.body.minSize = NSMakeSize(0, height);
    self.body.maxSize = NSMakeSize(documentSize.width, CGFLOAT_MAX);
    [self.body setFrameSize:documentSize];
    [self.body.layoutManager ensureLayoutForTextContainer:self.body.textContainer];
    [self.body scrollRangeToVisible:NSMakeRange(self.body.string.length, 0)];
}
- (void)copyMessage:(NSButton *)sender {
    NSPasteboard *pasteboard = [NSPasteboard generalPasteboard];
    [pasteboard clearContents];
    BOOL copied = [pasteboard setString:self.text forType:NSPasteboardTypeString];
    sender.title = copied ? @"已复制" : @"重试复制";
    sender.needsDisplay = YES;
}
- (void)closeMessage:(id)sender {
    (void)sender;
    [self.window close];
}
- (void)windowWillClose:(NSNotification *)notification {
    (void)notification;
    self.window.delegate = nil;
    if (messageWindow == self) {
        messageWindow = nil;
        @synchronized(messageLock) {
            retainedBytes = pendingMessages.count > 0 ? (pendingMessages.count - 1) * 2 : 0;
            for (NSString *entry in pendingMessages) { retainedBytes += [entry lengthOfBytesUsingEncoding:NSUTF8StringEncoding]; }
        }
    }
}
@end

// Queue ownership is independent of AppKit rendering and the popup lifetime.
// The authenticated worker only normalizes, bounds and enqueues the message.
int llremote_show_message(const char *utf8, size_t maxBytes) {
    @autoreleasepool {
        NSString *text = [[NSString alloc] initWithUTF8String:utf8];
        if (text == nil) { return 0; }
        NSDictionary *session = CFBridgingRelease(CGSessionCopyCurrentDictionary());
        if (![session[(__bridge NSString *)kCGSessionOnConsoleKey] boolValue]) { return 0; }
        text = [[text stringByReplacingOccurrencesOfString:@"\r\n" withString:@"\n"] stringByReplacingOccurrencesOfString:@"\r" withString:@"\n"];
        static dispatch_once_t once;
        dispatch_once(&once, ^{ messageLock = [NSObject new]; pendingMessages = [NSMutableArray new]; });
        @synchronized(messageLock) {
            NSUInteger next = retainedBytes + (retainedBytes > 0 ? 2 : 0) + [text lengthOfBytesUsingEncoding:NSUTF8StringEncoding];
            if (next > maxBytes) { return 2; }
            [pendingMessages addObject:text];
            retainedBytes = next;
            if (!deliveryScheduled) {
                deliveryScheduled = YES;
                dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 16 * NSEC_PER_MSEC), dispatch_get_main_queue(), ^{ DeliverPendingMessages(); });
            }
        }
        return 1;
    }
}

static void DeliverPendingMessages(void) {
    @autoreleasepool {
        NSApplication *application = [NSApplication sharedApplication];
        NSScreen *screen = [NSScreen mainScreen];
        @synchronized(messageLock) {
            deliveryScheduled = NO;
            // Keep accepted messages queued until a usable desktop is available.
            if (screen == nil || NSWidth(screen.visibleFrame) <= 0 || NSHeight(screen.visibleFrame) <= 0) {
                deliveryScheduled = YES;
                dispatch_after(dispatch_time(DISPATCH_TIME_NOW, NSEC_PER_SEC), dispatch_get_main_queue(), ^{ DeliverPendingMessages(); });
                return;
            }
        }
        if (application.activationPolicy == NSApplicationActivationPolicyProhibited) {
            [application setActivationPolicy:NSApplicationActivationPolicyAccessory];
        }
        NSArray<NSString *> *entries;
        @synchronized(messageLock) {
            entries = [pendingMessages copy];
            [pendingMessages removeAllObjects];
        }
        if (entries.count == 0) { return; }
        if (messageWindow != nil) {
            [messageWindow appendEntries:entries];
            FocusMessageWindow(messageWindow);
            return;
        }
        LLRemoteMessageWindow *controller = [LLRemoteMessageWindow new];
        controller.text = [NSMutableString new];
        NSFont *messageFont = [NSFont systemFontOfSize:14];
        CGFloat bodyHeight = 44;
        NSWindow *window = [[NSWindow alloc]
            initWithContentRect:NSMakeRect(0, 0, 420, bodyHeight + 140)
            styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskClosable
            backing:NSBackingStoreBuffered defer:NO];
        window.title = @"LL Remote";
        window.appearance = [NSAppearance appearanceNamed:NSAppearanceNameAqua];
        window.backgroundColor = [NSColor whiteColor];
        window.releasedWhenClosed = NO;
        window.delegate = controller;
        controller.window = window;
        NSView *content = window.contentView;

        NSView *dot = [NSView new];
        dot.wantsLayer = YES;
        dot.layer.backgroundColor = MessageColor(0x1f8a70).CGColor;
        dot.layer.cornerRadius = 3;
        dot.translatesAutoresizingMaskIntoConstraints = NO;
        [content addSubview:dot];
        NSTextField *heading = [NSTextField labelWithString:@"收到远程消息"];
        heading.font = [NSFont systemFontOfSize:16 weight:NSFontWeightSemibold];
        heading.textColor = MessageColor(0x1f2933);
        heading.translatesAutoresizingMaskIntoConstraints = NO;
        [content addSubview:heading];
        NSTextField *caption = [NSTextField labelWithString:@"来自远程协助连接"];
        caption.font = [NSFont systemFontOfSize:12];
        caption.textColor = MessageColor(0x708090);
        caption.translatesAutoresizingMaskIntoConstraints = NO;
        [content addSubview:caption];

        NSScrollView *scroll = [NSScrollView new];
        scroll.translatesAutoresizingMaskIntoConstraints = NO;
        scroll.hasVerticalScroller = YES;
        scroll.autohidesScrollers = YES;
        scroll.borderType = NSNoBorder;
        scroll.backgroundColor = [NSColor whiteColor];
        scroll.wantsLayer = YES;
        scroll.layer.borderColor = MessageColor(0xd7dee7).CGColor;
        scroll.layer.borderWidth = 1;
        scroll.layer.cornerRadius = 8;
        scroll.layer.masksToBounds = YES;
        NSTextView *body = [[NSTextView alloc] initWithFrame:NSMakeRect(0, 0, 380, bodyHeight)];
        body.editable = NO;
        body.selectable = YES;
        body.richText = NO;
        body.font = messageFont;
        body.textColor = MessageColor(0x1f2933);
        body.backgroundColor = [NSColor whiteColor];
        body.textContainerInset = NSMakeSize(12, 10);
        body.textContainer.lineFragmentPadding = 0;
        body.verticallyResizable = YES;
        body.horizontallyResizable = NO;
        body.autoresizingMask = NSViewWidthSizable;
        body.textContainer.widthTracksTextView = YES;
        [body setAccessibilityLabel:@"远程消息内容"];
        scroll.documentView = body;
        [content addSubview:scroll];
        controller.body = body;
        controller.scroll = scroll;
        controller.bodyHeight = [scroll.heightAnchor constraintEqualToConstant:bodyHeight];

        LLRemoteMessageButton *copy = [LLRemoteMessageButton buttonWithTitle:@"复制" target:controller action:@selector(copyMessage:)];
        LLRemoteMessageButton *close = [LLRemoteMessageButton buttonWithTitle:@"关闭" target:controller action:@selector(closeMessage:)];
        controller.clipboardButton = copy;
        close.primary = YES;
        close.keyEquivalent = @"\e";
        copy.translatesAutoresizingMaskIntoConstraints = NO;
        close.translatesAutoresizingMaskIntoConstraints = NO;
        [content addSubview:copy];
        [content addSubview:close];
        [NSLayoutConstraint activateConstraints:@[
            [dot.leadingAnchor constraintEqualToAnchor:content.leadingAnchor constant:20],
            [dot.centerYAnchor constraintEqualToAnchor:heading.centerYAnchor],
            [dot.widthAnchor constraintEqualToConstant:6], [dot.heightAnchor constraintEqualToConstant:6],
            [heading.topAnchor constraintEqualToAnchor:content.topAnchor constant:18],
            [heading.leadingAnchor constraintEqualToAnchor:content.leadingAnchor constant:34],
            [caption.topAnchor constraintEqualToAnchor:content.topAnchor constant:44],
            [caption.leadingAnchor constraintEqualToAnchor:content.leadingAnchor constant:20],
            [scroll.topAnchor constraintEqualToAnchor:content.topAnchor constant:72],
            [scroll.leadingAnchor constraintEqualToAnchor:content.leadingAnchor constant:20],
            [scroll.trailingAnchor constraintEqualToAnchor:content.trailingAnchor constant:-20],
            controller.bodyHeight,
            [close.topAnchor constraintEqualToAnchor:scroll.bottomAnchor constant:16],
            [close.trailingAnchor constraintEqualToAnchor:content.trailingAnchor constant:-20],
            [close.bottomAnchor constraintEqualToAnchor:content.bottomAnchor constant:-20],
            [close.widthAnchor constraintEqualToConstant:76], [close.heightAnchor constraintEqualToConstant:32],
            [copy.trailingAnchor constraintEqualToAnchor:close.leadingAnchor constant:-8],
            [copy.centerYAnchor constraintEqualToAnchor:close.centerYAnchor],
            [copy.widthAnchor constraintEqualToConstant:76], [copy.heightAnchor constraintEqualToConstant:32]
        ]];
        [controller appendEntries:entries];
        messageWindow = controller;
        [window center];
        FocusMessageWindow(controller);
    }
}
