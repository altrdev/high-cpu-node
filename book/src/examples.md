# Examples

## Docker Compose

```yaml
services:
  stress:
    image: altrdev/torchio:latest
    ports:
      - "3000:3000"
    environment:
      WORKERS: 2
      MAX_MEMORY_MB: 1024
    deploy:
      resources:
        limits:
          cpus: "1"
          memory: 512M
```

## Kubernetes

The [`deploy`](https://github.com/altrdev/torchio/tree/main/deploy) folder has what you need to run torchio in a cluster, with a memory and CPU limit, a liveness probe on `/` and a readiness probe on `/ready`, and a container that runs as non-root with a read-only filesystem:

```
kubectl apply -k deploy/kubernetes
```

applies the Deployment and the Service as they are in [`deploy/kubernetes`](https://github.com/altrdev/torchio/tree/main/deploy/kubernetes). Or install the Helm chart in [`deploy/helm/torchio`](https://github.com/altrdev/torchio/tree/main/deploy/helm/torchio), where the image tag, the replicas, `WORKERS`, `UI`, the `TOKEN` (kept in a Secret), any other variable and the resources are values:

```
helm install torchio deploy/helm/torchio --set replicaCount=3 --set config.workers=2
```

They are a starting point: copy and change them, or leave them as they are and override what you need (in a kustomization of your own, or with `-f my-values.yaml` for the chart; the `env` value takes any variable the chart has no value for).

Both use the image of the version they sit next to, `altrdev/torchio:X.Y.Z`, and CI fails when it is not the one of `Cargo.toml`. The memory budget of the manifests (`MAX_MEMORY_MB`) is above the container limit, so `/memory` can trigger an OOM kill.

Some things to try, after `kubectl port-forward deploy/torchio 3000:3000`:

- `/stress?number=45` on several terminals keeps the container at its CPU limit, which you can watch with `kubectl top pod` or use to trigger a CPU-based autoscaler.
- `/memory?mb=400` goes above the `256Mi` limit, so the container should be OOM killed and restarted.
- `/ready/off?seconds=60` takes the pod out of the Service endpoints for a minute without restarting it (the readiness probe now checks `/ready`; the liveness probe still checks `/`).
- `/crash?code=1` restarts the container, and calling it again each time shows the backoff growing.
- `/status/503` is a quick way to check how a probe or a retry policy reacts to errors.
