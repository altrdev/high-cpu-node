const {readParams} = require('../params');
const {specs} = require('../endpoints');

// Runs a Fibonacci calculation in a worker process, to keep a core busy
module.exports = function createStress(config, pool) {
    return (req, res) => {
        const params = readParams(req, res, specs(config).stress);
        if (!params) return;
        pool.run(params.number, res);
    };
};
