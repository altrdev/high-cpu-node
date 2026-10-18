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

describe('/ready, /crash and /fd', () => {
    let server;
    before(async () => {
        server = await startServer({MAX_FD: '50'});
    });
    after(() => server.stop());

    test('/ready switches to 503 and back, on request and by itself', async () => {
        assert.equal((await server.get('/ready')).status, 200);

        assert.equal((await server.get('/ready/off?seconds=1')).status, 200);
        const down = await server.get('/ready');
        assert.equal(down.status, 503);
        assert.match(await down.text(), /^Not ready, ready again in \d s$/);
        assert.equal((await server.info()).ready.ready, false);
        assert.match(await server.text('/metrics'), /^torchio_ready 0$/m);
        assert.equal((await server.get('/')).status, 200, 'the liveness check must keep answering');
        await waitFor(async () => (await server.get('/ready')).status === 200, 3000, 'the server to be ready again');

        await server.get('/ready/off?seconds=0');
        await sleep(1200);
        assert.equal(await server.text('/ready'), 'Not ready');
        assert.equal((await server.get('/ready/on')).status, 200);
        assert.equal((await server.get('/ready')).status, 200);
        assert.match(await server.text('/metrics'), /^torchio_ready 1$/m);

        assert.equal((await server.get('/ready/off?seconds=31')).status, 400);
        assert.equal((await server.get('/ready/off?seconds=-1')).status, 400);
    });

    test('/fd holds descriptors within its budget and releases them', async () => {
        const first = await server.get('/fd?count=30&hold=1');
        assert.equal(first.status, 200);
        assert.equal((await server.info()).fd.held, 30);
        assert.equal((await server.get('/fd?count=30&hold=1')).status, 429);
        assert.equal((await server.get('/fd?count=51')).status, 400);
        assert.equal((await server.get('/fd?count=0')).status, 400);
        await waitFor(async () => (await server.info()).fd.held === 0, 3000, 'the descriptors to be released');
        assert.match(await server.text('/metrics'), /^torchio_fd_limit 50$/m);
    });
});

describe('/crash', () => {
    test('ends the process with the exit code asked for', async () => {
        const server = await startServer();
        try {
            assert.match(await server.text('/crash?code=42'), /^Exiting with code 42$/);
            assert.equal(await server.exited, 42);
        } finally {
            await server.stop();
        }
    });

    test('can wait before exiting and checks its parameters', async () => {
        const server = await startServer();
        try {
            assert.equal((await server.get('/crash?code=256')).status, 400);
            assert.equal((await server.get('/crash?after=31')).status, 400);
            const started = Date.now();
            assert.match(await server.text('/crash?code=3&after=1'), /^Exiting with code 3 in 1 s$/);
            assert.equal((await server.get('/')).status, 200, 'the server must still answer before the exit');
            assert.equal(await server.exited, 3);
            assert.ok(Date.now() - started >= 900);
        } finally {
            await server.stop();
        }
    });
});

describe('/cpu, /leak and START_DELAY_SECONDS', () => {
    test('/cpu keeps the workers busy for a while, and can be stopped', async () => {
        const server = await startServer();
        try {
            assert.equal(await server.text('/cpu?percent=40&hold=20'), 'Burning 40% of 2 workers for 20 s');
            const {burn} = (await server.info()).stress;
            assert.equal(burn.percent, 40);
            assert.ok(burn.secondsLeft > 15 && burn.secondsLeft <= 20);
            assert.match(await server.text('/metrics'), /^torchio_cpu_burn_percent 40$/m);
            assert.match(await server.text('/stress?number=20'), /result: 6765$/, '/stress still works while /cpu runs');

            assert.equal(await server.text('/cpu?percent=0'), 'Stopped burning CPU');
            assert.equal((await server.info()).stress.burn, null);
            assert.match(await server.text('/metrics'), /^torchio_cpu_burn_percent 0$/m);

            assert.equal((await server.get('/cpu?percent=101')).status, 400);
            assert.equal((await server.get('/cpu?percent=-1')).status, 400);
            assert.equal((await server.get('/cpu?hold=31')).status, 400);
        } finally {
            await server.stop();
        }
    });

    test('/cpu really uses the CPU of the workers', async () => {
        const server = await startServer({WORKERS: '1'});
        try {
            await server.get('/cpu?percent=100&hold=5');
            await sleep(500);
            const worker = execFileSync('pgrep', ['-P', String(server.child.pid)]).toString().trim();
            const cpu = () => Number(execFileSync('ps', ['-o', 'cputime=', '-p', worker]).toString().trim().split(':').pop());
            const before = cpu();
            await sleep(2000);
            assert.ok(cpu() - before >= 1, 'the worker should have spent at least a second of CPU in two');
        } finally {
            await server.stop();
        }
    });

    test('/leak grows by itself within the budget of /memory and releases it', async () => {
        const server = await startServer();
        try {
            assert.equal(await server.text('/leak?rate=20&mb=60&hold=1'), 'Leaking 20 MB/s up to 60 MB (3 s), then held for 1 s');
            assert.equal((await server.info()).memory.heldMb, 20, 'the first step is taken right away');
            await waitFor(async () => (await server.info()).memory.heldMb === 60, 4000, 'the leak to reach its ceiling');
            await waitFor(async () => (await server.info()).memory.heldMb === 0, 4000, 'the leak to be released');
        } finally {
            await server.stop();
        }
    });

    test('/leak stops growing when the budget is full and checks its parameters', async () => {
        const server = await startServer();
        try {
            await server.get('/memory?mb=250&hold=4');
            await server.get('/leak?rate=40&mb=200&hold=3');
            await sleep(1500);
            // 250 + 40 fits in the 300 MB budget, the next 40 does not: it stops there
            assert.equal((await server.info()).memory.heldMb, 290);
            await waitFor(async () => (await server.info()).memory.heldMb === 0, 6000, 'everything to be released');

            await server.get('/memory?mb=300&hold=2');
            assert.equal((await server.get('/leak?mb=10')).status, 429);
            assert.equal((await server.get('/leak?mb=301')).status, 400);
            assert.equal((await server.get('/leak?rate=0')).status, 400);
        } finally {
            await server.stop();
        }
    });

    test('START_DELAY_SECONDS keeps the port closed for a while', async () => {
        const started = Date.now();
        const server = await startServer({START_DELAY_SECONDS: '2'});
        try {
            assert.ok(Date.now() - started >= 1800, 'it answered before the delay');
            assert.match(server.output, /will start listening in 2 s/);
            assert.equal(await server.text('/'), 'Status: OK');
        } finally {
            await server.stop();
        }
    });

    test('a START_DELAY_SECONDS that is not a number is reported', async () => {
        const server = await startServer({START_DELAY_SECONDS: '-1'});
        try {
            assert.match(server.output, /START_DELAY_SECONDS=-1 is not a non-negative integer, using 0/);
        } finally {
            await server.stop();
        }
    });
});

describe('/ui', () => {
    test('is not there unless UI=1', async () => {
        const server = await startServer();
        try {
            assert.equal((await server.get('/ui')).status, 404);
        } finally {
            await server.stop();
        }
    });

    test('serves the control panel with UI=1, and its script is valid', async () => {
        const server = await startServer({UI: '1'});
        try {
            const response = await server.get('/ui');
            assert.equal(response.status, 200);
            assert.match(response.headers.get('content-type'), /^text\/html/);
            const html = await response.text();
            assert.match(html, /<title>torchio<\/title>/);
            const script = html.match(/<script>([\s\S]*)<\/script>/)[1];
            assert.doesNotThrow(() => new (require('node:vm').Script)(script), 'the script of the page has a syntax error');
            // what the page calls must exist
            for (const path of ['/info', '/metrics', '/cpu', '/leak', '/memory', '/disk', '/fd', '/ready/off', '/ready/on', '/delay', '/crash', '/stress']) {
                assert.ok(script.includes(`'${path}`) || script.includes(`path: '${path}`), `the page does not use ${path}`);
            }
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
            assert.match(output, /FDs held .* 0 \/ 10000/);
            assert.match(output, /readiness: ready/);
        } finally {
            await server.stop();
        }
    });

    test('--once shows when the server is not ready and the descriptors held', async () => {
        const server = await startServer();
        try {
            await server.get('/ready/off?seconds=20');
            await server.get('/fd?count=40&hold=5');
            const output = execFileSync(process.execPath, ['cli.js', 'watch', '--once', '--url', server.url], {cwd: ROOT}).toString();
            assert.match(output, /FDs held .* 40 \/ 10000/);
            assert.match(output, /readiness: NOT READY \(ready again in \d+ s\)/);
        } finally {
            await server.stop();
        }
    });

    test('--once shows what /cpu is doing', async () => {
        const server = await startServer();
        try {
            await server.get('/cpu?percent=30&hold=20');
            const output = execFileSync(process.execPath, ['cli.js', 'watch', '--once', '--url', server.url], {cwd: ROOT}).toString();
            assert.match(output, /\/cpu: 30% on every worker, \d+ s left/);
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
