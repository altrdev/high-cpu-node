# For AI agents

An agent that runs tests with torchio has three things to read, from the least to the most work.

## The server describes itself

`GET /help` lists every endpoint with its parameters, defaults and the limits of that instance, as JSON, and `GET /info` says what the server holds right now and what is left of each budget. They come from the same definitions the endpoints use, so they are right for the version and the settings that are running. See [`/help`](endpoints.md#help) and [`/info`](endpoints.md#info).

## llms.txt

The site follows the [llms.txt](https://llmstxt.org) convention:

- <https://torchio.altr.dev/llms.txt> is an index of the pages, each with a line about it.
- <https://torchio.altr.dev/llms-full.txt> is the whole documentation in one file, without the changelog.

Both are written from the pages of the book when the site is built, so they match the version that is published.

## A skill

`skills/torchio/SKILL.md` is a skill that tells an agent how to put an instance under pressure: look at `/help` and `/info` first, which tests take the instance away and need a yes (`/crash`, `/oom`, `/block`), the budgets and their `429`, scenarios, and how to watch the result. It is also at <https://torchio.altr.dev/SKILL.md>.

To use it with Claude Code, put it in a folder of skills:

```
mkdir -p ~/.claude/skills/torchio
curl -fsSL https://torchio.altr.dev/SKILL.md -o ~/.claude/skills/torchio/SKILL.md
```

Use `.claude/skills/torchio/` inside a project to share it with the people who work on that project. Other agents that read `SKILL.md` files take it the same way, from the folder where they look for skills.
