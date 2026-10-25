import { defineConfig } from 'astro/config';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  // Published on GitHub Pages with a custom domain (backspn.app redirects here from Cloudflare).
  site: 'https://backspn.com',
  // URLs without a trailing slash: /es is built as es.html, which GitHub Pages serves at /es.
  trailingSlash: 'never',
  build: { format: 'file' },
  i18n: {
    locales: ['en', 'es'],
    defaultLocale: 'en',
    routing: { prefixDefaultLocale: false },
  },
  vite: {
    plugins: [tailwindcss()],
  },
});
