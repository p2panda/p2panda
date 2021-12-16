// SPDX-License-Identifier: AGPL-3.0-or-later

import * as webpack from 'webpack';
import CopyPlugin from 'copy-webpack-plugin';
import ESLintPlugin from 'eslint-webpack-plugin';

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
 * - Wasm-pack generates WebAssembly with `nodejs` target
 * - WebpackCopyPlugin copies the generated WebAssembly code manually into the
 *   final `lib` folder
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
    // Treating the WebAssembly module as external prevents a bug with Webpack
    // reformating the generated code by `wasm-pack` and breaking it badly.
    //
    // This Webpack issue mostly hits the WebAssembly code of the `getrandom`
    // crate using dynamic `require` statements based on the environment
    // (Browser / NodeJS).
    //
    // Related issue: https://github.com/webpack/webpack/issues/8826 and
    // https://github.com/rust-random/getrandom/issues/224
    wasm: `./${DIR_WASM}`,
    // `node-fetch` has a weird export that needs to be treated differently.
    'node-fetch': 'commonjs2 node-fetch',
    // Solve missing dependency warning for `ws` package.
    // Related issue: https://github.com/websockets/ws/issues/1126
    'utf-8-validate': 'utf-8-validate',
    bufferutil: 'bufferutil',
  },
  plugins: [
    // As we treat the wasm build as external we can't make use of Webpack's
    // magic bundling it for the right environment. We explicitly have to set
    // the `wasm-pack` target to `nodejs`.
    getWasmPlugin('nodejs'),
    // Since we treat the `wasm` module as "external", we have to import it
    // after the `wasm-pack` step into the `lib` folder.
    new CopyPlugin({
      patterns: [
        {
          from: `${getPath(DIR_WASM)}/*.{js,wasm}`,
          to: getPath(DIR_DIST),
        },
      ],
    }),
    new ESLintPlugin(),
  ],
  optimization: {
    minimize: false,
  },
};

export default configNode;
