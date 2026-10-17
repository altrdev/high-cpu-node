## Description
A simple nodejs server for stressing cpu with fibonacci sequence

## Install
```
npm install
```

## Usage
On your local machine: 

```
npm start
```

Alternatively you can use a docker image
```
docker pull altrdev/high-cpu-node:latest
docker run -p 3000:3000 altrdev/high-cpu-node:latest
```

The server exposes on port 3000:

`/` for health check

`/stress` for stress a cpu 

You can use a query string parameter `number` to stress a cpu with a sequence calculation.

Requests are spread round-robin over a pool of worker processes (one per core by default), so concurrent requests can saturate multiple cores. A worker that crashes is restarted automatically.

## Example
```
http://127.0.0.1:3000/stress
http://127.0.0.1:3000/stress?number=40
``` 
If you don't specify `number` or you passing a no number string, the default sequence is 45.
A `number` outside the range `0..MAX_NUMBER` gets a `400` response.

## Configuration
Environment variables:

| Variable     | Default         | Description                              |
|--------------|-----------------|------------------------------------------|
| `PORT`       | `3000`          | Port the server listens on               |
| `WORKERS`    | number of cores | Number of worker processes               |
| `MAX_NUMBER` | `50`            | Highest `number` accepted by `/stress`   |

```
docker run -p 3000:3000 -e WORKERS=2 altrdev/high-cpu-node:latest
```
