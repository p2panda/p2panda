// SPDX-License-Identifier: AGPL-3.0-or-later

import { Configuration, DefinePlugin } from 'webpack';
import ESLintPlugin from 'eslint-webpack-plugin';

import config, { getPath, tsRule, DIR_SRC } from './webpack.common';

const BUNDLE_NAME = 'browser';

/*
 * Extended configuration to build library targeting modern browsers:
 *
 * - Output can be minified for smaller library size
 * - Uses WebAssembly built with `web` target
 * - WebAssembly converted to base64 string and embedded inline
 * - Webpack bundles with `web` target
 */
const configBrowser = ({ minimize = true }): Configuration => {
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
    optimization: {
      minimize,
    },
  };
};

export default configBrowser;
