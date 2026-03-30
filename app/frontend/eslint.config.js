import antfu from '@antfu/eslint-config'
import tsParser from '@typescript-eslint/parser'
import pluginSvelte from 'eslint-plugin-svelte'
import svelteParser from 'svelte-eslint-parser'

export default antfu(
  {
    typescript: true,
    svelte: false,
  },
  ...pluginSvelte.configs['flat/recommended'].map(config => ({
    ...config,
    files: ['**/*.svelte'],
  })),
  {
    files: ['**/*.svelte'],
    languageOptions: {
      parser: svelteParser,
      parserOptions: {
        parser: tsParser,
      },
    },
  },
  {
    rules: {
      'no-console': 'warn',
      'svelte/no-at-html-tags': 'off',
    },
  },
)
