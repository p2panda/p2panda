module.exports = {
  parser: '@typescript-eslint/parser',
  extends: [
    // Applies the recommended rules from @typescript-eslint/eslint-plugin
    'plugin:@typescript-eslint/recommended',
    // Keep prettier last to make sure its style changes are not overwritten
    // by other rules
    'plugin:prettier/recommended',
  ],
  rules: {
    // Warn on prettier violations and continue with build
    'prettier/prettier': 1,
  },
};
