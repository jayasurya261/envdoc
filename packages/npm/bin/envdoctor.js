#!/usr/bin/env node

const { spawnSync } = require('child_process');
const path = require('path');
const os = require('os');
const fs = require('fs');

const ext = os.platform() === 'win32' ? '.exe' : '';
const binName = `envdoctor${ext}`;

// 1. Look in the local bin directory (bundled with package)
const localBin = path.join(__dirname, binName);

let binPath = null;
if (fs.existsSync(localBin)) {
  binPath = localBin;
} else {
  // 2. Fallback to platform-specific optional dependency if split packages are used
  const platform = os.platform();
  const arch = os.arch();
  const candidatePkg = `@envdoctor/cli-${platform}-${arch}`;
  try {
    const pkgRoot = path.dirname(require.resolve(`${candidatePkg}/package.json`));
    const candidateBin = path.join(pkgRoot, 'bin', binName);
    if (fs.existsSync(candidateBin)) {
      binPath = candidateBin;
    }
  } catch (_) {
    // Optional dependency not present
  }
}

if (!binPath) {
  console.error(`[envdoctor] Error: native binary not found at ${localBin}`);
  console.error(`[envdoctor] Please ensure your platform is supported (Windows x64, Linux x64, macOS ARM64/x64).`);
  process.exit(1);
}

const result = spawnSync(binPath, process.argv.slice(2), {
  stdio: 'inherit',
});

if (result.error) {
  console.error('[envdoctor] Failed to execute native binary:', result.error.message);
  process.exit(1);
}

process.exit(result.status ?? 0);

