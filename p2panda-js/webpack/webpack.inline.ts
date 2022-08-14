// SPDX-License-Identifier: AGPL-3.0-or-later

import webpack, { DefinePlugin } from 'webpack';
import ESLintPlugin from 'eslint-webpack-plugin';

import config, { getPath, tsRule, DIR_SRC } from './webpack.common';

/*
 * Extended configuration to build library targeting modern browsers:
 *
 * - Output is minified for smaller library size
 * - Uses WebAssembly built with `web` target
 * - WebAssembly converted to base64 string and embedded inline
 * - Webpack bundles with `web` target
 */
const configInline: webpack.Configuration = {
  ...config,
  entry: getPath(DIR_SRC, 'index.inline.ts'),
  name: 'inline',
  output: {
    ...config.output,
    filename: `inline/index.min.js`,
  },
  module: {
    rules: [
      tsRule,
      {
        test: /\.wasm$/,
        type: 'asset/inline',
      },
    ],
  },
  plugins: [
    new DefinePlugin({
      BUILD_TARGET_WEB: JSON.stringify(true),
    }),
    new ESLintPlugin(),
  ],
  target: 'web',
};

export default configInline;
