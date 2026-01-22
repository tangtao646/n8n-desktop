// scripts/sync-version.js
const fs = require('fs');
const path = require('path');

// 1. 获取 package.json 中的版本号
const pkgPath = path.resolve(__dirname, '../package.json');
const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
const version = pkg.version;

// 2. 更新 Cargo.toml
const cargoPath = path.resolve(__dirname, '../src-tauri/Cargo.toml');
if (fs.existsSync(cargoPath)) {
    let cargo = fs.readFileSync(cargoPath, 'utf8');
    cargo = cargo.replace(/^version = ".*"/m, `version = "${version}"`);
    fs.writeFileSync(cargoPath, cargo);
    console.log(`✅ Cargo.toml 已同步为: ${version}`);
}

// 3. 更新 tauri.conf.json
const tauriPath = path.resolve(__dirname, '../src-tauri/tauri.conf.json');
if (fs.existsSync(tauriPath)) {
    const tauri = JSON.parse(fs.readFileSync(tauriPath, 'utf8'));
    tauri.version = version;
    fs.writeFileSync(tauriPath, JSON.stringify(tauri, null, 2));
    console.log(`✅ tauri.conf.json 已同步为: ${version}`);
}