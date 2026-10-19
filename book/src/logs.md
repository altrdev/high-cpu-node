# Logs

By default the server writes plain text: what it is doing at the start, the settings it replaced and the errors, plus the lines of [`/log`](endpoints.md#log). It writes no line for each request.

`LOG_FORMAT=json` writes one JSON object a line, for a log aggregator that would otherwise have to parse the text. Every line has `timestamp` (ISO 8601, UTC), `level` (`info`, `warn` or `error`) and `message`; warnings and errors go to stderr, the rest to stdout. The lines of `/log` have the same fields. In this format every request also has a line, with `method`, `path`, `status` and `duration_ms` (the query is left out, because it can hold a token):

```json
{"timestamp":"2026-10-19T12:34:56.789Z","level":"info","message":"GET /status/418 418","method":"GET","path":"/status/418","status":418,"duration_ms":0.215}
```

The effective format is `logFormat` in [`/config`](endpoints.md#config).
