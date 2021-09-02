// SPDX-License-Identifier: AGPL-3.0-or-later

// We need var requires to make sure that only the right wasm source
// is imported.
/* eslint-disable @typescript-eslint/no-var-requires */

// Defined by WebpackDefinePlugin
declare const BUILD_TARGET_WEB: boolean;

let adapter:
  | typeof import('~/wasm-adapter/browser')
  | typeof import('~/wasm-adapter/node');
try {
  adapter = BUILD_TARGET_WEB
    ? (require('~/wasm-adapter/browser') as typeof import('~/wasm-adapter/browser'))
    : (require('~/wasm-adapter/node') as typeof import('~/wasm-adapter/node'));
} catch (err) {
  console.error(err);
  throw new Error('Attempted to import web assembly library before bundling.');
}

export const initializeWasm = adapter.default;
