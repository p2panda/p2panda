module.exports = {
  // Parser to lint TypeScript code, see:
  // https://standardjs.com/index.html#typescript
  parser: '@typescript-eslint/parser',
  plugins: [
    // Required plugin to lint TypeScript code
    '@typescript-eslint',
  ],
  rules: {
    // Standard incorrectly emits unused-variable errors, see:
    // https://github.com/standard/standard/issues/1283
    'no-unused-vars': 'off',
    '@typescript-eslint/no-unused-vars': 'error',
    // Always require dangling commas for multiline objects and arrays
    'comma-dangle': ['error', 'always-multiline'],
    // Standard does not like semicolons, semistandard likes them, we like
    // semicolons as well, but we're using standardx, therefore we have to
    // require them here manually
    semi: ['error', 'always'],
  },
};
