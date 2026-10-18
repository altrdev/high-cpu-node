const {test, describe, before, after} = require('node:test');
const assert = require('node:assert/strict');
const {spawn, spawnSync, execFileSync} = require('node:child_process');
const fs = require('node:fs');
const net = require('node:net');
const os = require('node:os');
const path = require('node:path');
const {version} = require('../package.json');

const ROOT = path.join(__dirname, '..');
const MB = 1024 * 1024;
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function freePort() {
    return new Promise((resolve, reject) => {
        const probe = net.createServer();
        probe.on('error', reject);
        probe.listen(0, '127.0.0.1', () => {
            const {port} = probe.address();
            probe.close(() => resolve(port));
        });
    });
}

// Runs a real server on a free port, with small limits and its own DISK_DIR
async function startServer(env = {}, prepare = () => {}) {
    const port = await freePort();
    const diskDir = fs.mkdtempSync(path.join(os.tmpdir(), 'torchio-test-'));
    prepare(diskDir);

    const child = spawn(process.execPath, ['server.js'], {
        cwd: ROOT,
        env: {
            ...process.env,
            PORT: String(port),
            WORKERS: '2',
            MAX_MEMORY_MB: '300',
            MAX_DISK_MB: '300',
            MAX_HOLD_SECONDS: '30',
            MAX_DELAY_MS: '5000',
            DISK_DIR: diskDir,
            ...env,
        },
        stdio: ['ignore', 'pipe', 'pipe'],
    });
    const exited = new Promise((resolve) => child.once('exit', resolve));

    const server = {child, port, diskDir, exited, output: '', url: `http://127.0.0.1:${port}`};
    for (const stream of [child.stdout, child.stderr]) {
        stream.on('data', (data) => {
            server.output += data;
        });
    }
    // a request that never answers must fail the test instead of hanging the run
    server.get = (route, options) => fetch(server.url + route, {signal: AbortSignal.timeout(10000), ...options});
    server.text = async (route) => (await server.get(route)).text();
    server.info = async () => (await server.get('/info')).json();
    server.diskFiles = () => fs.readdirSync(diskDir).filter((name) => name.startsWith('torchio-disk-'));
    server.stop = async () => {
        if (child.exitCode === null) {
            child.kill('SIGTERM');
            await exited;
        }
        fs.rmSync(diskDir, {recursive: true, force: true});
    };

    for (let i = 0; i < 100; i++) {
        if (child.exitCode !== null) throw new Error('the server exited while starting');
        try {
            if ((await server.get('/')).ok) return server;
        } catch (e) {
            // not listening yet
        }
        await sleep(100);
    }
    await server.stop();
    throw new Error('the server did not start');
}

async function waitFor(check, timeoutMs = 5000, what = 'the condition') {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
        if (await check()) return;
        await sleep(100);
    }
    throw new Error(`timed out waiting for ${what}`);
}

describe('endpoints', () => {
    let server;
    before(async () => {
        server = await startServer();
    });
    after(() => server.stop());

    test('/ answers as a health check', async () => {
        assert.equal(await server.text('/'), 'Status: OK');
    });

    test('/stress calculates and rejects values out of range', async () => {
        assert.match(await server.text('/stress?number=20'), /result: 6765$/);
        assert.equal((await server.get('/stress?number=999')).status, 400);
        assert.equal((await server.get('/stress?number=-1')).status, 400);
    });

    test('/delay waits without blocking the server', async () => {
        const started = Date.now();
        const slow = server.text('/delay?ms=300');
        await sleep(50);
        const quickStarted = Date.now();
        await server.text('/');
        assert.ok(Date.now() - quickStarted < 200, 'another request was blocked by the delay');
        assert.equal(await slow, 'Answered after 300 ms');
        assert.ok(Date.now() - started >= 290);
        assert.equal((await server.get('/delay?ms=6000')).status, 400);
        assert.equal((await server.get('/delay?ms=-1')).status, 400);
    });

    test('/delay stops counting a request the client gave up on', async () => {
        const controller = new AbortController();
        const request = server.get('/delay?ms=4000', {signal: controller.signal}).catch(() => {});
        await waitFor(async () => (await server.info()).delay.inFlight === 1, 2000, 'the delay to be counted');
        controller.abort();
        await request;
        await waitFor(async () => (await server.info()).delay.inFlight === 0, 2000, 'the delay to be forgotten');
    });

    test('/status/:code answers with the requested status', async () => {
        assert.equal((await server.get('/status/503')).status, 503);
        assert.equal((await server.get('/status/204')).status, 204);
        const ok = await server.get('/status/200');
        assert.equal(await ok.text(), 'Status: 200');
        for (const code of ['199', '600', 'abc', '2.5']) {
            assert.equal((await server.get(`/status/${code}`)).status, 400, code);
        }
    });

    test('/memory holds memory within its budget and releases it', async () => {
        const first = await server.get('/memory?mb=100&hold=1');
        assert.equal(first.status, 200);
        assert.equal((await server.info()).memory.heldMb, 100);
        assert.equal((await server.get('/memory?mb=250')).status, 429, 'over the budget left');
        assert.equal((await server.get('/memory?mb=301')).status, 400);
        assert.equal((await server.get('/memory?mb=0')).status, 400);
        assert.equal((await server.get('/memory?hold=31')).status, 400);
        await waitFor(async () => (await server.info()).memory.heldMb === 0, 4000, 'the memory to be released');
        assert.equal((await server.get('/memory?mb=250&hold=0')).status, 200, 'the budget is free again');
    });

    test('/disk writes a file, keeps it and removes it', async () => {
        const response = await server.get('/disk?mb=10&hold=1');
        assert.equal(response.status, 200);
        const files = server.diskFiles();
        assert.equal(files.length, 1);
        assert.equal(fs.statSync(path.join(server.diskDir, files[0])).size, 10 * MB);
        assert.equal((await server.info()).disk.heldMb, 10);
        assert.equal((await server.get('/disk?mb=301')).status, 400);
        await waitFor(() => server.diskFiles().length === 0, 4000, 'the file to be removed');
        assert.equal((await server.info()).disk.heldMb, 0);
    });

    test('/disk answers 429 when the budget is used up', async () => {
        assert.equal((await server.get('/disk?mb=200&hold=1')).status, 200);
        assert.equal((await server.get('/disk?mb=200')).status, 429);
        await waitFor(() => server.diskFiles().length === 0, 4000, 'the file to be removed');
    });

    test('/info describes the server', async () => {
        const info = await server.info();
        assert.equal(info.name, 'torchio');
        assert.equal(info.version, version);
        assert.equal(info.stress.workers, 2);
        assert.deepEqual(info.stress.perWorker, [0, 0]);
        assert.equal(info.memory.limitMb, 300);
        assert.equal(info.disk.limitMb, 300);
        assert.equal(info.disk.dir, server.diskDir);
        assert.equal(typeof info.disk.freeMb, 'number');
        assert.deepEqual(info.limits, {maxNumber: 50, maxDelayMs: 5000, maxHoldSeconds: 30});
    });

    test('/metrics is valid Prometheus text and follows the server', async () => {
        const response = await server.get('/metrics');
        assert.equal(response.headers.get('content-type'), 'text/plain; version=0.0.4; charset=utf-8');
        const body = await response.text();
        assert.ok(body.endsWith('\n'));

        const sample = /^([a-zA-Z_:][a-zA-Z0-9_:]*)(\{[^}]*\})? -?[0-9.]+(e[+-]?[0-9]+)?$/;
        const declared = new Set();
        for (const line of body.trimEnd().split('\n')) {
            if (line.startsWith('# TYPE ')) {
                declared.add(line.split(' ')[2]);
            } else if (!line.startsWith('# HELP ')) {
                const match = sample.exec(line);
                assert.ok(match, `not a sample: ${line}`);
                assert.ok(declared.has(match[1]), `no TYPE before ${match[1]}`);
            }
        }
        assert.match(body, new RegExp(`^torchio_info\\{version="${version}"\\} 1$`, 'm'));
        assert.match(body, /^torchio_workers 2$/m);
        assert.match(body, /^torchio_memory_limit_bytes 314572800$/m);

        const counter = async (code) => {
            const text = await server.text('/metrics');
            const match = new RegExp(`^torchio_http_requests_total\\{route="/status/:code",code="${code}"\\} (\\d+)$`, 'm').exec(text);
            return match ? Number(match[1]) : 0;
        };
        const before = await counter(418);
        await server.get('/status/418');
        await server.get('/status/418');
        assert.equal(await counter(418), before + 2);

        await server.get('/memory?mb=100&hold=2');
        assert.match(await server.text('/metrics'), new RegExp(`^torchio_memory_held_bytes ${100 * MB}$`, 'm'));
        await waitFor(async () => (await server.info()).memory.heldMb === 0, 4000, 'the memory to be released');
    });
});

describe('failures', () => {
    test('a worker that crashes is replaced and its request gets a 500', async () => {
        const server = await startServer({WORKERS: '1'});
        try {
            const request = server.get('/stress?number=45');
            await sleep(300);
            const pids = execFileSync('pgrep', ['-P', String(server.child.pid)]).toString().trim().split('\n');
            assert.equal(pids.length, 1);
            process.kill(Number(pids[0]), 'SIGKILL');

            const response = await request;
            assert.equal(response.status, 500);
            assert.equal(await response.text(), 'Worker crashed');

            await waitFor(async () => {
                try {
                    return (await server.get('/stress?number=10')).status === 200;
                } catch (e) {
                    return false;
                }
            }, 5000, 'the worker to be replaced');
        } finally {
            await server.stop();
        }
    });

    test('/disk answers 507 when the directory is not usable', async () => {
        const server = await startServer({DISK_DIR: '/dev/null/stress'});
        try {
            const response = await server.get('/disk?mb=1');
            assert.equal(response.status, 507);
            assert.match(await response.text(), /^Could not write 1 MB to /);
            assert.equal((await server.info()).disk.freeMb, null);
            assert.equal((await server.info()).disk.heldMb, 0);
            assert.equal(await server.text('/'), 'Status: OK');
        } finally {
            await server.stop();
        }
    });
});

describe('configuration', () => {
    test('a value that is not a positive integer is reported and replaced by the default', async () => {
        const server = await startServer({MAX_NUMBER: 'abc', MAX_DELAY_MS: '0', WORKERS: '-2', MAX_HOLD_SECONDS: '12'});
        try {
            const info = await server.info();
            assert.equal(info.limits.maxNumber, 50);
            assert.equal(info.limits.maxDelayMs, 60000);
            assert.equal(info.limits.maxHoldSeconds, 12, 'a valid value is used');
            assert.equal(info.stress.workers, os.cpus().length);
            assert.match(server.output, /MAX_NUMBER=abc is not a positive integer, using 50/);
            assert.match(server.output, /MAX_DELAY_MS=0 is not a positive integer, using 60000/);
            assert.match(server.output, /WORKERS=-2 is not a positive integer/);
            assert.doesNotMatch(server.output, /MAX_HOLD_SECONDS/);
        } finally {
            await server.stop();
        }
    });
});

describe('files of /disk', () => {
    test('leftovers of a previous run are removed at startup, other files are kept', async () => {
        const server = await startServer({}, (dir) => {
            fs.writeFileSync(path.join(dir, 'torchio-disk-old'), 'leftover');
            fs.writeFileSync(path.join(dir, 'keep.txt'), 'mine');
        });
        try {
            assert.deepEqual(fs.readdirSync(server.diskDir), ['keep.txt']);
        } finally {
            await server.stop();
        }
    });

    test('files still held are removed when the server shuts down', async () => {
        const server = await startServer();
        try {
            assert.equal((await server.get('/disk?mb=5&hold=30')).status, 200);
            assert.equal(server.diskFiles().length, 1);
            server.child.kill('SIGTERM');
            await server.exited;
            assert.equal(server.diskFiles().length, 0);
        } finally {
            await server.stop();
        }
    });
});

describe('cli watch', () => {
    test('--once prints the figures of a running server', async () => {
        const server = await startServer();
        try {
            await server.get('/memory?mb=100&hold=5');
            const output = execFileSync(process.execPath, ['cli.js', 'watch', '--once', '--url', server.url], {cwd: ROOT}).toString();
            assert.match(output, new RegExp(`torchio ${version.replace(/\./g, '\\.')}`));
            assert.match(output, /CPU workers .* 0 busy \/ 2/);
            assert.match(output, /Memory held .* 100 \/ 300 MB/);
            assert.match(output, /Disk held .* 0 \/ 300 MB/);
        } finally {
            await server.stop();
        }
    });

    test('--once exits with 1 when the server can not be reached', async () => {
        const port = await freePort();
        const result = spawnSync(process.execPath, ['cli.js', 'watch', '--once', '--url', `http://127.0.0.1:${port}`], {cwd: ROOT});
        assert.equal(result.status, 1);
        assert.match(result.stderr.toString(), /Cannot reach/);
    });

    test('an unknown command prints the usage and exits with 1', () => {
        const result = spawnSync(process.execPath, ['cli.js', 'nope'], {cwd: ROOT});
        assert.equal(result.status, 1);
        assert.match(result.stdout.toString(), /^Usage: node cli\.js watch/);
    });
});
