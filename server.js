const express = require('express');
const cors = require('cors');
const os = require('os');
const fs = require('fs');
const path = require('path');
const v8 = require('v8');
const vm = require('vm');
const {fork} = require('child_process');
const {version} = require('./package.json');

const config = {
    name: 'stress-node',
    port: parseInt(process.env.PORT) || 3000,
    host: '0.0.0.0',
    workers: parseInt(process.env.WORKERS) || os.cpus().length,
    defaultNumber: 45,
    maxNumber: parseInt(process.env.MAX_NUMBER) || 50,
    defaultDelayMs: 1000,
    maxDelayMs: parseInt(process.env.MAX_DELAY_MS) || 60000,
    defaultMemoryMb: 100,
    maxMemoryMb: parseInt(process.env.MAX_MEMORY_MB) || 512,
    defaultHoldSeconds: 10,
    maxHoldSeconds: parseInt(process.env.MAX_HOLD_SECONDS) || 300,
    defaultDiskMb: 100,
    maxDiskMb: parseInt(process.env.MAX_DISK_MB) || 1024,
    diskDir: process.env.DISK_DIR || os.tmpdir(),
};

const app = express();
app.use(express.json());
app.use(cors());

// Requests answered, by route and status code, for /metrics. The route is the
// pattern (/status/:code), not the URL, so the number of series stays small.
const httpRequests = new Map();
app.use((req, res, next) => {
    res.on('finish', () => {
        const key = `${req.route ? req.route.path : 'other'}\u0000${res.statusCode}`;
        httpRequests.set(key, (httpRequests.get(key) || 0) + 1);
    });
    next();
});

// Worker pool: one child process per core, requests are dispatched round-robin
const pending = new Map(); // request id -> {res, worker}
const workers = [];
let nextWorker = 0;
let nextId = 0;

function spawnWorker(index) {
    const worker = fork(`${__dirname}/fibonacci.js`);

    worker.on('message', ({id, value}) => {
        const request = pending.get(id);
        if (!request) return; // client already disconnected
        pending.delete(id);
        request.res.status(200).end(`I'm still alive after fibonacci calculation with result: ${value}`);
    });

    worker.on('exit', (code, signal) => {
        console.error(`worker ${index} exited (code: ${code}, signal: ${signal}), restarting`);
        for (const [id, request] of pending) {
            if (request.worker === worker) {
                pending.delete(id);
                request.res.status(500).end('Worker crashed');
            }
        }
        workers[index] = spawnWorker(index);
    });

    return worker;
}

for (let i = 0; i < config.workers; i++) {
    workers.push(spawnWorker(i));
}

app.get('/', (req, res) => {
    res.status(200).end('Status: OK');
});

app.get('/stress', (req, res) => {
    let number = parseInt(req.query.number);
    if (isNaN(number)) number = config.defaultNumber;
    if (number < 0 || number > config.maxNumber) {
        return res.status(400).end(`number must be between 0 and ${config.maxNumber}`);
    }
    stress(number, res);
});

// Answers after `ms` milliseconds without blocking the event loop
let delayInFlight = 0;
app.get('/delay', (req, res) => {
    let ms = parseInt(req.query.ms);
    if (isNaN(ms)) ms = config.defaultDelayMs;
    if (ms < 0 || ms > config.maxDelayMs) {
        return res.status(400).end(`ms must be between 0 and ${config.maxDelayMs}`);
    }
    delayInFlight++;
    let done = false;
    const finish = () => {
        if (done) return;
        done = true;
        delayInFlight--;
        clearTimeout(timer);
    };
    const timer = setTimeout(() => {
        finish();
        res.status(200).end(`Answered after ${ms} ms`);
    }, ms);
    res.on('close', finish);
});

// Answers with the requested HTTP status code, to test probes and retries
app.get('/status/:code', (req, res) => {
    const code = Number(req.params.code);
    if (!Number.isInteger(code) || code < 200 || code > 599) {
        return res.status(400).end('code must be an integer between 200 and 599');
    }
    res.status(code).end(`Status: ${code}`);
});

// Allocates `mb` megabytes and holds them for `hold` seconds. MAX_MEMORY_MB is the
// total that can be held at the same time, so concurrent requests can't grow it unbounded.
let heldMb = 0;

// Buffers are only returned to the OS when the garbage collector runs, so collect
// right after releasing instead of waiting for V8 to decide
v8.setFlagsFromString('--expose-gc');
const gc = vm.runInNewContext('gc');

app.get('/memory', (req, res) => {
    let mb = parseInt(req.query.mb);
    if (isNaN(mb)) mb = config.defaultMemoryMb;
    let hold = parseInt(req.query.hold);
    if (isNaN(hold)) hold = config.defaultHoldSeconds;

    if (mb < 1 || mb > config.maxMemoryMb) {
        return res.status(400).end(`mb must be between 1 and ${config.maxMemoryMb}`);
    }
    if (hold < 0 || hold > config.maxHoldSeconds) {
        return res.status(400).end(`hold must be between 0 and ${config.maxHoldSeconds}`);
    }
    if (heldMb + mb > config.maxMemoryMb) {
        return res.status(429).end(`${heldMb} MB already held, limit is ${config.maxMemoryMb} MB`);
    }

    heldMb += mb;
    // fill() touches every page, otherwise the OS may not actually commit the memory
    let buffer = Buffer.allocUnsafe(mb * 1024 * 1024).fill(1);
    setTimeout(() => {
        buffer = null;
        heldMb -= mb;
        gc();
    }, hold * 1000);

    res.status(200).end(`Allocated ${mb} MB, held for ${hold} s`);
});

// Writes `mb` megabytes to a file in DISK_DIR and keeps it for `hold` seconds. As for
// /memory, MAX_DISK_MB is the total that can be held at the same time.
const diskPrefix = 'stress-node-disk-';
const diskChunk = Buffer.alloc(1024 * 1024, 1);
const diskFiles = new Set();
let heldDiskMb = 0;
let nextDiskId = 0;

try {
    fs.mkdirSync(config.diskDir, {recursive: true});
    // remove files left behind by a previous run that didn't shut down cleanly
    for (const name of fs.readdirSync(config.diskDir)) {
        if (name.startsWith(diskPrefix)) fs.rmSync(path.join(config.diskDir, name), {force: true});
    }
} catch (e) {
    console.error(`DISK_DIR ${config.diskDir} is not usable, /disk will answer 507: ${e.code || e.message}`);
}

async function writeDiskFile(file, mb) {
    const handle = await fs.promises.open(file, 'w');
    try {
        for (let i = 0; i < mb; i++) {
            await handle.write(diskChunk, 0, diskChunk.length);
        }
        await handle.sync();
    } finally {
        await handle.close();
    }
}

app.get('/disk', async (req, res) => {
    let mb = parseInt(req.query.mb);
    if (isNaN(mb)) mb = config.defaultDiskMb;
    let hold = parseInt(req.query.hold);
    if (isNaN(hold)) hold = config.defaultHoldSeconds;

    if (mb < 1 || mb > config.maxDiskMb) {
        return res.status(400).end(`mb must be between 1 and ${config.maxDiskMb}`);
    }
    if (hold < 0 || hold > config.maxHoldSeconds) {
        return res.status(400).end(`hold must be between 0 and ${config.maxHoldSeconds}`);
    }
    if (heldDiskMb + mb > config.maxDiskMb) {
        return res.status(429).end(`${heldDiskMb} MB already held, limit is ${config.maxDiskMb} MB`);
    }

    const file = path.join(config.diskDir, `${diskPrefix}${nextDiskId++}`);
    heldDiskMb += mb;
    diskFiles.add(file);
    const release = async () => {
        diskFiles.delete(file);
        heldDiskMb -= mb;
        await fs.promises.rm(file, {force: true}).catch(() => {});
    };

    try {
        await writeDiskFile(file, mb);
    } catch (e) {
        await release();
        return res.status(507).end(`Could not write ${mb} MB to ${config.diskDir}: ${e.code || e.message}`);
    }
    setTimeout(release, hold * 1000);
    res.status(200).end(`Wrote ${mb} MB to ${config.diskDir}, held for ${hold} s`);
});

// What the server is doing right now, so you can see the remaining budgets
// without having to hit a 429 or a 507
const startedAt = Date.now();
const MB = 1024 * 1024;
const toMb = (bytes) => Math.round(bytes / MB);

// Raw figures shared by /info (in MB) and /metrics (in bytes)
async function collect() {
    const perWorker = workers.map(() => 0);
    for (const request of pending.values()) {
        perWorker[workers.indexOf(request.worker)]++;
    }

    // 0 or a huge value when the process has no memory limit
    const constrained = process.constrainedMemory();
    const containerLimitBytes = constrained > 0 && constrained < os.totalmem() ? constrained : null;

    let diskFreeBytes = null;
    try {
        const stats = await fs.promises.statfs(config.diskDir);
        diskFreeBytes = stats.bavail * stats.bsize;
    } catch (e) {
        // directory not usable, same case as the 507 of /disk
    }

    return {perWorker, containerLimitBytes, diskFreeBytes, rssBytes: process.memoryUsage().rss};
}

app.get('/info', async (req, res) => {
    const {perWorker, containerLimitBytes, diskFreeBytes, rssBytes} = await collect();

    res.json({
        name: config.name,
        version,
        uptimeSeconds: Math.round((Date.now() - startedAt) / 1000),
        stress: {workers: workers.length, inFlight: pending.size, perWorker},
        delay: {inFlight: delayInFlight},
        memory: {
            heldMb,
            limitMb: config.maxMemoryMb,
            processRssMb: toMb(rssBytes),
            containerLimitMb: containerLimitBytes === null ? null : toMb(containerLimitBytes),
        },
        disk: {
            heldMb: heldDiskMb,
            limitMb: config.maxDiskMb,
            dir: config.diskDir,
            freeMb: diskFreeBytes === null ? null : toMb(diskFreeBytes),
        },
        limits: {
            maxNumber: config.maxNumber,
            maxDelayMs: config.maxDelayMs,
            maxHoldSeconds: config.maxHoldSeconds,
        },
    });
});

// The same figures in the Prometheus text format
const escapeLabel = (value) => String(value).replace(/\\/g, '\\\\').replace(/\n/g, '\\n').replace(/"/g, '\\"');

function metric(lines, name, type, help, samples) {
    if (samples.length === 0) return;
    lines.push(`# HELP ${name} ${help}`, `# TYPE ${name} ${type}`);
    for (const [labels, value] of samples) {
        const text = labels ? `{${Object.entries(labels).map(([k, v]) => `${k}="${escapeLabel(v)}"`).join(',')}}` : '';
        lines.push(`${name}${text} ${value}`);
    }
}

app.get('/metrics', async (req, res) => {
    const {perWorker, containerLimitBytes, diskFreeBytes, rssBytes} = await collect();
    const lines = [];

    metric(lines, 'stress_node_info', 'gauge', 'Version of the server.', [[{version}, 1]]);
    metric(lines, 'stress_node_start_time_seconds', 'gauge', 'Start time of the server, in Unix time.', [[null, startedAt / 1000]]);
    metric(lines, 'stress_node_workers', 'gauge', 'Worker processes for /stress.', [[null, workers.length]]);
    metric(lines, 'stress_node_stress_requests_in_flight', 'gauge', '/stress requests being calculated.', [[null, pending.size]]);
    metric(lines, 'stress_node_worker_requests_in_flight', 'gauge', '/stress requests being calculated, by worker.',
        perWorker.map((count, index) => [{worker: index}, count]));
    metric(lines, 'stress_node_delay_requests_in_flight', 'gauge', '/delay requests still waiting.', [[null, delayInFlight]]);
    metric(lines, 'stress_node_memory_held_bytes', 'gauge', 'Memory held by /memory.', [[null, heldMb * MB]]);
    metric(lines, 'stress_node_memory_limit_bytes', 'gauge', 'Total memory /memory may hold (MAX_MEMORY_MB).', [[null, config.maxMemoryMb * MB]]);
    metric(lines, 'process_resident_memory_bytes', 'gauge', 'Resident memory of the server process.', [[null, rssBytes]]);
    metric(lines, 'stress_node_container_memory_limit_bytes', 'gauge', 'Memory limit of the container, when there is one.',
        containerLimitBytes === null ? [] : [[null, containerLimitBytes]]);
    metric(lines, 'stress_node_disk_held_bytes', 'gauge', 'Disk space held by /disk.', [[null, heldDiskMb * MB]]);
    metric(lines, 'stress_node_disk_limit_bytes', 'gauge', 'Total disk space /disk may hold (MAX_DISK_MB).', [[null, config.maxDiskMb * MB]]);
    metric(lines, 'stress_node_disk_free_bytes', 'gauge', 'Free space in DISK_DIR.', diskFreeBytes === null ? [] : [[null, diskFreeBytes]]);
    metric(lines, 'stress_node_http_requests_total', 'counter', 'Requests answered, by route and status code.',
        [...httpRequests].map(([key, count]) => {
            const [route, code] = key.split('\u0000');
            return [{route, code}, count];
        }));

    res.type('text/plain; version=0.0.4; charset=utf-8').send(lines.join('\n') + '\n');
});

function stress(number, res) {
    const id = nextId++;
    const worker = workers[nextWorker];
    nextWorker = (nextWorker + 1) % workers.length;

    pending.set(id, {res, worker});
    res.on('close', () => pending.delete(id));
    worker.send({id, num: number});
}

const server = app.listen(config.port, config.host, () => {
    console.log(`${config.name} running on ${config.host}:${config.port} with ${config.workers} workers`);
});

server.on('error', (e) => {
    console.error(`Internal Server Error: ${e}`);
    process.exit(1);
});

function shutdown() {
    server.close();
    diskFiles.forEach((file) => fs.rmSync(file, {force: true}));
    workers.forEach((worker) => worker.removeAllListeners('exit') && worker.kill());
    process.exit(0);
}

process.on('SIGTERM', shutdown);
process.on('SIGINT', shutdown);
