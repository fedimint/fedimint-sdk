const path = require('path');
const pkg = require('../../packages/react-native-bindings/package.json');
// This is required for autolinking in this workspace only, and not if the package is installed from npm
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
