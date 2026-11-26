const {
  IOSConfig,
  withDangerousMod,
  withXcodeProject,
} = require('expo/config-plugins')
const fs = require('fs')
const path = require('path')

/**
 * Ships the Geist fonts' licence with them, as the SIL Open Font License asks of every copy:
 * expo-font bundles the font files alone. On Android it goes beside them in the app's assets; on
 * iOS it joins the target's resources, the way expo-font adds the fonts.
 */
const licence = 'assets/fonts/OFL.txt'

module.exports = function withFontLicence(config) {
  config = withDangerousMod(config, [
    'android',
    async (config) => {
      const { platformProjectRoot, projectRoot } = config.modRequest
      const fonts = path.join(platformProjectRoot, 'app/src/main/assets/fonts')
      await fs.promises.mkdir(fonts, { recursive: true })
      await fs.promises.copyFile(
        path.join(projectRoot, licence),
        path.join(fonts, 'OFL.txt'),
      )
      return config
    },
  ])
  return withXcodeProject(config, (config) => {
    const { platformProjectRoot, projectRoot } = config.modRequest
    const project = config.modResults
    IOSConfig.XcodeUtils.ensureGroupRecursively(project, 'Resources')
    IOSConfig.XcodeUtils.addResourceFileToGroup({
      filepath: path.relative(
        platformProjectRoot,
        path.join(projectRoot, licence),
      ),
      groupName: 'Resources',
      project,
      isBuildFile: true,
    })
    return config
  })
}
