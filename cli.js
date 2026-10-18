#!/usr/bin/env node
// Terminal view of what a torchio server is doing, built on its /info endpoint.
//   node cli.js watch [--url http://host:3000] [--interval 1] [--once]
const {parseArgs} = require('node:util');

const HISTORY = 30;
const BAR_WIDTH = 20;
const SPARK = '▁▂▃▄▅▆▇█';

const useColor = process.stdout.isTTY && !process.env.NO_COLOR;
const paint = (code, text) => (useColor ? `\x1b[${code}m${text}\x1b[0m` : text);
const level = (ratio) => (ratio >= 0.85 ? 31 : ratio >= 0.6 ? 33 : 32); // red, yellow, green

// `neutral` draws the bar in blue instead of the green/yellow/red of a real limit
function bar(value, max, neutral) {
    const ratio = max > 0 ? Math.min(value / max, 1) : 0;
    const filled = Math.round(ratio * BAR_WIDTH);
    return paint(neutral ? 34 : level(ratio), '█'.repeat(filled)) + '░'.repeat(BAR_WIDTH - filled);
}

// One character per sample, scaled between 0 and `max`
function spark(history, max) {
    return history
        .map((value) => SPARK[Math.min(SPARK.length - 1, Math.floor((value / (max || 1)) * (SPARK.length - 1)))])
        .join('');
}

function duration(seconds) {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = seconds % 60;
    return h ? `${h}h${m}m${s}s` : m ? `${m}m${s}s` : `${s}s`;
}

// Keeps the end of a long path, which is the part that tells directories apart
function shorten(text, max) {
    return text.length > max ? `…${text.slice(text.length - max + 1)}` : text;
}

// What /ready answers, in words. A server older than 3.1 has no `ready` in /info.
function readiness(ready) {
    if (!ready) return '';
    if (ready.ready) return `readiness: ${paint(32, 'ready')}`;
    const back = ready.secondsLeft === null ? 'until /ready/on' : `ready again in ${ready.secondsLeft} s`;
    return `readiness: ${paint(31, 'NOT READY')} (${back})`;
}

function row(label, value, max, text, history, sparkMax, neutral = false) {
    return `${label.padEnd(14)}${bar(value, max, neutral)}  ${text.padEnd(26)} ${paint(36, spark(history, sparkMax))}`;
}

function render(url, info, history) {
    const busy = info.stress.perWorker.filter((n) => n > 0).length;
    const rssLimit = info.memory.containerLimitMb;
    const rssText = rssLimit ? `${info.memory.processRssMb} / ${rssLimit} MB (container)` : `${info.memory.processRssMb} MB (no limit)`;
    // without a container limit the bar is only a size reference: the memory budget of /memory
    const rssMax = rssLimit || Math.max(info.memory.limitMb, ...history.rss);
    const diskText = `${info.disk.heldMb} / ${info.disk.limitMb} MB`;
    const free = info.disk.freeMb === null ? 'not usable' : `${info.disk.freeMb} MB free`;

    return [
        `${paint(1, info.name)} ${info.version}  ${url}  up ${duration(info.uptimeSeconds)}`,
        '',
        row('CPU workers', busy, info.stress.workers, `${busy} busy / ${info.stress.workers}`, history.busy, info.stress.workers),
        row('Memory held', info.memory.heldMb, info.memory.limitMb, `${info.memory.heldMb} / ${info.memory.limitMb} MB`, history.held, info.memory.limitMb),
        row('Process RSS', info.memory.processRssMb, rssMax, rssText, history.rss, rssMax, !rssLimit),
        row('Disk held', info.disk.heldMb, info.disk.limitMb, diskText, history.disk, info.disk.limitMb),
        ...(info.fd ? [row('FDs held', info.fd.held, info.fd.limit, `${info.fd.held} / ${info.fd.limit}`, history.fd, info.fd.limit)] : []),
        '',
        `/stress in progress: ${info.stress.inFlight}   /delay waiting: ${info.delay.inFlight}`,
        ...(info.ready ? [readiness(info.ready)] : []),
        `disk dir: ${shorten(info.disk.dir, 40)}   ${free}`,
    ].join('\n');
}

function record(history, info) {
    const values = {
        busy: info.stress.perWorker.filter((n) => n > 0).length,
        held: info.memory.heldMb,
        rss: info.memory.processRssMb,
        disk: info.disk.heldMb,
        fd: info.fd ? info.fd.held : 0,
    };
    for (const key of Object.keys(values)) {
        history[key].push(values[key]);
        if (history[key].length > HISTORY) history[key].shift();
    }
}

async function fetchInfo(url) {
    const res = await fetch(new URL('/info', url), {signal: AbortSignal.timeout(2000)});
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return res.json();
}

async function main() {
    const {values, positionals} = parseArgs({
        allowPositionals: true,
        options: {
            url: {type: 'string', default: `http://127.0.0.1:${process.env.PORT || 3000}`},
            interval: {type: 'string', default: '1'},
            once: {type: 'boolean', default: false},
            help: {type: 'boolean', short: 'h', default: false},
        },
    });
    const command = positionals[0] || 'watch';

    if (values.help || command !== 'watch') {
        console.log(`Usage: node cli.js watch [--url <url>] [--interval <seconds>] [--once]

  --url       server to watch (default: http://127.0.0.1:$PORT or :3000)
  --interval  seconds between samples (default: 1)
  --once      print one sample and exit, with status 1 if the server can't be reached`);
        process.exit(values.help ? 0 : 1);
    }

    const interval = Math.max(0.2, Number(values.interval) || 1) * 1000;
    const history = {busy: [], held: [], rss: [], disk: [], fd: []};

    if (values.once) {
        try {
            const info = await fetchInfo(values.url);
            record(history, info);
            console.log(render(values.url, info, history));
        } catch (e) {
            console.error(`Cannot reach ${values.url}: ${e.message}`);
            process.exit(1);
        }
        return;
    }

    const screen = process.stdout.isTTY;
    if (screen) process.stdout.write('\x1b[?1049h\x1b[?25l'); // alternate screen, hide cursor
    const quit = () => {
        if (screen) process.stdout.write('\x1b[?25h\x1b[?1049l');
        process.exit(0);
    };
    process.on('SIGINT', quit);
    process.on('SIGTERM', quit);

    let ticking = false;
    const tick = async () => {
        if (ticking) return; // the previous request is still pending
        ticking = true;
        let frame;
        try {
            const info = await fetchInfo(values.url);
            record(history, info);
            frame = render(values.url, info, history);
        } catch (e) {
            frame = `Cannot reach ${values.url}: ${e.message}`;
        }
        process.stdout.write((screen ? '\x1b[H\x1b[J' : '\n') + frame + (screen ? '\n\nCtrl+C to quit\n' : '\n'));
        ticking = false;
    };
    await tick();
    setInterval(tick, interval);
}

if (require.main === module) main();
