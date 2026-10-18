const os = require('os');

// An integer from the environment, at least `min` (a positive one by default). A value that
// is not one is reported and replaced by the default, instead of being silently ignored.
function positiveInt(name, fallback, min = 1) {
    const raw = process.env[name];
    if (raw === undefined || raw === '') return fallback;
    const value = Number(raw);
    if (!Number.isInteger(value) || value < min) {
        console.warn(`${name}=${raw} is not ${min === 0 ? 'a non-negative' : 'a positive'} integer, using ${fallback}`);
        return fallback;
    }
    return value;
}

module.exports = {
    name: 'torchio',
    port: positiveInt('PORT', 3000),
    host: '0.0.0.0',
    workers: positiveInt('WORKERS', os.cpus().length),
    defaultNumber: 45,
    maxNumber: positiveInt('MAX_NUMBER', 50),
    defaultDelayMs: 1000,
    maxDelayMs: positiveInt('MAX_DELAY_MS', 60000),
    defaultMemoryMb: 100,
    maxMemoryMb: positiveInt('MAX_MEMORY_MB', 512),
    defaultHoldSeconds: 10,
    maxHoldSeconds: positiveInt('MAX_HOLD_SECONDS', 300),
    defaultDiskMb: 100,
    maxDiskMb: positiveInt('MAX_DISK_MB', 1024),
    defaultCpuPercent: 50,
    defaultLeakRateMb: 10,
    startDelaySeconds: positiveInt('START_DELAY_SECONDS', 0, 0),
    defaultFd: 100,
    maxFd: positiveInt('MAX_FD', 10000),
    defaultNotReadySeconds: 30,
    diskDir: process.env.DISK_DIR || os.tmpdir(),
};
