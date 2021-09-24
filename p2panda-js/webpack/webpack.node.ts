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
 * - Wasm-pack generates WebAssembly with `nodejs` target
 * - Webpack bundles library with `node` target
 * - WebpackCopyPlugin copies the generated WebAssembly code manually into the
 *   final `lib` folder
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
    // Treat exported wasm as external module to prevent Webpack bundling it.
    // See below under "plugins" why this workaround is needed.
    wasm: './wasm',
    // `node-fetch` has a weird export that needs to be treated differently.
    'node-fetch': 'commonjs2 node-fetch',
  },
  plugins: [
    // We explicitly have to set the `wasm-pack` target to `nodejs` even though
    // it would mostly also work to use the default `bundler` target. The
    // problem is that the `getrandom` crate in Rust has a WebAssembly
    // environment switch and it would break in NodeJS if we don't set it
    // explicitly here.
    // Related issue: https://github.com/rust-random/getrandom/issues/214
    getWasmPlugin('nodejs'),
    // Since we treat the `wasm` module as "external", we have to import it
    // into our final `lib` folder after building it. Bringing in the
    // WebAssembly module in like this prevents a bug with Webpack reformating
    // the generated code by `wasm-pack` during bundling and breaking it badly.
    // Related issue: https://github.com/webpack/webpack/issues/8826 and
    // https://github.com/rustwasm/wasm-pack/issues/822
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
