import browserAdapter from './browser';
import nodeAdapter from './node';

// Defined by WebpackDefinePlugin
declare const BUILD_TARGET_WEB: boolean;
const adapter = BUILD_TARGET_WEB ? browserAdapter : nodeAdapter;

export default adapter;
