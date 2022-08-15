// SPDX-License-Identifier: AGPL-3.0-or-later

import { Configuration, DefinePlugin } from 'webpack';
import CopyPlugin from 'copy-webpack-plugin';
import ESLintPlugin from 'eslint-webpack-plugin';

import config, { DIR_WASM, DIR_DIST, tsRule, getPath } from './webpack.common';

const BUNDLE_NAME = 'node';

/*
 * Extended configuration to build library targeting node applications:
 *
 * - Output is not minified
 * - Uses WebAssembly built with `nodejs` target
 * - WebpackCopyPlugin copies the generated WebAssembly code manually into the
 *   final `lib` folder
 * - Webpack bundles library with `node` target
 */
const configNode = (): Configuration => {
  return {
    ...config,
    name: BUNDLE_NAME,
    output: {
      ...config.output,
      filename: `${BUNDLE_NAME}/index.js`,
    },
    target: 'node',
    externals: {
      // Treating the WebAssembly module as external prevents a bug with Webpack
      // reformating the generated code by `wasm-pack` and breaking it badly.
      //
      // This Webpack issue mostly hits the WebAssembly code of the `getrandom`
      // crate using dynamic `require` statements based on the environment
      // (Browser / NodeJS).
      //
      // Related issue: https://github.com/webpack/webpack/issues/8826 and
      // https://github.com/rust-random/getrandom/issues/224
      //
      // Through this workaround, there are a couple of things to take care of:
      //
      // 1. We treat `../wasm/node` as an external dependency here,
      // but routing it to `<root>/<dist>/node/wasm/index.js` (note the <dist>!)
      //
      // 2. Since this folder doesn't exist in the final build we copy it from
      // `<root>/wasm/node` to `<root>/<dist>/node/wasm` via the CopyPlugin, see
      // further below.
      '../wasm/node': `./wasm/index.js`,
      // `node-fetch` has a weird export that needs to be treated differently.
      // @TODO: Remove fetch, https://github.com/p2panda/p2panda/issues/433
      'node-fetch': 'commonjs2 node-fetch',
    },
    module: {
      rules: [tsRule],
    },
    plugins: [
      new DefinePlugin({
        BUILD_TARGET_WEB: JSON.stringify(false),
      }),
      // Since we treat the `wasm` module as "external" as an workaround (read
      // more above), we have to copy it manually into the build.
      new CopyPlugin({
        patterns: [
          {
            from: `${getPath(DIR_WASM)}/node/*.{js,wasm}`,
            to: `${getPath(DIR_DIST)}/${BUNDLE_NAME}/wasm/[name][ext]`,
          },
          {
            from: `${getPath(DIR_WASM)}/node/*.d.ts`,
            to: `${getPath(DIR_DIST)}/types/wasm/[name][ext]`,
          },
        ],
      }),
      new ESLintPlugin(),
    ],
    optimization: {
      minimize: false,
    },
  };
};

export default configNode;
