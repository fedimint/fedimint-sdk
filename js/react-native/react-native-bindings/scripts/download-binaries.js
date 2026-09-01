const fs = require('fs');
const path = require('path');
const https = require('https');
const { execSync } = require('child_process');
const crypto = require('crypto');
const { pipeline, PassThrough } = require('stream');

const pkg = require('../package.json');

// Check if we should skip via environment variable
if (process.env.EXPO_PUBLIC_SKIP_POSTINSTALL || process.env.FEDIMINT_SKIP_BINARY_DOWNLOAD === 'true') {
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
const iosFrameworkCheck = path.join(__dirname, '../FedimintReactNativeBindingsFramework.xcframework');

const isCanary = pkg.version.includes('canary');
const isSnapshot = pkg.version.includes('-') || pkg.version.includes('snapshot');
const isMutableRelease = isCanary || isSnapshot;

if (!isMutableRelease && fs.existsSync(androidLibCheck) && fs.existsSync(iosFrameworkCheck)) {
    console.log('Binaries already present, skipping download.');
    process.exit(0);
}

if (isMutableRelease) {
    if (fs.existsSync(androidLibCheck)) {
        fs.rmSync(androidLibCheck, { recursive: true, force: true });
    }
    if (fs.existsSync(iosFrameworkCheck)) {
        fs.rmSync(iosFrameworkCheck, { recursive: true, force: true });
    }
}

const REPO = 'https://github.com/fedimint/fedimint-sdk';
let TAG = `react-native-v${pkg.version}`;
if (pkg.version.includes('canary')) {
    TAG = 'canary';
} else if (pkg.version.includes('-') || pkg.version.includes('snapshot')) {
    TAG = 'snapshot';
}
const ANDROID_CHECKSUM = pkg.checksums ? pkg.checksums.android : null;
const IOS_CHECKSUM = pkg.checksums ? pkg.checksums.ios : null;

if (!ANDROID_CHECKSUM || !IOS_CHECKSUM) {
    console.warn("Checksums not found in package.json. Skipping download (assume local dev or first install).");
    process.exit(0);
}

const downloadAndVerify = (url, dest, expectedChecksum) => {
    return new Promise((resolve, reject) => {
        const follow = (currentUrl) => {
            https.get(currentUrl, (response) => {
                if (response.statusCode === 302 || response.statusCode === 301) {
                    follow(response.headers.location);
                    return;
                }
                const file = fs.createWriteStream(dest);
                const hash = crypto.createHash('sha256');
                const passThrough = new PassThrough();
                
                passThrough.on('data', (chunk) => {
                    hash.update(chunk);
                });

                pipeline(
                    response,
                    passThrough,
                    file,
                    (err) => {
                        if (err) {
                            fs.unlink(dest, () => { });
                            reject(err);
                        } else {
                            const actualChecksum = hash.digest('hex');
                            if (expectedChecksum && actualChecksum !== expectedChecksum) {
                                fs.unlinkSync(dest);
                                reject(new Error(`Checksum mismatch for ${dest}. Expected: ${expectedChecksum}, Got: ${actualChecksum}`));
                            } else {
                                if (expectedChecksum) {
                                    console.log(`${dest} checksum verified successfully.`);
                                }
                                resolve();
                            }
                        }
                    }
                );
            }).on('error', (err) => {
                fs.unlink(dest, () => { });
                reject(err);
            });
        };
        follow(url);
    });
};

const unzip = (file, dest) => {
    try {
        if (process.platform === 'win32') {
            execSync(`tar -xf "${file}" -C "${dest}"`);
        } else {
            execSync(`unzip -o "${file}" -d "${dest}"`);
        }
    } catch (e) {
        console.error(`Failed to unzip ${file}: ${e.message}`);
        throw e;
    }
};

const main = async () => {
    const androidUrl = `${REPO}/releases/download/${TAG}/android-artifacts.zip`;
    const iosUrl = `${REPO}/releases/download/${TAG}/ios-artifacts.zip`;

    console.log(`Downloading Android artifacts from ${androidUrl}...`);
    await downloadAndVerify(androidUrl, 'android-artifacts.zip', ANDROID_CHECKSUM);

    console.log('Extracting Android artifacts...');
    // Adjust destination if needed based on zip structure
    unzip('android-artifacts.zip', path.join(__dirname, '../'));
    fs.unlinkSync('android-artifacts.zip');

    console.log(`Downloading iOS artifacts from ${iosUrl}...`);
    await downloadAndVerify(iosUrl, 'ios-artifacts.zip', IOS_CHECKSUM);

    console.log('Extracting iOS artifacts...');
    unzip('ios-artifacts.zip', path.join(__dirname, '../'));
    fs.unlinkSync('ios-artifacts.zip');

    console.log('Binaries downloaded and installed successfully.');
};

main().catch(err => {
    console.error('Error downloading binaries:', err);
    process.exit(1);
});