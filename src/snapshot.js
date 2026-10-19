const fs = require('fs');
const os = require('os');

// What the server is doing right now, in raw units, shared by /info (MB) and /metrics (bytes)
module.exports = function createSnapshot({config, pool, delay, memory, disk, fd, ready, log}) {
    return async () => {
        // 0 or a huge value when the process has no memory limit
        const constrained = process.constrainedMemory();

        let diskFreeBytes = null;
        try {
            const stats = await fs.promises.statfs(config.diskDir);
            diskFreeBytes = stats.bavail * stats.bsize;
        } catch (e) {
            // directory not usable, same case as the 507 of /disk
        }

        return {
            stress: pool.snapshot(),
            delayInFlight: delay.inFlight(),
            memory: {
                heldMb: memory.budget.held,
                limitMb: memory.budget.limit,
                rssBytes: process.memoryUsage().rss,
                containerLimitBytes: constrained > 0 && constrained < os.totalmem() ? constrained : null,
            },
            disk: {
                heldMb: disk.budget.held,
                limitMb: disk.budget.limit,
                dir: config.diskDir,
                freeBytes: diskFreeBytes,
            },
            fd: {held: fd.budget.held, limit: fd.budget.limit},
            ready: ready.state(),
            log: log.state(),
        };
    };
};
