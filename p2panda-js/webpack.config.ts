// SPDX-License-Identifier: AGPL-3.0-or-later

import configNode from './webpack/webpack.node';
import configWeb from './webpack/webpack.web';
import configInline from './webpack/webpack.inline';

export default [configNode, configWeb, configInline];
