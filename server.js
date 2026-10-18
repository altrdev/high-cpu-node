const express = require('express');
const cors = require('cors');
const os = require('os');
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
