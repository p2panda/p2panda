const path = require('path');

const getPath = (file) => {
  return path.resolve(__dirname, 'src', file);
};

module.exports = (env, argv) => {
  const isDevelopment = argv.mode === 'development';
  const filename = isDevelopment ? '[name]' : '[name]-[contenthash:6]';

  return {
    entry: {
      app: getPath('index.ts'),
    },
    output: {
      filename: `${filename}.js`,
    },
    resolve: {
      alias: {
        '~': path.resolve(__dirname, 'src'),
      },
      extensions: ['.js', '.ts', '.tsx'],
    },
    experiments: {
      // Support the new WebAssembly according to the updated specification, it
      // makes a WebAssembly module an async module.
      // See: https://webpack.js.org/configuration/experiments/
      asyncWebAssembly: true,
    },
    module: {
      rules: [
        {
          test: /\.tsx?/,
          exclude: /node_modules/,
          use: [
            {
              loader: 'babel-loader',
            },
            {
              loader: 'eslint-loader',
            },
            {
              loader: 'ts-loader',
            },
          ],
        },
      ],
    },
    plugins: [
      
    ],
    devtool: 'source-map',
  };
};
