const fs = require('fs');
const path = require('path');
const https = require('https');
const http = require('http');
const { execSync } = require('child_process');
const crypto = require('crypto');

const pkg = require('../package.json');

// Check if we should skip via environment variable
if (
  process.env.EXPO_PUBLIC_SKIP_POSTINSTALL ||
  process.env.FEDIMINT_SKIP_BINARY_DOWNLOAD === 'true'
) {
  console.log('Skipping postinstall due to environment variable');
  process.exit(0);
}

// Check if skipBinaryDownload is set in package.json
if (pkg.skipBinaryDownload === true) {
  console.log('Skipping postinstall due to skipBinaryDownload in package.json');
  process.exit(0);
}

// Check if artifacts already exist (simple check)
const androidLibCheck = path.join(__dirname, '../android/src/main/jniLibs');
const iosFrameworkCheck = path.join(
  __dirname,
  '../FedimintReactNativeBindingsFramework.xcframework'
);

if (fs.existsSync(androidLibCheck) && fs.existsSync(iosFrameworkCheck)) {
  console.log('Binaries already present, skipping download.');
  process.exit(0);
}

const REPO = 'https://github.com/fedimint/fedimint-sdk';
const isSnapshot =
  pkg.version.includes('snapshot') ||
  pkg.version.includes('canary');
let TAG = isSnapshot ? 'snapshot' : `react-native-v${pkg.version}`;

const ANDROID_CHECKSUM = pkg.checksums ? pkg.checksums.android : null;
const IOS_CHECKSUM = pkg.checksums ? pkg.checksums.ios : null;

if (!ANDROID_CHECKSUM || !IOS_CHECKSUM) {
  console.warn(
    'Checksums not found in package.json. Skipping download (assume local dev or first install).'
  );
  process.exit(0);
}

const MAX_RETRIES = 3;
const RETRY_DELAY_MS = 2000;

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const downloadFile = (url, dest, attempt = 1) => {
  return new Promise((resolve, reject) => {
    const client = url.startsWith('https') ? https : http;

    // Remove any stale partial file
    if (fs.existsSync(dest)) {
      fs.unlinkSync(dest);
    }

    const file = fs.createWriteStream(dest);
    client
      .get(url, { headers: { 'Cache-Control': 'no-cache' } }, (response) => {
        // Follow redirects (GitHub releases redirect to S3/CDN)
        if (response.statusCode === 302 || response.statusCode === 301) {
          file.close();
          fs.unlinkSync(dest);
          downloadFile(response.headers.location, dest, attempt)
            .then(resolve)
            .catch(reject);
          return;
        }

        if (response.statusCode !== 200) {
          file.close();
          fs.unlinkSync(dest);
          reject(
            new Error(
              `HTTP ${response.statusCode} when downloading ${url}`
            )
          );
          return;
        }

        response.pipe(file);
        file.on('finish', () => {
          file.close(resolve);
        });
      })
      .on('error', (err) => {
        file.close();
        if (fs.existsSync(dest)) fs.unlinkSync(dest);
        reject(err);
      });
  });
};

const verifyChecksum = (file, expected) => {
  const fileBuffer = fs.readFileSync(file);
  const hashSum = crypto.createHash('sha256');
  hashSum.update(fileBuffer);
  const actual = hashSum.digest('hex');

  if (actual !== expected) {
    console.error(`Checksum mismatch!`);
    console.error(`  Expected: ${expected}`);
    console.error(`  Actual:   ${actual}`);
    return false;
  }
  return true;
};

const unzip = (file, dest) => {
  try {
    execSync(`unzip -o "${file}" -d "${dest}"`, { stdio: 'pipe' });
  } catch (e) {
    console.error(`Failed to unzip ${file}: ${e.message}`);
    throw e;
  }
};

const downloadAndVerify = async (url, dest, expectedChecksum, label) => {
  for (let attempt = 1; attempt <= MAX_RETRIES; attempt++) {
    try {
      console.log(
        `Downloading ${label} artifacts (attempt ${attempt}/${MAX_RETRIES}) from ${url}...`
      );
      await downloadFile(url, dest, attempt);

      if (verifyChecksum(dest, expectedChecksum)) {
        console.log(`${label} checksum verified ✓`);
        return true;
      }

      console.warn(
        `${label} checksum mismatch on attempt ${attempt}/${MAX_RETRIES}`
      );

      // Clean up bad download
      if (fs.existsSync(dest)) fs.unlinkSync(dest);

      if (attempt < MAX_RETRIES) {
        console.log(`Retrying in ${RETRY_DELAY_MS / 1000}s...`);
        await sleep(RETRY_DELAY_MS);
      }
    } catch (err) {
      console.error(`Download error on attempt ${attempt}: ${err.message}`);
      if (fs.existsSync(dest)) fs.unlinkSync(dest);

      if (attempt < MAX_RETRIES) {
        console.log(`Retrying in ${RETRY_DELAY_MS / 1000}s...`);
        await sleep(RETRY_DELAY_MS);
      }
    }
  }

  return false;
};

const main = async () => {
  const androidUrl = `${REPO}/releases/download/${TAG}/android-artifacts.zip`;
  const iosUrl = `${REPO}/releases/download/${TAG}/ios-artifacts.zip`;
  const baseDir = path.join(__dirname, '../');

  // --- Android ---
  const androidOk = await downloadAndVerify(
    androidUrl,
    'android-artifacts.zip',
    ANDROID_CHECKSUM,
    'Android'
  );

  if (!androidOk) {
    if (isSnapshot) {
      console.warn(
        'WARNING: Android checksum verification failed after all retries.'
      );
      console.warn(
        'This is a snapshot version — the release artifacts may have been overwritten by a newer CI run.'
      );
      console.warn(
        'Re-run pnpm install, or set FEDIMINT_SKIP_BINARY_DOWNLOAD=true to skip.'
      );
    }
    console.error('Android artifact checksum verification failed!');
    process.exit(1);
  }

  console.log('Extracting Android artifacts...');
  unzip('android-artifacts.zip', baseDir);
  fs.unlinkSync('android-artifacts.zip');

  // --- iOS ---
  const iosOk = await downloadAndVerify(
    iosUrl,
    'ios-artifacts.zip',
    IOS_CHECKSUM,
    'iOS'
  );

  if (!iosOk) {
    if (isSnapshot) {
      console.warn(
        'WARNING: iOS checksum verification failed after all retries.'
      );
      console.warn(
        'This is a snapshot version — the release artifacts may have been overwritten by a newer CI run.'
      );
      console.warn(
        'Re-run pnpm install, or set FEDIMINT_SKIP_BINARY_DOWNLOAD=true to skip.'
      );
    }
    console.error('iOS artifact checksum verification failed!');
    process.exit(1);
  }

  console.log('Extracting iOS artifacts...');
  unzip('ios-artifacts.zip', baseDir);
  fs.unlinkSync('ios-artifacts.zip');

  console.log('Binaries downloaded and installed successfully. ✓');
};

main().catch((err) => {
  console.error('Error downloading binaries:', err);
  process.exit(1);
});