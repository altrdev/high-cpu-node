const {readParams} = require('../params');
const {specs} = require('../endpoints');

// Keeps the CPU at a level instead of running a calculation: each worker is busy for `percent`
// of the time during `hold` seconds, which is what a CPU based autoscaler reacts to. With
// one worker for each core, `percent` is the load of the machine. A new request replaces
// the previous one, and percent=0 stops it.
module.exports = function createCpu(config, pool) {
    return (req, res) => {
        const params = readParams(req, res, specs(config).cpu);
        if (!params) return;
        const {percent, hold} = params;

        pool.burn(percent, hold);
        if (percent === 0 || hold === 0) return res.status(200).end('Stopped burning CPU');
        res.status(200).end(`Burning ${percent}% of ${pool.workers.length} workers for ${hold} s`);
    };
};
