// SPDX-License-Identifier: AGPL-3.0-or-later

import { Configuration, DefinePlugin } from 'webpack';
import CopyPlugin from 'copy-webpack-plugin';
import ESLintPlugin from 'eslint-webpack-plugin';

import config, {
  DIR_DIST,
  DIR_SRC,
  DIR_WASM,
  getPath,
  tsRule,
} from './webpack.common';

const BUNDLE_NAME = 'slim';

/*
 * Extended configuration to build library targeting modern browsers:
 *
 * - Output can be minified for smaller library size
 * - Uses WebAssembly built with `web` target
 * - WebAssembly needs to be initialized with using external '.wasm' file to
 *   save bandwith
 * - Webpack bundles with `web` target
 */
const configSlim = ({ minimize = true }): Configuration => {
  return {
    ...config,
    entry: getPath(DIR_SRC, `index.${BUNDLE_NAME}.ts`),
    name: minimize ? `${BUNDLE_NAME}-minimize` : BUNDLE_NAME,
    output: {
      ...config.output,
      filename: minimize
        ? `${BUNDLE_NAME}/index.min.js`
        : `${BUNDLE_NAME}/index.js`,
    },
    target: 'web',
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
            to: `${getPath(DIR_DIST)}/${BUNDLE_NAME}/p2panda.wasm`,
          },
        ],
      }),
      new ESLintPlugin(),
    ],
    optimization: {
      minimize,
    },
  };
};

export default configSlim;
