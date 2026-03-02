const path = require('path');
const pkg = require('../../packages/react-native-bindings/package.json');

/**
 * This configuration is only necessary for developing and testing within this monorepo.
 * It manually points the React Native CLI to the local package root for autolinking native modules.
 * 
 * Users installing the package from npm do NOT need this, as standard autolinking 
 * will automatically find the package inside their node_modules directory.
 */
module.exports = {
  project: {
    ios: {
      automaticPodsInstallation: true,
    },
  },
  dependencies: {
    [pkg.name]: {
      root: path.join(__dirname, '../../packages/react-native-bindings'),
    },
  },
};
