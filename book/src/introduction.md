# torchio


**Break your containers on purpose.** torchio is a small server, a single static binary written in Rust, that burns CPU, holds memory, fills disk, answers slowly or returns any HTTP status, all on demand over HTTP. Use it to see how your setup behaves under pressure: autoscaling, resource limits, OOM kills, probes, timeouts and retries.

*Torchio* is the Italian word for the press that squeezes wine and oil, as in *mettere sotto torchio*: to put under pressure. Put your containers under the press. It began in 2019 as `high-cpu-node`, when it could only burn CPU.

![torchio watch: gradient bars and history of busy workers, held memory, process RSS and held disk, while keys start a memory test, a CPU burn and a leak](images/watch.svg)

This is the documentation of torchio {{#include version.md}}. The changes of each release are in the [changelog](changelog.md).

Source code and issues: https://github.com/altrdev/torchio

Using it from an AI agent: see [For AI agents](agents.md).

> **Warning:** this server is meant to consume resources on purpose. Don't expose it to the public internet.
