import { type ConfigPlugin, withGradleProperties } from '@expo/config-plugins';

/**
 * Add required configurations to gradle.properties
 */
const withGradlePropertiesConfig: ConfigPlugin = (config) => {
  return withGradleProperties(config, (config) => {
    // Ensure AndroidX is enabled
    config.modResults = config.modResults.filter(
      (item) => item.type !== 'property' || item.key !== 'android.useAndroidX'
    );
    config.modResults.push({
      type: 'property',
      key: 'android.useAndroidX',
      value: 'true',
    });

    // Enable new architecture if not already set
    const hasNewArchEnabled = config.modResults.some(
      (item) =>
        item.type === 'property' && item.key === 'newArchEnabled'
    );
    if (!hasNewArchEnabled) {
      config.modResults.push({
        type: 'property',
        key: 'newArchEnabled',
        value: 'true',
      });
    }

    return config;
  });
};

/**
 * Configure Android build settings for Fedimint SDK
 */
export const withFedimintAndroid: ConfigPlugin = (config) => {
  config = withGradlePropertiesConfig(config);
  return config;
};
