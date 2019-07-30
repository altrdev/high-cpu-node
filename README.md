## Description
A simple nodejs server for stressing cpu

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

You can use a query string parameter `seconds` to stress a cpu for those seconds.

## Example
```
http://127.0.0.1:3000/stress
``` 
Default `seconds` set at 1

```
http://127.0.0.1:3000/stress?seconds=10
```
Duration of stress test set at 10 seconds
