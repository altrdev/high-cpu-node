const {MB} = require('../units');
const {readParams} = require('../params');
const {specs} = require('../endpoints');

// Memory that grows by itself, `rate` MB each second up to `mb` MB, and is released after
// `hold` seconds: a leak you can watch from the outside before it ends in an OOM kill. It
// uses the budget of /memory (MAX_MEMORY_MB) as it grows, and stops growing when that is full.
module.exports = function createLeak(config, memory) {
    const leaks = new Set();

    const handler = (req, res) => {
        const params = readParams(req, res, specs(config).leak);
        if (!params) return;
        const {rate, mb, hold} = params;

        const leak = {buffers: [], releases: [], leaked: 0, interval: null, timer: null};

        const end = () => {
            leaks.delete(leak);
            leak.buffers.length = 0;
            leak.releases.forEach((release) => release());
            memory.collect();
        };
        const stopGrowing = () => {
            clearInterval(leak.interval);
            leak.timer = setTimeout(end, hold * 1000);
        };
        // returns false when nothing could be added
        const grow = () => {
            const chunk = Math.min(rate, mb - leak.leaked);
            const release = memory.budget.reserve(chunk);
            if (!release) return false;
            try {
                leak.buffers.push(Buffer.allocUnsafe(chunk * MB).fill(1));
            } catch (e) {
                release();
                return false;
            }
            leak.releases.push(release);
            leak.leaked += chunk;
            return true;
        };

        if (!grow()) {
            return res.status(429).end(`${memory.budget.held} MB already held, limit is ${memory.budget.limit} MB`);
        }
        leaks.add(leak);
        if (leak.leaked >= mb) {
            stopGrowing();
        } else {
            leak.interval = setInterval(() => {
                if (!grow() || leak.leaked >= mb) stopGrowing();
            }, 1000);
        }

        res.status(200).end(`Leaking ${rate} MB/s up to ${mb} MB (${Math.ceil(mb / rate)} s), then held for ${hold} s`);
    };

    const stop = () => {
        leaks.forEach((leak) => {
            clearInterval(leak.interval);
            clearTimeout(leak.timer);
        });
        leaks.clear();
    };

    return {handler, stop};
};
