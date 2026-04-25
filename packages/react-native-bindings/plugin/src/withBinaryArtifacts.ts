import type { ConfigPlugin } from '@expo/config-plugins';
import * as path from 'path';
import * as fs from 'fs';
import { execSync } from 'child_process';

const PACKAGE_NAME = '@fedimint/react-native-bindings';
const GITHUB_REPO = 'https://github.com/fedimint/fedimint-sdk';

/**
 * Downloads prebuilt binary artifacts for iOS only.
 * This runs synchronously during expo prebuild to ensure binaries are available.
 */
export const withBinaryArtifacts: ConfigPlugin = (config) => {
  try {
    downloadBinaryArtifacts();
  } catch (error) {
    console.warn('Failed to download Fedimint SDK binary artifacts:', error);
  }
  return config;
};

function downloadBinaryArtifacts(): void {
  const packageRoot = path.resolve(__dirname, '..', '..');

  const iosFrameworkPath = path.join(
    packageRoot,
    'FedimintReactNativeBindingsFramework.xcframework'
  );

  // Skip if we already have the iOS bindings downloaded
  if (fs.existsSync(iosFrameworkPath)) {
    console.log('Fedimint SDK iOS binary artifacts already exist, skipping download.');
    return;
  }
  
  const packageJsonPath = path.join(packageRoot, 'package.json');
  if (!fs.existsSync(packageJsonPath)) {
    throw new Error(`Could not find package.json at ${packageJsonPath}`);
  }
  
  const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf-8'));
  const version = packageJson.version;
  
  const iosChecksum = packageJson.checksums?.ios;
  if (!iosChecksum) {
    console.warn('Binary checksums not found in package.json. Skipping download (assume dev env or compiled from source).');
    return;
  }

  // Determine release tag based on version
  const isCanary = version.includes('canary');
  let releaseTag = `react-native-v${version}`;
  if (isCanary) {
    releaseTag = 'canary';
  } else if (version.includes('-') || version.includes('snapshot')) {
    releaseTag = 'snapshot';
  }
  const iosUrl = `${GITHUB_REPO}/releases/download/${releaseTag}/ios-artifacts.zip`;

  // Download and verify iOS artifacts
  try {
    console.log('Downloading Fedimint SDK iOS artifacts...');
    execSync(`curl -L "${iosUrl}" --output ios-artifacts.zip`, {
      cwd: packageRoot,
      stdio: 'inherit',
    });

    const actualIosChecksum = execSync(
      'shasum -a 256 ios-artifacts.zip | cut -d" " -f1',
      { cwd: packageRoot, encoding: 'utf-8' }
    ).trim();

    if (actualIosChecksum !== iosChecksum) {
      throw new Error(
        `iOS artifacts checksum mismatch. Expected: ${iosChecksum}, Got: ${actualIosChecksum}`
      );
    }

    execSync('unzip -o ios-artifacts.zip && rm -rf ios-artifacts.zip', {
      cwd: packageRoot,
      stdio: 'inherit',
    });
    console.log('iOS artifacts downloaded successfully.');
  } catch (error) {
    execSync('rm -f ios-artifacts.zip', { cwd: packageRoot });
    console.error('Failed to download or verify iOS artifacts');
    throw error;
  }
}
