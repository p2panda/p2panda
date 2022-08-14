// SPDX-License-Identifier: AGPL-3.0-or-later

import webpack, { DefinePlugin } from 'webpack';
import CopyPlugin from 'copy-webpack-plugin';
import ESLintPlugin from 'eslint-webpack-plugin';

import config, {
  getPath,
  tsRule,
  DIR_SRC,
  DIR_DIST,
  DIR_WASM,
} from './webpack.common';
import { version } from '../package.json';

/*
 * Extended configuration to build library targeting modern browsers:
 *
 * - Output is minified for smaller library size
 * - Uses WebAssembly built with `web` target
 * - WebAssembly needs to be initialized with using external '.wasm' file to
 *   save bandwith
 * - Webpack bundles with `web` target
 */
const configWeb: webpack.Configuration = {
  ...config,
  entry: getPath(DIR_SRC, 'index.web.ts'),
  name: 'web',
  output: {
    ...config.output,
    filename: `web/index.min.js`,
  },
  module: {
    rules: [tsRule],
  },
  plugins: [
    new DefinePlugin({
      BUILD_TARGET_WEB: JSON.stringify(true),
    }),
    // Make sure the `.wasm` file is also copied into the folder so developers
    // can load it from there to pass it over to the `initializeWebAssembly`
    // method
    new CopyPlugin({
      patterns: [
        {
          from: `${getPath(DIR_WASM)}/web/*.wasm`,
          to: `${getPath(DIR_DIST)}/web/p2panda-v${version}.wasm`,
        },
      ],
    }),
    new ESLintPlugin(),
  ],
  target: 'web',
};

export default configWeb;
