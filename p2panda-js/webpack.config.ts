// SPDX-License-Identifier: AGPL-3.0-or-later

import configNode from './webpack/webpack.node';
import configSlim from './webpack/webpack.slim';
import configBrowser from './webpack/webpack.browser';

export default [
  configNode(),
  configSlim({ minimize: true }),
  configSlim({ minimize: false }),
  configBrowser({ minimize: true }),
  configBrowser({ minimize: false }),
];
