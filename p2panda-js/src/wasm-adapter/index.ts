// We need var requires to make sure that only the right wasm source
// is imported.
/* eslint-disable @typescript-eslint/no-var-requires */

// Defined by WebpackDefinePlugin
declare const BUILD_TARGET_WEB: boolean;

const adapter = BUILD_TARGET_WEB
  ? (require('~/wasm-adapter/browser') as typeof import('~/wasm-adapter/browser'))
  : (require('~/wasm-adapter/node') as typeof import('~/wasm-adapter/node'));

export const initializeWasm = adapter.default;
