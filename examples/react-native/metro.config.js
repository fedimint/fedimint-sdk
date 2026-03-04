const { getDefaultConfig } = require('@react-native/metro-config');
const path = require('path');

const root = path.resolve(__dirname, '../..');

/**
 * Metro configuration
 * https://facebook.github.io/metro/docs/configuration
 *
 * @type {import('metro-config').MetroConfig}
 */
const config = getDefaultConfig(__dirname);

config.watchFolders = [root];
config.resolver.unstable_enableSymlinks = true;
config.resolver.nodeModulesPaths = [
  path.resolve(__dirname, 'node_modules'),
  path.resolve(root, 'node_modules'),
];

// Map all workspace packages to their source directories
config.resolver.extraNodeModules = {
  '@fedimint/react-native-bindings': path.resolve(
    __dirname,
    '../../packages/react-native-bindings',
  ),
  '@fedimint/react-native': path.resolve(
    __dirname,
    '../../packages/react-native',
  ),
  '@fedimint/core': path.resolve(__dirname, '../../packages/core'),
  '@fedimint/types': path.resolve(__dirname, '../../packages/types'),
};

module.exports = config;
