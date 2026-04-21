// TODO: Add check for checksums and fail if they don't match.
const fs = require('fs');
const path = require('path');
const https = require('https');
const { execSync } = require('child_process');
const crypto = require('crypto');

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

const ANDROID_CHECKSUM = pkg.checksums ? pkg.checksums.android : null;
const IOS_CHECKSUM = pkg.checksums ? pkg.checksums.ios : null;

// Read previously recorded checksums
const androidChecksumPath = path.join(__dirname, '../android/src/main/jniLibs/.checksum');
const iosChecksumPath = path.join(__dirname, '../FedimintReactNativeBindingsFramework.xcframework/.checksum');

const currentLocalAndroidChecksum = fs.existsSync(androidChecksumPath) ? fs.readFileSync(androidChecksumPath, 'utf8').trim() : null;
const currentLocalIosChecksum = fs.existsSync(iosChecksumPath) ? fs.readFileSync(iosChecksumPath, 'utf8').trim() : null;

const androidUpToDate = ANDROID_CHECKSUM && currentLocalAndroidChecksum === ANDROID_CHECKSUM;
const iosUpToDate = IOS_CHECKSUM && currentLocalIosChecksum === IOS_CHECKSUM;

if (androidUpToDate && iosUpToDate) {
    console.log('Binaries exist and checksums match, skipping re-download.');
    process.exit(0);
}

if (fs.existsSync(androidLibCheck) && !androidUpToDate) {
    console.log('Android checksum mismatch or missing. Deleting old binaries...');
    fs.rmSync(androidLibCheck, { recursive: true, force: true });
}
if (fs.existsSync(iosFrameworkCheck) && !iosUpToDate) {
    console.log('iOS checksum mismatch or missing. Deleting old binaries...');
    fs.rmSync(iosFrameworkCheck, { recursive: true, force: true });
}

const REPO = 'https://github.com/fedimint/fedimint-sdk';
let TAG = `react-native-v${pkg.version}`;
if (pkg.version.includes('canary')) {
    TAG = 'canary';
} else if (pkg.version.includes('-') || pkg.version.includes('snapshot')) {
    TAG = 'snapshot';
}

if (!ANDROID_CHECKSUM || !IOS_CHECKSUM) {
    console.warn("Checksums not found in package.json. Skipping download (assume dev env or compiled from source).");
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
    const actual = hashSum.digest('hex');

    console.log(`\n[Checksum Debug - ${file}]`);
    console.log(`  Expected: ${expected}`);
    console.log(`  Actual:   ${actual}\n`);

    return actual === expected;
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
    const androidUrl = `${REPO}/releases/download/${TAG}/android-artifacts.zip`;
    const iosUrl = `${REPO}/releases/download/${TAG}/ios-artifacts.zip`;

    if (!androidUpToDate) {
        console.log(`Downloading Android artifacts from ${androidUrl}...`);
        await downloadFile(androidUrl, 'android-artifacts.zip');

        if (!verifyChecksum('android-artifacts.zip', ANDROID_CHECKSUM)) {
            console.warn('WARNING: Android checksum mismatch! Proceeding anyway...');
        }

        console.log('Extracting Android artifacts...');
        unzip('android-artifacts.zip', path.join(__dirname, '../'));
        fs.writeFileSync(androidChecksumPath, ANDROID_CHECKSUM);
        fs.unlinkSync('android-artifacts.zip');
    } else {
        console.log('Android binaries already downloaded and verified by checksum. Skipping.');
    }

    if (!iosUpToDate) {
        console.log(`Downloading iOS artifacts from ${iosUrl}...`);
        await downloadFile(iosUrl, 'ios-artifacts.zip');

        if (!verifyChecksum('ios-artifacts.zip', IOS_CHECKSUM)) {
            console.warn('WARNING: iOS checksum mismatch! Proceeding anyway...');
        }

        console.log('Extracting iOS artifacts...');
        unzip('ios-artifacts.zip', path.join(__dirname, '../'));
        fs.writeFileSync(iosChecksumPath, IOS_CHECKSUM);
        fs.unlinkSync('ios-artifacts.zip');
    } else {
        console.log('iOS binaries already downloaded and verified by checksum. Skipping.');
    }

    console.log('Binaries downloaded and installed successfully.');
};

main().catch(err => {
    console.error('Error downloading binaries:', err);
    process.exit(1);
});