const express = require('express');
const cors = require('cors');
const os = require('os');
const v8 = require('v8');
const vm = require('vm');
const {fork} = require('child_process');

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
};

const app = express();
app.use(express.json());
app.use(cors());

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
app.get('/delay', (req, res) => {
    let ms = parseInt(req.query.ms);
    if (isNaN(ms)) ms = config.defaultDelayMs;
    if (ms < 0 || ms > config.maxDelayMs) {
        return res.status(400).end(`ms must be between 0 and ${config.maxDelayMs}`);
    }
    const timer = setTimeout(() => res.status(200).end(`Answered after ${ms} ms`), ms);
    res.on('close', () => clearTimeout(timer));
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
    workers.forEach((worker) => worker.removeAllListeners('exit') && worker.kill());
    process.exit(0);
}

process.on('SIGTERM', shutdown);
process.on('SIGINT', shutdown);
