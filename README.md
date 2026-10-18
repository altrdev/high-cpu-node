# stress-node

A simple Node.js server for stressing the CPU with a recursive Fibonacci calculation. Useful for testing autoscaling, load balancers and container CPU limits.

> **Warning:** this server is meant to burn CPU on purpose. Don't expose it to the public internet.

## Install

```
npm install
```

## Usage

On your local machine:

```
npm start
```

Or with Docker:

```
docker pull altrdev/stress-node:latest
docker run -p 3000:3000 altrdev/stress-node:latest
```

## Endpoints

The server listens on port 3000.

| Endpoint  | Description                                  |
|-----------|----------------------------------------------|
| `/`       | Health check, returns `Status: OK`           |
| `/stress` | Runs a Fibonacci calculation to load a CPU   |

`/stress` accepts an optional query string parameter `number`, the sequence length to calculate:

- if omitted or not a number, it defaults to `45`
- if outside the range `0..MAX_NUMBER`, the server answers `400`

The calculation time grows exponentially with `number`.

```
http://127.0.0.1:3000/stress
http://127.0.0.1:3000/stress?number=40
```

## How it works

Requests are spread round-robin over a pool of worker processes (one per core by default), so concurrent requests can saturate multiple cores. A worker that crashes is restarted automatically, and the request it was serving gets a `500`.

## Load testing

To keep several cores busy you need concurrent requests, at least as many as `WORKERS`. With [hey](https://github.com/rakyll/hey):

```
hey -n 100 -c 8 "http://127.0.0.1:3000/stress?number=40"
```

With [k6](https://k6.io), save this as `stress.js`:

```js
import http from 'k6/http';

export const options = { vus: 8, duration: '60s' };

export default function () {
    http.get('http://127.0.0.1:3000/stress?number=40');
}
```

and run `k6 run stress.js`.

With plain `curl` (8 parallel requests):

```
for i in $(seq 8); do curl -s "http://127.0.0.1:3000/stress?number=40" & done; wait
```

Lower or raise `number` to tune how long each request keeps a core busy.

## Configuration

Environment variables:

| Variable     | Default         | Description                            |
|--------------|-----------------|----------------------------------------|
| `PORT`       | `3000`          | Port the server listens on             |
| `WORKERS`    | number of cores | Number of worker processes             |
| `MAX_NUMBER` | `50`            | Highest `number` accepted by `/stress` |

```
docker run -p 3000:3000 -e WORKERS=2 altrdev/stress-node:latest
```

## Changelog

- **2.2.1**: Project renamed to `stress-node`. The Docker image is published as `altrdev/stress-node`; `altrdev/high-cpu-node` is deprecated and kept in sync for a transition period.
- **2.2.0**: Updated to Express 5, Node 26 base image and `cors` 2.8.6.
- **2.1.0**: Docker image has a `HEALTHCHECK`; the Docker Hub description is synced from this README on release.
- **2.0.0**: `/stress` rejects `number` outside `0..MAX_NUMBER` with `400` (previously unbounded). Added the worker pool and the `PORT`, `WORKERS`, `MAX_NUMBER` variables.

## License

MIT
