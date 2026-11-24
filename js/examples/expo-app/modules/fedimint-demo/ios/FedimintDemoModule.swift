import ExpoModulesCore
import LocalAuthentication
import UIKit
import UniformTypeIdentifiers

/// - `confirmOwner`: Face ID or Touch ID, falling back to the passcode; a failure rejects with the
///   system's own message.
/// - `copyBearer`: ecash notes kept on this device, so Universal Clipboard never syncs them to the
///   user's other devices, and expiring rather than staying for any app that reads the pasteboard.
public class FedimintDemoModule: Module {
  public func definition() -> ModuleDefinition {
    Name("FedimintDemo")

    AsyncFunction("confirmOwner") { (reason: String, promise: Promise) in
      let context = LAContext()
      guard context.canEvaluatePolicy(.deviceOwnerAuthentication, error: nil) else {
        promise.resolve("unavailable")
        return
      }
      context.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reason) { success, error in
        if success {
          promise.resolve("ok")
        } else {
          promise.reject("declined", error?.localizedDescription ?? "Authentication failed.")
        }
      }
    }

    AsyncFunction("copyBearer") { (value: String, lifetime: Double) in
      UIPasteboard.general.setItems(
        [[UTType.plainText.identifier: value]],
        options: [
          .localOnly: true,
          .expirationDate: Date(timeIntervalSinceNow: lifetime),
        ]
      )
    }.runOnQueue(.main)
  }
}
