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

## Example
```
http://127.0.0.1:3000/stress
http://127.0.0.1:3000/stress?number=50
``` 
If you don't specify `number` or you passing a no number string, the default sequence is 45
