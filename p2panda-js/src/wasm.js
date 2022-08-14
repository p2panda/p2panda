// SPDX-License-Identifier: AGPL-3.0-or-later

// `BUILD_TARGET_WEB` defined by webpack.DefinePlugin
const wasmAdapter = BUILD_TARGET_WEB
  ? require('../wasm/web')
  : require('../wasm/node');

// Re-export directly everything we've imported here
module.exports = wasmAdapter;
