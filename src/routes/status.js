const {STATUS_CODES} = require('../endpoints');

// Answers with the requested HTTP status code, to test probes and retries
module.exports = function status(req, res) {
    const code = Number(req.params.code);
    if (!Number.isInteger(code) || code < STATUS_CODES.min || code > STATUS_CODES.max) {
        return res.status(400).end(`code must be an integer between ${STATUS_CODES.min} and ${STATUS_CODES.max}`);
    }
    res.status(code).end(`Status: ${code}`);
};
