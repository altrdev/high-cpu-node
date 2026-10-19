const fs = require('fs');
const os = require('os');
const {Budget} = require('../budget');
const {readParams} = require('../params');
const {specs} = require('../endpoints');

// Opens `count` file descriptors and holds them for `hold` seconds, to test what happens when a
// process runs out of them (EMFILE). MAX_FD is the total that can be held at the same time.
// When the limit of the process is lower, the ones that could be opened stay open until `hold`
// ends: the server then can't accept connections either, which is the case being tested.
module.exports = function createFd(config) {
    const budget = new Budget(config.maxFd);
    const open = new Set();

    const close = (fd) => {
        open.delete(fd);
        try {
            fs.closeSync(fd);
        } catch (e) {
            // already closed
        }
    };

    const handler = (req, res) => {
        const params = readParams(req, res, specs(config).fd);
        if (!params) return;
        const {count, hold} = params;

        const release = budget.reserve(count);
        if (!release) {
            return res.status(429).end(`${budget.held} descriptors already held, limit is ${budget.limit}`);
        }

        const fds = [];
        let failure = null;
        for (let i = 0; i < count; i++) {
            try {
                const fd = fs.openSync(os.devNull, 'r');
                open.add(fd);
                fds.push(fd);
            } catch (e) {
                failure = e.code || e.message;
                break;
            }
        }

        if (failure) {
            // give back what was reserved but not opened
            release();
            const releaseOpened = fds.length > 0 ? budget.reserve(fds.length) : () => {};
            setTimeout(() => {
                fds.forEach(close);
                releaseOpened();
            }, hold * 1000);
            return res.status(503).end(`Opened ${fds.length} of ${count} descriptors, then ${failure}. Held for ${hold} s`);
        }

        setTimeout(() => {
            fds.forEach(close);
            release();
        }, hold * 1000);
        res.status(200).end(`Opened ${count} descriptors, held for ${hold} s`);
    };

    const closeAll = () => [...open].forEach(close);

    return {handler, budget, closeAll};
};
