import type { APIRoute } from 'astro';
import { getRelativeLocaleUrl } from 'astro:i18n';
import { dictionaries, type Lang } from '../i18n';

const langs = Object.keys(dictionaries) as Lang[];

// One entry per language, each listing every translation (hreflang) so search engines pair them.
export const GET: APIRoute = ({ site }) => {
  const abs = (l: Lang, path: string) => new URL(getRelativeLocaleUrl(l, path), site).href.replace(/\/$/, '');
  const urls = ['', 'insights']
    .map((path) => {
      const alternates = langs.map((l) => `<xhtml:link rel="alternate" hreflang="${l}" href="${abs(l, path)}"/>`).join('');
      return langs.map((l) => `<url><loc>${abs(l, path)}</loc>${alternates}<xhtml:link rel="alternate" hreflang="x-default" href="${abs('en', path)}"/></url>`).join('');
    })
    .join('');
  const xml = `<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">${urls}</urlset>`;
  return new Response(xml, { headers: { 'Content-Type': 'application/xml' } });
};
