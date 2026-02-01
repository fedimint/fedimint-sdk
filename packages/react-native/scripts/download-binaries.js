const fs = require('fs');
const path = require('path');
const https = require('https');
const { execSync } = require('child_process');
const crypto = require('crypto');

const pkg = require('../package.json');

// Check if we should skip
if (process.env.EXPO_PUBLIC_SKIP_POSTINSTALL) {
    console.log('Skipping postinstall due to EXPO_PUBLIC_SKIP_POSTINSTALL');
    process.exit(0);
}

// Check if artifacts already exist (simple check)
const androidLibCheck = path.join(__dirname, '../android/src/main/jniLibs');
const iosFrameworkCheck = path.join(__dirname, '../ios/FedimintReactNativeFramework.xcframework');

if (fs.existsSync(androidLibCheck) && fs.existsSync(iosFrameworkCheck)) {
    console.log('Binaries already present, skipping download.');
    process.exit(0);
}

const REPO = 'https://github.com/fedimint/fedimint-sdk';
const TAG = pkg.version;
const ANDROID_CHECKSUM = pkg.checksums ? pkg.checksums.android : null;
const IOS_CHECKSUM = pkg.checksums ? pkg.checksums.ios : null;

if (!ANDROID_CHECKSUM || !IOS_CHECKSUM) {
    console.warn("Checksums not found in package.json. Skipping download (assume local dev or first install).");
    process.exit(0);
}

const downloadFile = (url, dest) => {
    return new Promise((resolve, reject) => {
        const file = fs.createWriteStream(dest);
        https.get(url, (response) => {
            if (response.statusCode === 302 || response.statusCode === 301) {
                downloadFile(response.headers.location, dest).then(resolve).catch(reject);
                return;
            }
            response.pipe(file);
            file.on('finish', () => {
                file.close(resolve);
            });
        }).on('error', (err) => {
            fs.unlink(dest, () => { });
            reject(err);
        });
    });
};

const verifyChecksum = (file, expected) => {
    const fileBuffer = fs.readFileSync(file);
    const hashSum = crypto.createHash('sha256');
    hashSum.update(fileBuffer);
    const hex = hashSum.digest('hex');
    return hex === expected;
};

const unzip = (file, dest) => {
    try {
        execSync(`unzip -o ${file} -d ${dest}`);
    } catch (e) {
        console.error(`Failed to unzip ${file}: ${e.message}`);
        throw e;
    }
};

const main = async () => {
    const androidUrl = `${REPO}/releases/download/v${TAG}/android-artifacts.zip`; // Note the 'v' prefix if your tags have it
    const iosUrl = `${REPO}/releases/download/v${TAG}/ios-artifacts.zip`;

    console.log(`Downloading Android artifacts from ${androidUrl}...`);
    await downloadFile(androidUrl, 'android-artifacts.zip');

    if (!verifyChecksum('android-artifacts.zip', ANDROID_CHECKSUM)) {
        console.error('Android checkum mismatch!');
        process.exit(1);
    }

    console.log('Extracting Android artifacts...');
    // Adjust destination if needed based on zip structure
    unzip('android-artifacts.zip', path.join(__dirname, '../'));
    fs.unlinkSync('android-artifacts.zip');


    console.log(`Downloading iOS artifacts from ${iosUrl}...`);
    await downloadFile(iosUrl, 'ios-artifacts.zip');

    if (!verifyChecksum('ios-artifacts.zip', IOS_CHECKSUM)) {
        console.error('iOS checkum mismatch!');
        process.exit(1);
    }

    console.log('Extracting iOS artifacts...');
    unzip('ios-artifacts.zip', path.join(__dirname, '../'));
    fs.unlinkSync('ios-artifacts.zip');

    console.log('Binaries downloaded and installed successfully.');
};

main().catch(err => {
    console.error('Error downloading binaries:', err);
    // Don't fail install in case of network error, just warn? 
    // Or fail strictly? The user requested script did `exit 1` on checksum fail, so we might want to fail.
    process.exit(1);
});
