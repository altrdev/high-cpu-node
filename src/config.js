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

// TRUST_PROXY as Express reads it: true, a number of hops, or addresses/names (loopback, 10.0.0.0/8)
function trustProxy() {
    const raw = process.env.TRUST_PROXY;
    if (raw === undefined || raw === '' || raw === 'false') return false;
    if (raw === 'true') return true;
    return /^\d+$/.test(raw) ? Number(raw) : raw;
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
    token: process.env.TOKEN || '',
    ui: ['1', 'true'].includes(process.env.UI),
    defaultCpuPercent: 50,
    defaultLeakRateMb: 10,
    startDelaySeconds: positiveInt('START_DELAY_SECONDS', 0, 0),
    defaultFd: 100,
    maxFd: positiveInt('MAX_FD', 10000),
    defaultNotReadySeconds: 30,
    defaultLogLines: 1000,
    maxLogLines: positiveInt('MAX_LOG_LINES', 100000),
    defaultLogRate: 100,
    maxLogRate: positiveInt('MAX_LOG_RATE', 10000),
    trustProxy: trustProxy(),
    diskDir: process.env.DISK_DIR || os.tmpdir(),
};
