# envdoctor-cli

> **Fast, safe-by-construction AST auditor keeping `.env.example` in sync with Rust, TypeScript, and JavaScript source code.**

## Quick Usage via npx

Run without installing:
```bash
npx envdoctor-cli
```

Or audit with an active `.env` file:
```bash
npx envdoctor-cli --env .env
```

Fail on warnings in CI:
```bash
npx envdoctor-cli --fail-on warn
```

## Installation

```bash
npm install -D envdoctor-cli
# or
pnpm add -D envdoctor-cli
# or
yarn add -D envdoctor-cli
```

Add to your `package.json` scripts:
```json
{
  "scripts": {
    "audit:env": "envdoctor --fail-on warn"
  }
}
```

For full documentation and CLI options, visit the [EnvDoctor GitHub Repository](https://github.com/jayasurya261/envdoc).
