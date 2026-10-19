const v8 = require('v8');
const vm = require('vm');
const {Budget} = require('../budget');
const {MB} = require('../units');
const {readParams} = require('../params');
const {specs} = require('../endpoints');

// Buffers are only returned to the OS when the garbage collector runs, so collect
// right after releasing instead of waiting for V8 to decide
v8.setFlagsFromString('--expose-gc');
const gc = vm.runInNewContext('gc');

// Allocates `mb` megabytes and holds them for `hold` seconds. MAX_MEMORY_MB is the
// total that can be held at the same time.
module.exports = function createMemory(config) {
    const budget = new Budget(config.maxMemoryMb);

    const handler = (req, res) => {
        const params = readParams(req, res, specs(config).memory);
        if (!params) return;
        const {mb, hold} = params;

        const release = budget.reserve(mb);
        if (!release) {
            return res.status(429).end(`${budget.held} MB already held, limit is ${budget.limit} MB`);
        }

        let buffer;
        try {
            // fill() touches every page, otherwise the OS may not actually commit the memory
            buffer = Buffer.allocUnsafe(mb * MB).fill(1);
        } catch (e) {
            release();
            throw e;
        }
        setTimeout(() => {
            buffer = null;
            release();
            gc();
        }, hold * 1000);

        res.status(200).end(`Allocated ${mb} MB, held for ${hold} s`);
    };

    return {handler, budget, collect: gc};
};
