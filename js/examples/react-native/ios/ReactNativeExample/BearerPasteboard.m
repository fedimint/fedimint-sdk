#import <React/RCTBridgeModule.h>
#import <UIKit/UIKit.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>

// Copies bearer value, ecash notes, the way the iOS demo does (ios/Demo/Views/Components/
// Pasteboard.swift): kept on this device, so Universal Clipboard never syncs it to the user's
// other devices, and expiring rather than sitting there for any app that reads the pasteboard
// later. The clipboard library the example uses offers neither option.
@interface BearerPasteboard : NSObject <RCTBridgeModule>
@end

@implementation BearerPasteboard

RCT_EXPORT_MODULE()

+ (BOOL)requiresMainQueueSetup
{
  return NO;
}

- (dispatch_queue_t)methodQueue
{
  return dispatch_get_main_queue();
}

RCT_EXPORT_METHOD(copy : (NSString *)value lifetime : (double)seconds)
{
  [UIPasteboard.generalPasteboard setItems:@[ @{UTTypePlainText.identifier : value} ]
                                   options:@{
                                     UIPasteboardOptionLocalOnly : @YES,
                                     UIPasteboardOptionExpirationDate : [NSDate dateWithTimeIntervalSinceNow:seconds],
                                   }];
}

@end
