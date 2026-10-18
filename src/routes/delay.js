const {readParams} = require('../params');

// Answers after `ms` milliseconds without blocking the event loop
module.exports = function createDelay(config) {
    let inFlight = 0;

    const handler = (req, res) => {
        const params = readParams(req, res, {
            ms: {fallback: config.defaultDelayMs, min: 0, max: config.maxDelayMs},
        });
        if (!params) return;
        const {ms} = params;

        inFlight++;
        let done = false;
        const finish = () => {
            if (done) return;
            done = true;
            inFlight--;
            clearTimeout(timer);
        };
        const timer = setTimeout(() => {
            finish();
            res.status(200).end(`Answered after ${ms} ms`);
        }, ms);
        res.on('close', finish); // the client gave up
    };

    return {handler, inFlight: () => inFlight};
};
