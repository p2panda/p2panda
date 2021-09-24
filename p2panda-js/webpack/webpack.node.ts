// SPDX-License-Identifier: AGPL-3.0-or-later

import * as webpack from 'webpack';
import CopyPlugin from 'copy-webpack-plugin';

import config, {
  DIR_WASM,
  DIR_DIST,
  getWasmPlugin,
  getPath,
} from './webpack.common';

/*
 * Extended configuration to build library targeting node applications:
 *
 * - Output is not minified
 * - Webpack bundles library with `node` target
 */
const configNode: webpack.Configuration = {
  ...config,
  name: 'node',
  output: {
    ...config.output,
    filename: '[name].js',
  },
  target: 'node',
  externals: {
    // Treat exported wasm as external module
    wasm: './wasm',
    // `node-fetch` has a weird export that needs to be treated differently
    'node-fetch': 'commonjs2 node-fetch',
  },
  plugins: [
    getWasmPlugin('nodejs'),
    new CopyPlugin({
      patterns: [
        {
          from: `${getPath(DIR_WASM)}/*.{js,wasm}`,
          to: getPath(DIR_DIST),
        },
      ],
    }),
  ],
  optimization: {
    minimize: false,
  },
};

export default configNode;
