#!/usr/bin/env python3
"""Writes sitemap.xml and robots.txt for the site, from the pages of the book in the order of SUMMARY.md.
Usage: python3 book/sitemap.py book/book   (after mdbook build book)"""
import os
import re
import sys

out = sys.argv[1]
src = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'src')
site = 'https://' + open(os.path.join(src, 'CNAME')).read().strip()

urls = [f'{site}/']
for line in open(os.path.join(src, 'SUMMARY.md')):
    m = re.match(r'^\s*-?\s*\[[^\]]+\]\(([^)]+)\.md\)', line)
    # the first page of the book is the home page, already listed
    if m and m.group(1) != 'introduction':
        assert os.path.exists(os.path.join(out, m.group(1) + '.html')), f'{m.group(1)}.html is not in {out}'
        urls.append(f'{site}/{m.group(1)}.html')

xml = ['<?xml version="1.0" encoding="UTF-8"?>', '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">']
xml += [f'  <url><loc>{url}</loc></url>' for url in urls]
xml.append('</urlset>')
open(os.path.join(out, 'sitemap.xml'), 'w').write('\n'.join(xml) + '\n')
open(os.path.join(out, 'robots.txt'), 'w').write(f'User-agent: *\nAllow: /\n\nSitemap: {site}/sitemap.xml\n')
print(f'sitemap.xml: {len(urls)} pages')
