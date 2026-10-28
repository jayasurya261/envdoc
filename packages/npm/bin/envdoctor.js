#!/usr/bin/env node

const { spawnSync } = require('child_process');
const path = require('path');
const os = require('os');
const fs = require('fs');
const https = require('https');

const pkgJson = require('../package.json');
const VERSION = pkgJson.version || '0.1.2';

const SUPPORTED_TARGETS = {
  'win32-x64': {
    pkg: 'envdoc-cli-win32-x64',
    altPkg: '@envdoctor/cli-win32-x64',
    binName: 'envdoctor.exe',
    triple: 'x86_64-pc-windows-msvc',
  },
  'linux-x64': {
    pkg: 'envdoc-cli-linux-x64',
    altPkg: '@envdoctor/cli-linux-x64',
    binName: 'envdoctor',
    triple: 'x86_64-unknown-linux-gnu',
  },
  'darwin-arm64': {
    pkg: 'envdoc-cli-darwin-arm64',
    altPkg: '@envdoctor/cli-darwin-arm64',
    binName: 'envdoctor',
    triple: 'aarch64-apple-darwin',
  },
  'darwin-x64': {
    pkg: 'envdoc-cli-darwin-x64',
    altPkg: '@envdoctor/cli-darwin-x64',
    binName: 'envdoctor',
    triple: 'x86_64-apple-darwin',
  },
};

const platform = os.platform();
const arch = os.arch();
const targetKey = `${platform}-${arch}`;
const targetInfo = SUPPORTED_TARGETS[targetKey];

if (!targetInfo) {
  console.error(`[envdoctor] Fatal Error: Unsupported operating system or CPU architecture: ${platform} (${arch}).`);
  console.error('[envdoctor] Officially supported platforms:');
  console.error('  - Linux x64 (linux-x64)');
  console.error('  - Windows x64 (win32-x64)');
  console.error('  - macOS Apple Silicon (darwin-arm64)');
  console.error('  - macOS Intel (darwin-x64)');
  console.error('\n[envdoctor] To compile and install envdoctor directly from source, run:');
  console.error('  cargo install envdoc');
  process.exit(1);
}

const ext = platform === 'win32' ? '.exe' : '';
const binName = `envdoctor${ext}`;

let binPath = null;

// 1. Check local bundled bin directory
const localBin = path.join(__dirname, binName);
if (fs.existsSync(localBin)) {
  binPath = localBin;
}

// 2. Check platform-specific optional package
if (!binPath) {
  for (const pkgName of [targetInfo.pkg, targetInfo.altPkg]) {
    try {
      const pkgPath = require.resolve(`${pkgName}/package.json`);
      const candidate = path.join(path.dirname(pkgPath), 'bin', binName);
      if (fs.existsSync(candidate)) {
        binPath = candidate;
        break;
      }
    } catch (_) {
      // Optional dependency not installed
    }
  }
}

// 3. Fallback: Check local user cache or download from GitHub release
if (!binPath) {
  const cacheBase = process.env.XDG_CACHE_HOME || (
    platform === 'win32'
      ? process.env.LOCALAPPDATA || path.join(os.homedir(), 'AppData', 'Local')
      : path.join(os.homedir(), '.cache')
  );
  const cacheDir = path.join(cacheBase, 'envdoctor', `v${VERSION}`);
  const cachedBinary = path.join(cacheDir, binName);

  if (fs.existsSync(cachedBinary)) {
    binPath = cachedBinary;
  } else {
    // Attempt automatic download from GitHub Releases
    const downloadUrl = `https://github.com/jayasurya261/envdoc/releases/download/v${VERSION}/envdoctor-${targetInfo.triple}${ext}`;
    try {
      fs.mkdirSync(cacheDir, { recursive: true });
      process.stderr.write(`[envdoctor] Fetching native binary for ${platform}-${arch} from GitHub Releases...\n`);
      
      const curlRes = spawnSync('curl', ['-fL', '-o', cachedBinary, downloadUrl], { stdio: 'inherit' });
      if (curlRes.status === 0 && fs.existsSync(cachedBinary)) {
        if (platform !== 'win32') {
          fs.chmodSync(cachedBinary, 0o755);
        }
        binPath = cachedBinary;
      }
    } catch (_) {}
  }
}

// 4. Fail with actionable diagnostic message if binary could not be resolved
if (!binPath) {
  console.error(`[envdoctor] Error: Native binary could not be found for ${platform}-${arch}.`);
  console.error('[envdoctor] To resolve this, install via any of:');
  console.error(`  1. Install platform package: npm install ${targetInfo.pkg}`);
  console.error('  2. Install via Rust Cargo: cargo install envdoc');
  console.error(`  3. Download manually from https://github.com/jayasurya261/envdoc/releases/tag/v${VERSION}`);
  process.exit(1);
}

// 5. Execute native binary forwarding all arguments and I/O streams
const result = spawnSync(binPath, process.argv.slice(2), {
  stdio: 'inherit',
});

if (result.error) {
  console.error('[envdoctor] Execution error:', result.error.message);
  process.exit(1);
}

process.exit(result.status ?? 0);
