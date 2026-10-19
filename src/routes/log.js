const {readParams} = require('../params');
const {specs} = require('../endpoints');

const TICK_MS = 20;

// Writes `lines` lines to stdout at `rate` lines each second and stops, to test log collection,
// rotation and what the logs of a container do to a disk. A new request replaces the one still
// running, and the lines already written stay where they are.
module.exports = function createLog(config) {
    let job = null; // {lines, rate, sent, startedAt, timer}

    const stop = () => {
        if (job) clearInterval(job.timer);
        job = null;
    };

    const handler = (req, res) => {
        const params = readParams(req, res, specs(config).log);
        if (!params) return;
        const {lines, rate} = params;

        stop();
        const current = {lines, rate, sent: 0, startedAt: Date.now(), timer: null};
        current.timer = setInterval(() => {
            const due = Math.min(lines, Math.floor(((Date.now() - current.startedAt) / 1000) * rate));
            if (due > current.sent) {
                let chunk = '';
                for (let n = current.sent + 1; n <= due; n++) chunk += `${new Date().toISOString()} torchio-log line ${n} of ${lines}\n`;
                process.stdout.write(chunk);
                current.sent = due;
            }
            if (current.sent >= lines) {
                clearInterval(current.timer);
                if (job === current) job = null;
            }
        }, TICK_MS);
        job = current;

        res.status(200).end(`Writing ${lines} lines at ${rate} lines/s (${Math.ceil(lines / rate)} s)`);
    };

    // what is still being written, null when nothing is
    const state = () => (job ? {sent: job.sent, lines: job.lines, rate: job.rate} : null);

    return {handler, state, stop};
};
