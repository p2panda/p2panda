// SPDX-License-Identifier: AGPL-3.0-or-later

// We need var requires to make sure that only the right wasm source
// is imported.
/* eslint-disable @typescript-eslint/no-var-requires */

// Defined by WebpackDefinePlugin or Jest configuration in test environment
declare const BUILD_TARGET_WEB: boolean;

let adapter;
try {
  adapter = BUILD_TARGET_WEB
    ? require('~/wasm-adapter/browser')
    : require('~/wasm-adapter/node');
} catch (err) {
  console.error(err);
  throw new Error('Attempted to import web assembly library before bundling.');
}

export const initializeWasm = adapter.default;
