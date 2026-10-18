const os = require('os');

// A positive integer from the environment. A value that is not one is reported
// and replaced by the default, instead of being silently ignored.
function positiveInt(name, fallback) {
    const raw = process.env[name];
    if (raw === undefined || raw === '') return fallback;
    const value = Number(raw);
    if (!Number.isInteger(value) || value < 1) {
        console.warn(`${name}=${raw} is not a positive integer, using ${fallback}`);
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
    diskDir: process.env.DISK_DIR || os.tmpdir(),
};
