import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const rootDir = path.join(__dirname, '..');

// 1. Read version from package.json
const pkgPath = path.join(rootDir, 'package.json');
const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
const version = pkg.version;

console.log(`Syncing version ${version} to all config files...`);

// 2. Update tauri.conf.json
const tauriConfPath = path.join(rootDir, 'src-tauri', 'tauri.conf.json');
if (fs.existsSync(tauriConfPath)) {
    const tauriConf = JSON.parse(fs.readFileSync(tauriConfPath, 'utf8'));
    tauriConf.version = version;
    fs.writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + '\n');
    console.log(`- Updated src-tauri/tauri.conf.json`);
}

// 3. Update Cargo.toml files
const updateCargoToml = (filePath) => {
    if (!fs.existsSync(filePath)) return;
    let content = fs.readFileSync(filePath, 'utf8');
    // Replace version = "x.y.z" only in the [package] section
    // This is a bit naive but works for our simple Cargo.toml files
    content = content.replace(/^version = ".*"/m, `version = "${version}"`);
    fs.writeFileSync(filePath, content);
    console.log(`- Updated ${path.relative(rootDir, filePath)}`);
};

updateCargoToml(path.join(rootDir, 'src-tauri', 'Cargo.toml'));
updateCargoToml(path.join(rootDir, 'src-tauri', 'boop-core', 'Cargo.toml'));

// 4. Update Cargo.lock
import { execSync } from 'child_process';
console.log('- Updating Cargo.lock...');
try {
    const tauriDir = path.join(rootDir, 'src-tauri');
    execSync('cargo update -p app', { cwd: tauriDir });
    execSync('cargo update -p boop-core', { cwd: tauriDir });
    console.log('- Updated src-tauri/Cargo.lock');
} catch (err) {
    console.error('Failed to update Cargo.lock:', err.message);
}

console.log('Version sync complete.');
