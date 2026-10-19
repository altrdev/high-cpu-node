#!/usr/bin/env python3
"""Writes llms.txt and llms-full.txt from the pages of the book, in the order of SUMMARY.md.
llms.txt is the index for a language model (llmstxt.org); llms-full.txt has every page in one file.
Usage: python3 book/llms.py book/book   (after mdbook build book)"""
import os
import re
import sys

out = sys.argv[1]
src = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'src')
site = 'https://' + open(os.path.join(src, 'CNAME')).read().strip()
version = open(os.path.join(src, 'version.md')).read().strip() if os.path.exists(os.path.join(src, 'version.md')) else 'dev'
description = re.search(r'^description = "(.*)"$', open(os.path.join(os.path.dirname(src), 'book.toml')).read(), re.M).group(1)
# the changelog is a long list of every release and is not what a model needs to use the tool
left_out = {'changelog.md'}


def read(page):
    text = open(os.path.join(src, page)).read().replace('{{#include version.md}}', version)
    # a link to a page of the book becomes the address of that page on the site
    text = re.sub(r'\]\(([a-z0-9-]+)\.md(#[^)]*)?\)', lambda m: f']({site}/{m.group(1)}.html{m.group(2) or ""})', text)
    # the images are of no use in text
    return '\n'.join(line for line in text.split('\n') if not line.startswith('![')).strip() + '\n'


# what each page is about, in a line: a sentence taken from the page itself says too little. A page that is not here
# is listed with its title only.
about = {
    'introduction.md': 'What torchio is and what it is for',
    'install.md': 'Docker, Homebrew, the install script, release binaries, building from source',
    'getting-started.md': 'Start a server and watch it in 30 seconds',
    'what-you-can-test.md': 'Recipes: OOM kills, autoscaling, probes, timeouts, disk, leaks, readiness, restarts, descriptors',
    'endpoints.md': 'What each endpoint does, with examples and answers',
    'reference.md': 'Every endpoint with its parameters, defaults, limits and command, written from the same catalog as GET /help',
    'starting-a-test.md': 'The torchio commands that call the endpoints of a running server, including scenario and payload',
    'scenarios.md': 'POST /scenario: a list of steps (cpu, memory, disk, fd, connections, ready, block) run one after the other',
    'agents.md': 'How an agent can use torchio: /help, llms.txt and the skill',
    'shell-completion.md': 'Completion scripts for bash, zsh and fish',
    'watch.md': 'torchio watch: a terminal view of a running server that can also start tests',
    'control-panel.md': 'The page at /ui to watch a server and start tests with sliders',
    'configuration.md': 'Every environment variable, its default and its limit',
    'token.md': 'TOKEN: which endpoints need a bearer token',
    'logs.md': 'Plain text and JSON logs (LOG_FORMAT), the access log',
    'shutdown.md': 'What happens on SIGTERM and SIGINT, and the drain',
    'docker.md': 'The image, its tags, platforms, limits and health check',
    'examples.md': 'Kubernetes manifests, the Helm chart and Compose files',
    'how-it-works.md': 'Worker threads and the async runtime',
    'load-testing.md': 'Using torchio with hey and k6 to load several cores',
    'development.md': 'Source layout, tests, documentation site and releases',
    'security.md': 'Where it is safe to run it, and the license',
}


sections = []  # (heading, [(title, page)])
for line in open(os.path.join(src, 'SUMMARY.md')):
    heading = re.match(r'^# (.+)$', line)
    entry = re.match(r'^\s*- \[([^\]]+)\]\(([^)]+\.md)\)', line)
    if heading and heading.group(1) != 'Summary':
        sections.append((heading.group(1), []))
    elif entry:
        if not sections:
            sections.append(('Start', []))
        sections[-1][1].append((entry.group(1), entry.group(2)))
    elif line.startswith('[') and (m := re.match(r'^\[([^\]]+)\]\(([^)]+\.md)\)', line)):
        sections.append(('About', [(m.group(1), m.group(2))]))

index = [
    '# torchio',
    '',
    f'> {description}',
    '',
    f'torchio {version} is a single static binary written in Rust that burns CPU, holds memory, fills disk, answers slowly or '
    'returns any HTTP status, on demand over HTTP. Use it to test autoscaling, resource limits, OOM kills, probes, timeouts and '
    'retries. A running server describes its own endpoints, parameters and limits as JSON at `GET /help`, and what it holds right '
    'now at `GET /info`; prefer those to guessing. Every endpoint that changes state is also a command: `torchio cpu 80 --hold 60`. '
    f'The whole documentation in one file is {site}/llms-full.txt.',
    '',
]
full = [f'# torchio {version}: documentation', '', f'> {description}', '']
for heading, pages in sections:
    index += [f'## {heading}', '']
    for title, page in pages:
        text = read(page)
        said = about.get(page, '')
        index.append(f'- [{title}]({site}/{page[:-3]}.html)' + (f': {said}' if said else ''))
        if page not in left_out:
            full += [text, '---', '']
    index.append('')
index += ['## Optional', '', f'- [Skill for agents]({site}/SKILL.md): how to run a test with torchio, as a skill file', '']

open(os.path.join(out, 'llms.txt'), 'w').write('\n'.join(index))
open(os.path.join(out, 'llms-full.txt'), 'w').write('\n'.join(full))
print(f'llms.txt: {len(index)} lines, llms-full.txt: {len(full)} lines')
