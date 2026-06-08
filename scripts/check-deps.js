import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const rootDir = path.join(__dirname, '..');

// Read tauri.conf.json
const tauriConfPath = path.join(rootDir, 'src-tauri', 'tauri.conf.json');
const tauriConf = JSON.parse(fs.readFileSync(tauriConfPath, 'utf8'));
const runtimeDeps = tauriConf.bundle?.linux?.deb?.depends || [];

// Read setup.sh
const setupShPath = path.join(rootDir, 'scripts', 'setup.sh');
const setupShContent = fs.readFileSync(setupShPath, 'utf8');

// Map runtime libraries to their corresponding dev/compilation packages
const runtimeToDevMap = {
    'libwebkit2gtk-4.1-0': 'libwebkit2gtk-4.1-dev',
    'libgtk-3-0': 'libgtk-3-dev',
    'libayatana-appindicator3-1': 'libayatana-appindicator3-dev',
    'librsvg2-2': 'librsvg2-dev',
    'libasound2': 'libasound2-dev',
    'libssl3': 'libssl-dev'
};

let hasErrors = false;

console.log('Checking system dependency alignment...');

runtimeDeps.forEach(dep => {
    const expectedDevDep = runtimeToDevMap[dep] || dep;
    // Check if the expected dev dependency is present in setup.sh
    // Simple regex check to see if the package is in the apt-get install block
    const isPresent = setupShContent.includes(expectedDevDep);

    if (isPresent) {
        console.log(`✓ Runtime '${dep}' matches build-time '${expectedDevDep}' in setup.sh`);
    } else {
        console.error(`✗ Missing: Runtime dependency '${dep}' requires '${expectedDevDep}' in scripts/setup.sh, but it was not found.`);
        hasErrors = true;
    }
});

if (hasErrors) {
    console.error('\nDependency alignment check failed! Please synchronize scripts/setup.sh and src-tauri/tauri.conf.json.');
    process.exit(1);
} else {
    console.log('\nAll dependencies are perfectly aligned!');
    process.exit(0);
}
