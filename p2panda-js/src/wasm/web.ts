// SPDX-License-Identifier: AGPL-3.0-or-later

import * as wasm from '../../wasm/web/index.js';
import init from '../../wasm/web/index.js';
import wasmData from '../../wasm/web/index_bg.wasm';

export default async () => {
  await init(wasmData);
};

module.exports = wasm;
