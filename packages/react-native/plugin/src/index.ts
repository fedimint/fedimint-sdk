import {
  type ConfigPlugin,
  withPlugins,
  createRunOncePlugin,
} from '@expo/config-plugins';
import { withBinaryArtifacts } from './withBinaryArtifacts';
import { withFedimintAndroid } from './withAndroid';
import { withFedimintIOS } from './withIOS';
import { sdkPackage } from './utils';

export type FedimintPluginOptions = {
  /**
   * Skip downloading binary artifacts (default: false)
   * Set to true if you want to handle binary downloads manually
   */
  skipBinaryDownload?: boolean;
};

const withFedimintSdk: ConfigPlugin<FedimintPluginOptions | void> = (
  config,
  options
) => {
  const { skipBinaryDownload = false } = options || {};

  return withPlugins(config, [
    // Download binary artifacts first
    ...(skipBinaryDownload ? [] : [withBinaryArtifacts]),
    // Configure Android
    withFedimintAndroid,
    // Configure iOS
    withFedimintIOS,
  ]);
};

export default createRunOncePlugin(
  withFedimintSdk,
  sdkPackage.name,
  sdkPackage.version
);
