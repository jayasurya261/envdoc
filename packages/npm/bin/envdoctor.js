#!/usr/bin/env node

const { spawnSync } = require('child_process');
const path = require('path');
const os = require('os');
const fs = require('fs');

const SUPPORTED_TARGETS = {
  'win32-x64': {
    pkg: '@envdoctor/cli-win32-x64',
    binName: 'envdoctor.exe',
    triple: 'x86_64-pc-windows-msvc',
  },
  'linux-x64': {
    pkg: '@envdoctor/cli-linux-x64',
    binName: 'envdoctor',
    triple: 'x86_64-unknown-linux-gnu',
  },
  'darwin-arm64': {
    pkg: '@envdoctor/cli-darwin-arm64',
    binName: 'envdoctor',
    triple: 'aarch64-apple-darwin',
  },
  'darwin-x64': {
    pkg: '@envdoctor/cli-darwin-x64',
    binName: 'envdoctor',
    triple: 'x86_64-apple-darwin',
  },
};

const platform = os.platform();
const arch = os.arch();
const targetKey = `${platform}-${arch}`;
const targetInfo = SUPPORTED_TARGETS[targetKey];

// 1. Fail loudly if the current platform / architecture is not supported
if (!targetInfo) {
  console.error(`[envdoctor] Fatal Error: Unsupported operating system or CPU architecture: ${platform} (${arch}).`);
  console.error('[envdoctor] Officially supported platforms:');
  console.error('  - Windows x64 (win32-x64)');
  console.error('  - Linux x64 (linux-x64)');
  console.error('  - macOS Apple Silicon (darwin-arm64)');
  console.error('  - macOS Intel (darwin-x64)');
  console.error('\n[envdoctor] To compile and install envdoctor directly from source, run:');
  console.error('  cargo install envdoc');
  process.exit(1);
}

const ext = platform === 'win32' ? '.exe' : '';
const binName = `envdoctor${ext}`;

let binPath = null;

// 2. Check local bundled bin directory
const localBin = path.join(__dirname, binName);
if (fs.existsSync(localBin)) {
  binPath = localBin;
}

// 3. Check platform-specific optional package
if (!binPath) {
  try {
    const pkgPath = require.resolve(`${targetInfo.pkg}/package.json`);
    const candidate = path.join(path.dirname(pkgPath), 'bin', binName);
    if (fs.existsSync(candidate)) {
      binPath = candidate;
    }
  } catch (_) {
    // Optional dependency not installed
  }
}

// 4. Fail loudly with diagnostic message if binary could not be resolved
if (!binPath) {
  console.error(`[envdoctor] Error: Native binary could not be found for ${platform}-${arch}.`);
  console.error(`[envdoctor] Checked locations:`);
  console.error(`  - Local: ${localBin}`);
  console.error(`  - Optional package: ${targetInfo.pkg}`);
  console.error('\n[envdoctor] If you installed with `--no-optional` or npm failed to install optional dependencies,');
  console.error(`[envdoctor] install the platform package manually:\n  npm install ${targetInfo.pkg}`);
  console.error('\n[envdoctor] Or install via Rust Cargo:\n  cargo install envdoc');
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
