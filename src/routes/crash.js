const {readParams} = require('../params');
const {specs} = require('../endpoints');

// Ends the whole process with the exit code you ask for, after `after` seconds, to test
// restart policies, backoff and CrashLoopBackOff. It is answered before the process exits.
module.exports = function createCrash(config) {
    return (req, res) => {
        const params = readParams(req, res, specs(config).crash);
        if (!params) return;
        const {code, after} = params;

        const exit = () => process.exit(code);
        res.status(200).end(`Exiting with code ${code}${after > 0 ? ` in ${after} s` : ''}`, () => {
            if (after > 0) setTimeout(exit, after * 1000);
            else exit();
        });
    };
};
