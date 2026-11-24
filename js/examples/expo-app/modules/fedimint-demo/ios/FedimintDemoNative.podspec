Pod::Spec.new do |s|
  s.name           = 'FedimintDemoNative'
  # CocoaPods requires a version and the four fields after it; nothing else reads them.
  s.version        = '1.0.0'
  s.summary        = "The example's own native code: owner authentication and the ecash copy"
  s.author         = 'Fedimint'
  s.homepage       = 'https://github.com/fedimint/fedimint-sdk'
  s.license        = 'MIT'
  s.platforms      = { :ios => '15.1' }
  s.swift_version  = '5.9'
  s.source         = { git: '' }
  s.static_framework = true

  s.dependency 'ExpoModulesCore'

  s.source_files = '**/*.swift'
end
