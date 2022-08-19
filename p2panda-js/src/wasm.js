// SPDX-License-Identifier: AGPL-3.0-or-later

// This file serves as an "adapter" for the external WebAssembly builds coming
// from Rust.
//
// We build two different versions of WebAssembly for "web" and "node" targets.
// Both come with various platform-specific optimizations and module formats
// which make it easier for us to work with them in the different JavaScript
// contexts we're bundling for.
//
// Which final "adapter" is picked is defined by the rollup build process by
// setting the "BUILD_TARGET_WEB" flag.
//
// This approach is rather strange for TypeScript. To still have
// developer-friendly TypeScript definitions we're copying the "wasm.d.ts"
// types manually next to this file (see "wasm:types" script in package.json)
// every time we run "npm run build". This is also the reason why this file
// stays ".js" and not ".ts", to be able to take the definitions from the
// "outside".

// `BUILD_TARGET_WEB` defined by "rollup-plugin-define" plugin during rollup
// build process
const wasmAdapter = BUILD_TARGET_WEB
  ? require('../wasm/web')
  : require('../wasm/node');

// Re-export directly everything we've imported here
module.exports = wasmAdapter;
