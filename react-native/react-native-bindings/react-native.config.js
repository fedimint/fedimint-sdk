/**
 * @fedimint/react-native-bindings configuration
 * @type {import('@react-native-community/cli-types').UserDependencyConfig}
 */
module.exports = {
  dependency: {
    platforms: {
      android: {
        cmakeListsPath: 'generated/jni/CMakeLists.txt',
        packageImportPath: 'import com.fedimint.reactnative.ReactNativeBindingsPackage;',
        packageInstance: 'new ReactNativeBindingsPackage()',
      },
    },
  },
};