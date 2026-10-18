const {toMb} = require('../units');

// What the server is doing right now, so you can see the remaining budgets
// without having to hit a 429 or a 507
module.exports = function createInfo({config, version, startedAt, snapshot}) {
    return async (req, res) => {
        const {stress, delayInFlight, memory, disk, fd, ready} = await snapshot();

        res.json({
            name: config.name,
            version,
            uptimeSeconds: Math.round((Date.now() - startedAt) / 1000),
            auth: Boolean(config.token),
            stress,
            delay: {inFlight: delayInFlight},
            memory: {
                heldMb: memory.heldMb,
                limitMb: memory.limitMb,
                processRssMb: toMb(memory.rssBytes),
                containerLimitMb: memory.containerLimitBytes === null ? null : toMb(memory.containerLimitBytes),
            },
            disk: {
                heldMb: disk.heldMb,
                limitMb: disk.limitMb,
                dir: disk.dir,
                freeMb: disk.freeBytes === null ? null : toMb(disk.freeBytes),
            },
            fd: {held: fd.held, limit: fd.limit},
            ready,
            limits: {
                maxNumber: config.maxNumber,
                maxDelayMs: config.maxDelayMs,
                maxHoldSeconds: config.maxHoldSeconds,
            },
        });
    };
};
