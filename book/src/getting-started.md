# Try it in 30 seconds

Start the server with the image or with the binary (see [Install](install.md) if you have neither yet):

```
docker run --name torchio -p 3000:3000 altrdev/torchio    # with Docker
torchio serve                                             # with the binary
```

`torchio` with no command does the same as `torchio serve`: it is what the Docker image runs. The server stays in the foreground, so Ctrl+C stops it; use a second terminal tab for `torchio watch`.

In a second terminal, watch what it does:

```
docker exec -it torchio torchio watch    # with Docker
torchio watch                            # on your local machine, press m to start a memory test
```

In a third one, put it under load, and see the bars move:

```
torchio memory 200 --hold 20
torchio disk 100 --hold 20
torchio stress 44
```

The same thing with `curl`: `curl "http://127.0.0.1:3000/memory?mb=200&hold=20"`. See [Starting a test](starting-a-test.md#starting-a-test).
