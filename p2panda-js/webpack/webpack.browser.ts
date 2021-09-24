// SPDX-License-Identifier: AGPL-3.0-or-later

import * as webpack from 'webpack';

import config, { getWasmPlugin } from './webpack.common';

/*
 * Extended configuration to build library targeting modern browsers:
 *
 * - Output is minified for smaller library size
 * - Webpack bundles with `web` target
 */
const configBrowser: webpack.Configuration = {
  ...config,
  name: 'browser',
  output: {
    ...config.output,
    filename: '[name].min.js',
  },
  plugins: [getWasmPlugin('bundler')],
  target: 'web',
};

export default configBrowser;
