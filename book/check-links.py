#!/usr/bin/env python3
"""Checks the links between the pages of the built book: every link to a page, and to a heading in a
page, must have a target. Usage: python3 book/check-links.py book/book"""
import glob
import os
import re
import sys

root = sys.argv[1]
ids = {os.path.basename(f): set(re.findall(r'id="([^"]+)"', open(f).read())) for f in glob.glob(root + '/*.html')}
broken = 0
for f in sorted(glob.glob(root + '/*.html')):
    base = os.path.basename(f)
    for href in re.findall(r'href="([^"]+)"', open(f).read()):
        if re.match(r'^(https?:|mailto:|data:)', href):
            continue
        page, _, fragment = href.partition('#')
        page = page or base
        if page.startswith('.') or '/' in page or page.endswith(('.css', '.js', '.svg', '.png', '.ico')):
            continue
        if page not in ids:
            print(f'{base}: no page {page}')
            broken += 1
        elif fragment and fragment not in ids[page]:
            print(f'{base}: no heading #{fragment} in {page}')
            broken += 1
print(f'{broken} broken links')
sys.exit(1 if broken else 0)
