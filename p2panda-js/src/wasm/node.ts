// SPDX-License-Identifier: AGPL-3.0-or-later

import * as wasm from '../../wasm/node/index.js';

export default async () => {
  // No need to initialize any WebAssembly for NodeJS
  return;
};

module.exports = wasm;
