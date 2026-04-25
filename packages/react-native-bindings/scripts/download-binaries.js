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
const iosFrameworkCheck = path.join(__dirname, '../FedimintReactNativeBindingsFramework.xcframework');

if (fs.existsSync(iosFrameworkCheck)) {
    console.log('iOS binaries already exist. Skipping download.');
    process.exit(0);
}

const IOS_CHECKSUM = pkg.checksums ? pkg.checksums.ios : null;

const REPO = 'https://github.com/fedimint/fedimint-sdk';
let TAG = `react-native-v${pkg.version}`;
if (pkg.version.includes('canary')) {
    TAG = 'canary';
} else if (pkg.version.includes('-') || pkg.version.includes('snapshot')) {
    TAG = 'snapshot';
}

if (!IOS_CHECKSUM) {
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
    const iosUrl = `${REPO}/releases/download/${TAG}/ios-artifacts.zip`;

    console.log(`Downloading iOS artifacts from ${iosUrl}...`);
    await downloadFile(iosUrl, 'ios-artifacts.zip');

    if (!verifyChecksum('ios-artifacts.zip', IOS_CHECKSUM)) {
        console.warn('WARNING: iOS checksum mismatch! Proceeding anyway...');
    }

    console.log('Extracting iOS artifacts...');
    unzip('ios-artifacts.zip', path.join(__dirname, '../'));
    fs.unlinkSync('ios-artifacts.zip');

    console.log('Binaries downloaded and installed successfully.');
};

main().catch(err => {
    console.error('Error downloading binaries:', err);
    process.exit(1);
});
