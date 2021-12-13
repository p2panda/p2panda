// SPDX-License-Identifier: AGPL-3.0-or-later

import * as webpack from 'webpack';
import ESLintPlugin from 'eslint-webpack-plugin';

import config, { getWasmPlugin } from './webpack.common';

/*
 * Extended configuration to build library targeting modern browsers:
 *
 * - Output is minified for smaller library size
 * - Wasm-pack generates WebAssembly with default `bundler` target
 * - Webpack bundles with `web` target
 */
const configWeb: webpack.Configuration = {
  ...config,
  name: 'web',
  output: {
    ...config.output,
    filename: '[name].min.js',
  },
  plugins: [getWasmPlugin('bundler'), new ESLintPlugin()],
  target: 'web',
};

export default configWeb;
