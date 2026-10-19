# Token

Anyone who can reach the port can call `/crash` or `/disk`. Set `TOKEN` to ask for a password on the endpoints that change the state of the instance (`/stress`, `/cpu`, `/memory`, `/leak`, `/disk`, `/fd`, `/connections`, `/log`, `/ready/off`, `/ready/on`, `/block`, `/oom` and `/crash`):

```
docker run -p 3000:3000 -e TOKEN=secret altrdev/torchio:latest
curl -H "Authorization: Bearer secret" localhost:3000/crash
```

Without the header, or with a wrong token, they answer `401`. `/`, `/ready`, `/info`, `/metrics`, `/help`, `/config`, `/echo`, `/delay`, `/status/:code` and `/flaky` stay open, so probes, Prometheus and `watch` work as before. [`/info`](endpoints.md#info) tells whether a token is needed (`auth`). The control panel asks for it the first time a server answers `401` and keeps it for that tab. Without `TOKEN` nothing is checked.

It is a shared password, not a login: without HTTPS it travels in clear, so on the internet put a proxy with TLS in front.
