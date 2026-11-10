#import <LocalAuthentication/LocalAuthentication.h>
#import <React/RCTBridgeModule.h>

// Device-owner authentication the way the iOS demo asks for it (ios/Demo/DemoModel.swift,
// toggleSeed): Face ID or Touch ID, falling back to the passcode, and a failure reported in the
// system's own words, its localizedDescription.
@interface OwnerAuthentication : NSObject <RCTBridgeModule>
@end

@implementation OwnerAuthentication

RCT_EXPORT_MODULE()

+ (BOOL)requiresMainQueueSetup
{
  return NO;
}

// Resolves "ok", or "unavailable" on a device with no passcode; rejects when declined.
RCT_EXPORT_METHOD(confirm : (NSString *)reason resolver : (RCTPromiseResolveBlock)resolve rejecter : (RCTPromiseRejectBlock)reject)
{
  LAContext *context = [LAContext new];
  NSError *unavailable = nil;
  if (![context canEvaluatePolicy:LAPolicyDeviceOwnerAuthentication error:&unavailable]) {
    resolve(@"unavailable");
    return;
  }
  [context evaluatePolicy:LAPolicyDeviceOwnerAuthentication
          localizedReason:reason
                    reply:^(BOOL success, NSError *error) {
                      if (success) {
                        resolve(@"ok");
                      } else {
                        reject(@"declined", error.localizedDescription, error);
                      }
                    }];
}

@end
