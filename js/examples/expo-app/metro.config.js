const { getDefaultConfig } = require('expo/metro-config')
const path = require('path')

const root = path.resolve(__dirname, '../..')

const config = getDefaultConfig(__dirname)

config.watchFolders = [root]
config.resolver.unstable_enableSymlinks = true
config.resolver.nodeModulesPaths = [
  path.resolve(__dirname, 'node_modules'),
  path.resolve(root, 'node_modules'),
]
config.resolver.disableHierarchicalLookup = true

// Map all workspace packages to their source directories
config.resolver.extraNodeModules = {
  '@fedimint/react-native-bindings': path.resolve(
    __dirname,
    '../../react-native/react-native-bindings',
  ),
  '@fedimint/react-native': path.resolve(
    __dirname,
    '../../react-native/react-native',
  ),
  '@fedimint/core': path.resolve(__dirname, '../../shared/core'),
  '@fedimint/types': path.resolve(__dirname, '../../shared/types'),
}

module.exports = config
