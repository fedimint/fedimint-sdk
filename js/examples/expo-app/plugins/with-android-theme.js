const { withAndroidStyles } = require('expo/config-plugins')

/**
 * Two changes to the Android theme Expo generates:
 *
 * - Light whatever the system setting. Expo's theme follows the system, so in dark mode the
 *   system's own views (the biometric prompt, alerts, the text field's handles) would turn dark
 *   around the light screens.
 * - No enforced contrast behind the gesture bar: without it, some skins (Xiaomi's HyperOS) paint a
 *   light strip there even with edge-to-edge on.
 */
module.exports = function withAndroidTheme(config) {
  return withAndroidStyles(config, (config) => {
    const theme = config.modResults.resources.style?.find(
      (style) => style.$.name === 'AppTheme',
    )
    if (!theme) throw new Error('with-android-theme: no AppTheme in styles.xml')
    theme.$.parent = 'Theme.AppCompat.Light.NoActionBar'
    theme.item = (theme.item ?? []).filter(
      (item) => item.$.name !== 'android:enforceNavigationBarContrast',
    )
    theme.item.push({
      $: {
        name: 'android:enforceNavigationBarContrast',
        'tools:targetApi': '29',
      },
      _: 'false',
    })
    return config
  })
}
