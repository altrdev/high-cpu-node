# How it works

`/stress` requests are spread round-robin over a pool of worker threads (one for each CPU by default), so concurrent requests can saturate multiple cores. The other endpoints run on an async runtime (tokio), so a slow request never blocks the rest. The memory of `/memory` and `/leak` comes straight from the OS and goes back as soon as it is released.
