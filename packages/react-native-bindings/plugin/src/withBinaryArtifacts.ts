import type { ConfigPlugin } from '@expo/config-plugins';
import * as path from 'path';
import * as fs from 'fs';
import { execSync } from 'child_process';

const PACKAGE_NAME = '@fedimint/react-native-bindings';
const GITHUB_REPO = 'https://github.com/fedimint/fedimint-sdk';

/**
 * Downloads prebuilt binary artifacts for Android and iOS
 * This runs synchronously during expo prebuild to ensure binaries are available
 */
export const withBinaryArtifacts: ConfigPlugin = (config) => {
  try {
    downloadBinaryArtifacts();
  } catch (error) {
    console.warn('Failed to download Fedimint SDK binary artifacts:', error);
    console.warn(
      'You may need to run the postinstall script manually or check your network connection.'
    );
  }
  return config;
};

function downloadBinaryArtifacts(): void {
  const packageRoot = findPackageRoot();
  if (!packageRoot) {
    throw new Error(`Could not find ${PACKAGE_NAME} package`);
  }

  // Check if artifacts already exist
  const androidLibsPath = path.join(packageRoot, 'android/src/main/jniLibs');
  const iosFrameworkPath = path.join(
    packageRoot,
    'FedimintReactNativeBindingsFramework.xcframework'
  );

  const packageJsonPath = path.join(packageRoot, 'package.json');
  const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf-8'));
  const version = packageJson.version;
  const isCanary = version.includes('canary');
  const isSnapshot = version.includes('-') || version.includes('snapshot');
  const isMutableRelease = isCanary || isSnapshot;

  if (!isMutableRelease && fs.existsSync(androidLibsPath) && fs.existsSync(iosFrameworkPath)) {
    console.log('Fedimint SDK binary artifacts already exist, skipping download.');
    return;
  }

  if (isMutableRelease) {
    if (fs.existsSync(androidLibsPath)) {
        fs.rmSync(androidLibsPath, { recursive: true, force: true });
    }
    if (fs.existsSync(iosFrameworkPath)) {
        fs.rmSync(iosFrameworkPath, { recursive: true, force: true });
    }
  }
  
  const androidChecksum = packageJson.checksums?.android;
  const iosChecksum = packageJson.checksums?.ios;

  if (!androidChecksum || !iosChecksum) {
    console.warn('Binary checksums not found in package.json, skipping verification.');
  }

  // Determine release tag based on version
  let releaseTag = `react-native-v${version}`;
  if (isCanary) {
    releaseTag = 'canary';
  } else if (version.includes('-') || version.includes('snapshot')) {
    releaseTag = 'snapshot';
  }
  const androidUrl = `${GITHUB_REPO}/releases/download/${releaseTag}/android-artifacts.zip`;
  const iosUrl = `${GITHUB_REPO}/releases/download/${releaseTag}/ios-artifacts.zip`;

  // Download and verify Android artifacts
  try {
    console.log('Downloading Fedimint SDK Android artifacts...');
    execSync(`curl -L "${androidUrl}" --output android-artifacts.zip`, {
      cwd: packageRoot,
      stdio: 'inherit',
    });

    if (androidChecksum) {
      const actualAndroidChecksum = execSync(
        'shasum -a 256 android-artifacts.zip | cut -d" " -f1',
        { cwd: packageRoot, encoding: 'utf-8' }
      ).trim();

      if (actualAndroidChecksum !== androidChecksum) {
        throw new Error(
          `Android artifacts checksum mismatch. Expected: ${androidChecksum}, Got: ${actualAndroidChecksum}`
        );
      }
    }

    execSync('unzip -o android-artifacts.zip && rm -rf android-artifacts.zip', {
      cwd: packageRoot,
      stdio: 'inherit',
    });
    console.log('Android artifacts downloaded successfully.');
  } catch (error) {
    execSync('rm -f android-artifacts.zip', { cwd: packageRoot });
    console.error('Failed to download or verify Android artifacts');
    throw error;
  }

  // Download and verify iOS artifacts
  try {
    console.log('Downloading Fedimint SDK iOS artifacts...');
    execSync(`curl -L "${iosUrl}" --output ios-artifacts.zip`, {
      cwd: packageRoot,
      stdio: 'inherit',
    });

    if (iosChecksum) {
      const actualIosChecksum = execSync(
        'shasum -a 256 ios-artifacts.zip | cut -d" " -f1',
        { cwd: packageRoot, encoding: 'utf-8' }
      ).trim();

      if (actualIosChecksum !== iosChecksum) {
        throw new Error(
          `iOS artifacts checksum mismatch. Expected: ${iosChecksum}, Got: ${actualIosChecksum}`
        );
      }
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

function findPackageRoot(): string | null {
  // Try to find from node_modules
  try {
    const resolved = require.resolve(`${PACKAGE_NAME}/package.json`);
    return path.dirname(resolved);
  } catch {
    // Fall back to searching from __dirname
  }

  let currentDir = __dirname;

  // Walk up the directory tree to find the package root
  while (currentDir !== path.dirname(currentDir)) {
    const packageJsonPath = path.join(currentDir, 'package.json');

    if (fs.existsSync(packageJsonPath)) {
      try {
        const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf-8'));
        if (packageJson.name === PACKAGE_NAME) {
          return currentDir;
        }
      } catch {
        // Continue searching
      }
    }

    currentDir = path.dirname(currentDir);
  }

  return null;
}
