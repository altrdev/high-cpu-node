const {readParams} = require('../params');

// Runs a Fibonacci calculation in a worker process, to keep a core busy
module.exports = function createStress(config, pool) {
    return (req, res) => {
        const params = readParams(req, res, {
            number: {fallback: config.defaultNumber, min: 0, max: config.maxNumber},
        });
        if (!params) return;
        pool.run(params.number, res);
    };
};
