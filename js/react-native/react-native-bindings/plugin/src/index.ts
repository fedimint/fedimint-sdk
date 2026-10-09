import {
  type ConfigPlugin,
  withPlugins,
  createRunOncePlugin,
} from '@expo/config-plugins';
import { withFedimintAndroid } from './withAndroid';
import { withFedimintIOS } from './withIOS';
import { sdkPackage } from './utils';

const withFedimintSdk: ConfigPlugin = (config) =>
  withPlugins(config, [withFedimintAndroid, withFedimintIOS]);

export default createRunOncePlugin(
  withFedimintSdk,
  sdkPackage.name,
  sdkPackage.version
);
