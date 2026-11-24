const { withAppDelegate, withInfoPlist } = require('expo/config-plugins')

/**
 * Apps built with the iOS 27 SDK must use the scene lifecycle, or UIKit stops them at launch.
 * Expo's AppDelegate makes its own window instead, so this moves React Native into a scene
 * delegate's window and declares that scene in Info.plist. Links reach a scene, not the app
 * delegate, so the scene hands them on to React Native's Linking, which Expo Router reads.
 */
const windowBlock =
  /#if os\(iOS\) \|\| os\(tvOS\)\n\s*window = UIWindow\(frame: UIScreen\.main\.bounds\)\n\s*factory\.startReactNative\(\n\s*withModuleName: "main",\n\s*in: window,\n\s*launchOptions: launchOptions\)\n#endif\n/

const sceneDelegate = `
/// The app's one scene: React Native starts in its window (plugins/with-scene-lifecycle.js).
class SceneDelegate: UIResponder, UIWindowSceneDelegate {
  var window: UIWindow?

  func scene(
    _ scene: UIScene,
    willConnectTo session: UISceneSession,
    options connectionOptions: UIScene.ConnectionOptions
  ) {
    guard let windowScene = scene as? UIWindowScene,
          let app = UIApplication.shared.delegate as? AppDelegate,
          let factory = app.reactNativeFactory
    else { return }
    let window = UIWindow(windowScene: windowScene)
    self.window = window
    // Code that still looks for the window on the app delegate finds it there too.
    app.window = window
    // A link the app was opened with is the initial URL Linking reports.
    let launchOptions = connectionOptions.urlContexts.first.map {
      [UIApplication.LaunchOptionsKey.url: $0.url]
    }
    factory.startReactNative(withModuleName: "main", in: window, launchOptions: launchOptions)
  }

  func scene(_ scene: UIScene, openURLContexts URLContexts: Set<UIOpenURLContext>) {
    for context in URLContexts {
      _ = RCTLinkingManager.application(UIApplication.shared, open: context.url, options: [:])
    }
  }
}
`

module.exports = function withSceneLifecycle(config) {
  config = withAppDelegate(config, (config) => {
    const delegate = config.modResults
    if (delegate.language !== 'swift') {
      throw new Error('with-scene-lifecycle expects a Swift AppDelegate')
    }
    if (delegate.contents.includes('class SceneDelegate')) return config
    if (!windowBlock.test(delegate.contents)) {
      throw new Error(
        "with-scene-lifecycle: the AppDelegate's window setup has changed; update the plugin",
      )
    }
    delegate.contents =
      delegate.contents.replace(
        windowBlock,
        '    // The window comes from the scene, in SceneDelegate.\n',
      ) + sceneDelegate
    return config
  })
  return withInfoPlist(config, (config) => {
    config.modResults.UIApplicationSceneManifest = {
      UIApplicationSupportsMultipleScenes: false,
      UISceneConfigurations: {
        UIWindowSceneSessionRoleApplication: [
          {
            UISceneConfigurationName: 'Default Configuration',
            UISceneDelegateClassName: '$(PRODUCT_MODULE_NAME).SceneDelegate',
          },
        ],
      },
    }
    return config
  })
}
