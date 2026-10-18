const express = require('express');
const cors = require('cors');
const {version} = require('../package.json');
const {WorkerPool} = require('./workers');
const {createRequestCounter} = require('./counters');
const createSnapshot = require('./snapshot');
const createStress = require('./routes/stress');
const createDelay = require('./routes/delay');
const status = require('./routes/status');
const createMemory = require('./routes/memory');
const createDisk = require('./routes/disk');
const createInfo = require('./routes/info');
const createMetrics = require('./routes/metrics');

// Builds the Express app and what it keeps (workers, budgets, files). `stop` releases them.
function createApp(config) {
    const startedAt = Date.now();
    const app = express();
    app.use(express.json());
    app.use(cors());

    const counters = createRequestCounter();
    app.use(counters.middleware);

    const pool = new WorkerPool(config.workers);
    const delay = createDelay(config);
    const memory = createMemory(config);
    const disk = createDisk(config);
    const snapshot = createSnapshot({config, pool, delay, memory, disk});

    app.get('/', (req, res) => res.status(200).end('Status: OK'));
    app.get('/stress', createStress(config, pool));
    app.get('/delay', delay.handler);
    app.get('/status/:code', status);
    app.get('/memory', memory.handler);
    app.get('/disk', disk.handler);
    app.get('/info', createInfo({config, version, startedAt, snapshot}));
    app.get('/metrics', createMetrics({version, startedAt, snapshot, counters}));

    const stop = () => {
        disk.removeFiles();
        pool.stop();
    };

    return {app, stop};
}

module.exports = {createApp};
